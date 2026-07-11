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
pub use ids::{EntityPrototypeId, FluidId, ItemId, RecipeId, TechnologyId, TileId};
pub use model::{
    AmmoPrototype, AssemblingMachinePrototype, BoilerPrototype, BuildingCategory, BurnerPrototype,
    CollisionLayer, CollisionMask, CraftingCategory, ElectricEnergySourcePrototype,
    ElectricPolePrototype, EnemyBaseGenerationConfig, EnemyGameplayConfig, EnemySpawnerPrototype,
    EntityKind, EntityPrototype, FluidAmount, FluidBoxIo, FluidBoxPrototype,
    FluidConnectionPrototype, FluidConnectionSide, FluidPrototype, GunTurretPrototype,
    InserterPrototype, ItemAmount, ItemPrototype, MiningDrillPrototype, OffshorePumpPrototype,
    PumpjackPrototype, RecipePrototype, RepairToolPrototype, ResourceDistanceScalingConfig,
    ResourceExtraction, ResourceGenerationConfig, ResourcePatchGridConfig, SplitterPrototype,
    StartingAreaConfig, SteamEnginePrototype, TechnologyEffect, TechnologyPrototype,
    TerrainLayerConfig, TerrainNoiseConfig, TilePrototype, TransportBeltPrototype,
    UndergroundBeltPart, UndergroundBeltPrototype, UnitPrototype, WORLD_GENERATION_FORMAT_VERSION,
    WorldGenerationConfig,
};
