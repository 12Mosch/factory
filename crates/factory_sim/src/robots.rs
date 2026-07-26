//! Robot networks: the fourth connectivity graph alongside power, fluids, and
//! heat.
//!
//! What it shares with the other three is the shape: a cached topology built by
//! a disjoint set over placed entities, invalidated whenever placement changes
//! and rebuilt lazily. What differs is that nothing flows. A robot network is
//! pure geometry — which roboports reach each other, and which tiles the
//! network covers — so the "solve" is the union-find itself and the per-network
//! snapshot is a summary of that geometry rather than a settled quantity.
//!
//! Two rules define it, and they deliberately use different radii:
//!
//! * **Connection.** Two roboports belong to the same network when their
//!   *logistic* squares overlap. This is the rule a player builds against:
//!   place the next roboport within logistic reach and the network grows.
//! * **Coverage.** A network's construction coverage is the *union* of its
//!   members' construction squares, not one rectangle spanning the network.
//!   A network shaped like an L covers an L; the bounding box in
//!   [`RobotNetworkSnapshot`] is a summary for presentation, never the
//!   authority on whether a tile is covered.
//!
//! The moving half lives here too. A robot is an item while it sits in a
//! roboport's robot slots and a free-flying unit once dispatched, so
//! [`RobotFlightSubsystem`] holds units that are deliberately outside
//! `EntityStore`, `OccupancyGrid`, and `DenseEntityMap` — all three are
//! tile-locked, and a robot is not. This mirrors how enemy units are stored:
//! an ordered map keyed by a monotonic id, fixed-point positions at
//! [`crate::simulation::POSITION_SCALE`], and no collision between units.

use crate::construction::ConstructionJob;
use crate::ids::EntityId;
use crate::inventory::{Inventory, ItemStack};
use crate::prototypes::ItemId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Durable state of one roboport.
///
/// The two inventories are kept apart rather than merged into one filtered
/// inventory because they answer to different rules: robots are dispatched from
/// `robots`, while `materials` is drawn down by repairs. A single inventory
/// would let a full load of repair packs starve the network of robot slots.
#[derive(Clone, Debug, Deserialize, PartialEq, Hash, Serialize)]
pub struct RoboportState {
    /// Construction and logistic robots stationed here.
    pub robots: Inventory,
    /// Repair packs and other construction material.
    pub materials: Inventory,
    /// Energy buffered for robot charging, in joules. Filled from the electric
    /// network up to the prototype's capacity.
    pub charge_energy_joules: u64,
}

impl RoboportState {
    pub(crate) fn new(robot_slot_count: usize, material_slot_count: usize) -> Self {
        Self {
            robots: Inventory::with_slot_count(robot_slot_count),
            materials: Inventory::with_slot_count(material_slot_count),
            charge_energy_joules: 0,
        }
    }
}

/// Inclusive tile rectangle, used for the coverage summaries a network reports.
///
/// Empty networks cannot occur — a network exists because a roboport is in it —
/// so every bounds value here is a real rectangle rather than an optional one.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TileBounds {
    pub min_x: i64,
    pub min_y: i64,
    pub max_x: i64,
    pub max_y: i64,
}

impl TileBounds {
    pub fn contains(self, x: i64, y: i64) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    pub fn intersects(self, other: Self) -> bool {
        self.min_x <= other.max_x
            && other.min_x <= self.max_x
            && self.min_y <= other.max_y
            && other.min_y <= self.max_y
    }

    pub(crate) fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }
}

/// Presentation view of one robot network.
///
/// `construction_bounds` and `logistic_bounds` are the bounding boxes of the
/// member squares. They are the right thing to draw a network's extent with and
/// the wrong thing to answer coverage with: coverage is the union of
/// `roboports[..].construction_bounds`, which an L-shaped network's bounding
/// box overstates.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RobotNetworkSnapshot {
    pub network_id: u32,
    /// Members in ascending entity-id order.
    pub roboports: Vec<RobotNetworkRoboportSnapshot>,
    pub construction_bounds: TileBounds,
    pub logistic_bounds: TileBounds,
    /// Energy buffered across the network's roboports, and the capacity it is
    /// filling toward.
    pub charge_energy_joules: u64,
    pub charge_capacity_joules: u64,
    pub available_construction_robots: u32,
    pub total_construction_robots: u32,
    pub jobs: RobotNetworkJobCounts,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RobotNetworkJobCounts {
    pub build: u32,
    pub deconstruction: u32,
    pub repair: u32,
}

impl RobotNetworkJobCounts {
    pub(crate) fn add(&mut self, job: ConstructionJob) {
        let count = match job {
            ConstructionJob::BuildGhost(_) => &mut self.build,
            ConstructionJob::Deconstruct(_) => &mut self.deconstruction,
            ConstructionJob::Repair(_) => &mut self.repair,
        };
        *count = count.saturating_add(1);
    }

