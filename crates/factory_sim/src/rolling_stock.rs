//! Rolling stock: locomotives and wagons coupled into trains, running along the
//! rail graph.
//!
//! Three decisions shape everything here, and each of them is a decision about
//! where a train *is* rather than about how fast it goes.
//!
//! * **Off the tile grid.** A train sits between tiles, so stock is not in
//!   `EntityStore`, `OccupancyGrid`, or `DenseEntityMap` — all three are
//!   tile-locked. [`RollingStockSubsystem`] holds it instead, keyed by a
//!   monotonic id in a `BTreeMap`, so the per-tick pass visits stock in
//!   creation order on every machine and every replay. This is the shape
//!   [`crate::robots::RobotFlightSubsystem`] already uses for robots and the
//!   enemy subsystem uses for units.
//! * **On the track, not on the map.** A position is a rail edge and a distance
//!   along it ([`RailPosition`]) rather than a free `(x, y)`. A train cannot
//!   leave the track, so a free position would be a wider claim than the truth,
//!   and edge-relative distance is what makes "stopped exactly at the end of
//!   this rail" an exact statement instead of an approximate one — the property
//!   block occupancy and station stopping will both need. World position is
//!   derived from the rail's geometry on demand, for rendering and reach
//!   checks.
//! * **Integer motion.** Velocity is an integer in units of
//!   [`TRAIN_VELOCITY_SCALE`] per fixed-point unit per tick, and the sub-unit
//!   part of a tick's travel is carried in a remainder rather than rounded away
//!   — the accumulator rule productivity already follows. Nothing in the motion
//!   model is a float, so two machines running the same train land it on the
//!   same fixed-point unit.
//!
//! Coupling is what makes a *train* rather than a pile of wagons: the pieces
//! share one velocity and keep their spacing because they all advance by the
//! same distance along the same track. Nothing here picks a route — an end that
//! joins no other end stops the train — because pathfinding, signals, and
//! schedules are separate work.

use crate::ids::EntityId;
use crate::inventory::Inventory;
use crate::machines::BurnerEnergy;
use factory_data::EntityPrototypeId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// Velocity is stored as an integer in millionths of a fixed-point position
/// unit per tick.
///
/// A tick of acceleration is a small fraction of one position unit — a
/// locomotive gains about 1.5 units per tick per tick — so storing velocity at
/// position resolution would truncate acceleration to zero and a train would
/// never move. The scale is what gives the integer model enough room to hold
/// the derivative of a position.
pub const TRAIN_VELOCITY_SCALE: i64 = 1_000_000;

/// Resistance every tonne of a train drags against, in newtons.
///
/// Proportional to mass on purpose: the deceleration it produces is then the
/// same for a light train and a heavy one, so coasting behaves the way a player
/// expects rather than turning train length into a second, hidden speed stat.
pub const ROLLING_RESISTANCE_NEWTONS_PER_TONNE: i64 = 500;

/// What a train pays, in fixed-point units of track, for turning around once on
/// a route.
///
/// A reversal is not free the way a bend is: the train has to brake to a
/// standstill and accelerate again, which costs far more time than the distance
/// it saves usually earns back. A hundred tiles is the exchange rate — a loop
/// under a hundred tiles longer than a there-and-back is preferred to reversing
/// — and it is stated as a distance rather than as ticks because everything else
/// the search adds up is a distance.
pub const TRAIN_REVERSAL_PENALTY_FIXED: i64 = 100 * crate::POSITION_SCALE;

/// What a route pays for each rail it plans to run over that something else is
/// standing on, in fixed-point units of track.
///
/// A penalty rather than a prohibition: track someone is parked on is track a
/// train can still be routed over once whatever is there has moved, and a route
/// that refused it outright would strand a train that has no other way round.
/// Occupancy is read from the rolling stock in the world today; signals will
/// replace that source with reserved blocks without changing what a route does
/// with the answer.
pub const TRAIN_OCCUPIED_RAIL_PENALTY_FIXED: i64 = 25 * crate::POSITION_SCALE;

/// Gap left between coupled stock, in fixed-point units.
///
/// Couplers are not points that touch: leaving a small gap is what stops two
/// pieces from being drawn inside each other, and it is the distance placement
/// looks across when deciding whether a new piece joins the train in front of
/// it.
pub const TRAIN_COUPLING_GAP_FIXED: i64 = 256;

