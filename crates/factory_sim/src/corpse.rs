use crate::{ItemAmount, PlayerWeaponState, WorldTileCoord};
use serde::{Deserialize, Serialize};

/// The single player's recovery container. Corpses do not occupy or block tiles,
/// cannot be destroyed or automated, and persist until completely emptied.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PlayerCorpse {
    pub(crate) id: u64,
    pub(crate) created_tick: u64,
    pub(crate) x: WorldTileCoord,
    pub(crate) y: WorldTileCoord,
    pub(crate) items: Vec<ItemAmount>,
    pub(crate) weapon: PlayerWeaponState,
    pub(crate) repair_remaining_health: u32,
}

impl PlayerCorpse {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn created_tick(&self) -> u64 {
        self.created_tick
    }
    pub fn tile_position(&self) -> (WorldTileCoord, WorldTileCoord) {
        (self.x, self.y)
    }
    pub fn items(&self) -> &[ItemAmount] {
        &self.items
    }
    pub fn remaining_ammunition(&self) -> u32 {
        self.weapon.loaded_shots
    }
    pub fn remaining_repair_health(&self) -> u32 {
        self.repair_remaining_health
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.weapon.loaded_shots == 0 && self.repair_remaining_health == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseRecoveryError {
    PlayerDead,
    MissingCorpse,
    OutOfReach,
    /// No items fit, or an opened consumable would overwrite one already held.
    NoCapacity,
}
