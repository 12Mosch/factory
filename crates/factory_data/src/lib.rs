use glam::IVec2;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u16);

        impl $name {
            pub const fn new(raw: u16) -> Self {
                Self(raw)
            }

            pub const fn raw(self) -> u16 {
                self.0
            }

            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

id_type!(ItemId);
id_type!(RecipeId);
id_type!(EntityPrototypeId);
id_type!(TileId);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PrototypeCatalog {
    pub items: Vec<ItemPrototype>,
    pub recipes: Vec<RecipePrototype>,
    pub entities: Vec<EntityPrototype>,
    pub tiles: Vec<TilePrototype>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ItemPrototype {
    pub id: ItemId,
    pub name: String,
    pub stack_size: u16,
    pub fuel_value_joules: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RecipePrototype {
    pub id: RecipeId,
    pub name: String,
    pub category: CraftingCategory,
    pub crafting_time_ticks: u32,
    pub ingredients: Vec<ItemAmount>,
    pub products: Vec<ItemAmount>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EntityPrototype {
    pub id: EntityPrototypeId,
    pub name: String,
    pub entity_kind: EntityKind,
    pub size: IVec2,
    pub collision_mask: CollisionMask,
    pub inventory_slot_count: Option<usize>,
    pub burner: Option<BurnerPrototype>,
    pub mining_drill: Option<MiningDrillPrototype>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash)]
pub struct BurnerPrototype {
    pub energy_usage_watts: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MiningDrillPrototype {
    pub mining_area: IVec2,
    pub ticks_per_item: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TilePrototype {
    pub id: TileId,
    pub name: String,
    pub collision_mask: CollisionMask,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ItemAmount {
    pub item: ItemId,
    pub amount: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
pub enum CraftingCategory {
    Manual,
    Smelting,
    Crafting,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
pub enum EntityKind {
    ResourcePatch,
    Furnace,
    MiningDrill,
    AssemblingMachine,
    Inserter,
    TransportBelt,
    Lab,
    Chest,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CollisionMask {
    pub layers: Vec<CollisionLayer>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CollisionLayer {
    Ground,
    Water,
    Resource,
    Building,
    Transport,
}

#[derive(Debug)]
pub enum PrototypeLoadError {
    Io(std::io::Error),
    Ron(ron::error::SpannedError),
    DuplicateId {
        group: &'static str,
        id: u16,
    },
    DuplicateName {
        group: &'static str,
        name: String,
    },
    NonContiguousIds {
        group: &'static str,
        expected: u16,
        actual: u16,
    },
    MissingItemReference {
        recipe: String,
        item: String,
    },
    InvalidCollisionLayer {
        owner: String,
        layer: String,
    },
}

impl PrototypeCatalog {
    pub fn load_base() -> Result<Self, PrototypeLoadError> {
        Self::from_ron_str(include_str!("../data/base.ron"))
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, PrototypeLoadError> {
        let data = std::fs::read_to_string(path).map_err(PrototypeLoadError::Io)?;
        Self::from_ron_str(&data)
    }

    pub fn from_ron_str(data: &str) -> Result<Self, PrototypeLoadError> {
        let raw: RawPrototypeCatalog = ron::from_str(data).map_err(PrototypeLoadError::Ron)?;

        let mut items = raw.items;
        validate_group(&mut items, "items")?;
        let mut recipes = raw.recipes;
        validate_group(&mut recipes, "recipes")?;
        let mut entities = raw.entities;
        validate_group(&mut entities, "entities")?;
        let mut tiles = raw.tiles;
        validate_group(&mut tiles, "tiles")?;

        let mut item_ids_by_name = HashMap::with_capacity(items.len());
        let loaded_items = items
            .into_iter()
            .map(|item| {
                let id = ItemId::new(item.id);
                item_ids_by_name.insert(item.name.clone(), id);
                ItemPrototype {
                    id,
                    name: item.name,
                    stack_size: item.stack_size,
                    fuel_value_joules: item.fuel_value_joules,
                }
            })
            .collect();

        let loaded_recipes = recipes
            .into_iter()
            .map(|recipe| {
                let recipe_name = recipe.name.clone();
                Ok(RecipePrototype {
                    id: RecipeId::new(recipe.id),
                    name: recipe.name,
                    category: recipe.category,
                    crafting_time_ticks: recipe.crafting_time_ticks,
                    ingredients: resolve_item_amounts(
                        &recipe_name,
                        recipe.ingredients,
                        &item_ids_by_name,
                    )?,
                    products: resolve_item_amounts(
                        &recipe_name,
                        recipe.products,
                        &item_ids_by_name,
                    )?,
                })
            })
            .collect::<Result<_, PrototypeLoadError>>()?;

        let loaded_entities = entities
            .into_iter()
            .map(|entity| {
                let name = entity.name;
                Ok(EntityPrototype {
                    id: EntityPrototypeId::new(entity.id),
                    name: name.clone(),
                    entity_kind: entity.entity_kind,
                    size: IVec2::new(entity.size.x, entity.size.y),
                    collision_mask: resolve_collision_mask(name, entity.collision_mask)?,
                    inventory_slot_count: entity.inventory_slot_count,
                    burner: entity.burner,
                    mining_drill: entity
                        .mining_drill
                        .map(|mining_drill| MiningDrillPrototype {
                            mining_area: IVec2::new(
                                mining_drill.mining_area.x,
                                mining_drill.mining_area.y,
                            ),
                            ticks_per_item: mining_drill.ticks_per_item,
                        }),
                })
            })
            .collect::<Result<_, PrototypeLoadError>>()?;

        let loaded_tiles = tiles
            .into_iter()
            .map(|tile| {
                let name = tile.name;
                Ok(TilePrototype {
                    id: TileId::new(tile.id),
                    name: name.clone(),
                    collision_mask: resolve_collision_mask(name, tile.collision_mask)?,
                })
            })
            .collect::<Result<_, PrototypeLoadError>>()?;

        Ok(Self {
            items: loaded_items,
            recipes: loaded_recipes,
            entities: loaded_entities,
            tiles: loaded_tiles,
        })
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }
}

impl fmt::Display for PrototypeLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "failed to read prototype data: {error}"),
            Self::Ron(error) => write!(formatter, "failed to parse prototype data: {error}"),
            Self::DuplicateId { group, id } => {
                write!(formatter, "duplicate {group} prototype id {id}")
            }
            Self::DuplicateName { group, name } => {
                write!(formatter, "duplicate {group} prototype name {name:?}")
            }
            Self::NonContiguousIds {
                group,
                expected,
                actual,
            } => write!(
                formatter,
                "{group} prototype ids must be contiguous from 0: expected {expected}, got {actual}"
            ),
            Self::MissingItemReference { recipe, item } => {
                write!(
                    formatter,
                    "recipe {recipe:?} references missing item {item:?}"
                )
            }
            Self::InvalidCollisionLayer { owner, layer } => {
                write!(
                    formatter,
                    "prototype {owner:?} uses invalid collision layer {layer:?}"
                )
            }
        }
    }
}

impl std::error::Error for PrototypeLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Ron(error) => Some(error),
            _ => None,
        }
    }
}

trait RawPrototype {
    fn id(&self) -> u16;
    fn name(&self) -> &str;
}

fn validate_group<T>(prototypes: &mut [T], group: &'static str) -> Result<(), PrototypeLoadError>
where
    T: RawPrototype,
{
    {
        let mut seen_ids = HashSet::new();
        let mut seen_names = HashSet::new();

        for prototype in prototypes.iter() {
            if !seen_ids.insert(prototype.id()) {
                return Err(PrototypeLoadError::DuplicateId {
                    group,
                    id: prototype.id(),
                });
            }

            if !seen_names.insert(prototype.name()) {
                return Err(PrototypeLoadError::DuplicateName {
                    group,
                    name: prototype.name().to_string(),
                });
            }
        }
    }

    prototypes.sort_by_key(RawPrototype::id);

    for (expected, prototype) in prototypes.iter().enumerate() {
        let expected = u16::try_from(expected).expect("prototype group exceeds u16 id range");
        let actual = prototype.id();
        if actual != expected {
            return Err(PrototypeLoadError::NonContiguousIds {
                group,
                expected,
                actual,
            });
        }
    }

    Ok(())
}

fn resolve_item_amounts(
    recipe: &str,
    amounts: Vec<RawItemAmount>,
    item_ids_by_name: &HashMap<String, ItemId>,
) -> Result<Vec<ItemAmount>, PrototypeLoadError> {
    amounts
        .into_iter()
        .map(|amount| {
            let item = *item_ids_by_name.get(&amount.item).ok_or_else(|| {
                PrototypeLoadError::MissingItemReference {
                    recipe: recipe.to_string(),
                    item: amount.item.clone(),
                }
            })?;
            Ok(ItemAmount {
                item,
                amount: amount.amount,
            })
        })
        .collect()
}

fn resolve_collision_mask(
    owner: String,
    raw: RawCollisionMask,
) -> Result<CollisionMask, PrototypeLoadError> {
    let layers = raw
        .layers
        .into_iter()
        .map(|layer| {
            let normalized = layer.as_str();
            match normalized {
                "ground" => Ok(CollisionLayer::Ground),
                "water" => Ok(CollisionLayer::Water),
                "resource" => Ok(CollisionLayer::Resource),
                "building" => Ok(CollisionLayer::Building),
                "transport" => Ok(CollisionLayer::Transport),
                _ => Err(PrototypeLoadError::InvalidCollisionLayer {
                    owner: owner.clone(),
                    layer,
                }),
            }
        })
        .collect::<Result<_, _>>()?;

    Ok(CollisionMask { layers })
}

#[derive(Debug, Deserialize)]
struct RawPrototypeCatalog {
    items: Vec<RawItemPrototype>,
    recipes: Vec<RawRecipePrototype>,
    entities: Vec<RawEntityPrototype>,
    tiles: Vec<RawTilePrototype>,
}

#[derive(Debug, Deserialize)]
struct RawItemPrototype {
    id: u16,
    name: String,
    stack_size: u16,
    fuel_value_joules: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawRecipePrototype {
    id: u16,
    name: String,
    category: CraftingCategory,
    crafting_time_ticks: u32,
    ingredients: Vec<RawItemAmount>,
    products: Vec<RawItemAmount>,
}

#[derive(Debug, Deserialize)]
struct RawEntityPrototype {
    id: u16,
    name: String,
    entity_kind: EntityKind,
    size: RawIVec2,
    collision_mask: RawCollisionMask,
    inventory_slot_count: Option<usize>,
    burner: Option<BurnerPrototype>,
    mining_drill: Option<RawMiningDrillPrototype>,
}

#[derive(Debug, Deserialize)]
struct RawMiningDrillPrototype {
    mining_area: RawIVec2,
    ticks_per_item: u32,
}

#[derive(Debug, Deserialize)]
struct RawTilePrototype {
    id: u16,
    name: String,
    collision_mask: RawCollisionMask,
}

#[derive(Debug, Deserialize)]
struct RawItemAmount {
    item: String,
    amount: u16,
}

#[derive(Debug, Deserialize)]
struct RawIVec2 {
    x: i32,
    y: i32,
}

#[derive(Debug, Deserialize)]
struct RawCollisionMask {
    layers: Vec<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM_NAMES: [&str; 19] = [
        "iron_ore",
        "copper_ore",
        "coal",
        "stone",
        "iron_plate",
        "copper_plate",
        "steel_plate",
        "iron_gear_wheel",
        "copper_cable",
        "electronic_circuit",
        "inserter",
        "transport_belt",
        "assembling_machine",
        "stone_furnace",
        "burner_mining_drill",
        "lab",
        "automation_science_pack",
        "chest",
        "stone_brick",
    ];

    const RECIPE_NAMES: [&str; 15] = [
        "iron_plate",
        "copper_plate",
        "steel_plate",
        "iron_gear_wheel",
        "copper_cable",
        "electronic_circuit",
        "inserter",
        "transport_belt",
        "assembling_machine",
        "stone_furnace",
        "burner_mining_drill",
        "lab",
        "automation_science_pack",
        "chest",
        "stone_brick",
    ];

    const ENTITY_NAMES: [&str; 11] = [
        "iron_ore_patch",
        "copper_ore_patch",
        "coal_patch",
        "stone_patch",
        "stone_furnace",
        "assembling_machine",
        "inserter",
        "transport_belt",
        "burner_mining_drill",
        "lab",
        "chest",
    ];

    const TILE_NAMES: [&str; 3] = ["grass", "dirt", "water"];

    #[test]
    fn base_catalog_loads_from_ron() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

        assert_eq!(catalog.items.len(), 19);
        assert_eq!(catalog.recipes.len(), 15);
        assert_eq!(catalog.entities.len(), 11);
        assert_eq!(catalog.tiles.len(), 3);
    }

    #[test]
    fn base_catalog_contains_expected_names() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

        for name in ITEM_NAMES {
            assert!(
                catalog.items.iter().any(|prototype| prototype.name == name),
                "missing item {name}"
            );
        }

        for name in RECIPE_NAMES {
            assert!(
                catalog
                    .recipes
                    .iter()
                    .any(|prototype| prototype.name == name),
                "missing recipe {name}"
            );
        }

        for name in ENTITY_NAMES {
            assert!(
                catalog
                    .entities
                    .iter()
                    .any(|prototype| prototype.name == name),
                "missing entity {name}"
            );
        }

        for name in TILE_NAMES {
            assert!(
                catalog.tiles.iter().any(|prototype| prototype.name == name),
                "missing tile {name}"
            );
        }
    }

    #[test]
    fn explicit_ids_are_sorted_and_stable() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

        for (expected, item) in catalog.items.iter().enumerate() {
            assert_eq!(item.id.index(), expected);
        }

        for (expected, recipe) in catalog.recipes.iter().enumerate() {
            assert_eq!(recipe.id.index(), expected);
        }

        for (expected, entity) in catalog.entities.iter().enumerate() {
            assert_eq!(entity.id.index(), expected);
        }

        for (expected, tile) in catalog.tiles.iter().enumerate() {
            assert_eq!(tile.id.index(), expected);
        }
    }

    #[test]
    fn recipe_item_references_resolve_to_valid_item_ids() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

        for recipe in &catalog.recipes {
            for amount in recipe.ingredients.iter().chain(recipe.products.iter()) {
                assert!(amount.item.index() < catalog.items.len());
            }
        }
    }

    #[test]
    fn chest_entity_loads_inventory_slot_count() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let chest = catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == "chest")
            .expect("base catalog should contain chest entity");

        assert_eq!(chest.inventory_slot_count, Some(16));
    }

    #[test]
    fn coal_loads_fuel_value() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let coal = catalog
            .items
            .iter()
            .find(|prototype| prototype.name == "coal")
            .expect("base catalog should contain coal");
        let iron_ore = catalog
            .items
            .iter()
            .find(|prototype| prototype.name == "iron_ore")
            .expect("base catalog should contain iron ore");

        assert_eq!(coal.fuel_value_joules, Some(4_000_000));
        assert_eq!(iron_ore.fuel_value_joules, None);
    }

    #[test]
    fn burner_mining_drill_loads_energy_and_mining_metadata() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let drill = catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == "burner_mining_drill")
            .expect("base catalog should contain burner mining drill");

        assert_eq!(
            drill
                .burner
                .as_ref()
                .map(|burner| burner.energy_usage_watts),
            Some(150_000)
        );
        assert_eq!(
            drill.mining_drill.as_ref().map(|mining| mining.mining_area),
            Some(IVec2::new(2, 2))
        );
        assert_eq!(
            drill
                .mining_drill
                .as_ref()
                .map(|mining| mining.ticks_per_item),
            Some(240)
        );
    }

    #[test]
    fn stone_furnace_loads_burner_metadata() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let furnace = catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == "stone_furnace")
            .expect("base catalog should contain stone furnace");

        assert_eq!(
            furnace
                .burner
                .as_ref()
                .map(|burner| burner.energy_usage_watts),
            Some(90_000)
        );
    }

    #[test]
    fn stone_brick_smelting_recipe_loads() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let stone = catalog
            .items
            .iter()
            .find(|prototype| prototype.name == "stone")
            .expect("base catalog should contain stone")
            .id;
        let stone_brick = catalog
            .items
            .iter()
            .find(|prototype| prototype.name == "stone_brick")
            .expect("base catalog should contain stone brick")
            .id;
        let recipe = catalog
            .recipes
            .iter()
            .find(|prototype| prototype.name == "stone_brick")
            .expect("base catalog should contain stone brick recipe");

        assert_eq!(recipe.category, CraftingCategory::Smelting);
        assert_eq!(recipe.crafting_time_ticks, 210);
        assert_eq!(
            recipe.ingredients,
            vec![ItemAmount {
                item: stone,
                amount: 1
            }]
        );
        assert_eq!(
            recipe.products,
            vec![ItemAmount {
                item: stone_brick,
                amount: 1
            }]
        );
    }

    #[test]
    fn duplicate_ids_fail() {
        let error = PrototypeCatalog::from_ron_str(
            r#"
            (
                items: [
                    (id: 0, name: "iron_ore", stack_size: 100),
                    (id: 0, name: "copper_ore", stack_size: 100),
                ],
                recipes: [],
                entities: [],
                tiles: [],
            )
            "#,
        )
        .expect_err("duplicate item ids should fail");

        assert!(matches!(
            error,
            PrototypeLoadError::DuplicateId {
                group: "items",
                id: 0,
            }
        ));
    }

    #[test]
    fn duplicate_names_fail() {
        let error = PrototypeCatalog::from_ron_str(
            r#"
            (
                items: [
                    (id: 0, name: "iron_ore", stack_size: 100),
                    (id: 1, name: "iron_ore", stack_size: 100),
                ],
                recipes: [],
                entities: [],
                tiles: [],
            )
            "#,
        )
        .expect_err("duplicate item names should fail");

        assert!(matches!(
            error,
            PrototypeLoadError::DuplicateName {
                group: "items",
                name,
            } if name == "iron_ore"
        ));
    }

    #[test]
    fn missing_item_references_fail() {
        let error = PrototypeCatalog::from_ron_str(
            r#"
            (
                items: [(id: 0, name: "iron_plate", stack_size: 100)],
                recipes: [(
                    id: 0,
                    name: "missing_recipe",
                    category: Crafting,
                    crafting_time_ticks: 30,
                    ingredients: [(item: "missing_item", amount: 1)],
                    products: [(item: "iron_plate", amount: 1)],
                )],
                entities: [],
                tiles: [],
            )
            "#,
        )
        .expect_err("missing item references should fail");

        assert!(matches!(
            error,
            PrototypeLoadError::MissingItemReference { recipe, item }
                if recipe == "missing_recipe" && item == "missing_item"
        ));
    }

    #[test]
    fn invalid_collision_layers_fail() {
        let error = PrototypeCatalog::from_ron_str(
            r#"
            (
                items: [],
                recipes: [],
                entities: [(
                    id: 0,
                    name: "bad_entity",
                    entity_kind: Furnace,
                    size: (x: 2, y: 2),
                    collision_mask: (layers: ["invalid"]),
                )],
                tiles: [],
            )
            "#,
        )
        .expect_err("invalid collision layers should fail");

        assert!(matches!(
            error,
            PrototypeLoadError::InvalidCollisionLayer { owner, layer }
                if owner == "bad_entity" && layer == "invalid"
        ));
    }
}
