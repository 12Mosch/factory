//! Sending a train somewhere: planning a route, driving it, and throwing it
//! away when the track under it changes.
//!
//! Three rules hold this together.
//!
//! * **A destination outlives a plan.** [`Train::destination`] is what the train
//!   was asked for; [`Train::route`] is the plan it is currently driving. A
//!   train with a destination and no route is one waiting for a search, which is
//!   the state it is in after track was pulled up under its plan and the state
//!   it is in when the tick's expansion budget was already spent. Nothing
//!   special has to happen for the re-search: the pass that plans routes finds
//!   it in exactly that state on a later tick.
//! * **Following a route is O(1) per tick.** A leg carries the distance still to
//!   run rather than the distance already covered, so a train spends its travel
//!   down against it instead of measuring back up the track it came along. What
//!   a train has to know each tick is how far to the next stop, and that is a
//!   subtraction.
//! * **The route can only stop a train early, never carry it further.** The leg
//!   distance clips the step the same way the end of the line and other stock
//!   do, so a train stops exactly at its mark instead of rolling past it and
//!   being dragged back. Which of the three limits bound the step is what tells
//!   the difference between arriving, waiting for someone to move, and running
//!   out of track.
//!
//! Manual driving wins: a drive command clears whatever the train was doing, so
//! the debug throttle keys are never fighting a plan the player cannot see.

use crate::ids::EntityId;
use crate::rolling_stock::{
    RailPosition, RailTarget, RollingStockSubsystem, TRAIN_OCCUPIED_RAIL_PENALTY_FIXED,
    TRAIN_REVERSAL_PENALTY_FIXED, TrainControlError, TrainId, TrainThrottle,
};
use crate::simulation::rail_ops::{
    RailGraph, RailRouteOutcome, RailRouteRequest, RailRouteScratch,
};
use crate::simulation::*;
use factory_data::PrototypeCatalog;

use super::traversal::edges_along;
use super::{TrainStepLimit, braking_distance_fixed, travel};

/// Node expansions every route search in one tick may spend between them.
///
/// Trains re-path rarely — on a new destination, on a route invalidated by
/// track changing, and later on a signal-driven reroute — so this is a budget
/// for the unusual tick where several of them do it at once. What it buys is
/// that no single tick can spend an unbounded amount of time in here, however
/// many trains want a plan.
const ROUTE_EXPANSIONS_PER_TICK: usize = 8_192;

/// Node expansions one route search may spend before giving up.
///
/// Two states per rail, so this covers a railway of two thousand pieces even if
/// the search has to look at all of it. A search that wants more is deferred
/// rather than truncated: half a route is not a route.
const ROUTE_MAX_EXPANSIONS: usize = 4_096;

/// The reusable half of train routing: search scratch and the tick's remaining
/// expansion budget.
///
/// Derived, runtime-only state. Every route it produces is durable and lives on
/// the train; nothing here survives a save, and nothing here takes part in
/// simulation identity.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct TrainRouting {
    scratch: RailRouteScratch,
    /// Rails other trains are standing on, ascending. Refilled per search and
    /// never freed, so the search's one input that is proportional to the world
    /// is not also an allocation.
    occupied: Vec<EntityId>,
    remaining_expansions: usize,
}

impl_runtime_only_identity!(TrainRouting);

impl TrainRouting {
    pub(in crate::simulation) fn begin_tick(&mut self) {
        self.remaining_expansions = ROUTE_EXPANSIONS_PER_TICK;
    }

    /// Whether this tick can still pay for a whole search.
    ///
    /// Asked before the search's inputs are gathered rather than inside it: what
    /// is standing where is proportional to the world's stock, and a tick where
    /// every train wants a route would otherwise pay that for every train it was
    /// about to defer anyway.
    fn can_search(&self) -> bool {
        self.remaining_expansions >= ROUTE_MAX_EXPANSIONS
    }

