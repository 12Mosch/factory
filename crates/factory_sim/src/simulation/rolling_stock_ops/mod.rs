//! The moving half of the rolling-stock subsystem: trains accelerating,
//! braking, and running out of track.
//!
//! Three properties shape the tick.
//!
//! * **One velocity per train.** Every piece of a train advances by the same
//!   signed distance along the track it faces, which is what keeps the
//!   couplings rigid without a per-coupling constraint to solve. When any piece
//!   would run past a free rail end, the whole train is clipped to the distance
//!   that piece could make and stops — a train never stretches.
//! * **Bounded per tick.** A step is a force sum over the train's stock and one
//!   walk per piece along at most a couple of rail edges, plus a single walk
//!   per *train* to find what is on the track ahead of it and another to find
//!   the signal it may not pass. Nothing here scans the world per piece, and the
//!   searches that are proportional to the track around a click — finding what a
//!   new piece couples to — run on placement rather than every tick.
//! * **Integers all the way.** Forces, velocity, and travel are integer; the
//!   sub-unit part of a tick's travel is carried in a remainder rather than
//!   rounded away. The only float in sight is the fuel a locomotive burns,
//!   which is the ordinary burner path every other burner machine uses and
//!   which only gates *whether* a locomotive pulls, never how far it gets.
//!
//! The train runs at the full 60 Hz fixed rate with the rest of the simulation
//! rather than on a coarser schedule of its own. Trains number in the tens, the
//! per-train step is O(1), and a coarser schedule would need its own
//! sub-stepping to keep stopping points exact once signals and stations land.

mod loading;
mod motion;
mod placement;
mod routing;
mod traversal;

pub(in crate::simulation) use loading::{
    StoppedStock, StoppedStockIndex, StoppedStockMut, drop_stock_item, stock_can_accept,
    stock_pickup_item, take_stock_item,
};
pub use motion::braking_distance_fixed;
pub(in crate::simulation) use routing::{TrainRouting, push_stock_rails};
pub(in crate::simulation) use traversal::{TravelOutcome, travel, world_point};

use crate::rail::RailPoint;
use crate::rolling_stock::{
    RailPosition, RollingStock, RollingStockId, TRAIN_VELOCITY_SCALE, Train, TrainControlError,
    TrainForces, TrainId, TrainThrottle,
};
use crate::simulation::rail_ops::RailRouteOutcome;

use crate::circuits::SignalId;
use crate::rolling_stock::{
    RailTarget, TrainSchedule, TrainStopState, TrainWaitCondition, TrainWaitContext,
};
use crate::simulation::*;
use std::collections::BTreeSet;
use std::ops::ControlFlow;

use self::motion::stepped_velocity;

/// What stopped a train's step short of the distance its velocity asked for.
///
/// The four are not interchangeable, which is why the step reports which one
/// bound it rather than a bare "blocked": a train held up by the route in front
/// of it has arrived somewhere, a train held up by the end of the line will
/// never get further, and a train held up by a signal or by other stock is
/// waiting for something that can move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) enum TrainStepLimit {
    /// The leg being driven ends here.
    Route,
    /// The end of the line.
    Track,
    /// A signal the train has not been let past.
    Signal,
    /// Other rolling stock in the way.
    Stock,
}

/// What a train's step this tick came to.
///
/// The distance asked for is kept alongside the distance covered because the two
/// can disagree about *direction*, not only about size: a train still rolling one
/// way while its plan has turned it round is asked for travel the plan does not
/// want, and what a blocked step means depends on which way it was headed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) struct TrainStep {
    /// Signed distance the train's velocity asked for, before anything clipped
    /// it. Zero only when the train did not try to move at all.
    attempted_fixed: i64,
    /// Signed distance the train actually covered.
    travelled_fixed: i64,
    /// What cut the step short, if anything did.
    limit: Option<TrainStepLimit>,
}

impl Simulation {
    /// Gives a freshly placed stop a name of its own.
    ///
    /// Numbered by entity id rather than by a counter, so it is unique without
    /// any state to keep and the same on every machine. A default name matters
    /// more than it looks: a schedule asks for a *name*, and two stops sharing
    /// one is the supported way to say "either platform of this station" — so a
    /// default that repeated would silently build two-platform stations out of
    /// unrelated stops.
    pub(in crate::simulation) fn on_train_stop_placed(&mut self, entity_id: EntityId) {
        if let Some(state) = self.entities.train_stops.get_mut(&entity_id) {
            state.name = format!("Stop {}", entity_id.raw());
        }
    }

    /// Every placed train stop, in entity-id order.
    pub fn train_stops(&self) -> impl Iterator<Item = (EntityId, &TrainStopState)> {
        self.entities
            .train_stops
            .iter()
            .map(|(&entity_id, state)| (entity_id, state))
    }

    pub fn train_stop(&self, entity_id: EntityId) -> Option<&TrainStopState> {
        self.entities.train_stops.get(&entity_id)
    }

    /// Where on the track a stop brings a train to rest, or `None` for a stop
    /// with no rail beside it.
    ///
    /// Derived with the rail graph rather than stored, so a stop is never
    /// holding a mark on track that has since been mined.
    pub fn train_stop_target(&self, entity_id: EntityId) -> Option<RailTarget> {
        debug_assert!(
            !self.rails.graph_dirty,
            "rail graph must be ensured before querying stop marks"
        );
        self.rails.stop_targets.get(&entity_id).copied()
    }