    pub(crate) fn remove(&mut self, job: ConstructionJob) {
        let count = match job {
            ConstructionJob::BuildGhost(_) => &mut self.build,
            ConstructionJob::Deconstruct(_) => &mut self.deconstruction,
            ConstructionJob::Repair(_) => &mut self.repair,
        };
        debug_assert!(*count > 0);
        *count = count.saturating_sub(1);
    }
}

/// What one robot network holds and wants of a single item.
///
/// The three counters answer three different questions and are deliberately
/// kept apart: `available` is what a robot could be sent to fetch, `requested`
/// is unmet demand a dispatcher would work through, and `stored` is everything
/// the network's logistic chests hold — the figure a circuit network reads,
/// because a requester's delivered stock is still stock.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LogisticItemTotals {
    pub available: u32,
    pub requested: u32,
    pub stored: u32,
}

impl LogisticItemTotals {
    pub(crate) fn add(&mut self, other: Self) {
        self.available = self.available.saturating_add(other.available);
        self.requested = self.requested.saturating_add(other.requested);
        self.stored = self.stored.saturating_add(other.stored);
    }

    pub(crate) fn subtract(&mut self, other: Self) {
        self.available = self.available.saturating_sub(other.available);
        self.requested = self.requested.saturating_sub(other.requested);
        self.stored = self.stored.saturating_sub(other.stored);
    }

    pub(crate) fn is_zero(self) -> bool {
        self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RobotNetworkRoboportSnapshot {
    pub entity_id: EntityId,
    pub construction_bounds: TileBounds,
    pub logistic_bounds: TileBounds,
    pub charge_energy_joules: u64,
    pub charge_capacity_joules: u64,
}

/// Robot-network status of one entity, for machine panels and diagnostics.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityRoboportStatus {
    pub network_id: Option<u32>,
    pub charge_energy_joules: u64,
    pub charge_capacity_joules: u64,
    pub construction_bounds: TileBounds,
    pub logistic_bounds: TileBounds,
    pub available_construction_robots: u32,
    pub total_construction_robots: u32,
    pub jobs: RobotNetworkJobCounts,
}

/// Identity of one flying robot, monotonic per world.
///
/// Separate from [`EntityId`] because robots are not placed entities: nothing
/// occupies a tile, nothing is in the occupancy grid, and the id space must not
/// be confused with the one entity states are keyed by.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct RobotId(u64);

impl RobotId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// What a robot is doing this tick.
///
/// Charging is split into three states rather than one because a roboport has a
/// fixed number of pads: arriving, waiting, and charging are distinguishable so
/// a robot in a queue can be told apart from one actually drawing energy, and
/// so the queue can be replayed deterministically after a save.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum RobotActivity {
    /// Flying its errand, or flying home once the errand is done.
    Flying,
    /// Out of energy: flying to `roboport` to recharge before resuming.
    SeekingCharge(EntityId),
    /// Hovering over `roboport`, waiting for a charging pad to free up.
    Queued(EntityId),
    /// Occupying one of `roboport`'s charging pads.
    Charging(EntityId),
}

impl RobotActivity {
    /// Roboport this activity is bound to, if any.
    pub const fn roboport(self) -> Option<EntityId> {
        match self {
            Self::Flying => None,
            Self::SeekingCharge(entity_id)
            | Self::Queued(entity_id)
            | Self::Charging(entity_id) => Some(entity_id),
        }
    }
}

/// One robot in flight.
///
/// `item_id` is both the flight profile (speed, energy capacity, draw) and what
/// the robot becomes again when it docks, so a robot never has stats of its own
/// that could drift from the catalog.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Robot {
    pub id: RobotId,
    pub item_id: ItemId,
    /// Fixed-point position, 1024 units per tile.
    pub x: i64,
    pub y: i64,
    pub energy_joules: u64,
    /// Roboport this robot docks into when it is done. `None` only while the
    /// world has no roboport left to adopt it.
    pub home_roboport: Option<EntityId>,
    /// Fixed-point errand target; `None` once the robot is on its way home.
    pub errand: Option<(i64, i64)>,
    pub activity: RobotActivity,
    /// Construction work this robot exclusively owns. Jobless robots retain
    /// the public debug errand behavior.
    pub construction_job: Option<ConstructionJob>,
    /// Reserved build item or repair pack. It remains separate from recovered
    /// cargo so validation can enforce the job-specific payload contract.
    pub payload: Option<ItemStack>,
    /// Items recovered by deconstruction, or unused payload returning after an
    /// abort. Cargo may be deposited partially when network storage is tight.
    pub cargo: Vec<ItemStack>,
}