    /// Searches for a route, or defers when this tick can no longer pay for a
    /// whole search.
    ///
    /// Deferral is a `None`, and it is deliberately not an outcome of its own:
    /// the caller does nothing either way, and the train asks again next tick
    /// because it still has a destination and no route.
    fn plan(
        &mut self,
        graph: &RailGraph,
        start: RailPosition,
        target: RailTarget,
    ) -> Option<RailRouteOutcome> {
        if !self.can_search() {
            return None;
        }
        let Self {
            scratch,
            occupied,
            remaining_expansions,
        } = self;
        let (outcome, expansions) = scratch.find_route(&RailRouteRequest {
            graph,
            start,
            target,
            reversal_penalty_fixed: TRAIN_REVERSAL_PENALTY_FIXED,
            occupied_penalty_fixed: TRAIN_OCCUPIED_RAIL_PENALTY_FIXED,
            occupied,
            max_expansions: ROUTE_MAX_EXPANSIONS,
        });
        *remaining_expansions = remaining_expansions.saturating_sub(expansions);
        Some(outcome)
    }

    /// Collects the rails every train other than `train_id` is standing on.
    ///
    /// A train is not in its own way, which is why its own stock is skipped —
    /// including the piece the search starts under.
    fn collect_occupied_rails(
        &mut self,
        graph: &RailGraph,
        rolling_stock: &RollingStockSubsystem,
        prototypes: &PrototypeCatalog,
        train_id: TrainId,
    ) {
        self.occupied.clear();
        for stock in rolling_stock.iter() {
            if stock.train == train_id {
                continue;
            }
            let Some(length_fixed) = prototypes
                .entity(stock.prototype_id)
                .and_then(|prototype| prototype.rolling_stock)
                .map(|rolling_stock| i64::from(rolling_stock.length_fixed))
            else {
                continue;
            };
            let back = travel(graph, stock.position, -(length_fixed / 2)).position;
            edges_along(graph, back, length_fixed, |edge| self.occupied.push(edge));
        }
        self.occupied.sort_unstable();
        self.occupied.dedup();
    }
}

impl Simulation {
    /// Sends a train to a rail, stopping in the middle of it.
    ///
    /// The search does not run here. A command lands on a tick boundary and the
    /// rail graph is rebuilt inside the tick, so planning here would either
    /// route over a graph that is about to change or force a rebuild out of
    /// turn; what this does is record where the train is going and let the
    /// routing pass plan it against the track that actually exists.
    pub fn set_train_destination(
        &mut self,
        train_id: TrainId,
        rail: EntityId,
    ) -> Result<(), TrainControlError> {
        let geometry = self
            .rail_piece_geometry(rail)
            .ok_or(TrainControlError::NotRail(rail))?;
        let train = self
            .rolling_stock
            .trains
            .get_mut(&train_id)
            .ok_or(TrainControlError::MissingTrain(train_id))?;
        train.destination = Some(RailTarget::new(rail, geometry.length_fixed / 2));
        train.route = None;
        Ok(())
    }

    /// Cancels where a train was going. It brakes rather than coasting: a train
    /// whose orders were withdrawn should come to a stop, not keep rolling
    /// toward a destination nobody is steering it to any more.
    pub fn clear_train_destination(&mut self, train_id: TrainId) -> Result<(), TrainControlError> {
        let train = self
            .rolling_stock
            .trains
            .get_mut(&train_id)
            .ok_or(TrainControlError::MissingTrain(train_id))?;
        train.destination = None;
        train.route = None;
        train.throttle = TrainThrottle::Brake;
        Ok(())
    }

