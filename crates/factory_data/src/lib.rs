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
    BaseItemIds, BasePrototypeIds, BaseTileIds, entity_prototype_id_by_name, item_id_by_name,
    recipe_id_by_name, technology_id_by_name, tile_id_by_name,
};
pub use catalog::PrototypeCatalog;
pub use error::PrototypeLoadError;
pub use ids::{EntityPrototypeId, ItemId, RecipeId, TechnologyId, TileId};
pub use model::{
    AssemblingMachinePrototype, BurnerPrototype, CollisionLayer, CollisionMask, CraftingCategory,
    EntityKind, EntityPrototype, ItemAmount, ItemPrototype, MiningDrillPrototype, RecipePrototype,
    SplitterPrototype, TechnologyEffect, TechnologyPrototype, TilePrototype,
    TransportBeltPrototype, UndergroundBeltPart, UndergroundBeltPrototype,
};
