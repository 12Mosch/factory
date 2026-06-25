#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrototypeCatalog {
    pub items: Vec<ItemPrototype>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemPrototype {
    pub id: String,
}

impl PrototypeCatalog {
    pub fn test_catalog() -> Self {
        Self {
            items: vec![ItemPrototype {
                id: "iron_ore".to_string(),
            }],
        }
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }
}