    /// Renames one stop, and the schedules that named it only when the old name
    /// leaves the world with it.
    ///
    /// Rewriting schedules is the friendly answer to renaming the *last* stop of
    /// a name — a schedule left pointing at a station nobody answers to is a
    /// train with nowhere to go — but it is the wrong answer when other stops
    /// still bear the old name. Several stops sharing a name is the supported way
    /// to give one station two platforms, and rewriting a schedule then would
    /// quietly stop the platforms the player did not touch from ever being
    /// served.
    pub fn rename_train_stop(
        &mut self,
        entity_id: EntityId,
        name: impl Into<String>,
    ) -> Result<(), TrainControlError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(TrainControlError::EmptyStopName);
        }
        let state = self
            .entities
            .train_stops
            .get_mut(&entity_id)
            .ok_or(TrainControlError::MissingStop(entity_id))?;
        let old = std::mem::replace(&mut state.name, name.clone());
        if old == name {
            return Ok(());
        }
        if !self.stop_name_exists(&old) {
            for train in self.rolling_stock.trains.values_mut() {
                for entry in &mut train.schedule.entries {
                    if entry.stop_name == old {
                        entry.stop_name.clone_from(&name);
                    }
                }
            }
        }
        // A claim is a booking made against the name an entry asked for, so a
        // platform renamed out from under the train that booked it is a train
        // running to a station its schedule no longer names — and loading there
        // when it arrives. It gives the place back and books one that does
        // answer to its entry instead.
        //
        // Which is exactly the trains whose entry does *not* now name this stop.
        // When the rewrite above ran, every entry that asked for the old name
        // asks for the new one, so the trains already heading here keep their
        // place and nothing moves: renaming the last platform of a station is
        // renaming the station, not closing it.
        self.release_stop_claims(entity_id, |train| {
            train
                .schedule
                .current_entry()
                .is_some_and(|entry| entry.stop_name == name)
        });
        Ok(())
    }

    /// Gives up every claim on `stop` that `keep` does not vouch for, and stops
    /// the trains that lose one.
    ///
    /// A train that loses its claim loses the plan that went with it: the
    /// destination was this stop's mark, and the route was measured to it. Both
    /// go, and the train brakes — the schedule pass books it somewhere on one of
    /// the next ticks, which for a train that still wants this station is this
    /// very tick.
    fn release_stop_claims(&mut self, stop: EntityId, keep: impl Fn(&Train) -> bool) {
        for train in self.rolling_stock.trains.values_mut() {
            if train.scheduled_stop != Some(stop) || keep(train) {
                continue;
            }
            train.release_scheduled_stop();
            train.destination = None;
            train.route = None;
            train.route_search_exhausted_at = None;
            train.throttle = TrainThrottle::Brake;
        }
    }

    /// Gives up every claim on a stop whose mark has moved to another rail.
    ///
    /// A claim aims a train at a point on the track: the destination is that
    /// point and the route is measured to it. A stop that binds to a different
    /// rail — because a nearer one was laid beside it — leaves both describing
    /// where the platform *used* to be, and the train would run out its old
    /// route and call that an arrival on track the station no longer marks.
    /// Nothing else catches it: the old rail is still there, so the route is
    /// still valid track, and only the meaning of it changed.
    pub(in crate::simulation) fn release_claims_on_moved_stops(&mut self, moved: &[EntityId]) {
        for stop in moved {
            self.release_stop_claims(*stop, |_| false);
        }
    }

    /// Sets how many trains may be booked into one stop at a time.
    ///
    /// Zero is refused: a stop no train may be sent to is a stop that should not
    /// exist, and a player who wants one closed has the signal-driven limit
    /// below — which says so explicitly, and says it from the factory rather
    /// than by hand.
    pub fn set_train_stop_limit(
        &mut self,
        entity_id: EntityId,
        train_limit: u32,
    ) -> Result<(), TrainControlError> {
        if train_limit == 0 {
            return Err(TrainControlError::InvalidTrainLimit);
        }
        let state = self
            .entities
            .train_stops
            .get_mut(&entity_id)
            .ok_or(TrainControlError::MissingStop(entity_id))?;
        state.train_limit = train_limit;
        Ok(())
    }

    /// Picks the channel a stop reads its train limit from, or `None` to go back
    /// to the hand-set one.
    pub fn set_train_stop_limit_signal(
        &mut self,
        entity_id: EntityId,
        signal: Option<crate::circuits::SignalId>,
    ) -> Result<(), TrainControlError> {
        if let Some(signal) = signal
            && self.signal_role(signal) != circuit_ops::SignalRole::Value
        {
            return Err(TrainControlError::WildcardSignal(signal));
        }
        let state = self
            .entities
            .train_stops
            .get_mut(&entity_id)
            .ok_or(TrainControlError::MissingStop(entity_id))?;
        state.train_limit_signal = signal;
        Ok(())
    }

    /// How many trains this stop admits right now: what the network says when
    /// the player has wired the limit up, and the hand-set number otherwise.
    ///
    /// A network reading below zero is read as none: a limit is a count of
    /// trains, and a negative one is the same instruction as zero — send
    /// nobody.
    ///
    /// An enable condition the network does not satisfy admits nobody either,
    /// which is what "controllable" means for a stop. A stop's work is taking
    /// trains, so a condition that switches it off has to switch that off — a
    /// connector that accepted a condition and then ignored it would be a
    /// control that looks wired and does nothing.
    ///
    /// Neither closes a stop a train is already standing at: a claim is given up
    /// by the train holding it, never taken back by the station.
    pub fn train_stop_effective_limit(&self, entity_id: EntityId) -> u32 {
        let Some(state) = self.entities.train_stops.get(&entity_id) else {
            return 0;
        };
        if !self.circuit_work_allowed(entity_id) {
            return 0;
        }
        let Some(signal) = state.train_limit_signal else {
            return state.train_limit;
        };
        let node = CircuitNode::new(entity_id, ConnectorPort::Single);
        u32::try_from(self.circuits.value_at(node, signal)).unwrap_or(0)
    }

    /// Whether any stop still answers to `name`.
    fn stop_name_exists(&self, name: &str) -> bool {
        self.entities
            .train_stops
            .values()
            .any(|state| state.name == name)
    }

    /// Releases what a stop that is about to go leaves behind on the trains: the
    /// claim any of them held on it, and — once no stop answers to its name at
    /// all — the schedule entries which can no longer be served.
    ///
    /// The entries are what would otherwise strand a train. An entry naming a
    /// station that no longer exists anywhere can be neither arrived at nor
    /// claimed, so a train that reaches it idles on it for ever with no escape.
    /// Dropping such entries is the escape, and it is only taken when the name
    /// has left the world — while another stop still bears it, the train simply
    /// goes there instead. It mirrors what renaming the last stop of a name
    /// already does to the schedules pointing at it.
    ///
    /// Every matching entry rather than the one being served, because the
    /// schedule is a loop: an entry left behind further down it is the same dead
    /// end, reached a lap later.
    ///
    /// Called while the stop is still in the store, from the destroy path, so
    /// "does the name survive" is asked of the world the removal leaves behind.
    pub(in crate::simulation) fn forget_train_stop(&mut self, entity_id: EntityId) {
        let Some(name) = self
            .entities
            .train_stops
            .get(&entity_id)
            .map(|state| state.name.clone())
        else {
            return;
        };
        let name_remains = self
            .entities
            .train_stops
            .iter()
            .any(|(&other, state)| other != entity_id && state.name == name);
        for train in self.rolling_stock.trains.values_mut() {
            if train.scheduled_stop == Some(entity_id) {
                train.release_scheduled_stop();
                train.destination = None;
                train.route = None;
                train.route_search_exhausted_at = None;
                train.throttle = TrainThrottle::Brake;
            }
            if !name_remains {
                train.schedule.remove_entries_named(&name);
            }
        }
    }

    /// Replaces a train's automatic orders, cancelling whatever it was doing to
    /// serve the old ones.
    ///
    /// Entries are checked before anything is written: an empty station name, or
    /// a circuit condition on one of the wildcard channels, is refused here
    /// rather than accepted and reported much later as a broken world by
    /// validation.
    pub fn set_train_schedule(
        &mut self,
        train_id: TrainId,
        mut schedule: TrainSchedule,
    ) -> Result<(), TrainControlError> {
        if schedule
            .entries
            .iter()
            .any(|entry| entry.stop_name.trim().is_empty())
        {
            return Err(TrainControlError::EmptyStopName);
        }
        for signal in schedule
            .entries
            .iter()
            .flat_map(|entry| &entry.wait_conditions)
            .flat_map(|group| &group.0)
            .flat_map(wait_condition_signals)
        {
            if self.signal_role(signal) != circuit_ops::SignalRole::Value {
                return Err(TrainControlError::WildcardSignal(signal));
            }
        }
        let train = self
            .rolling_stock
            .trains
            .get_mut(&train_id)
            .ok_or(TrainControlError::MissingTrain(train_id))?;
        if schedule.entries.is_empty() {
            schedule.current = 0;
        } else {
            schedule.current %= schedule.entries.len();
        }
        train.schedule = schedule;
        train.release_scheduled_stop();
        train.destination = None;
        train.route = None;
        train.route_search_exhausted_at = None;
        Ok(())
    }

    /// Rolling stock in the world, in ascending id order.
    pub fn rolling_stock(&self) -> impl Iterator<Item = &RollingStock> {
        self.rolling_stock.iter()
    }

    pub fn rolling_stock_piece(&self, id: RollingStockId) -> Option<&RollingStock> {
        self.rolling_stock.get(id)
    }

    pub fn rolling_stock_count(&self) -> usize {
        self.rolling_stock.len()
    }

    /// Trains in the world, in ascending id order.
    pub fn trains(&self) -> impl Iterator<Item = &Train> {
        self.rolling_stock.trains()
    }

    pub fn train(&self, id: TrainId) -> Option<&Train> {
        self.rolling_stock.train(id)
    }

    pub fn train_count(&self) -> usize {
        self.rolling_stock.train_count()
    }

    /// World point of one piece of stock, in fixed-point units, derived from
    /// the geometry of the rail it stands on.
    pub fn rolling_stock_world_point(&self, id: RollingStockId) -> Option<RailPoint> {
        let stock = self.rolling_stock.get(id)?;
        world_point(self, stock.position)
    }

    /// World points of a piece's two ends, which is what a renderer needs to
    /// lay a body along the track it is standing on and what a reach check
    /// measures against.
    pub fn rolling_stock_body(&self, id: RollingStockId) -> Option<(RailPoint, RailPoint)> {
        let stock = self.rolling_stock.get(id)?;
        let half = self.rolling_stock_half_length(stock)?;
        let front = travel(&self.rails.graph, stock.position, half).position;
        let back = travel(&self.rails.graph, stock.position, -half).position;
        Some((world_point(self, back)?, world_point(self, front)?))
    }

    /// The tile a piece of stock stands on, for visibility and cursor queries.
    pub fn rolling_stock_tile(&self, id: RollingStockId) -> Option<(i64, i64)> {
        self.rolling_stock_world_point(id).map(|point| point.tile())
    }

    /// Whether a piece of stock lies over `(x, y)`.
    ///
    /// A cursor query, never a per-tick one: the pre-filter below is what keeps
    /// a held right-click from walking every wagon in the world, and the
    /// per-tick answer to the same question comes from the stopped-stock index
    /// instead.
    pub fn rolling_stock_covers_tile(&self, id: RollingStockId, x: i64, y: i64) -> bool {
        let Some(stock) = self.rolling_stock.get(id) else {
            return false;
        };
        let Some(length) = self.rolling_stock_half_length(stock).map(|half| half * 2) else {
            return false;
        };
        // Cheap reject first. Every point of the body is within the piece's own
        // length of its centre along the track, so it is within that distance
        // in a straight line too — a tile further out cannot be covered.
        // Deliberately a distance from the centre rather than the box the two
        // ends span: a body across an S-bend leaves that box, and a pre-filter
        // that is merely usually right would hide stock instead of
        // over-reporting it.
        let Some(center) = self.rolling_stock_world_point(id) else {
            return false;
        };
        let margin = length.div_euclid(crate::POSITION_SCALE) + 1;
        let (center_x, center_y) = center.tile();
        if (x - center_x).abs() > margin || (y - center_y).abs() > margin {
            return false;
        }

        let mut covered = false;
        self.for_each_rolling_stock_tile(stock, |tile| {
            covered = tile == (x, y);
            if covered {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        covered
    }

    /// Visits every tile a piece of stock lies over, in order from its back end
    /// to its front, repeats included.
    ///
    /// Walked along the body rather than derived from the rectangle its two
    /// ends span: a piece taking a quarter turn spans a square whose far
    /// corners its arc never crosses, and a tile in one of those corners is not
    /// one an inserter should be able to reach into. Samples are half a tile
    /// apart, short enough that consecutive ones cannot skip a tile at any
    /// curvature a rail can declare.
    ///
    /// The one place the "which tiles is this wagon on" question is answered,
    /// so the cursor and the stopped-stock index cannot disagree about where a
    /// wagon is.
    pub(in crate::simulation) fn for_each_rolling_stock_tile(
        &self,
        stock: &RollingStock,
        mut visit: impl FnMut((WorldTileCoord, WorldTileCoord)) -> ControlFlow<()>,
    ) {
        let Some(half) = self.rolling_stock_half_length(stock) else {
            return;
        };
        let length = half * 2;
        let back = travel(&self.rails.graph, stock.position, -half).position;
        let mut travelled = 0;
        loop {
            let sampled = travel(&self.rails.graph, back, travelled).position;
            if let Some(point) = world_point(self, sampled)
                && visit(point.tile()).is_break()
            {
                return;
            }
            if travelled >= length {
                return;
            }
            travelled = (travelled + crate::POSITION_SCALE / 2).min(length);
        }
    }

    /// Sets what a train is doing by hand, cancelling wherever it was going.
    ///
    /// Manual control wins over a plan on purpose. The routing pass writes the
    /// throttle of every train it is steering, so a drive command that left the
    /// destination in place would be overwritten within the tick and the train
    /// would appear to ignore the player.
    pub fn set_train_throttle(
        &mut self,
        train_id: TrainId,
        throttle: TrainThrottle,
    ) -> Result<(), TrainControlError> {
        let train = self
            .rolling_stock
            .trains
            .get_mut(&train_id)
            .ok_or(TrainControlError::MissingTrain(train_id))?;
        train.throttle = throttle;
        train.destination = None;
        train.route = None;
        train.route_search_exhausted_at = None;
        // The claim goes with the plan. A train being driven by hand is not a
        // train on its way to the platform it booked, and a claim it kept would
        // hold a place at that stop against every train that is actually coming.
        train.release_scheduled_stop();
        Ok(())
    }

    /// The forces a train's motion follows from, summed over its stock.
    ///
    /// Locomotives contribute tractive force only while `fuelled` lists them:
    /// a locomotive out of fuel still weighs and still brakes, it simply stops
    /// pulling. Deriving the totals every tick rather than caching them on the
    /// train is what keeps them honest when stock is coupled on or mined off.
    fn train_forces(&self, train: &Train, fuelled: &BTreeSet<RollingStockId>) -> TrainForces {
        let mut forces = TrainForces {
            max_speed: i64::MAX,
            ..TrainForces::default()
        };
        for stock_id in &train.stock {
            let Some(prototype) = self
                .rolling_stock
                .get(*stock_id)
                .and_then(|stock| self.world.prototypes.entity(stock.prototype_id))
                .and_then(|prototype| prototype.rolling_stock)
            else {
                continue;
            };
            forces.weight_kilograms += i64::from(prototype.weight_kilograms);
            forces.braking_force_newtons += i64::from(prototype.braking_force_newtons);
            forces.max_speed = forces
                .max_speed
                .min(i64::from(prototype.max_speed_fixed_per_tick) * TRAIN_VELOCITY_SCALE);
            if let Some(locomotive) = prototype.locomotive
                && fuelled.contains(stock_id)
            {
                forces.tractive_force_newtons += i64::from(locomotive.tractive_force_newtons);
            }
        }
        if forces.max_speed == i64::MAX {
            forces.max_speed = 0;
        }
        forces
    }

    /// Public reading of a train's current force totals, with every fuelled
    /// locomotive counted. This is what a station's stopping-distance question
    /// is asked against.
    pub fn train_forces_now(&self, train_id: TrainId) -> Option<TrainForces> {
        let train = self.rolling_stock.train(train_id)?;
        let fuelled = train
            .stock
            .iter()
            .filter(|stock_id| {
                self.rolling_stock
                    .get(**stock_id)
                    .and_then(|stock| stock.energy.as_ref())
                    .is_some_and(|energy| {
                        !energy.fuel_slot.is_empty()
                            || energy.energy_remaining_joules > f64::EPSILON
                    })
            })
            .copied()
            .collect();
        Some(self.train_forces(train, &fuelled))
    }

    fn rolling_stock_half_length(&self, stock: &RollingStock) -> Option<i64> {
        Some(
            i64::from(
                self.world
                    .prototypes
                    .entity(stock.prototype_id)?
                    .rolling_stock?
                    .length_fixed,
            ) / 2,
        )
    }

    /// Advances every train by one tick.
    ///
    /// Runs after [`Simulation::ensure_rail_graph`] in the tick: a train walks
    /// the graph to move, so it must be walking the world as it is now rather
    /// than as it was before the last piece of track went down.
    pub(in crate::simulation) fn advance_trains(&mut self) {
        // The tick's expansion budget opens before the schedules rather than
        // after them, because choosing between platforms of one station is
        // itself a search — the cheapest way there is what picks the platform —
        // and it is charged against the same budget every other search is.
        self.train_routing.begin_tick();
        self.advance_train_schedules();
        // Planning is one pass for the whole tick, before any train is stepped:
        // the expansion budget and the occupancy every search reads are both
        // tick-wide, and a train planned for here is a train steered on the very
        // step below rather than a tick later.
        self.plan_train_routes();
        // Reservation is likewise one pass for the whole tick, and it runs after
        // the plans because a train's plan is what says which way it is about to
        // go — and before any train is stepped, because the step is clipped to
        // the allowance this produces.
        //
        // Ahead of the early exit below rather than behind it: a railway with
        // signals on it and no trains still has aspects to show, and a signal
        // that went on showing what it showed while the last train was still
        // there would be a red lamp over empty track.
        self.advance_rail_signals();
        if self.rolling_stock.trains.is_empty() {
            return;
        }

        let train_ids = self
            .rolling_stock
            .trains
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for train_id in train_ids {
            self.advance_train(train_id);
        }
        // Last, once every train is where this tick leaves it: the transfers
        // later in the tick resolve wagons by tile out of this index, and a
        // train that arrived is a train they may load from the moment it stops.
        self.refresh_stopped_stock_index();
    }

    /// Advances station waits and assigns unclaimed destinations before route
    /// planning. The pass is in train-id order and stop ties are in stop-entity
    /// order, so limits cannot introduce hash-map-order nondeterminism.
    fn advance_train_schedules(&mut self) {
        if self.rolling_stock.trains.is_empty() {
            return;
        }
        let ids = self
            .rolling_stock
            .trains
            .keys()
            .copied()
            .collect::<Vec<_>>();
        // How many trains hold each stop, counted once for the pass and kept up
        // to date as claims are taken and given back. Counting it per candidate
        // stop per unassigned train — which is what asking the trains again would
        // be — is a scan over every train in the world squared, on every tick a
        // queue forms at a capacity-limited station.
        let mut claims = BTreeMap::<EntityId, usize>::new();
        for stop_id in self
            .rolling_stock
            .trains
            .values()
            .filter_map(|train| train.scheduled_stop)
        {
            *claims.entry(stop_id).or_default() += 1;
        }
        let tick = self.tick;
        for id in ids {
            if self
                .rolling_stock
                .train(id)
                .is_some_and(Train::is_waiting_at_scheduled_stop)
            {
                let mut context = self.train_wait_context(id);
                let train = self
                    .rolling_stock
                    .trains
                    .get_mut(&id)
                    .expect("id was collected");
                // Loading or unloading is what the inactivity clock is about, and
                // the cargo it compares against is the cargo this pass read a
                // tick ago: a transfer either side of it changes what is aboard,
                // whichever machine made it.
                if train.schedule_activity_cargo.as_ref() != Some(&context.cargo) {
                    train.schedule_activity_cargo = Some(context.cargo.clone());
                    train.schedule_last_activity_tick = Some(tick);
                }
                context.waited_ticks =
                    tick.saturating_sub(train.schedule_arrival_tick.unwrap_or(tick));
                context.inactive_ticks =
                    tick.saturating_sub(train.schedule_last_activity_tick.unwrap_or(tick));
                if train
                    .schedule
                    .current_entry()
                    .is_some_and(|entry| entry.may_depart(&context))
                {
                    train.schedule.advance();
                    if let Some(stop_id) = train.scheduled_stop
                        && let Some(held) = claims.get_mut(&stop_id)
                    {
                        // Given back within the pass, so a train queueing for
                        // this stop takes the free place on the same tick the
                        // train ahead of it leaves rather than a tick later.
                        *held = held.saturating_sub(1);
                    }
                    train.release_scheduled_stop();
                }
            }

            self.release_stranded_claim(id, &mut claims);
            self.claim_scheduled_stop(id, &mut claims);
        }
    }

    /// Gives back the claim of a train that is booked into a stop it is no
    /// longer on its way to.
    ///
    /// A claim is taken with a destination and given up on arrival, so a train
    /// holding one while having neither is a train whose plan went out from
    /// under it — its stop's rail was mined, or the route to it was invalidated
    /// and the destination went with the track. Releasing here is what lets it
    /// book the station again, or another platform of it, on this very tick;
    /// left alone it would hold a place at a stop it will never reach against
    /// every train that could.
    fn release_stranded_claim(
        &mut self,
        train_id: TrainId,
        claims: &mut BTreeMap<EntityId, usize>,
    ) {
        let Some(train) = self.rolling_stock.train(train_id) else {
            return;
        };
        let Some(stop_id) = train.scheduled_stop else {
            return;
        };
        let stranded = train.schedule_arrival_tick.is_none()
            && train.destination.is_none()
            && train.route.is_none();
        if !stranded {
            return;
        }
        if let Some(held) = claims.get_mut(&stop_id) {
            *held = held.saturating_sub(1);
        }
        if let Some(train) = self.rolling_stock.trains.get_mut(&train_id) {
            train.release_scheduled_stop();
        }
    }

    /// Books an idle scheduled train into a stop serving the entry it is on, and
    /// sends it there.
    ///
    /// Several stops may answer to one name — that is how a station gets a
    /// second platform — and the one chosen is the one this train can *get to*
    /// most cheaply, not the first in id order. The two differ exactly where it
    /// matters: a platform on the far side of the yard is nearer in a straight
    /// line and much further to drive to, and a train sent there queues behind
    /// the junction it has to cross twice.
    ///
    /// One search decides it, for the same reason the routing pass runs one per
    /// train: the search that ranks the platforms is the search that produces
    /// the route to the winner, so the plan comes back with the choice rather
    /// than being looked up again afterwards. When the tick cannot pay for a
    /// search, or the search runs out of expansions, the lowest stop id is taken
    /// instead and the ordinary routing pass plans for it — a deterministic
    /// fallback that costs nothing, rather than a train standing still because
    /// the budget was busy.
    fn claim_scheduled_stop(&mut self, train_id: TrainId, claims: &mut BTreeMap<EntityId, usize>) {
        let Some(name) = self.rolling_stock.train(train_id).and_then(|train| {
            (train.scheduled_stop.is_none() && train.destination.is_none())
                .then(|| {
                    train
                        .schedule
                        .current_entry()
                        .map(|entry| entry.stop_name.clone())
                })
                .flatten()
        }) else {
            return;
        };

        let candidates = self
            .entities
            .train_stops
            .iter()
            .filter(|(entity_id, state)| {
                state.name == name
                    && claims.get(*entity_id).copied().unwrap_or(0)
                        < self.train_stop_effective_limit(**entity_id) as usize
            })
            .filter_map(|(entity_id, _)| Some((*entity_id, self.train_stop_target(*entity_id)?)))
            .collect::<Vec<_>>();
        let Some((&(first_stop, first_target), rest)) = candidates.split_first() else {
            return;
        };

        let (stop_id, target, route) = match rest.is_empty() {
            true => (first_stop, first_target, None),
            false => self.nearest_train_stop(train_id, &candidates).unwrap_or((
                first_stop,
                first_target,
                None,
            )),
        };
        let Some(train) = self.rolling_stock.trains.get_mut(&train_id) else {
            return;
        };
        train.scheduled_stop = Some(stop_id);
        *claims.entry(stop_id).or_default() += 1;
        train.destination = Some(target);
        train.route = route;
        train.route_search_exhausted_at = None;
    }

    /// The candidate a train can reach most cheaply, with the route that gets it
    /// there, or `None` when no search could be made or none of them is
    /// reachable.
    ///
    /// Ties go to the lowest stop id, which is the order the candidates are
    /// gathered in and the order the search itself breaks ties by.
    fn nearest_train_stop(
        &mut self,
        train_id: TrainId,
        candidates: &[(EntityId, RailTarget)],
    ) -> Option<(
        EntityId,
        RailTarget,
        Option<crate::rolling_stock::TrainRoute>,
    )> {
        let start = self.train_search_position(train_id)?;
        if !self.train_routing.can_search() {
            return None;
        }
        self.ensure_train_occupancy();
        let Simulation {
            train_routing,
            rails,
            ..
        } = self;
        train_routing.collect_exempt_rails(train_id);
        let targets = candidates
            .iter()
            .map(|(_, target)| *target)
            .collect::<Vec<_>>();
        match train_routing.plan(&rails.graph, start, &targets)? {
            RailRouteOutcome::Found { target, route } => {
                let (stop_id, stop_target) = candidates[target];
                Some((stop_id, stop_target, Some(route)))
            }
            // Nowhere to go and no answer are the same answer here: the claim
            // falls back to the lowest stop id, and the routing pass — which
            // owns the "this destination is unreachable" rule and the schedule
            // step past it — settles what happens next.
            RailRouteOutcome::Unreachable | RailRouteOutcome::Exhausted => None,
        }
    }

    /// What a waiting train's conditions are asked about: its cargo, and whether
    /// that cargo fills or fails to fill the containers it declares.
    ///
    /// The two tick counts are left at zero here and filled in by the caller,
    /// which is the half of the answer that lives on the train rather than in its
    /// wagons.
    fn train_wait_context(&self, id: TrainId) -> TrainWaitContext {
        let Some(train) = self.rolling_stock.train(id) else {
            return TrainWaitContext::default();
        };
        let (cargo, declares_cargo, every_container_full) = self.train_cargo_snapshot(id);
        let mut context = TrainWaitContext {
            cargo_empty: cargo.is_empty(),
            cargo_full: declares_cargo && every_container_full,
            cargo,
            ..TrainWaitContext::default()
        };
        // The stop is where the wires are, so the network a condition compares
        // against is the one reaching the platform this train is standing at.
        // Read every tick rather than when the train arrives: what the factory
        // is saying is exactly the thing that changes while it waits.
        if let Some(stop_id) = train.scheduled_stop {
            self.circuits.merged_at(
                CircuitNode::new(stop_id, ConnectorPort::Single),
                &mut context.circuit_signals,
            );
        }
        context
    }

    /// What a train is carrying, whether it can carry anything at all, and
    /// whether every container it declares is full to capacity.
    ///
    /// The three come out of one walk over the train's stock because they are
    /// three readings of the same thing: the wait conditions ask all three, and
    /// a stop's connector publishes the first.
    fn train_cargo_snapshot(&self, id: TrainId) -> (TrainCargo, bool, bool) {
        let mut cargo = TrainCargo::default();
        let Some(train) = self.rolling_stock.train(id) else {
            return (cargo, false, false);
        };
        // A train with nowhere to put cargo is never full: "full" is a statement
        // about the containers a train declares, and a locomotive on its own
        // declares none. Tanks count as much as wagons — a fluid train that could
        // never satisfy `CargoFull` would sit at its loading station for ever.
        let mut declares_cargo = false;
        let mut every_container_full = true;
        for stock_id in &train.stock {
            let Some(stock) = self.rolling_stock.get(*stock_id) else {
                continue;
            };
            if let Some(inventory) = &stock.inventory {
                declares_cargo = true;
                for slot in inventory.slots() {
                    // Full means nothing more would go in, not merely occupied: a
                    // wagon holding one plate in each of forty slots is all but
                    // empty, and departing it as "full" would send a train away
                    // with a fortieth of a load.
                    let Some(stack) = slot.stack() else {
                        every_container_full = false;
                        continue;
                    };
                    every_container_full &= slot
                        .insert_capacity(&self.world.prototypes, stack.item_id())
                        .unwrap_or(0)
                        == 0;
                    cargo.add_item(stack.item_id(), i32::from(stack.count()));
                }
            }
            // Capacity comes from the prototype, which is where every other fluid
            // box in the world reads it from; the two lists agree in length by
            // validation, so a box without a declaration is a catalog the world
            // no longer matches rather than an empty tank.
            let declared_boxes = self
                .world
                .prototypes
                .entity(stock.prototype_id)
                .map(|prototype| prototype.fluid_boxes.as_slice())
                .unwrap_or_default();
            for (fluid_box, declared) in stock.fluid_boxes.iter().zip(declared_boxes) {
                declares_cargo = true;
                every_container_full &= fluid_box.amount_milliunits >= declared.capacity_milliunits;
                if let Some(fluid) = fluid_box.fluid_id {
                    cargo.add_fluid(
                        fluid,
                        i32::try_from(fluid_box.amount_milliunits).unwrap_or(i32::MAX),
                    );
                }
            }
        }
        (cargo, declares_cargo, every_container_full)
    }

    /// What one train is carrying, summed over its wagons and its tanks.
    ///
    /// What a stop's connector publishes, and the same figure the cargo wait
    /// conditions are asked about — one walk over the train's stock, so the two
    /// cannot disagree about what is aboard.
    pub(in crate::simulation) fn train_cargo(&self, train_id: TrainId) -> TrainCargo {
        self.train_cargo_snapshot(train_id).0
    }

    fn advance_train(&mut self, train_id: TrainId) {
        // Plan and throttle first: the step below reads the throttle, so a train
        // being steered has to be steered before it is stepped rather than a
        // tick behind.
        self.steer_train(train_id);
        let Some(train) = self.rolling_stock.trains.get(&train_id) else {
            return;
        };
        let throttle = train.throttle;
        let stock_ids = train.stock.clone();

        // Fuel is spent before the forces are summed, so a locomotive that
        // could not pay for this tick does not also get to pull through it.
        let fuelled = self.burn_train_fuel(&stock_ids, throttle);
        let train = self
            .rolling_stock
            .trains
            .get(&train_id)
            .expect("the train was just read");
        let forces = self.train_forces(train, &fuelled);
        let velocity = stepped_velocity(train.velocity, throttle, forces);
        let owed = velocity + train.travel_remainder;
        let travel_fixed = owed.div_euclid(TRAIN_VELOCITY_SCALE);
        let remainder = owed - travel_fixed * TRAIN_VELOCITY_SCALE;

        let step = self.clipped_train_travel(train_id, &stock_ids, travel_fixed);
        let travelled = step.travelled_fixed;
        let blocked = step.limit.is_some();
        if travelled != 0 {
            // The train has stirred, so whatever tiles it was indexed under are
            // now a description of where it used to be. Dropped before the
            // positions are written rather than after, so no window exists in
            // which the index and the stock disagree — and dropped on *any*
            // movement, including a train that starts and stops inside one tick,
            // which is the case a "is it stationary now?" check would miss.
            self.forget_stopped_train(train_id);
            for stock_id in &stock_ids {
                let Some(stock) = self.rolling_stock.stock.get(stock_id) else {
                    continue;
                };
                let moved = travel(&self.rails.graph, stock.position, travelled).position;
                self.rolling_stock
                    .stock
                    .get_mut(stock_id)
                    .expect("the stock was just read")
                    .position = moved;
            }
        }

        let train = self
            .rolling_stock
            .trains
            .get_mut(&train_id)
            .expect("the train was just read");
        train.velocity = if blocked { 0 } else { velocity };
        // A stopped train owes nothing. Keeping a remainder past a standstill
        // would leave sub-unit travel that no later tick can ever spend —
        // velocity is zero, so nothing is added to it and nothing is taken out
        // — and `is_stationary` would stay false forever, which is exactly the
        // question a station or a stop-and-wait will ask.
        train.travel_remainder = if blocked || train.velocity == 0 {
            0
        } else {
            remainder
        };

        // Last, because what a leg has left to run is the distance the step just
        // covered subtracted from it, and because whether the leg is finished
        // depends on the train having already come to a stand.
        self.advance_train_route(train_id, step);
    }

    /// The distance the whole train may travel this tick, and what cut it short
    /// if anything did.
    ///
    /// A train is rigid, so the answer is the shortest distance any of its
    /// pieces could make: letting the blocked piece stop while the rest carried
    /// on would stretch the couplings.
    ///
    /// Four things can cut the step short: the route the train is following, the
    /// end of the line, the last signal it was let past, and other stock on the
    /// track. All four are clipped here rather than only the physical ones, so a
    /// train stops *on* its mark instead of rolling past it and having to be
    /// dragged back — and so two trains sharing a run cannot drive through one
    /// another, which is the very overlap placement refuses to create.
    ///
    /// Which one is reported when several bind at once follows the order they
    /// are checked in, and that order is the useful one: a train that arrives
    /// exactly where the track runs out has arrived, and a train stopped at a
    /// red signal is waiting for the signal rather than for the stock beyond it
    /// — which is what made the signal red in the first place.
    fn clipped_train_travel(
        &self,
        train_id: TrainId,
        stock_ids: &[RollingStockId],
        travel_fixed: i64,
    ) -> TrainStep {
        if travel_fixed == 0 {
            return TrainStep {
                attempted_fixed: 0,
                travelled_fixed: 0,
                limit: None,
            };
        }
        let limits = [
            (
                self.route_clearance_fixed(train_id, travel_fixed),
                TrainStepLimit::Route,
            ),
            (
                Some(self.track_clearance_fixed(stock_ids, travel_fixed)),
                TrainStepLimit::Track,
            ),
            (
                self.signal_clearance_fixed(train_id, travel_fixed),
                TrainStepLimit::Signal,
            ),
            (
                Some(self.train_clearance_fixed(train_id, stock_ids, travel_fixed)),
                TrainStepLimit::Stock,
            ),
        ];

        let mut allowed = travel_fixed;
        let mut limit = None;
        for (candidate, reason) in limits {
            let Some(candidate) = candidate else {
                continue;
            };
            if candidate.abs() < allowed.abs() {
                allowed = candidate;
                limit = Some(reason);
            }
        }
        TrainStep {
            attempted_fixed: travel_fixed,
            travelled_fixed: allowed,
            limit,
        }
    }

    /// How far the train may travel before one of its pieces runs off the end of
    /// the line.
    ///
    /// The measurement is taken from each piece's *leading end* rather than its
    /// centre, so a train comes to rest with its nose at the buffer instead of
    /// hanging half a locomotive past the last rail. Which end leads follows
    /// from the sign of the step, which is why the trailing end never needs
    /// checking: it cannot run out of track before the end in front of it.
    fn track_clearance_fixed(&self, stock_ids: &[RollingStockId], travel_fixed: i64) -> i64 {
        let mut allowed = travel_fixed;
        for stock_id in stock_ids {
            let Some(stock) = self.rolling_stock.get(*stock_id) else {
                continue;
            };
            let half = self.rolling_stock_half_length(stock).unwrap_or(0);
            let lead = if travel_fixed > 0 { half } else { -half };
            let leading_end = travel(&self.rails.graph, stock.position, lead);
            let outcome = travel(&self.rails.graph, leading_end.position, travel_fixed);
            // A leading end that is already off the end of the line has nothing
            // left to spend, which is what stops a train that was cut short by a
            // destroyed rail from creeping forward afterwards.
            let reachable = travel_fixed - outcome.blocked_fixed - leading_end.blocked_fixed;
            if reachable.abs() < allowed.abs() || reachable.signum() != travel_fixed.signum() {
                allowed = reachable.clamp(travel_fixed.min(0), travel_fixed.max(0));
            }
        }
        allowed
    }

    /// How far the last signal the train was let past lets it travel this tick,
    /// or `None` when no signal limits the step.
    ///
    /// Looked up by the direction the step is actually taking. The signalling pass
    /// walks every direction a train may travel this tick — two of them while a
    /// reversal is being commanded — so a step in either direction finds the
    /// allowance measured for it, and an absent one really means "nothing ahead
    /// this way" rather than "measured for the other way".
    fn signal_clearance_fixed(&self, train_id: TrainId, travel_fixed: i64) -> Option<i64> {
        let forward = travel_fixed > 0;
        let allowance_fixed = self.rails.signalling.limit(train_id, forward)?;
        Some(if forward {
            allowance_fixed
        } else {
            -allowance_fixed
        })
    }

    /// The distance a signal leaves a train to run toward the leg it is driving,
    /// or `None` when no signal stands in the way of it.
    ///
    /// What the steering pass brakes against, so a train comes to rest at a red
    /// signal instead of being clipped to a standstill against it. Only an
    /// allowance in the leg's own direction counts: a signal behind a reversing
    /// train is not something it is running at.
    pub(in crate::simulation) fn signal_allowance_fixed(
        &self,
        train_id: TrainId,
        forward: bool,
    ) -> Option<i64> {
        self.rails.signalling.limit(train_id, forward)
    }

    /// Burns one tick of fuel in every locomotive of the train that is being
    /// asked to pull, and reports which of them managed it.
    ///
    /// A coasting or braking train burns nothing: the throttle is what spends
    /// fuel, so a train parked with coal aboard keeps it.
    fn burn_train_fuel(
        &mut self,
        stock_ids: &[RollingStockId],
        throttle: TrainThrottle,
    ) -> BTreeSet<RollingStockId> {
        let mut fuelled = BTreeSet::new();
        if throttle.drive_sign() == 0 {
            return fuelled;
        }
        let Simulation {
            rolling_stock,
            world,
            ..
        } = self;
        let mut burnt = Vec::new();
        for stock_id in stock_ids {
            let Some(stock) = rolling_stock.stock.get_mut(stock_id) else {
                continue;
            };
            let Some(energy) = stock.energy.as_mut() else {
                continue;
            };
            let joules_per_tick = energy.energy_usage_watts / FIXED_SIM_TICKS_PER_SECOND_F64;
            if energy.energy_remaining_joules + f64::EPSILON < joules_per_tick {
                burnt.extend(machine_ops::try_consume_fuel(&world.prototypes, energy));
            }
            if energy.energy_remaining_joules + f64::EPSILON < joules_per_tick {
                continue;
            }
            energy.energy_remaining_joules -= joules_per_tick;
            fuelled.insert(*stock_id);
        }
        // Coal a locomotive burns is coal the factory produced, so it leaves the
        // statistics the same way a furnace's does. Recorded after the loop
        // because the borrow above holds the store the recorder also reads.
        for item_id in burnt {
            self.record_item_consumed(item_id, 1);
        }
        fuelled
    }
}

/// The channels one wait condition names, so a schedule can be checked against
/// the catalog before it is taken.
fn wait_condition_signals(condition: &TrainWaitCondition) -> Vec<SignalId> {
    let TrainWaitCondition::Circuit(condition) = condition else {
        return Vec::new();
    };
    let mut signals = vec![condition.left];
    if let crate::circuits::SignalOperand::Signal(signal) = condition.right {
        signals.push(signal);
    }
    signals
}

/// The along-track distance one piece of stock keeps from the next when the two
/// are coupled: half of each body plus the coupler gap.
pub(in crate::simulation) fn coupled_spacing_fixed(first_length: i64, second_length: i64) -> i64 {
    (first_length + second_length) / 2 + crate::rolling_stock::TRAIN_COUPLING_GAP_FIXED
}

/// Both ends of a piece of stock as positions on the track.
pub(in crate::simulation) fn stock_ends(
    graph: &crate::simulation::rail_ops::RailGraph,
    position: RailPosition,
    length_fixed: i64,
) -> (TravelOutcome, TravelOutcome) {
    let half = length_fixed / 2;
    (
        travel(graph, position, -half),
        travel(graph, position, half),
    )
}
