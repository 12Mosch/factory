mod catalog;

pub mod error;
pub mod ids;
pub mod loader;
pub mod model;
pub mod prelude;
mod raw;
mod validation;

pub use catalog::PrototypeCatalog;
pub use error::PrototypeLoadError;
pub use ids::{EntityPrototypeId, ItemId, RecipeId, TechnologyId, TileId};
pub use model::{
    AssemblingMachinePrototype, BurnerPrototype, CollisionLayer, CollisionMask, CraftingCategory,
    EntityKind, EntityPrototype, ItemAmount, ItemPrototype, MiningDrillPrototype, RecipePrototype,
    TechnologyEffect, TechnologyPrototype, TilePrototype,
};
