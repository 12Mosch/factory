use glam::IVec2;
use serde::{Deserialize, Serialize};

use crate::ids::{EntityPrototypeId, FluidId, ItemId, RecipeId, TechnologyId, TileId};

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
    pub inventory_slot_count: Option<usize>,
    pub burner: Option<BurnerPrototype>,
    pub mining_drill: Option<MiningDrillPrototype>,
    pub assembling_machine: Option<AssemblingMachinePrototype>,
    pub transport_belt: Option<TransportBeltPrototype>,
    pub splitter: Option<SplitterPrototype>,
    pub inserter: Option<InserterPrototype>,
    pub electric_pole: Option<ElectricPolePrototype>,
    pub electric_energy_source: Option<ElectricEnergySourcePrototype>,
    pub steam_engine: Option<SteamEnginePrototype>,
    pub boiler: Option<BoilerPrototype>,
    pub offshore_pump: Option<OffshorePumpPrototype>,
    pub pumpjack: Option<PumpjackPrototype>,
    pub fluid_boxes: Vec<FluidBoxPrototype>,
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
pub struct TilePrototype {
    pub id: TileId,
    pub name: String,
    pub collision_mask: CollisionMask,
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
    Pumpjack,
    Pipe,
    StorageTank,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CollisionMask {
    pub layers: Vec<CollisionLayer>,
}

/// Format version accepted for [`WorldGenerationConfig`]; configs declaring a
/// different version are rejected at load time instead of being misread.
pub const WORLD_GENERATION_FORMAT_VERSION: u32 = 1;

/// Data-driven world generation rules: terrain distribution, starting area,
/// and resource patch definitions. Loaded from the `world_generation` section
/// of a prototype catalog; a catalog without that section gets the empty
/// default, which generates a bare fallback-tile world without resources.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct WorldGenerationConfig {
    pub version: u32,
    pub starting_area: StartingAreaConfig,
    /// Weighted terrain layers mapped onto a coherent noise field: each
    /// layer's weight is its share of the noise value range, assigned in
    /// declaration order from the lowest values upward (so an early "water"
    /// layer fills basins, a late "grass" layer covers highlands). Tile
    /// collision behaviour derives from the tile prototype's collision mask.
    pub terrain: Vec<TerrainLayerConfig>,
    pub terrain_noise: TerrainNoiseConfig,
    pub patch_grid: ResourcePatchGridConfig,
    /// Distance-based reward for expanding outward; `None` keeps every patch
    /// at its base richness and radius.
    pub distance_scaling: Option<ResourceDistanceScalingConfig>,
    pub resources: Vec<ResourceGenerationConfig>,
}

impl Default for WorldGenerationConfig {
    fn default() -> Self {
        Self {
            version: WORLD_GENERATION_FORMAT_VERSION,
            starting_area: StartingAreaConfig {
                min_chunk: 0,
                max_chunk: 0,
            },
            terrain: Vec::new(),
            terrain_noise: TerrainNoiseConfig::default(),
            patch_grid: ResourcePatchGridConfig {
                cell_size: 40,
                jitter: 16,
                edge_noise: 3,
            },
            distance_scaling: None,
            resources: Vec::new(),
        }
    }
}

/// Inclusive chunk range generated up front when a world is created.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct StartingAreaConfig {
    pub min_chunk: i32,
    pub max_chunk: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TerrainLayerConfig {
    pub tile: TileId,
    pub weight: u32,
}

/// Fractal value-noise parameters for the terrain field. `scale` is the base
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
    /// Chance (0-100) that a grid cell spawns a patch of this resource.
    pub frequency_percent: u8,
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
