use crate::ids::EntityId;
use crate::world::{ChunkCoord, WorldTileCoord};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct EnemyId(u64);

impl EnemyId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Behavioral stance of a mobile enemy unit. Guards linger near their home
/// spawner and only engage what comes close; attackers (spawned from absorbed
/// pollution) march on the player's nearest structure.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum EnemyMode {
    Guard,
    Attack,
}

/// A mobile enemy unit. Combat stats are copied from the spawner's unit
/// prototype at spawn time so the unit stays self-contained even if its home
/// spawner is destroyed.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Enemy {
    pub id: EnemyId,
    /// Fixed-point center position, 1024 units per tile.
    pub x: i64,
    pub y: i64,
    pub health: u32,
    pub max_health: u32,
    pub damage: u32,
    pub attack_cooldown_ticks: u32,
    pub speed_fixed_per_tick: u32,
    pub aggro_radius_tiles: u32,
    pub mode: EnemyMode,
    pub home_spawner: Option<EntityId>,
    /// Entity currently being approached or attacked.
    pub target: Option<EntityId>,
    /// Remaining tile waypoints toward the target, front first.
    pub path: VecDeque<(WorldTileCoord, WorldTileCoord)>,
    pub next_attack_tick: u64,
    /// Next tick this unit re-evaluates targets or recomputes its path;
    /// staggered so unit AI cost is spread over ticks.
    pub next_decision_tick: u64,
}

impl Enemy {
    pub fn position_tiles(&self) -> (f32, f32) {
        (
            self.x as f32 / crate::simulation::POSITION_SCALE as f32,
            self.y as f32 / crate::simulation::POSITION_SCALE as f32,
        )
    }

    pub fn tile(&self) -> (WorldTileCoord, WorldTileCoord) {
        (
            self.x.div_euclid(crate::simulation::POSITION_SCALE),
            self.y.div_euclid(crate::simulation::POSITION_SCALE),
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EnemySubsystem {
    pub(crate) enemies: BTreeMap<EnemyId, Enemy>,
    pub(crate) next_enemy_id: u64,
    /// Chunks already rolled for spawner placement; grows with generated
    /// chunks so each chunk is seeded exactly once per world.
    pub(crate) seeded_chunks: BTreeSet<ChunkCoord>,
}

impl EnemySubsystem {
    pub fn len(&self) -> usize {
        self.enemies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.enemies.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Enemy> {
        self.enemies.values()
    }

    pub fn get(&self, id: EnemyId) -> Option<&Enemy> {
        self.enemies.get(&id)
    }

    pub(crate) fn allocate_id(&mut self) -> EnemyId {
        self.next_enemy_id += 1;
        EnemyId::new(self.next_enemy_id)
    }
}
