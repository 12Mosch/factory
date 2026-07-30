mod base_ids;
mod catalog;

pub mod error;
pub mod ids;
pub mod loader;
pub mod model;
pub mod prelude;
mod raw;
mod validation;

pub use base_ids::{
    BaseFluidIds, BaseItemIds, BasePrototypeIds, BaseTileIds, entity_prototype_id_by_name,
    fluid_id_by_name, item_id_by_name, recipe_id_by_name, technology_id_by_name, tile_id_by_name,
};
pub use catalog::PrototypeCatalog;
pub use error::PrototypeLoadError;
pub use ids::{
    EntityPrototypeId, FluidId, ItemId, RecipeId, TechnologyId, TileId, VirtualSignalId,
};
pub use model::{
    AccumulatorPrototype, AmmoPrototype, ArmorPrototype, AssemblingMachinePrototype,
    BeaconPrototype, BiomeConfig, BoilerPrototype, BuildingCategory, BurnerPrototype,
    CircuitConnectorPrototype, CircuitPortLayout, ClimateNoiseConfig, ClimateRange, CollisionLayer,
    CollisionMask, CombinatorKind, CombinatorPrototype, ConnectionSide, CraftingCategory,
    DamageResistancePrototype, DamageType, DayNightCycleConfig, EdgeConnectionPrototype,
    ElectricEnergySourcePrototype, ElectricPolePrototype, EnemyBaseGenerationConfig,
    EnemyGameplayConfig, EnemySpawnerPrototype, EntityKind, EntityPrototype,
    EquipmentEffectPrototype, EquipmentPrototype, FluidAmount, FluidBoxIo, FluidBoxPrototype,
    FluidConnectionPrototype, FluidConnectionSide, FluidPrototype, FurnacePrototype,
    GunTurretPrototype, HEAT_AMBIENT_TEMPERATURE_DEGREES, HeatBufferPrototype,
    HeatEnergySourcePrototype, InserterPrototype, ItemAmount, ItemPrototype, LaserTurretPrototype,
    LocomotivePrototype, LogisticChestMode, LogisticChestPrototype, MiningDrillPrototype,
    ModuleEffectPrototype, NuclearReactorPrototype, OffshorePumpPrototype, POSITION_SCALE,
    PumpPrototype, PumpjackPrototype, RadarPrototype, RailCurvePrototype, RailEndPrototype,
    RailHeading, RailPiecePrototype, RailPointPrototype, RailSignalKind, RecipePrototype,
    RepairToolPrototype, ResourceDistanceScalingConfig, ResourceExtraction,
    ResourceGenerationConfig, ResourcePatchGridConfig, RoboportPrototype, RobotKind,
    RobotPrototype, RollingStockPrototype, SolarPanelPrototype, SplitterPrototype,
    StartingAreaConfig, SteamEnginePrototype, TechnologyEffect, TechnologyPrototype,
    TerrainNoiseConfig, TilePlacementPrototype, TilePrototype, TransportBeltPrototype,
    UndergroundBeltPart, UndergroundBeltPrototype, UndergroundPipePrototype, UnitPrototype,
    VirtualSignalKind, VirtualSignalPrototype, WORLD_GENERATION_FORMAT_VERSION,
    WorldGenerationConfig, roboport_coverage_bounds,
};
