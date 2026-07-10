use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PlayerState {
    pub(crate) x: i64,
    pub(crate) y: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ManualMiningTarget {
    pub x: crate::world::WorldTileCoord,
    pub y: crate::world::WorldTileCoord,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ManualMiningProgress {
    pub target: ManualMiningTarget,
    pub progress_ticks: u32,
    pub required_ticks: u32,
}