/// Identity of one piece of rolling stock, monotonic per world.
///
/// Separate from [`EntityId`] because stock is not a placed entity: nothing
/// occupies a tile, nothing is in the occupancy grid, and the id space must not
/// be confused with the one entity states are keyed by.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct RollingStockId(u64);

impl RollingStockId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Identity of one train: an ordered run of coupled stock sharing a velocity.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct TrainId(u64);

impl TrainId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Where a piece of rolling stock is: a rail edge and how far along it.
///
/// `distance_fixed` is measured from the piece's `start` end — the first of the
/// two [`crate::rail::RailPieceGeometry::ends`] — and is always within
/// `0..=length`. `forward` is which way the piece faces: `true` when its front
/// points toward the `end` end, so a train moving forwards raises
/// `distance_fixed`.
///
/// The edge is named by the rail's [`EntityId`], not by an index into the rail
/// graph. The graph is a derived cache rebuilt whenever track changes, so an
/// index would be meaningless one placement later and unusable across a save;
/// the rail piece itself is an ordinary placed entity and outlives both.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct RailPosition {
    pub edge: EntityId,
    pub distance_fixed: i64,
    pub forward: bool,
}

impl RailPosition {
    pub const fn new(edge: EntityId, distance_fixed: i64, forward: bool) -> Self {
        Self {
            edge,
            distance_fixed,
            forward,
        }
    }

    /// The same point on the same edge, facing the other way. Reversing a train
    /// turns every piece around without moving any of them.
    pub const fn reversed(self) -> Self {
        Self {
            forward: !self.forward,
            ..self
        }
    }
}

/// A point on the track a train can be sent to.
///
/// Deliberately not a [`RailPosition`]: a destination is a place to stop, not a
/// facing. Which way round a train ends up is decided by the route that reaches
/// it, and a target that carried a `forward` flag would be quietly asking for
/// something the search does not promise.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct RailTarget {
    pub edge: EntityId,
    pub distance_fixed: i64,
}

impl RailTarget {
    pub const fn new(edge: EntityId, distance_fixed: i64) -> Self {
        Self {
            edge,
            distance_fixed,
        }
    }
}

/// One run of a planned route: a distance to cover without changing direction.
///
/// `forward` is which way the *train* drives, not which way the track runs, so
/// a leg maps straight onto a throttle. Two consecutive legs always disagree
/// about it — a leg boundary is a reversal — and the last one ends at the
/// destination.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TrainRouteLeg {
    /// Distance still to run on this leg, in fixed-point units. Spent down as
    /// the train travels rather than compared against a distance already
    /// covered, so following a route is O(1) per tick instead of a walk back up
    /// the track.
    pub distance_fixed: i64,
    pub forward: bool,
}

/// The route a train is following: what it will drive, in order.
///
/// Durable state rather than a cache. A train mid-route through a save has a
/// plan it is part way through, and rediscovering that plan on load would be a
/// second search whose answer could differ from the one being followed.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TrainRoute {
    /// Legs still to run, the one being driven first.
    pub legs: VecDeque<TrainRouteLeg>,
    /// Every rail the route runs over, in travel order, including the one the
    /// train started on and the one it stops on. A rail that appears twice is a
    /// stretch the route runs down and then back up.
    ///
    /// Kept so that track removed under a plan invalidates exactly the plans it
    /// was part of, rather than every plan in the world.
    pub edges: Vec<EntityId>,
}

impl TrainRoute {
    /// The leg being driven, or `None` for a route that has been run out.
    pub fn current_leg(&self) -> Option<TrainRouteLeg> {
        self.legs.front().copied()
    }

    /// Whether the route runs over `edge` at any point.
    pub fn uses_edge(&self, edge: EntityId) -> bool {
        self.edges.contains(&edge)
    }
}

