use crate::model::{
    EntityPrototype, ItemPrototype, RecipePrototype, TechnologyPrototype, TilePrototype,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PrototypeCatalog {
    pub items: Vec<ItemPrototype>,
    pub recipes: Vec<RecipePrototype>,
    pub entities: Vec<EntityPrototype>,
    pub tiles: Vec<TilePrototype>,
    pub technologies: Vec<TechnologyPrototype>,
}

impl PrototypeCatalog {
    pub fn item_count(&self) -> usize {
        self.items.len()
    }
}
