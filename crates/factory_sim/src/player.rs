use crate::HealthState;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PlayerState {
    /// Tick of the one-time death transition; None means alive.
    pub(crate) dead_since: Option<u64>,
    pub(crate) respawn_requested: bool,
    pub(crate) x: i64,
    pub(crate) y: i64,
    /// Health the currently opened repair pack can still restore; a new pack
    /// is consumed from the inventory when this reaches zero mid-repair.
    pub(crate) repair_remaining_health: u32,
    pub(crate) health: HealthState,
}

impl PlayerState {
    pub fn is_dead(self) -> bool {
        self.dead_since.is_some()
    }
    pub fn dead_since(self) -> Option<u64> {
        self.dead_since
    }

    pub fn health(self) -> HealthState {
        self.health
    }
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