/// What a train is being asked to do this tick.
///
/// Coasting and braking are distinct: a coasting train keeps rolling against
/// resistance alone, while a braking one is actively being stopped. Telling
/// them apart is what lets a station later ask for a stop at an exact point
/// rather than hoping resistance gets there.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum TrainThrottle {
    /// No tractive force; the train rolls on against resistance.
    #[default]
    Coast,
    /// Full tractive force in the direction the train faces.
    Forward,
    /// Full tractive force against the direction the train faces.
    Reverse,
    /// Full braking force against whichever way the train is moving.
    Brake,
}

impl TrainThrottle {
    /// Sign of the tractive force this throttle applies along the train's
    /// facing, or zero when it applies none.
    pub const fn drive_sign(self) -> i64 {
        match self {
            Self::Forward => 1,
            Self::Reverse => -1,
            Self::Coast | Self::Brake => 0,
        }
    }
}

/// One piece of rolling stock.
///
/// `prototype_id` is both what the piece is and the whole of its physics
/// profile, so a wagon never carries stats of its own that could drift from the
/// catalog. Cargo is declared by the prototype in the ordinary way — an
/// inventory for a cargo wagon, a fluid box for a fluid wagon — and is present
/// here exactly when the prototype declares it. What *fills* them is separate
/// work; this issue only gives them somewhere to live.
#[derive(Clone, Debug, Deserialize, PartialEq, Hash, Serialize)]
pub struct RollingStock {
    pub id: RollingStockId,
    pub prototype_id: EntityPrototypeId,
    /// Train this piece is coupled into. Every piece belongs to exactly one,
    /// including a piece standing on its own.
    pub train: TrainId,
    pub position: RailPosition,
    /// Item cargo, present exactly when the prototype declares inventory slots.
    pub inventory: Option<Inventory>,
    /// Fluid cargo, one box per declared fluid box.
    pub fluid_boxes: Vec<crate::fluids::FluidBoxState>,
    /// Fuel and stored energy, present exactly on stock with a burner — which
    /// is to say on locomotives. Burnt through the ordinary burner path, so a
    /// locomotive is fuelled like any other burner machine.
    pub energy: Option<BurnerEnergy>,
}

/// An ordered run of coupled stock sharing one velocity.
///
/// The order is front to back along the train's own facing: `stock[0]` leads
/// when the throttle is `Forward`. Every piece advances by the same distance
/// each tick, which is what keeps the couplings rigid without a per-coupling
/// constraint to solve.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Train {
    pub id: TrainId,
    pub stock: Vec<RollingStockId>,
    /// Signed velocity in [`TRAIN_VELOCITY_SCALE`] units per tick. Positive is
    /// travel toward the front.
    pub velocity: i64,
    /// Travel earned but not yet spent, in [`TRAIN_VELOCITY_SCALE`] units.
    /// Carrying it rather than rounding is what stops a slow train from
    /// standing still forever and a fast one from drifting off the distance it
    /// was owed.
    pub travel_remainder: i64,
    pub throttle: TrainThrottle,
    /// Where the train has been told to go, if anywhere.
    ///
    /// Kept separately from the route because the two answer different
    /// questions: this is what the train was asked for and outlives any single
    /// plan, while the route is the plan currently being driven. Track pulled up
    /// under a plan clears the route and leaves this, which is precisely what
    /// makes the re-search happen.
    pub destination: Option<RailTarget>,
    /// The route being driven toward [`Train::destination`]. Absent while a
    /// destination is waiting for a search that the tick's expansion budget has
    /// not paid for yet.
    pub route: Option<TrainRoute>,
}

impl Train {
    /// Whether the train is stopped: no speed and nothing owed.
    pub const fn is_stationary(&self) -> bool {
        self.velocity == 0 && self.travel_remainder == 0
    }

    /// A train given somewhere to be. Its throttle belongs to the routing pass
    /// until it arrives or the plan is cancelled.
    pub const fn is_routed(&self) -> bool {
        self.destination.is_some()
    }
}

/// The forces and limits a train's motion follows from, summed over its stock.
///
/// Derived from the catalog every tick rather than cached on the train: a train
/// changes shape when stock is coupled or mined off, and a cached total is one
/// more thing that could disagree with the pieces it summarizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrainForces {
    pub weight_kilograms: i64,
    /// Tractive force from locomotives that actually have energy to burn.
    pub tractive_force_newtons: i64,
    pub braking_force_newtons: i64,
    /// Lowest top speed among the stock, in [`TRAIN_VELOCITY_SCALE`] units per
    /// tick.
    pub max_speed: i64,
}

