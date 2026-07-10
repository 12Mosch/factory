use crate::inventory::Inventory;
use serde::{Deserialize, Serialize};

pub const GUN_TURRET_AMMO_SLOT_COUNT: usize = 1;

/// Runtime state of a placed gun turret. Magazines are loaded one at a time
/// from the ammo inventory; `loaded_shots` tracks the opened magazine, whose
/// per-shot damage is captured at load time so the magazine item itself no
/// longer needs to exist.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct GunTurretState {
    pub ammo: Inventory,
    pub loaded_shots: u32,
    pub loaded_damage: u32,
    pub next_ready_tick: u64,
}

impl GunTurretState {
    pub fn new() -> Self {
        Self {
            ammo: Inventory::with_slot_count(GUN_TURRET_AMMO_SLOT_COUNT),
            loaded_shots: 0,
            loaded_damage: 0,
            next_ready_tick: 0,
        }
    }
}

impl Default for GunTurretState {
    fn default() -> Self {
        Self::new()
    }
}

/// Runtime state of an enemy spawner: pollution it has soaked up from its
/// chunk (spent on spawning attackers) and the schedule for free guard
/// spawns.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EnemySpawnerState {
    pub absorbed_pollution_micro: u64,
    pub next_free_spawn_tick: u64,
}

/// Current health of a placed entity whose prototype declares `max_health`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HealthState {
    pub current: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairError {
    MissingEntity(crate::ids::EntityId),
    /// The entity has no health and cannot be repaired.
    NotRepairable(crate::ids::EntityId),
    OutOfReach,
    NoRepairPacks,
}
