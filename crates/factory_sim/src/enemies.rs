use crate::ids::EntityId;
use crate::world::{ChunkCoord, WorldTileCoord};
use crate::{AttackDefinition, Faction, HealthState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

macro_rules! enemy_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
        )]
        pub struct $name(u64);
        impl $name {
            pub const fn new(raw: u64) -> Self {
                Self(raw)
            }
            pub const fn raw(self) -> u64 {
                self.0
            }
        }
    };
}

enemy_id!(EnemyBaseId);
enemy_id!(RaidId);
enemy_id!(ExpansionId);

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

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum EnemyMission {
    Guard,
    Staging(EnemyBaseId),
    Raid(RaidId),
    Expansion(ExpansionId),
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum EnemyDifficultyPreset {
    Peaceful,
    Standard,
    Aggressive,
    Custom,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EnemyWorldSettings {
    pub base_density_percent: u16,
    pub starting_safe_radius_tiles: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EnemyRuntimeSettings {
    pub proactive_raids: bool,
    pub expansion: bool,
    pub strength_percent: u16,
    pub pollution_sensitivity_percent: u16,
    pub evolution_rate_percent: u16,
    pub raid_frequency_percent: u16,
    pub expansion_frequency_percent: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SimulationConfig {
    pub preset: EnemyDifficultyPreset,
    pub world: EnemyWorldSettings,
    pub runtime: EnemyRuntimeSettings,
}

impl EnemyDifficultyPreset {
    pub const fn config(self) -> SimulationConfig {
        match self {
            Self::Peaceful => SimulationConfig {
                preset: self,
                world: EnemyWorldSettings {
                    base_density_percent: 75,
                    starting_safe_radius_tiles: 180,
                },
                runtime: EnemyRuntimeSettings {
                    proactive_raids: false,
                    expansion: false,
                    strength_percent: 75,
                    pollution_sensitivity_percent: 50,
                    evolution_rate_percent: 50,
                    raid_frequency_percent: 0,
                    expansion_frequency_percent: 0,
                },
            },
            Self::Aggressive => SimulationConfig {
                preset: self,
                world: EnemyWorldSettings {
                    base_density_percent: 150,
                    starting_safe_radius_tiles: 80,
                },
                runtime: EnemyRuntimeSettings {
                    proactive_raids: true,
                    expansion: true,
                    strength_percent: 150,
                    pollution_sensitivity_percent: 150,
                    evolution_rate_percent: 150,
                    raid_frequency_percent: 175,
                    expansion_frequency_percent: 175,
                },
            },
            Self::Standard | Self::Custom => SimulationConfig {
                preset: self,
                world: EnemyWorldSettings {
                    base_density_percent: 100,
                    starting_safe_radius_tiles: 120,
                },
                runtime: EnemyRuntimeSettings {
                    proactive_raids: true,
                    expansion: true,
                    strength_percent: 100,
                    pollution_sensitivity_percent: 100,
                    evolution_rate_percent: 100,
                    raid_frequency_percent: 100,
                    expansion_frequency_percent: 100,
                },
            },
        }
    }
}

impl Default for SimulationConfig {
    fn default() -> Self {
        EnemyDifficultyPreset::Standard.config()
    }
}

impl SimulationConfig {
    pub fn is_valid(self) -> bool {
        self.world.base_density_percent <= 200
            && (64..=320).contains(&self.world.starting_safe_radius_tiles)
            && (50..=200).contains(&self.runtime.strength_percent)
            && (25..=200).contains(&self.runtime.pollution_sensitivity_percent)
            && (25..=200).contains(&self.runtime.evolution_rate_percent)
            && (self.runtime.raid_frequency_percent == 0
                || (25..=200).contains(&self.runtime.raid_frequency_percent))
            && (self.runtime.expansion_frequency_percent == 0
                || (25..=200).contains(&self.runtime.expansion_frequency_percent))
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum ThreatTier {
    #[default]
    Low,
    Elevated,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum ThreatLocation {
    Exact {
        x: WorldTileCoord,
        y: WorldTileCoord,
    },
    Sector(ChunkCoord),
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum ThreatEventKind {
    PollutionContact,
    RaidPreparing,
    RaidLaunched,
    StructureUnderAttack,
    ExpansionSpotted,
    BaseDestroyed,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ThreatEvent {
    pub sequence: u64,
    pub tick: u64,
    pub kind: ThreatEventKind,
    pub location: ThreatLocation,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThreatSnapshot {
    pub tier: ThreatTier,
    pub evolution_percent: u8,
    pub total_pollution_micro: u64,
    pub pollution_active_colonies: usize,
    pub staged_units: usize,
    pub maximum_launch_countdown_ticks: u64,
    pub inbound_raids: usize,
    pub spotted_expansions: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnemyMapSnapshot {
    pub contacted_sectors: Vec<ChunkCoord>,
    pub known_bases: Vec<(EnemyBaseId, WorldTileCoord, WorldTileCoord)>,
    pub raids: Vec<(RaidId, ThreatLocation)>,
    pub expansions: Vec<(ExpansionId, ThreatLocation)>,
    pub raid_targets: Vec<(RaidId, EntityId)>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EnemyBase {
    pub id: EnemyBaseId,
    pub anchor: ChunkCoord,
    pub spawners: BTreeSet<EntityId>,
    pub creation_tick: u64,
    pub attack_budget_micro: u64,
    pub staged_units: BTreeSet<EnemyId>,
    pub staging_started_tick: Option<u64>,
    pub next_raid_tick: u64,
    pub next_expansion_tick: u64,
    pub next_growth_tick: u64,
    pub pollution_contact: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Raid {
    pub id: RaidId,
    pub base_id: EnemyBaseId,
    pub members: BTreeSet<EnemyId>,
    pub target: Option<EntityId>,
    pub launched_tick: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Expansion {
    pub id: ExpansionId,
    pub base_id: EnemyBaseId,
    pub members: BTreeSet<EnemyId>,
    pub destination: (WorldTileCoord, WorldTileCoord),
    pub spotted: bool,
    pub spawner_prototype: factory_data::EntityPrototypeId,
}

/// A mobile enemy unit. Its reusable combat state is derived from the
/// spawner's unit prototype at spawn time and remains self-contained if its
/// home spawner is destroyed.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Enemy {
    pub id: EnemyId,
    /// Fixed-point center position, 1024 units per tile.
    pub x: i64,
    pub y: i64,
    pub health: HealthState,
    pub attack: AttackDefinition,
    pub speed_fixed_per_tick: u32,
    pub aggro_radius_tiles: u32,
    pub mode: EnemyMode,
    pub mission: EnemyMission,
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
    pub const fn faction(&self) -> Faction {
        self.health.faction
    }

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
    pub(crate) bases: BTreeMap<EnemyBaseId, EnemyBase>,
    pub(crate) spawner_bases: BTreeMap<EntityId, EnemyBaseId>,
    pub(crate) raids: BTreeMap<RaidId, Raid>,
    pub(crate) expansions: BTreeMap<ExpansionId, Expansion>,
    pub(crate) next_base_id: u64,
    pub(crate) next_raid_id: u64,
    pub(crate) next_expansion_id: u64,
    pub(crate) evolution_points: u16,
    pub(crate) evolution_remainder: u32,
    pub(crate) pollution_evolution_micro_remainder: u64,
    pub(crate) threat_sequence: u64,
    pub(crate) threat_events: VecDeque<ThreatEvent>,
    pub(crate) structure_warning_ticks: BTreeMap<ChunkCoord, u64>,
    #[serde(skip, default)]
    pub(crate) placement_base: Option<EnemyBaseId>,
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

    pub(crate) fn allocate_base_id(&mut self) -> EnemyBaseId {
        self.next_base_id += 1;
        EnemyBaseId::new(self.next_base_id)
    }
    pub(crate) fn allocate_raid_id(&mut self) -> RaidId {
        self.next_raid_id += 1;
        RaidId::new(self.next_raid_id)
    }
    pub(crate) fn allocate_expansion_id(&mut self) -> ExpansionId {
        self.next_expansion_id += 1;
        ExpansionId::new(self.next_expansion_id)
    }
}