/// Every piece of rolling stock in the world and the trains they are coupled
/// into.
///
/// Both maps are ordered by their monotonic ids, so the per-tick pass and every
/// query over them are functions of the world rather than of iteration order.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Hash, Serialize)]
pub struct RollingStockSubsystem {
    pub(crate) stock: BTreeMap<RollingStockId, RollingStock>,
    pub(crate) trains: BTreeMap<TrainId, Train>,
    pub(crate) next_stock_id: u64,
    pub(crate) next_train_id: u64,
}

impl RollingStockSubsystem {
    pub fn len(&self) -> usize {
        self.stock.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stock.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &RollingStock> {
        self.stock.values()
    }

    pub fn get(&self, id: RollingStockId) -> Option<&RollingStock> {
        self.stock.get(&id)
    }

    pub fn train_count(&self) -> usize {
        self.trains.len()
    }

    pub fn trains(&self) -> impl Iterator<Item = &Train> {
        self.trains.values()
    }

    pub fn train(&self, id: TrainId) -> Option<&Train> {
        self.trains.get(&id)
    }

    pub(crate) fn allocate_stock_id(&mut self) -> RollingStockId {
        self.next_stock_id += 1;
        RollingStockId::new(self.next_stock_id)
    }

    pub(crate) fn allocate_train_id(&mut self) -> TrainId {
        self.next_train_id += 1;
        TrainId::new(self.next_train_id)
    }
}

/// Why a piece of rolling stock could not be put on the track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollingStockPlacementError {
    /// The prototype is not rolling stock, so it belongs on tiles instead.
    NotRollingStock(EntityPrototypeId),
    /// No rail under the requested tile.
    NoRail,
    /// The run of track through the requested point is shorter than the piece.
    TrackTooShort,
    /// Another piece of stock already occupies the space.
    Occupied(RollingStockId),
    /// The player is not carrying the item that builds this stock.
    InsufficientInventory { item_id: factory_data::ItemId },
    /// The recipe producing the build item has not been researched.
    Locked(EntityPrototypeId),
    /// The prototype declares no build item, so nothing could place it. A
    /// catalog problem, not something a player did.
    MissingBuildItem(EntityPrototypeId),
    /// The item offered is not the one that builds this stock. Kept apart from
    /// [`RollingStockPlacementError::MissingBuildItem`] because the two are
    /// different failures: one says the prototype can never be placed, the
    /// other that this particular call passed the wrong item.
    ItemDoesNotBuildStock {
        item_id: factory_data::ItemId,
        prototype_id: EntityPrototypeId,
    },
}

/// Why a piece of rolling stock could not be taken off the track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollingStockMiningError {
    MissingStock(RollingStockId),
    /// The stock's build item, its fuel, or its cargo would not fit in the
    /// player inventory. Mining is all-or-nothing so a half-recovered wagon
    /// cannot leave items behind on a train that no longer exists.
    InsufficientInventory {
        item_id: factory_data::ItemId,
    },
    /// The prototype declares no build item, so there is nothing to recover.
    MissingBuildItem(EntityPrototypeId),
}

/// Why a train would not take a drive command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainControlError {
    MissingTrain(TrainId),
    /// The entity a train was told to drive to is not a rail.
    NotRail(EntityId),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reversing_a_position_turns_it_around_without_moving_it() {
        let position = RailPosition::new(EntityId::new(7), 512, true);
        let reversed = position.reversed();

        assert_eq!(reversed.edge, position.edge);
        assert_eq!(reversed.distance_fixed, position.distance_fixed);
        assert!(!reversed.forward);
        assert_eq!(reversed.reversed(), position);
    }

    #[test]
    fn only_the_driving_throttles_apply_tractive_force() {
        assert_eq!(TrainThrottle::Forward.drive_sign(), 1);
        assert_eq!(TrainThrottle::Reverse.drive_sign(), -1);
        assert_eq!(TrainThrottle::Coast.drive_sign(), 0);
        assert_eq!(TrainThrottle::Brake.drive_sign(), 0);
    }
}
