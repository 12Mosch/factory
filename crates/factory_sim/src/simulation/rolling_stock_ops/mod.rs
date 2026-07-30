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

mod motion;
mod placement;
mod routing;
mod traversal;

pub use motion::braking_distance_fixed;
pub(in crate::simulation) use routing::{TrainRouting, push_stock_rails};
pub(in crate::simulation) use traversal::{TravelOutcome, travel, world_point};

use crate::rail::RailPoint;
use crate::rolling_stock::{
    RailPosition, RollingStock, RollingStockId, TRAIN_VELOCITY_SCALE, Train, TrainControlError,
    TrainForces, TrainId, TrainThrottle,
};

use crate::rolling_stock::{RailTarget, TrainSchedule, TrainStop, TrainStopId};
use crate::simulation::*;
use std::collections::BTreeSet;

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
    /// Registers a named stopping mark. Stop ids are monotonic, making equal
    /// name/distance choices deterministic across saves and replays.
    pub fn create_train_stop(
        &mut self,
        name: impl Into<String>,
        rail: EntityId,
        distance_fixed: i64,
        train_limit: u32,
    ) -> Result<TrainStopId, TrainControlError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(TrainControlError::EmptyStopName);
        }
        if train_limit == 0 {
            return Err(TrainControlError::InvalidTrainLimit);
        }
        let geometry = self
            .rail_piece_geometry(rail)
            .ok_or(TrainControlError::NotRail(rail))?;
        if !(0..=geometry.length_fixed).contains(&distance_fixed) {
            return Err(TrainControlError::NotRail(rail));
        }
        self.rolling_stock.next_stop_id += 1;
        let id = TrainStopId::new(self.rolling_stock.next_stop_id);
        self.rolling_stock.stops.insert(
            id,
            TrainStop {
                id,
                name,
                target: RailTarget::new(rail, distance_fixed),
                train_limit,
            },
        );
        Ok(id)
    }

    pub fn train_stops(&self) -> impl Iterator<Item = &TrainStop> {
        self.rolling_stock.stops.values()
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
        id: TrainStopId,
        name: impl Into<String>,
    ) -> Result<(), TrainControlError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(TrainControlError::EmptyStopName);
        }
        let stop = self
            .rolling_stock
            .stops
            .get_mut(&id)
            .ok_or(TrainControlError::MissingStop(id))?;
        let old = std::mem::replace(&mut stop.name, name.clone());
        if old == name || self.stop_name_exists(&old) {
            return Ok(());
        }
        for train in self.rolling_stock.trains.values_mut() {
            for entry in &mut train.schedule.entries {
                if entry.stop_name == old {
                    entry.stop_name.clone_from(&name);
                }
            }
        }
        Ok(())
    }

    pub fn remove_train_stop(&mut self, id: TrainStopId) -> Result<TrainStop, TrainControlError> {
        let stop = self
            .rolling_stock
            .stops
            .remove(&id)
            .ok_or(TrainControlError::MissingStop(id))?;
        self.forget_train_stop(&stop);
        Ok(stop)
    }

    /// Whether any stop still answers to `name`.
    fn stop_name_exists(&self, name: &str) -> bool {
        self.rolling_stock
            .stops
            .values()
            .any(|stop| stop.name == name)
    }

    /// Releases what a stop that has just gone leaves behind on the trains: the
    /// claim any of them held on it, and — once no stop answers to its name at
    /// all — the schedule entries which can no longer be served.
    ///
    /// The cursor is what would otherwise strand a train. A train that had
    /// claimed the removed stop has no claim and no destination, and its current
    /// entry names a station that no longer exists anywhere, so neither the
    /// arrival check nor the assignment beneath it can fire again: the train
    /// idles on that entry for ever with no escape. Stepping past the entry is
    /// the escape, and it is only taken when the name has left the world — while
    /// another stop still bears it, the train simply goes there instead.
    fn forget_train_stop(&mut self, stop: &TrainStop) {
        let name_remains = self.stop_name_exists(&stop.name);
        for train in self.rolling_stock.trains.values_mut() {
            if train.scheduled_stop == Some(stop.id) {
                train.release_scheduled_stop();
                train.destination = None;
                train.route = None;
                train.route_search_exhausted_at = None;
                train.throttle = TrainThrottle::Brake;
            }
            if !name_remains
                && train
                    .schedule
                    .current_entry()
                    .is_some_and(|entry| entry.stop_name == stop.name)
            {
                train.schedule.advance();
            }
        }
    }

    /// Drops the stops whose rail is no longer there.
    ///
    /// The mirror of [`Simulation::prune_rolling_stock`], at the same moment and
    /// for the same reason: a stop names a rail, and mining that rail would
    /// otherwise leave the stop naming nothing — a state `validate_rolling_stock`
    /// rejects, so an ordinary bit of track-pulling would make the world
    /// unsaveable.
    pub(in crate::simulation) fn prune_train_stops(&mut self) {
        if self.rolling_stock.stops.is_empty() {
            return;
        }
        let stranded = self
            .rolling_stock
            .stops
            .values()
            .filter(|stop| self.rail_piece_geometry(stop.target.edge).is_none())
            .map(|stop| stop.id)
            .collect::<Vec<_>>();
        for id in stranded {
            let Some(stop) = self.rolling_stock.stops.remove(&id) else {
                continue;
            };
            self.forget_train_stop(&stop);
        }
    }

    /// Replaces a train's automatic orders, cancelling whatever it was doing to
    /// serve the old ones.
    ///
    /// Entries are checked before anything is written: an empty station name is
    /// refused where the stop APIs refuse it rather than accepted here and
    /// reported much later as a broken world by validation.
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
    /// Walked along the body rather than tested against the rectangle its two
    /// ends span: a piece taking a quarter turn spans a square whose far
    /// corners its arc never crosses, and a cursor in one of those corners
    /// would otherwise pick up — and drive, or mine — a wagon that is visibly
    /// somewhere else. Samples are half a tile apart, short enough that
    /// consecutive ones cannot skip a tile at any curvature a rail can declare,
    /// and this lives here rather than in the caller because the track geometry
    /// the answer follows from does.
    ///
    /// A cursor query, never a per-tick one.
    pub fn rolling_stock_covers_tile(&self, id: RollingStockId, x: i64, y: i64) -> bool {
        let Some(stock) = self.rolling_stock.get(id) else {
            return false;
        };
        let Some(half) = self.rolling_stock_half_length(stock) else {
            return false;
        };
        let length = half * 2;
        // Cheap reject first. Every point of the body is within the piece's own
        // length of its centre along the track, so it is within that distance
        // in a straight line too — a tile further out cannot be covered, and
        // this is what keeps a held right-click from walking every wagon in the
        // world. Deliberately a distance from the centre rather than the box
        // the two ends span: a body across an S-bend leaves that box, and a
        // pre-filter that is merely usually right would hide stock instead of
        // over-reporting it.
        let Some(center) = self.rolling_stock_world_point(id) else {
            return false;
        };
        let margin = length.div_euclid(crate::POSITION_SCALE) + 1;
        let (center_x, center_y) = center.tile();
        if (x - center_x).abs() > margin || (y - center_y).abs() > margin {
            return false;
        }

        let back = travel(&self.rails.graph, stock.position, -half).position;
        let mut travelled = 0;
        loop {
            let sampled = travel(&self.rails.graph, back, travelled).position;
            if world_point(self, sampled).is_some_and(|point| point.tile() == (x, y)) {
                return true;
            }
            if travelled >= length {
                return false;
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
        self.advance_train_schedules();
        // Planning is one pass for the whole tick, before any train is stepped:
        // the expansion budget and the occupancy every search reads are both
        // tick-wide, and a train planned for here is a train steered on the very
        // step below rather than a tick later.
        self.train_routing.begin_tick();
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
    }

    /// Advances station waits and assigns unclaimed destinations before route
    /// planning. The pass is in train-id order and stop ties are in stop-id
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
        let mut claims = BTreeMap::<TrainStopId, usize>::new();
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

            let Some(name) = self.rolling_stock.train(id).and_then(|train| {
                (train.scheduled_stop.is_none() && train.destination.is_none())
                    .then(|| {
                        train
                            .schedule
                            .current_entry()
                            .map(|entry| entry.stop_name.clone())
                    })
                    .flatten()
            }) else {
                continue;
            };
            let chosen = self
                .rolling_stock
                .stops
                .values()
                .find(|stop| {
                    stop.name == name
                        && claims.get(&stop.id).copied().unwrap_or(0) < stop.train_limit as usize
                })
                .map(|stop| (stop.id, stop.target));
            if let Some((stop_id, target)) = chosen
                && let Some(train) = self.rolling_stock.trains.get_mut(&id)
            {
                train.scheduled_stop = Some(stop_id);
                *claims.entry(stop_id).or_default() += 1;
                train.destination = Some(target);
                train.route = None;
                train.route_search_exhausted_at = None;
            }
        }
    }

    /// What a waiting train's conditions are asked about: its cargo, and whether
    /// that cargo fills or fails to fill the containers it declares.
    ///
    /// The two tick counts are left at zero here and filled in by the caller,
    /// which is the half of the answer that lives on the train rather than in its
    /// wagons.
    fn train_wait_context(&self, id: TrainId) -> crate::rolling_stock::TrainWaitContext {
        let mut context = crate::rolling_stock::TrainWaitContext::default();
        let Some(train) = self.rolling_stock.train(id) else {
            return context;
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
                    context
                        .cargo
                        .add_item(stack.item_id(), i32::from(stack.count()));
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
                    context.cargo.add_fluid(
                        fluid,
                        i32::try_from(fluid_box.amount_milliunits).unwrap_or(i32::MAX),
                    );
                }
            }
        }
        context.cargo_empty = context.cargo.is_empty();
        context.cargo_full = declares_cargo && every_container_full;
        context
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
