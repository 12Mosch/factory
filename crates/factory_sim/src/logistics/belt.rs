use crate::entities::Direction;
use crate::ids::EntityId;
use factory_data::{ItemId, UndergroundBeltPart};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UndergroundBeltLinkPreview {
    Entrance {
        max_distance: u8,
        matched_exit_tile: Option<(crate::world::WorldTileCoord, crate::world::WorldTileCoord)>,
    },
    Exit {
        max_distance: u8,
        matched_entrance_tile: Option<(crate::world::WorldTileCoord, crate::world::WorldTileCoord)>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BeltSegment {
    pub dir: Direction,
    pub speed_subtiles_per_tick: u16,
    pub underground: Option<UndergroundBeltSegment>,
    pub lanes: [BeltLane; 2],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SplitterState {
    pub dir: Direction,
    pub speed_subtiles_per_tick: u16,
    pub input_lanes: [[BeltLane; 2]; 2],
    pub next_output_by_lane: [usize; 2],
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct UndergroundBeltSegment {
    pub part: UndergroundBeltPart,
    pub max_distance: u8,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BeltLane {
    pub items: SmallVec<[BeltItem; 8]>,
}

/// Persistent identity of one physical item while it remains in transport.
///
/// Routing preserves this value across belts and splitters. Removing an item
/// into an inventory ends the identity's lifetime.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct BeltItemId(u64);

impl BeltItemId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BeltItem {
    pub id: BeltItemId,
    pub item_id: ItemId,
    pub position_subtile: u16,
}

impl BeltSegment {
    pub fn new(dir: Direction, speed_subtiles_per_tick: u16) -> Self {
        Self {
            dir,
            speed_subtiles_per_tick,
            underground: None,
            lanes: [BeltLane::default(), BeltLane::default()],
        }
    }

    pub fn underground(
        dir: Direction,
        speed_subtiles_per_tick: u16,
        part: UndergroundBeltPart,
        max_distance: u8,
    ) -> Self {
        Self {
            dir,
            speed_subtiles_per_tick,
            underground: Some(UndergroundBeltSegment { part, max_distance }),
            lanes: [BeltLane::default(), BeltLane::default()],
        }
    }
}

impl SplitterState {
    pub fn new(dir: Direction, speed_subtiles_per_tick: u16) -> Self {
        Self {
            dir,
            speed_subtiles_per_tick,
            input_lanes: [
                [BeltLane::default(), BeltLane::default()],
                [BeltLane::default(), BeltLane::default()],
            ],
            next_output_by_lane: [0, 0],
        }
    }
}

impl Default for BeltSegment {
    fn default() -> Self {
        Self::new(Direction::default(), 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeltError {
    MissingEntity(EntityId),
    NotTransportBelt(EntityId),
    InvalidLane { lane_index: usize },
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitterError {
    MissingEntity(EntityId),
    NotSplitter(EntityId),
}
