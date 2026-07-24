use glam::IVec2;
use serde::{Deserialize, Serialize};

use crate::ids::{EntityPrototypeId, FluidId, ItemId, RecipeId, TechnologyId, TileId};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DamageType {
    Physical,
    Fire,
    Explosion,
    Acid,
    Laser,
}

impl DamageType {
    pub const COUNT: usize = 5;

    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidPrototype {
    pub id: FluidId,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ItemPrototype {
    pub id: ItemId,
    pub name: String,
    pub stack_size: u16,
    pub fuel_value_joules: Option<u64>,
    /// Present when the item can be loaded into gun turrets as ammunition.
    pub ammo: Option<AmmoPrototype>,
    /// Present when the item can be consumed to repair damaged entities.
    pub repair: Option<RepairToolPrototype>,
    /// Present when the item can be equipped as the player's armor.
    pub armor: Option<ArmorPrototype>,
    /// Present when the item can be installed in an equipped armor grid.
    pub equipment: Option<EquipmentPrototype>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AmmoPrototype {
    pub damage_per_shot: u32,
    pub shots_per_item: u32,
    pub damage_type: DamageType,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RepairToolPrototype {
    /// Total health one item restores before it is used up.
    pub restore_health: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ArmorPrototype {
    pub grid_width: u8,
    pub grid_height: u8,
    pub resistances: Vec<DamageResistancePrototype>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct DamageResistancePrototype {
    pub damage_type: DamageType,
    pub flat_reduction: u32,
    pub percent_reduction_permyriad: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EquipmentPrototype {
    pub width: u8,
    pub height: u8,
    pub effect: EquipmentEffectPrototype,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum EquipmentEffectPrototype {
    PowerGeneration {
        power_watts: u64,
    },
    Battery {
        capacity_joules: u64,
    },
    EnergyShield {
        capacity_points: u32,
        max_recharge_watts: u64,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RecipePrototype {
    pub id: RecipeId,
    pub name: String,
    pub category: CraftingCategory,
    pub crafting_time_ticks: u32,
    pub ingredients: Vec<ItemAmount>,
    pub products: Vec<ItemAmount>,
    pub fluid_ingredients: Vec<FluidAmount>,
    pub fluid_products: Vec<FluidAmount>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityPrototype {
    pub id: EntityPrototypeId,
    pub name: String,
    pub entity_kind: EntityKind,
    pub size: IVec2,
    pub collision_mask: CollisionMask,
    pub build_item: Option<ItemId>,
    pub building_category: Option<BuildingCategory>,
    pub building_menu_order: Option<u16>,
    pub inventory_slot_count: Option<usize>,
    pub burner: Option<BurnerPrototype>,
    pub mining_drill: Option<MiningDrillPrototype>,
    pub furnace: Option<FurnacePrototype>,
    pub assembling_machine: Option<AssemblingMachinePrototype>,
    pub transport_belt: Option<TransportBeltPrototype>,
    pub splitter: Option<SplitterPrototype>,
    pub inserter: Option<InserterPrototype>,
    pub electric_pole: Option<ElectricPolePrototype>,
    pub electric_energy_source: Option<ElectricEnergySourcePrototype>,
    pub steam_engine: Option<SteamEnginePrototype>,
    pub boiler: Option<BoilerPrototype>,
    pub offshore_pump: Option<OffshorePumpPrototype>,
    pub pump: Option<PumpPrototype>,
    pub pumpjack: Option<PumpjackPrototype>,
    pub underground_pipe: Option<UndergroundPipePrototype>,
    pub fluid_boxes: Vec<FluidBoxPrototype>,
    /// Present when the entity can take damage and be destroyed.
    pub max_health: Option<u32>,
    /// Pollution emitted into the entity's chunk while it is actively
    /// working, in milli-pollution-units per minute.
    pub pollution_per_minute_milli: Option<u32>,
    pub gun_turret: Option<GunTurretPrototype>,
    pub laser_turret: Option<LaserTurretPrototype>,
    pub enemy_spawner: Option<EnemySpawnerPrototype>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
pub enum BuildingCategory {
    Logistics,
    Production,
    Power,
    Fluids,
    Storage,
    Defense,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct GunTurretPrototype {
    /// Maximum distance from the turret's footprint to a target, in tiles.
    pub range_tiles: u32,
    pub cooldown_ticks: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LaserTurretPrototype {
    /// Maximum distance from the turret's footprint to a target, in tiles.
    pub range_tiles: u32,
    pub damage: u32,
    pub cooldown_ticks: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EnemySpawnerPrototype {
    /// Upper bound on units alive at once that this spawner produced.
    pub max_alive_units: u32,
    /// Units kept alive near the spawner without pollution input.
    pub guard_units: u32,
    /// Ticks between free guard spawns while below `guard_units`.
    pub free_spawn_interval_ticks: u32,
    /// Absorbed pollution required to spawn one attacking unit, in
    /// milli-pollution-units.
    pub unit_spawn_pollution_cost_milli: u32,
    /// Pollution drained from the spawner's chunk each tick, in
    /// milli-pollution-units.
    pub pollution_absorption_per_tick_milli: u32,
    pub unit: UnitPrototype,
}

/// Combat stats of the mobile unit an enemy spawner produces.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct UnitPrototype {
    pub max_health: u32,
    pub damage: u32,
    pub attack_cooldown_ticks: u32,
    /// Movement speed in fixed-point position units per tick (1024 = one
    /// tile per tick).
    pub speed_fixed_per_tick: u32,
    /// Distance within which an idle unit acquires player targets, in tiles.
    pub aggro_radius_tiles: u32,
}

/// Deterministic enemy simulation tuning stored in the prototype catalog.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EnemyGameplayConfig {
    pub generated_colony_min_spawners: u8,
    pub generated_colony_max_spawners: u8,
    pub max_spawners_per_colony: u8,
    pub colony_spawner_radius_tiles: u8,
    pub outpost_growth_interval_ticks: u32,
    pub raid_staging_timeout_ticks: u32,
    pub raid_cooldown_ticks: u32,
    pub expansion_minimum_age_ticks: u32,
    pub expansion_interval_ticks: u32,
    pub expansion_retry_ticks: u32,
    pub expansion_min_distance_chunks: u8,
    pub expansion_max_distance_chunks: u8,
    pub expansion_candidate_limit: u16,
    pub expansion_colony_spacing_chunks: u8,
    pub expansion_player_spacing_tiles: u16,
    pub evolution_time_interval_ticks: u32,
    pub evolution_time_points: u16,
    pub evolution_pollution_units_per_point: u16,
    pub evolution_spawner_destroyed_points: u16,
    pub evolution_colony_destroyed_points: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidBoxPrototype {
    pub capacity_milliunits: u64,
    pub filter: Option<FluidId>,
    pub io: FluidBoxIo,
    pub connections: Vec<FluidConnectionPrototype>,
}

/// Recipe-facing role of a fluid box. Passive boxes (pipes, tanks) are
/// `InputOutput`; crafting machines declare which boxes feed fluid
/// ingredients and which receive fluid products. The role only affects
/// recipe matching; network equalization treats all boxes alike.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum FluidBoxIo {
    #[default]
    InputOutput,
    Input,
    Output,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidConnectionPrototype {
    pub local_offset: IVec2,
    pub side: FluidConnectionSide,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum FluidConnectionSide {
    North,
    East,
    South,
    West,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BurnerPrototype {
    pub energy_usage_watts: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct MiningDrillPrototype {
    pub mining_area: IVec2,
    pub ticks_per_item: u32,
}

/// Furnace crafting behavior. The speed fraction scales smelting recipe
/// times the same way assembler crafting speed does; the energy source
/// (burner or electric) comes from the entity's `burner` /
/// `electric_energy_source` sections.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FurnacePrototype {
    pub crafting_speed_numerator: u32,
    pub crafting_speed_denominator: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AssemblingMachinePrototype {
    pub crafting_speed_numerator: u32,
    pub crafting_speed_denominator: u32,
    pub input_slot_count: usize,
    pub output_slot_count: usize,
    /// Recipe category this machine crafts; recipes of other categories
    /// cannot be selected on it.
    #[serde(default = "default_assembler_crafting_category")]
    pub crafting_category: CraftingCategory,
}

fn default_assembler_crafting_category() -> CraftingCategory {
    CraftingCategory::Crafting
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TransportBeltPrototype {
    pub speed_subtiles_per_tick: u16,
    pub underground: Option<UndergroundBeltPrototype>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SplitterPrototype {
    pub speed_subtiles_per_tick: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct InserterPrototype {
    pub pickup_offset: IVec2,
    pub drop_offset: IVec2,
    pub pickup_ticks: u32,
    pub drop_ticks: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ElectricPolePrototype {
    pub supply_area_tiles: IVec2,
    pub wire_reach_tiles_x2: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ElectricEnergySourcePrototype {
    pub energy_usage_watts: u64,
    pub drain_watts: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SteamEnginePrototype {
    pub max_power_output_watts: u64,
    pub steam_consumption_per_second_milliunits: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BoilerPrototype {
    pub water_consumption_per_second_milliunits: u64,
    pub steam_output_per_second_milliunits: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct OffshorePumpPrototype {
    pub pumping_speed_per_second_milliunits: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PumpPrototype {
    pub pumping_speed_per_second_milliunits: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PumpjackPrototype {
    pub pumping_speed_per_second_milliunits: u64,
    /// Resource cell item this pumpjack must be placed over.
    pub resource_item: ItemId,
    /// Fluid produced into the pumpjack's output fluid box.
    pub output_fluid: FluidId,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct UndergroundBeltPrototype {
    pub part: UndergroundBeltPart,
    pub max_distance: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum UndergroundBeltPart {
    Entrance,
    Exit,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct UndergroundPipePrototype {
    pub part: UndergroundBeltPart,
    pub max_distance: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TilePrototype {
    pub id: TileId,
    pub name: String,
    pub collision_mask: CollisionMask,
    /// Pollution absorbed by one tile of this terrain, in
    /// milli-pollution-units per minute.
    pub pollution_absorption_per_minute_milli: u32,
    /// Base sRGB color `[r, g, b]` used by the front-end to paint this
    /// terrain. Inert data here (this crate has no rendering dependency); the
    /// renderer reads it to give each biome a visual identity instead of
    /// hard-coding terrain colors.
    pub color: [u8; 3],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TechnologyPrototype {
    pub id: TechnologyId,
    pub name: String,
    pub prerequisites: Vec<TechnologyId>,
    pub science_packs: Vec<ItemAmount>,
    pub required_units: u32,
    pub research_time_ticks: u32,
    pub effects: Vec<TechnologyEffect>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum TechnologyEffect {
    UnlockRecipe(RecipeId),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ItemAmount {
    pub item: ItemId,
    pub amount: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidAmount {
    pub fluid: FluidId,
    pub amount_milliunits: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum CraftingCategory {
    Manual,
    Smelting,
    Crafting,
    OilProcessing,
    Chemistry,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum EntityKind {
    ResourcePatch,
    Furnace,
    MiningDrill,
    AssemblingMachine,
    Inserter,
    TransportBelt,
    Splitter,
    Lab,
    Chest,
    ElectricPole,
    SteamEngine,
    Boiler,
    OffshorePump,
    Pump,
    Pumpjack,
    Pipe,
    StorageTank,
    Wall,
    GunTurret,
    LaserTurret,
    EnemySpawner,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CollisionMask {
    pub layers: Vec<CollisionLayer>,
}

/// Format version accepted for [`WorldGenerationConfig`]; configs declaring a
/// different version are rejected at load time instead of being misread.
///
/// Version 2 replaced the single weighted-band `terrain` selector with a
/// data-driven biome table classified from three independent climate channels.
/// Version 3 split resource patch density from resource selection weights.
pub const WORLD_GENERATION_FORMAT_VERSION: u32 = 3;

/// Data-driven world generation rules: terrain distribution, starting area,
/// and resource patch definitions. Loaded from the `world_generation` section
/// of a prototype catalog; a catalog without that section gets the empty
/// default, which generates a bare fallback-tile world without resources.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct WorldGenerationConfig {
    pub version: u32,
    pub starting_area: StartingAreaConfig,
    /// Independent fractal-noise parameters for the elevation, moisture, and
    /// temperature climate channels that drive biome classification.
    pub climate_noise: ClimateNoiseConfig,
    /// Ordered biome table: each tile is classified by finding the first biome
    /// whose elevation/moisture/temperature ranges all contain the sampled
    /// climate. Order encodes priority (specialized biomes first, catch-alls
    /// last); a tile matching no biome falls back to the first tile prototype.
    /// Tile collision behaviour derives from the tile prototype's collision
    /// mask.
    pub biomes: Vec<BiomeConfig>,
    pub patch_grid: ResourcePatchGridConfig,
    /// Distance-based reward for expanding outward; `None` keeps every patch
    /// at its base richness and radius.
    pub distance_scaling: Option<ResourceDistanceScalingConfig>,
    pub resources: Vec<ResourceGenerationConfig>,
    /// Enemy spawner placement rules; `None` generates a world without
    /// enemies.
    pub enemy_bases: Option<EnemyBaseGenerationConfig>,
}

impl Default for WorldGenerationConfig {
    fn default() -> Self {
        Self {
            version: WORLD_GENERATION_FORMAT_VERSION,
            starting_area: StartingAreaConfig {
                min_chunk: 0,
                max_chunk: 0,
            },
            climate_noise: ClimateNoiseConfig::default(),
            biomes: Vec::new(),
            patch_grid: ResourcePatchGridConfig {
                cell_size: 40,
                jitter: 16,
                edge_noise: 3,
                patch_chance_percent: 100,
            },
            distance_scaling: None,
            resources: Vec::new(),
            enemy_bases: None,
        }
    }
}

/// Deterministic per-chunk enemy spawner placement: each generated chunk
/// beyond `min_distance_tiles` from the origin rolls `frequency_percent` for
/// one spawner at a seed-derived position inside the chunk.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EnemyBaseGenerationConfig {
    pub spawner_entity: EntityPrototypeId,
    /// Chance (0-100) that an eligible chunk contains a spawner.
    pub frequency_percent: u8,
    /// Chunks whose center is closer to the origin than this stay clear.
    pub min_distance_tiles: u32,
}

/// Inclusive chunk range generated up front when a world is created.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct StartingAreaConfig {
    pub min_chunk: i32,
    pub max_chunk: i32,
}

/// One biome in the classification table: a terrain tile plus the inclusive
/// climate box it occupies. A tile is classified into the first biome (in
/// declaration order) whose three ranges all contain the sampled climate.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BiomeConfig {
    pub tile: TileId,
    pub elevation: ClimateRange,
    pub moisture: ClimateRange,
    pub temperature: ClimateRange,
}

/// Half-open percent range `[min, max)` (`0..=100`) matched against a climate
/// channel sample. `min` is inclusive, `max` exclusive, so adjacent biomes can
/// tile the range without overlap.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ClimateRange {
    pub min: u8,
    pub max: u8,
}

impl ClimateRange {
    /// Whether `percent` (`0..=100`) falls in `[min, max)`.
    pub fn contains(self, percent: u8) -> bool {
        percent >= self.min && percent < self.max
    }
}

/// Independent fractal-noise parameters for the three climate channels that
/// drive biome selection. Each channel is sampled from its own seed-salted
/// noise field so elevation, moisture, and temperature vary independently.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ClimateNoiseConfig {
    pub elevation: TerrainNoiseConfig,
    pub moisture: TerrainNoiseConfig,
    pub temperature: TerrainNoiseConfig,
}

/// Fractal value-noise parameters for one climate channel. `scale` is the base
/// wavelength in tiles of the lowest-frequency octave; each further octave
/// halves the wavelength and amplitude, adding finer detail such as ragged
/// coastlines.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TerrainNoiseConfig {
    pub scale: u32,
    pub octaves: u32,
}

impl Default for TerrainNoiseConfig {
    fn default() -> Self {
        Self {
            scale: 32,
            octaves: 3,
        }
    }
}

/// Poisson-like placement grid for resource patch centers: one candidate
/// center per `cell_size` tiles, offset by up to `jitter` tiles, with patch
/// edges roughened by up to `edge_noise` tiles.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResourcePatchGridConfig {
    pub cell_size: i32,
    pub jitter: i32,
    pub edge_noise: i32,
    /// Chance (0-100) that a grid cell contains a non-starting resource patch.
    pub patch_chance_percent: u8,
}

/// Linear distance scaling for grid-placed resource patches, rewarding
/// expansion away from the spawn: for every `interval_tiles` of distance
/// between a patch center and the world origin, the patch gains
/// `richness_bonus_percent` percent of its base richness and
/// `radius_bonus_tiles` tiles of radius. The radius bonus is capped at
/// `max_radius_bonus_tiles` so chunk generation can bound how far away a
/// patch center may still reach into a chunk. Starting patches are spawn
/// guarantees and are never scaled.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResourceDistanceScalingConfig {
    pub interval_tiles: u32,
    pub richness_bonus_percent: u32,
    pub radius_bonus_tiles: u8,
    pub max_radius_bonus_tiles: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResourceGenerationConfig {
    pub resource_item: ItemId,
    pub extraction: ResourceExtraction,
    /// Relative weight used to select this resource when a grid cell spawns a
    /// patch. A weight of zero excludes it from random patch selection.
    pub selection_weight: u32,
    pub radius: i32,
    pub richness: u32,
    /// Guaranteed patch center near the origin so starter worlds always
    /// contain the resource; offsets are in tiles.
    pub starting_patch: Option<IVec2>,
}

/// How a generated resource cell is extracted. `Solid` resources are minable
/// by drills and the player; `Fluid` resources are extracted by pumpjacks and
/// excluded from mining. This is authoritative for minability regardless of
/// which machine prototypes exist.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum ResourceExtraction {
    Solid,
    Fluid,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum CollisionLayer {
    Ground,
    Water,
    Resource,
    Building,
    Transport,
}
