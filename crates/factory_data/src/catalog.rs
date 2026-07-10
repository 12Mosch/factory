use crate::ids::{EntityPrototypeId, FluidId, ItemId, RecipeId, TechnologyId, TileId};
use crate::model::{
    EntityPrototype, FluidPrototype, ItemPrototype, RecipePrototype, TechnologyPrototype,
    TilePrototype, WorldGenerationConfig,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PrototypeCatalog {
    pub items: Vec<ItemPrototype>,
    pub fluids: Vec<FluidPrototype>,
    pub recipes: Vec<RecipePrototype>,
    pub entities: Vec<EntityPrototype>,
    pub tiles: Vec<TilePrototype>,
    pub technologies: Vec<TechnologyPrototype>,
    pub world_generation: WorldGenerationConfig,
}

/// Generates a typed lookup method on [`PrototypeCatalog`]. Ids double as
/// vector indices, but the id check guards against a stale id being used
/// against a catalog it was not issued from.
macro_rules! catalog_accessor {
    ($fn_name:ident, $field:ident, $id_ty:ty, $proto_ty:ty) => {
        pub fn $fn_name(&self, id: $id_ty) -> Option<&$proto_ty> {
            self.$field.get(id.index()).filter(|p| p.id == id)
        }
    };
}

impl PrototypeCatalog {
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    catalog_accessor!(item, items, ItemId, ItemPrototype);
    catalog_accessor!(fluid, fluids, FluidId, FluidPrototype);
    catalog_accessor!(recipe, recipes, RecipeId, RecipePrototype);
    catalog_accessor!(entity, entities, EntityPrototypeId, EntityPrototype);
    catalog_accessor!(tile, tiles, TileId, TilePrototype);
    catalog_accessor!(technology, technologies, TechnologyId, TechnologyPrototype);
}