    /// Plans what a routed train has no plan for, and sets its throttle for this
    /// tick.
    ///
    /// Runs before the train is stepped, so the throttle the step reads is the
    /// one this leg asks for.
    pub(super) fn steer_train(&mut self, train_id: TrainId) {
        if self
            .rolling_stock
            .train(train_id)
            .is_none_or(|train| !train.is_routed())
        {
            return;
        }
        if self
            .rolling_stock
            .train(train_id)
            .is_some_and(|train| train.route.is_none())
        {
            self.plan_train_route(train_id);
        }

        let Some(train) = self.rolling_stock.train(train_id) else {
            return;
        };
        let Some(leg) = train.route.as_ref().and_then(|route| route.current_leg()) else {
            return;
        };
        // Braking is decided against the distance left rather than against a
        // point on the map, and against the model's own stopping distance rather
        // than a guessed margin, so a heavy train starts braking earlier than a
        // light one without either of them being told to.
        let forces = self.train_forces_now(train_id).unwrap_or_default();
        let throttle = if leg.distance_fixed <= braking_distance_fixed(train.velocity, forces) {
            TrainThrottle::Brake
        } else if leg.forward {
            TrainThrottle::Forward
        } else {
            TrainThrottle::Reverse
        };
        if let Some(train) = self.rolling_stock.trains.get_mut(&train_id) {
            train.throttle = throttle;
        }
    }

    /// Searches for a route from where the train stands to where it was sent.
    ///
    /// The route is measured from the train's leading piece. Every piece of a
    /// train advances by the same distance, so which piece the plan is measured
    /// from only fixes where "arrived" leaves the train standing, and the front
    /// is the piece a destination is naturally about.
    fn plan_train_route(&mut self, train_id: TrainId) {
        let Some((start, target)) = self.rolling_stock.train(train_id).and_then(|train| {
            let start = self
                .rolling_stock
                .get(*train.stock.first()?)
                .map(|stock| stock.position)?;
            Some((start, train.destination?))
        }) else {
            return;
        };

        let Simulation {
            train_routing,
            rails,
            rolling_stock,
            world,
            ..
        } = self;
        if !train_routing.can_search() {
            return;
        }
        train_routing.collect_occupied_rails(
            &rails.graph,
            rolling_stock,
            &world.prototypes,
            train_id,
        );
        let Some(outcome) = train_routing.plan(&rails.graph, start, target) else {
            return;
        };
        let Some(train) = rolling_stock.trains.get_mut(&train_id) else {
            return;
        };
        match outcome {
            RailRouteOutcome::Found(route) => train.route = Some(route),
            // Nothing to drive toward, so the train stops asking. Braking rather
            // than coasting matters here: a route invalidated mid-run can leave
            // a train at speed with nowhere to go, and rolling on until
            // resistance stops it would be a train nobody steered.
            RailRouteOutcome::Unreachable | RailRouteOutcome::Exhausted => {
                train.destination = None;
                train.route = None;
                train.throttle = TrainThrottle::Brake;
            }
        }
    }

    /// Spends a tick's travel against the route and retires legs that are done.
    ///
    /// Runs after the step, and is told what bound it: a leg the *track* cut
    /// short is a leg that will never be finished — a train reversing at a
    /// buffer stops with its nose against it, short of the point the plan turned
    /// around at — so it counts as run out rather than leaving the train pushing
    /// at a dead end forever. A leg cut short by other stock is a leg the train
    /// is still on: it waits.
    pub(super) fn advance_train_route(
        &mut self,
        train_id: TrainId,
        travelled_fixed: i64,
        limit: Option<TrainStepLimit>,
    ) {
        let Some(train) = self.rolling_stock.trains.get_mut(&train_id) else {
            return;
        };
        let stationary = train.is_stationary();
        let Some(route) = train.route.as_mut() else {
            return;
        };
        let Some(leg) = route.legs.front_mut() else {
            train.route = None;
            return;
        };
        // Travel away from the leg's end lengthens it. That is not a correction
        // — it is the distance the train now has to make up — and it is why the
        // leg is spent down by signed travel rather than by how far the train
        // moved.
        leg.distance_fixed -= if leg.forward {
            travelled_fixed
        } else {
            -travelled_fixed
        };
        let stalled = matches!(limit, Some(TrainStepLimit::Track));
        if !stationary || (leg.distance_fixed > 0 && !stalled) {
            return;
        }

        let unrun = leg.distance_fixed.max(0);
        route.legs.pop_front();
        match route.legs.front_mut() {
            // The next leg runs back down the track this one came up, so track
            // this one could not cover is track the next one no longer has to.
            Some(next) => next.distance_fixed = (next.distance_fixed - unrun).max(0),
            None => {
                train.route = None;
                train.destination = None;
                train.throttle = TrainThrottle::Coast;
            }
        }
    }