impl Robot {
    pub fn position_tiles(&self) -> (f32, f32) {
        (
            self.x as f32 / crate::simulation::POSITION_SCALE as f32,
            self.y as f32 / crate::simulation::POSITION_SCALE as f32,
        )
    }

    pub fn tile(&self) -> (i64, i64) {
        (
            self.x.div_euclid(crate::simulation::POSITION_SCALE),
            self.y.div_euclid(crate::simulation::POSITION_SCALE),
        )
    }
}

/// Charging occupancy of one roboport.
///
/// The pads are a set rather than a `Vec<Option<RobotId>>` because which pad a
/// robot sits on carries no meaning; how many are taken does. `queue` is the
/// order arrivals will be served in, and being a `VecDeque` is what makes that
/// order survive a save rather than depending on iteration order.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RoboportChargingState {
    pub charging: BTreeSet<RobotId>,
    pub queue: VecDeque<RobotId>,
}

impl RoboportChargingState {
    pub fn is_empty(&self) -> bool {
        self.charging.is_empty() && self.queue.is_empty()
    }
}

/// Every robot in flight, plus the charging occupancy of the roboports they use.
///
/// Iteration is over a `BTreeMap` keyed by [`RobotId`], so the per-tick pass
/// visits robots in creation order on every machine and every replay.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RobotFlightSubsystem {
    pub(crate) robots: BTreeMap<RobotId, Robot>,
    pub(crate) next_robot_id: u64,
    /// Keyed by roboport; entries exist only while a roboport has robots
    /// charging or queued.
    pub(crate) charging: BTreeMap<EntityId, RoboportChargingState>,
}

impl RobotFlightSubsystem {
    pub fn len(&self) -> usize {
        self.robots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.robots.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Robot> {
        self.robots.values()
    }

    pub fn get(&self, id: RobotId) -> Option<&Robot> {
        self.robots.get(&id)
    }

    pub fn charging_state(&self, roboport: EntityId) -> Option<&RoboportChargingState> {
        self.charging.get(&roboport)
    }

    pub(crate) fn allocate_id(&mut self) -> RobotId {
        self.next_robot_id += 1;
        RobotId::new(self.next_robot_id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoboportError {
    MissingEntity(EntityId),
    NotRoboport(EntityId),
    /// An item was refused by the robot slots, which only hold robots.
    InvalidRobot(crate::prototypes::ItemId),
    /// An item was refused by the material slots, which only hold repair
    /// material.
    InvalidMaterial(crate::prototypes::ItemId),
    InvalidSlot {
        slot_index: usize,
    },
    EmptySlot {
        slot_index: usize,
    },
    InsufficientSpace,
    UnknownItem,
}

/// Why a roboport could not send a robot out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RobotDispatchError {
    MissingEntity(EntityId),
    NotRoboport(EntityId),
    /// The robot slots hold no robot to send.
    NoRobotAvailable,
    /// A robot is sent out fully charged, and the roboport's buffer does not
    /// hold that much. Filling the buffer is the electric network's job, so
    /// this is what an under-powered network looks like from the dispatch side.
    InsufficientCharge {
        required_joules: u64,
        available_joules: u64,
    },
    /// The stored robot has no flight profile in the catalog.
    InvalidRobot(ItemId),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_intersect_when_they_share_a_tile() {
        let first = TileBounds {
            min_x: 0,
            min_y: 0,
            max_x: 10,
            max_y: 10,
        };
        let touching = TileBounds {
            min_x: 10,
            min_y: 10,
            max_x: 20,
            max_y: 20,
        };
        let separated = TileBounds {
            min_x: 11,
            min_y: 0,
            max_x: 20,
            max_y: 10,
        };

        assert!(first.intersects(touching));
        assert!(touching.intersects(first));
        assert!(!first.intersects(separated));
        assert!(!separated.intersects(first));
    }

    #[test]
    fn union_covers_both_rectangles() {
        let first = TileBounds {
            min_x: -4,
            min_y: 2,
            max_x: 0,
            max_y: 6,
        };
        let second = TileBounds {
            min_x: 3,
            min_y: -1,
            max_x: 5,
            max_y: 4,
        };

        assert_eq!(
            first.union(second),
            TileBounds {
                min_x: -4,
                min_y: -1,
                max_x: 5,
                max_y: 6,
            }
        );
    }

    #[test]
    fn bounds_contain_their_own_corners_only() {
        let bounds = TileBounds {
            min_x: -2,
            min_y: -2,
            max_x: 2,
            max_y: 2,
        };

        assert!(bounds.contains(-2, -2));
        assert!(bounds.contains(2, 2));
        assert!(!bounds.contains(3, 2));
        assert!(!bounds.contains(-2, -3));
    }
}
