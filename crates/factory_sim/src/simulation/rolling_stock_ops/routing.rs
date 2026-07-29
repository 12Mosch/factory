//! Sending a train somewhere: planning a route, driving it, and throwing it
//! away when the track under it changes.
//!
//! Three rules hold this together.
//!
//! * **A destination outlives a plan.** [`Train::destination`] is what the train
//!   was asked for; [`Train::route`] is the plan it is currently driving. A
//!   train with a destination and no route is one waiting for a search: track
//!   was pulled up under its plan, or the tick's expansion budget was already
//!   spent, or its last search ran out of expansions without an answer. Nothing
//!   special has to happen for any of those: the pass that plans routes finds it
//!   in exactly that state on a later tick. Only a search that comes back with
//!   *no route exists* takes the destination away.
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

/// The reusable half of train routing: search scratch, the tick's remaining
/// expansion budget, and where in the trains the last tick got to.
///
/// Derived, runtime-only state. Every route it produces is durable and lives on
/// the train; nothing here survives a save, and nothing here takes part in
/// simulation identity.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct TrainRouting {
    scratch: RailRouteScratch,
    /// Every rail some piece of stock is standing on, ascending.
    ///
    /// Gathered once per tick rather than once per search: it is proportional
    /// to the world's stock, and rebuilding it for each train that wants a
    /// route would make a tick where many of them do quadratic in the stock.
    /// Held here and never freed, so the gathering is not also an allocation.
    occupied: Vec<EntityId>,
    /// Rails the train currently being planned for is itself standing on,
    /// ascending. Subtracted from the occupancy above, because a train is not
    /// in its own way.
    exempt: Vec<EntityId>,
    /// Trains that have somewhere to be and no plan for getting there, in id
    /// order. Refilled each tick the pass runs.
    waiting: Vec<TrainId>,
    /// The train planned for last. Planning resumes after it rather than at the
    /// lowest id, so a train whose search costs the whole budget cannot keep
    /// every train behind it from ever being planned.
    planned_last: Option<TrainId>,
    remaining_expansions: usize,
}

impl_runtime_only_identity!(TrainRouting);

impl TrainRouting {
    pub(in crate::simulation) fn begin_tick(&mut self) {
        self.remaining_expansions = ROUTE_EXPANSIONS_PER_TICK;
    }

    /// Whether this tick can still pay for a whole search.
    ///
    /// Asked before a search's own inputs are gathered rather than inside it, so
    /// a train the tick is about to defer costs nothing to defer.
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
            exempt,
            remaining_expansions,
            ..
        } = self;
        let (outcome, expansions) = scratch.find_route(&RailRouteRequest {
            graph,
            start,
            target,
            reversal_penalty_fixed: TRAIN_REVERSAL_PENALTY_FIXED,
            occupied_penalty_fixed: TRAIN_OCCUPIED_RAIL_PENALTY_FIXED,
            occupied,
            exempt,
            max_expansions: ROUTE_MAX_EXPANSIONS,
        });
        *remaining_expansions = remaining_expansions.saturating_sub(expansions);
        Some(outcome)
    }

    /// Collects every rail any stock is standing on, for the tick.
    fn collect_occupied_rails(
        &mut self,
        graph: &RailGraph,
        rolling_stock: &RollingStockSubsystem,
        prototypes: &PrototypeCatalog,
    ) {
        self.occupied.clear();
        for stock in rolling_stock.iter() {
            push_stock_rails(graph, prototypes, stock, &mut self.occupied);
        }
        self.occupied.sort_unstable();
        self.occupied.dedup();
    }

    /// Collects the rails `train_id`'s own stock is standing on, which the
    /// search then reads as track nothing is in the way on.
    fn collect_exempt_rails(
        &mut self,
        graph: &RailGraph,
        rolling_stock: &RollingStockSubsystem,
        prototypes: &PrototypeCatalog,
        train_id: TrainId,
    ) {
        self.exempt.clear();
        for stock in rolling_stock.iter().filter(|stock| stock.train == train_id) {
            push_stock_rails(graph, prototypes, stock, &mut self.exempt);
        }
        self.exempt.sort_unstable();
        self.exempt.dedup();
    }
}

/// The waiting trains in the order this tick plans for them: id order, resumed
/// just after the one planned for last and wrapping round once.
///
/// The rotation is what makes the budget fair. A train whose search costs the
/// whole of it — a destination across a railway larger than one search may walk
/// — would otherwise be reached first on every tick and deny every train behind
/// it a plan for as long as it kept asking.
fn planning_order(
    waiting: &[TrainId],
    planned_last: Option<TrainId>,
) -> impl Iterator<Item = &TrainId> {
    let resume = planned_last.map_or(0, |last| waiting.partition_point(|id| *id <= last));
    waiting[resume..].iter().chain(&waiting[..resume])
}

/// Records what a search came back with on the train that asked for it.
///
/// The two failures are not the same failure, and the difference is the whole
/// of this function. `Unreachable` is an answer: there is no way there, so the
/// train stops asking. `Exhausted` is the absence of one — the search ran out of
/// expansions, which says nothing about whether a route exists — so the
/// destination is kept and tried again on a later tick. Throwing it away there
/// would strand a train whose destination is perfectly reachable, on the very
/// railways big enough to need the cap.
///
/// Both brake. A plan can be lost mid-run, which leaves a train at speed with
/// nothing steering it, and rolling on until resistance stops it is not a train
/// anybody is driving.
fn record_route_outcome(train: &mut crate::rolling_stock::Train, outcome: RailRouteOutcome) {
    match outcome {
        RailRouteOutcome::Found(route) => train.route = Some(route),
        RailRouteOutcome::Unreachable => {
            train.destination = None;
            train.route = None;
            train.throttle = TrainThrottle::Brake;
        }
        RailRouteOutcome::Exhausted => {
            train.route = None;
            train.throttle = TrainThrottle::Brake;
        }
    }
}

