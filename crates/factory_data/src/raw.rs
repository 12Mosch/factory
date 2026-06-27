use serde::Deserialize;

use crate::model::{AssemblingMachinePrototype, BurnerPrototype, CraftingCategory, EntityKind};
use crate::validation::RawPrototype;

#[derive(Debug, Deserialize)]
pub(crate) struct RawPrototypeCatalog {
    pub(crate) items: Vec<RawItemPrototype>,
    pub(crate) recipes: Vec<RawRecipePrototype>,
    pub(crate) entities: Vec<RawEntityPrototype>,
    pub(crate) tiles: Vec<RawTilePrototype>,
    #[serde(default)]
    pub(crate) technologies: Vec<RawTechnologyPrototype>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawItemPrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) stack_size: u16,
    pub(crate) fuel_value_joules: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawRecipePrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) category: CraftingCategory,
    pub(crate) crafting_time_ticks: u32,
    pub(crate) ingredients: Vec<RawItemAmount>,
    pub(crate) products: Vec<RawItemAmount>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawEntityPrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) entity_kind: EntityKind,
    pub(crate) size: RawIVec2,
    pub(crate) collision_mask: RawCollisionMask,
    pub(crate) inventory_slot_count: Option<usize>,
    pub(crate) burner: Option<BurnerPrototype>,
    pub(crate) mining_drill: Option<RawMiningDrillPrototype>,
    pub(crate) assembling_machine: Option<AssemblingMachinePrototype>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawMiningDrillPrototype {
    pub(crate) mining_area: RawIVec2,
    pub(crate) ticks_per_item: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawTilePrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) collision_mask: RawCollisionMask,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawTechnologyPrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) prerequisites: Vec<String>,
    pub(crate) science_packs: Vec<RawItemAmount>,
    pub(crate) required_units: u32,
    pub(crate) research_time_ticks: u32,
    pub(crate) effects: Vec<RawTechnologyEffect>,
}

#[derive(Debug, Deserialize)]
pub(crate) enum RawTechnologyEffect {
    UnlockRecipe(String),
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawItemAmount {
    pub(crate) item: String,
    pub(crate) amount: u16,
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