    /// How far the route lets the train travel this tick, or `None` when it does
    /// not limit the step at all.
    ///
    /// Only travel *toward* the leg's end is limited. A train still rolling the
    /// other way is getting further from the point it must stop at, and clipping
    /// that would pin it in place instead of letting it come round.
    pub(super) fn route_clearance_fixed(
        &self,
        train_id: TrainId,
        travel_fixed: i64,
    ) -> Option<i64> {
        let leg = self
            .rolling_stock
            .train(train_id)?
            .route
            .as_ref()?
            .current_leg()?;
        let sign = if leg.forward { 1 } else { -1 };
        (travel_fixed.signum() == sign).then(|| sign * leg.distance_fixed.max(0))
    }

    /// Throws away a train's plan while leaving it its destination, so the
    /// routing pass plans it again from wherever it now stands.
    ///
    /// Called whenever a train changes shape. A route is measured from the
    /// train's leading piece and priced for the stock that was coupled when it
    /// was found; a train that has just gained a wagon, lost one, or been cut in
    /// two is not that train, and driving its old plan would run the new one to
    /// the wrong place.
    pub(super) fn discard_train_route(&mut self, train_id: TrainId) {
        if let Some(train) = self.rolling_stock.trains.get_mut(&train_id) {
            train.route = None;
        }
    }

    /// Drops the plans that ran over track which is no longer there.
    ///
    /// Called when the rail graph is invalidated, which is the one moment a
    /// route can stop describing the world. A train whose route went is left
    /// with its destination, so the routing pass plans it again from where it
    /// now stands; a train whose *destination* went has nowhere to be sent and
    /// stops.
    pub(in crate::simulation) fn invalidate_train_routes(&mut self) {
        if !self
            .rolling_stock
            .trains()
            .any(|train| train.is_routed() || train.route.is_some())
        {
            return;
        }
        let train_ids = self
            .rolling_stock
            .trains
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for train_id in train_ids {
            let Some(train) = self.rolling_stock.trains.get(&train_id) else {
                continue;
            };
            // A rail that is gone from the entity store is gone: nothing else
            // can remove one, and asking the store is a lookup rather than the
            // geometry resolve asking the catalog would be. Deliberately not a
            // question for the rail graph, which is mid-rebuild.
            let placed = |entity_id: &EntityId| self.entities.placed_entity(*entity_id).is_some();
            let destination_gone = train
                .destination
                .is_some_and(|destination| !placed(&destination.edge));
            let route_gone = train
                .route
                .as_ref()
                .is_some_and(|route| !route.edges.iter().all(placed));
            if !destination_gone && !route_gone {
                continue;
            }
            let Some(train) = self.rolling_stock.trains.get_mut(&train_id) else {
                continue;
            };
            train.route = None;
            if destination_gone {
                train.destination = None;
                train.throttle = TrainThrottle::Brake;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The budget is per tick and a search is all-or-nothing, so a tick that has
    /// already spent most of its allowance defers rather than starting a search
    /// it might not be able to finish. The train keeps its destination, so the
    /// next tick — with a full budget again — plans it.
    #[test]
    fn a_tick_that_has_spent_its_budget_defers_rather_than_searching() {
        let mut routing = TrainRouting::default();
        routing.begin_tick();
        assert!(routing.can_search());

        routing.remaining_expansions = ROUTE_MAX_EXPANSIONS - 1;
        assert!(!routing.can_search());
        assert_eq!(
            routing.plan(
                &RailGraph::default(),
                RailPosition::new(EntityId::new(1), 0, true),
                RailTarget::new(EntityId::new(2), 0),
            ),
            None,
            "a deferred search reports nothing rather than a failure to find a route"
        );

        routing.begin_tick();
        assert!(
            routing.can_search(),
            "the next tick pays for searches again"
        );
    }
}
