use crate::model::{
    EntityPrototype, ItemPrototype, RecipePrototype, TechnologyPrototype, TilePrototype,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
