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
//! Robots themselves are not modelled here yet. What exists is the static half:
//! the roboports, their storage, their charging buffers, and the networks they
//! form.

use crate::ids::EntityId;
use crate::inventory::Inventory;
use serde::{Deserialize, Serialize};

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
