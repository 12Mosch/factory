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
//!   per *train* to find what is on the track ahead of it. Nothing here scans
//!   the world per piece, and the searches that are proportional to the track
//!   around a click — finding what a new piece couples to — run on placement
//!   rather than every tick.
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
mod traversal;

pub use motion::braking_distance_fixed;
pub(in crate::simulation) use traversal::{TravelOutcome, travel, world_point};

use crate::rail::RailPoint;
use crate::rolling_stock::{
    RailPosition, RollingStock, RollingStockId, TRAIN_VELOCITY_SCALE, Train, TrainControlError,
    TrainForces, TrainId, TrainThrottle,
};
use crate::simulation::*;
use std::collections::BTreeSet;

use self::motion::stepped_velocity;

impl Simulation {
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

    /// Sets what a train is doing. This is the drive command: with no
    /// pathfinding, signals, or schedules yet, it is the only thing that makes
    /// a train move, and it goes through the ordinary command queue so it lands
    /// on a tick boundary like every other input.
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

    fn advance_train(&mut self, train_id: TrainId) {
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

        let (travelled, blocked) = self.clipped_train_travel(train_id, &stock_ids, travel_fixed);
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
    }

    /// The distance the whole train may travel this tick, and whether that is
    /// less than it asked for.
    ///
    /// A train is rigid, so the answer is the shortest distance any of its
    /// pieces could make: letting the blocked piece stop while the rest carried
    /// on would stretch the couplings.
    ///
    /// Two things can cut the step short: the end of the line, and other stock
    /// on it. Both are clipped here rather than only the first, or two trains
    /// sharing a run would drive through one another — the very overlap
    /// placement refuses to create.
    ///
    /// The track measurement is taken from each piece's *leading end* rather
    /// than its centre, so a train comes to rest with its nose at the buffer
    /// instead of hanging half a locomotive past the last rail. Which end leads
    /// follows from the sign of the step, which is why the trailing end never
    /// needs checking: it cannot run out of track before the end in front of it.
    fn clipped_train_travel(
        &self,
        train_id: TrainId,
        stock_ids: &[RollingStockId],
        travel_fixed: i64,
    ) -> (i64, bool) {
        if travel_fixed == 0 {
            return (0, false);
        }
        let mut allowed = self.train_clearance_fixed(train_id, stock_ids, travel_fixed);
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
        (allowed, allowed != travel_fixed)
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
