use serde::Deserialize;

use crate::model::{
    AccumulatorPrototype, AmmoPrototype, ArmorPrototype, AssemblingMachinePrototype,
    BeaconPrototype, BoilerPrototype, BuildingCategory, BurnerPrototype, CircuitConnectorPrototype,
    CombinatorPrototype, ConnectionSide, CraftingCategory, ElectricEnergySourcePrototype,
    EnemyGameplayConfig, EntityKind, EquipmentPrototype, FluidBoxIo, FurnacePrototype,
    GunTurretPrototype, HeatEnergySourcePrototype, LaserTurretPrototype, LogisticChestPrototype,
    ModuleEffectPrototype, NuclearReactorPrototype, OffshorePumpPrototype, PumpPrototype,
    RadarPrototype, RailPiecePrototype, RepairToolPrototype, ResourceExtraction, RoboportPrototype,
    RobotPrototype, RollingStockPrototype, SolarPanelPrototype, SplitterPrototype,
    SteamEnginePrototype, TransportBeltPrototype, UndergroundPipePrototype, UnitPrototype,
    VirtualSignalKind,
};
use crate::validation::RawPrototype;

#[derive(Debug, Deserialize)]
pub(crate) struct RawPrototypeCatalog {
    pub(crate) items: Vec<RawItemPrototype>,
    #[serde(default)]
    pub(crate) fluids: Vec<RawFluidPrototype>,
    pub(crate) recipes: Vec<RawRecipePrototype>,
    pub(crate) entities: Vec<RawEntityPrototype>,
    pub(crate) tiles: Vec<RawTilePrototype>,
    #[serde(default)]
    pub(crate) technologies: Vec<RawTechnologyPrototype>,
    #[serde(default)]
    pub(crate) virtual_signals: Vec<RawVirtualSignalPrototype>,
    #[serde(default)]
    pub(crate) world_generation: Option<RawWorldGenerationConfig>,
    #[serde(default)]
    pub(crate) enemy_gameplay: Option<EnemyGameplayConfig>,
    #[serde(default)]
    pub(crate) day_night_cycle: Option<crate::model::DayNightCycleConfig>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawWorldGenerationConfig {
    pub(crate) version: u32,
    pub(crate) starting_area: RawStartingArea,
    pub(crate) climate_noise: RawClimateNoise,
    pub(crate) biomes: Vec<RawBiomeConfig>,
    pub(crate) patch_grid: RawResourcePatchGrid,
    #[serde(default)]
    pub(crate) distance_scaling: Option<RawResourceDistanceScaling>,
    #[serde(default)]
    pub(crate) resources: Vec<RawResourceGeneration>,
    #[serde(default)]
    pub(crate) enemy_bases: Option<RawEnemyBaseGeneration>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawEnemyBaseGeneration {
    pub(crate) spawner_entity: String,
    pub(crate) frequency_percent: u8,
    pub(crate) min_distance_tiles: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawStartingArea {
    pub(crate) min_chunk: i32,
    pub(crate) max_chunk: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawClimateNoise {
    pub(crate) elevation: RawTerrainNoise,
    pub(crate) moisture: RawTerrainNoise,
    pub(crate) temperature: RawTerrainNoise,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawBiomeConfig {
    pub(crate) tile: String,
    pub(crate) elevation: RawClimateRange,
    pub(crate) moisture: RawClimateRange,
    pub(crate) temperature: RawClimateRange,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawClimateRange {
    pub(crate) min: u8,
    pub(crate) max: u8,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawTerrainNoise {
    pub(crate) scale: u32,
    pub(crate) octaves: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawResourcePatchGrid {
    pub(crate) cell_size: i32,
    pub(crate) jitter: i32,
    pub(crate) edge_noise: i32,
    #[serde(default = "default_patch_chance_percent")]
    pub(crate) patch_chance_percent: u8,
}

fn default_patch_chance_percent() -> u8 {
    100
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawResourceDistanceScaling {
    pub(crate) interval_tiles: u32,
    pub(crate) richness_bonus_percent: u32,
    pub(crate) radius_bonus_tiles: u8,
    pub(crate) max_radius_bonus_tiles: u8,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawResourceGeneration {
    pub(crate) item: String,
    pub(crate) extraction: ResourceExtraction,
    #[serde(alias = "frequency_percent")]
    pub(crate) selection_weight: u32,
    pub(crate) radius: i32,
    pub(crate) richness: u32,
    #[serde(default)]
    pub(crate) starting_patch: Option<RawIVec2>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawItemPrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) stack_size: u16,
    pub(crate) fuel_value_joules: Option<u64>,
    /// Item name of the residue left after burning one unit, resolved against
    /// the item group once every item is known.
    #[serde(default)]
    pub(crate) burnt_result: Option<String>,
    pub(crate) ammo: Option<AmmoPrototype>,
    pub(crate) repair: Option<RepairToolPrototype>,
    pub(crate) armor: Option<ArmorPrototype>,
    pub(crate) equipment: Option<EquipmentPrototype>,
    pub(crate) module_effect: Option<ModuleEffectPrototype>,
    #[serde(default)]
    pub(crate) place_as_tile: Option<RawTilePlacement>,
    #[serde(default)]
    pub(crate) robot: Option<RobotPrototype>,
    /// Products returned when one unit of this item is launched as rocket
    /// cargo. `None` means the item is not launchable; an explicitly empty
    /// definition is rejected during catalog loading.
    #[serde(default)]
    pub(crate) launch_products: Option<Vec<RawItemAmount>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawTilePlacement {
    /// Tile name, resolved to a `TileId` against the tile group.
    pub(crate) tile: String,
    #[serde(default)]
    pub(crate) fills_water: bool,
    #[serde(default = "default_tile_building_category")]
    pub(crate) building_category: BuildingCategory,
    pub(crate) building_menu_order: u16,
}

const fn default_tile_building_category() -> BuildingCategory {
    BuildingCategory::Terrain
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawFluidPrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawRecipePrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) category: CraftingCategory,
    pub(crate) crafting_time_ticks: u32,
    #[serde(default)]
    pub(crate) ingredients: Vec<RawItemAmount>,
    #[serde(default)]
    pub(crate) products: Vec<RawItemAmount>,
    #[serde(default)]
    pub(crate) fluid_ingredients: Vec<RawFluidAmount>,
    #[serde(default)]
    pub(crate) fluid_products: Vec<RawFluidAmount>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawEntityPrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) entity_kind: EntityKind,
    pub(crate) size: RawIVec2,
    pub(crate) collision_mask: RawCollisionMask,
    pub(crate) build_item: Option<String>,
    #[serde(default)]
    pub(crate) building_category: Option<BuildingCategory>,
    #[serde(default)]
    pub(crate) building_menu_order: Option<u16>,
    pub(crate) inventory_slot_count: Option<usize>,
    #[serde(default)]
    pub(crate) module_slot_count: usize,
    pub(crate) beacon: Option<BeaconPrototype>,
    pub(crate) burner: Option<BurnerPrototype>,
    pub(crate) mining_drill: Option<RawMiningDrillPrototype>,
    pub(crate) furnace: Option<FurnacePrototype>,
    pub(crate) assembling_machine: Option<AssemblingMachinePrototype>,
    #[serde(default)]
    pub(crate) rocket_silo: Option<RawRocketSiloPrototype>,
    pub(crate) transport_belt: Option<TransportBeltPrototype>,
    pub(crate) splitter: Option<SplitterPrototype>,
    pub(crate) inserter: Option<RawInserterPrototype>,
    pub(crate) electric_pole: Option<RawElectricPolePrototype>,
    pub(crate) electric_energy_source: Option<ElectricEnergySourcePrototype>,
    pub(crate) steam_engine: Option<SteamEnginePrototype>,
    pub(crate) solar_panel: Option<SolarPanelPrototype>,
    pub(crate) accumulator: Option<AccumulatorPrototype>,
    pub(crate) radar: Option<RadarPrototype>,
    pub(crate) boiler: Option<BoilerPrototype>,
    pub(crate) offshore_pump: Option<OffshorePumpPrototype>,
    pub(crate) pump: Option<PumpPrototype>,
    pub(crate) pumpjack: Option<RawPumpjackPrototype>,
    pub(crate) underground_pipe: Option<UndergroundPipePrototype>,
    #[serde(default)]
    pub(crate) fluid_boxes: Vec<RawFluidBoxPrototype>,
    #[serde(default)]
    pub(crate) heat_buffer: Option<RawHeatBufferPrototype>,
    #[serde(default)]
    pub(crate) heat_energy_source: Option<HeatEnergySourcePrototype>,
    #[serde(default)]
    pub(crate) nuclear_reactor: Option<NuclearReactorPrototype>,
    #[serde(default)]
    pub(crate) roboport: Option<RoboportPrototype>,
    #[serde(default)]
    pub(crate) logistic_chest: Option<LogisticChestPrototype>,
    pub(crate) max_health: Option<u32>,
    pub(crate) pollution_per_minute_milli: Option<u32>,
    pub(crate) gun_turret: Option<GunTurretPrototype>,
    pub(crate) laser_turret: Option<LaserTurretPrototype>,
    pub(crate) enemy_spawner: Option<RawEnemySpawnerPrototype>,
    #[serde(default)]
    pub(crate) circuit_connector: Option<CircuitConnectorPrototype>,
    #[serde(default)]
    pub(crate) combinator: Option<CombinatorPrototype>,
    /// Sub-tile rail geometry. Every field is a plain scalar or enum, so the
    /// declared form is already the resolved one and needs no raw counterpart.
    #[serde(default)]
    pub(crate) rail_piece: Option<RailPiecePrototype>,
    /// Rolling-stock motion metadata. Plain scalars like `rail_piece`, so the
    /// declared form is already the resolved one.
    #[serde(default)]
    pub(crate) rolling_stock: Option<RollingStockPrototype>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawVirtualSignalPrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) kind: VirtualSignalKind,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawEnemySpawnerPrototype {
    pub(crate) max_alive_units: u32,
    pub(crate) guard_units: u32,
    pub(crate) free_spawn_interval_ticks: u32,
    pub(crate) unit_spawn_pollution_cost_milli: u32,
    pub(crate) pollution_absorption_per_tick_milli: u32,
    pub(crate) unit: UnitPrototype,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawPumpjackPrototype {
    pub(crate) pumping_speed_per_second_milliunits: u64,
    pub(crate) resource_item: String,
    pub(crate) output_fluid: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawFluidBoxPrototype {
    pub(crate) capacity_milliunits: u64,
    pub(crate) filter: Option<String>,
    #[serde(default)]
    pub(crate) io: FluidBoxIo,
    pub(crate) connections: Vec<RawFluidConnectionPrototype>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawEdgeConnectionPrototype {
    pub(crate) local_offset: RawIVec2,
    pub(crate) side: ConnectionSide,
}

pub(crate) type RawFluidConnectionPrototype = RawEdgeConnectionPrototype;

#[derive(Debug, Deserialize)]
pub(crate) struct RawHeatBufferPrototype {
    pub(crate) specific_heat_joules_per_degree: u64,
    pub(crate) max_temperature_degrees: u32,
    pub(crate) connections: Vec<RawEdgeConnectionPrototype>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawMiningDrillPrototype {
    pub(crate) mining_area: RawIVec2,
    pub(crate) ticks_per_item: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawRocketSiloPrototype {
    pub(crate) crafting_speed_numerator: u32,
    pub(crate) crafting_speed_denominator: u32,
    pub(crate) input_slot_count: usize,
    pub(crate) parts_per_rocket: u32,
    pub(crate) output_slot_count: usize,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawInserterPrototype {
    pub(crate) pickup_offset: RawIVec2,
    pub(crate) drop_offset: RawIVec2,
    pub(crate) pickup_ticks: u32,
    pub(crate) drop_ticks: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawElectricPolePrototype {
    pub(crate) supply_area_tiles: RawIVec2,
    pub(crate) wire_reach_tiles_x2: u16,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawTilePrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) collision_mask: RawCollisionMask,
    #[serde(default)]
    pub(crate) pollution_absorption_per_minute_milli: u32,
    /// Base sRGB color `[r, g, b]`; defaults to magenta so any tile missing a
    /// color is glaringly visible rather than silently invisible.
    #[serde(default = "default_tile_color")]
    pub(crate) color: [u8; 3],
    /// Walking speed percentage; defaults to the unmodified base speed.
    #[serde(default = "default_walking_speed_percent")]
    pub(crate) walking_speed_percent: u16,
}

fn default_tile_color() -> [u8; 3] {
    [255, 0, 255]
}

const fn default_walking_speed_percent() -> u16 {
    100
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawTechnologyPrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) prerequisites: Vec<String>,
    pub(crate) science_packs: Vec<RawItemAmount>,
    pub(crate) required_units: u64,
    #[serde(default)]
    pub(crate) level_model: RawTechnologyLevelModel,
    pub(crate) research_time_ticks: u32,
    pub(crate) effects: Vec<RawTechnologyEffect>,
}

#[derive(Debug, Deserialize)]
pub(crate) enum RawTechnologyEffect {
    UnlockRecipe(String),
    MiningDrillProductivity { bonus_permyriad: u32 },
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub(crate) enum RawTechnologyLevelModel {
    #[default]
    Finite,
    Repeatable {
        cost_curve: RawTechnologyCostCurve,
    },
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub(crate) enum RawTechnologyCostCurve {
    Linear { additional_units_per_level: u64 },
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawItemAmount {
    pub(crate) item: String,
    pub(crate) amount: u16,
}

/// Fluid quantity in whole fluid units; converted to milliunits on load.
#[derive(Debug, Deserialize)]
pub(crate) struct RawFluidAmount {
    pub(crate) fluid: String,
    pub(crate) amount: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawIVec2 {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawCollisionMask {
    pub(crate) layers: Vec<String>,
}

impl RawPrototype for RawItemPrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl RawPrototype for RawRecipePrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl RawPrototype for RawFluidPrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl RawPrototype for RawEntityPrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl RawPrototype for RawTilePrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl RawPrototype for RawTechnologyPrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl RawPrototype for RawVirtualSignalPrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}