/// Adds every rail one piece of stock lies on to `rails`.
///
/// The body is walked from its back end forward over its own length — the same
/// extent [`super::stock_ends`] measures, so occupancy covers exactly the track
/// the piece stands on and not a unit more of it at either end.
fn push_stock_rails(
    graph: &RailGraph,
    prototypes: &PrototypeCatalog,
    stock: &crate::rolling_stock::RollingStock,
    rails: &mut Vec<EntityId>,
) {
    let Some(half) = prototypes
        .entity(stock.prototype_id)
        .and_then(|prototype| prototype.rolling_stock)
        .map(|rolling_stock| i64::from(rolling_stock.length_fixed) / 2)
    else {
        return;
    };
    let back = travel(graph, stock.position, -half).position;
    edges_along(graph, back, half * 2, |edge| rails.push(edge));
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

    /// Plans a route for every train that has somewhere to be and no way of
    /// getting there, until the tick's budget runs out.
    ///
    /// One pass for the whole tick rather than a search wedged into each train's
    /// step, because the budget and the occupancy are both tick-wide: the rails
    /// something is standing on are gathered once here and read by every search,
    /// and where the pass stopped last tick is where it starts this one.
    ///
    /// Round-robin rather than id order. A train whose search costs the whole
    /// budget every tick — a destination across a railway larger than one search
    /// may walk — would otherwise deny every train behind it a plan forever,
    /// which is the difference between one train not moving and none of them
    /// moving.
    pub(in crate::simulation) fn plan_train_routes(&mut self) {
        let mut waiting = std::mem::take(&mut self.train_routing.waiting);
        waiting.clear();
        waiting.extend(
            self.rolling_stock
                .trains()
                .filter(|train| train.is_routed() && train.route.is_none())
                .map(|train| train.id),
        );
        if waiting.is_empty() || !self.train_routing.can_search() {
            self.train_routing.waiting = waiting;
            return;
        }

        let Simulation {
            train_routing,
            rails,
            rolling_stock,
            world,
            ..
        } = self;
        train_routing.collect_occupied_rails(&rails.graph, rolling_stock, &world.prototypes);

        for train_id in planning_order(&waiting, self.train_routing.planned_last) {
            if !self.train_routing.can_search() {
                break;
            }
            self.train_routing.planned_last = Some(*train_id);
            self.plan_train_route(*train_id);
        }
        self.train_routing.waiting = waiting;
    }

    /// Sets a routed train's throttle for this tick.
    ///
    /// Runs before the train is stepped, so the throttle the step reads is the
    /// one this leg asks for. A train whose plan is still being waited on — the
    /// tick's budget was spent, or its last search ran out of expansions — is
    /// left alone: it has already been braked, and there is nothing to steer it
    /// along yet.
    pub(super) fn steer_train(&mut self, train_id: TrainId) {
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
        train_routing.collect_exempt_rails(
            &rails.graph,
            rolling_stock,
            &world.prototypes,
            train_id,
        );
        let Some(outcome) = train_routing.plan(&rails.graph, start, target) else {
            return;
        };
        if let Some(train) = rolling_stock.trains.get_mut(&train_id) {
            record_route_outcome(train, outcome);
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

    fn train(raw: u64) -> crate::rolling_stock::Train {
        crate::rolling_stock::Train {
            id: TrainId::new(raw),
            stock: Vec::new(),
            velocity: 0,
            travel_remainder: 0,
            throttle: TrainThrottle::Forward,
            destination: Some(RailTarget::new(EntityId::new(raw), 0)),
            route: None,
        }
    }

    /// A search that ran out of expansions has not answered the question, so the
    /// train keeps where it was going and is planned for again later. Giving up
    /// here would strand a train whose destination is perfectly reachable on
    /// exactly the railways that are big enough to reach the cap.
    #[test]
    fn an_exhausted_search_keeps_the_destination_and_an_unreachable_one_does_not() {
        let mut exhausted = train(1);
        record_route_outcome(&mut exhausted, RailRouteOutcome::Exhausted);
        assert!(exhausted.destination.is_some());
        assert_eq!(exhausted.route, None);
        assert_eq!(exhausted.throttle, TrainThrottle::Brake);

        let mut unreachable = train(2);
        record_route_outcome(&mut unreachable, RailRouteOutcome::Unreachable);
        assert_eq!(unreachable.destination, None);
        assert_eq!(unreachable.route, None);
        assert_eq!(unreachable.throttle, TrainThrottle::Brake);
    }

    /// Planning resumes after the train it last reached and wraps round, so a
    /// train that spends the whole budget every tick cannot hold the queue
    /// behind it still.
    #[test]
    fn planning_resumes_after_the_train_it_last_reached() {
        let waiting = [TrainId::new(1), TrainId::new(4), TrainId::new(9)];
        let order = |planned_last| {
            planning_order(&waiting, planned_last)
                .map(|train_id| train_id.raw())
                .collect::<Vec<_>>()
        };

        assert_eq!(order(None), vec![1, 4, 9]);
        assert_eq!(order(Some(TrainId::new(1))), vec![4, 9, 1]);
        assert_eq!(order(Some(TrainId::new(9))), vec![1, 4, 9]);
        // A train that has since arrived, been mined, or been cut in two is no
        // longer waiting; the pass resumes at the next one that is.
        assert_eq!(order(Some(TrainId::new(5))), vec![9, 1, 4]);
        assert_eq!(order(Some(TrainId::new(99))), vec![1, 4, 9]);
    }
}
