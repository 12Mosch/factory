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
id_type!(TechnologyId);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PrototypeCatalog {
    pub items: Vec<ItemPrototype>,
    pub recipes: Vec<RecipePrototype>,
    pub entities: Vec<EntityPrototype>,
    pub tiles: Vec<TilePrototype>,
    pub technologies: Vec<TechnologyPrototype>,
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
    pub assembling_machine: Option<AssemblingMachinePrototype>,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash)]
pub struct AssemblingMachinePrototype {
    pub crafting_speed_numerator: u32,
    pub crafting_speed_denominator: u32,
    pub input_slot_count: usize,
    pub output_slot_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TilePrototype {
    pub id: TileId,
    pub name: String,
    pub collision_mask: CollisionMask,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TechnologyPrototype {
    pub id: TechnologyId,
    pub name: String,
    pub prerequisites: Vec<TechnologyId>,
    pub science_packs: Vec<ItemAmount>,
    pub required_units: u32,
    pub effects: Vec<TechnologyEffect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TechnologyEffect {
    UnlockRecipe(RecipeId),
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
    MissingTechnologyPrerequisite {
        technology: String,
        prerequisite: String,
    },
    MissingTechnologySciencePackItem {
        technology: String,
        item: String,
    },
    MissingTechnologyUnlockRecipe {
        technology: String,
        recipe: String,
    },
    InvalidTechnologyRequiredUnits {
        technology: String,
    },
    TechnologySelfPrerequisite {
        technology: String,
    },
    TechnologyPrerequisiteCycle {
        technology: String,
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
        let mut technologies = raw.technologies;
        validate_group(&mut technologies, "technologies")?;

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

        let loaded_recipes: Vec<RecipePrototype> = recipes
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
        let recipe_ids_by_name = loaded_recipes
            .iter()
            .map(|recipe: &RecipePrototype| (recipe.name.clone(), recipe.id))
            .collect::<HashMap<_, _>>();

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
                    assembling_machine: entity.assembling_machine,
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

        let technology_ids_by_name = technologies
            .iter()
            .map(|technology| (technology.name.clone(), TechnologyId::new(technology.id)))
            .collect::<HashMap<_, _>>();
        let loaded_technologies = technologies
            .into_iter()
            .map(|technology| {
                if technology.required_units == 0 {
                    return Err(PrototypeLoadError::InvalidTechnologyRequiredUnits {
                        technology: technology.name,
                    });
                }

                let id = TechnologyId::new(technology.id);
                let prerequisites = technology
                    .prerequisites
                    .into_iter()
                    .map(|prerequisite| {
                        let prerequisite_id = *technology_ids_by_name
                            .get(&prerequisite)
                            .ok_or_else(|| PrototypeLoadError::MissingTechnologyPrerequisite {
                                technology: technology.name.clone(),
                                prerequisite: prerequisite.clone(),
                            })?;
                        if prerequisite_id == id {
                            return Err(PrototypeLoadError::TechnologySelfPrerequisite {
                                technology: technology.name.clone(),
                            });
                        }
                        Ok(prerequisite_id)
                    })
                    .collect::<Result<_, PrototypeLoadError>>()?;

                let science_packs = technology
                    .science_packs
                    .into_iter()
                    .map(|amount| {
                        let item = *item_ids_by_name.get(&amount.item).ok_or_else(|| {
                            PrototypeLoadError::MissingTechnologySciencePackItem {
                                technology: technology.name.clone(),
                                item: amount.item.clone(),
                            }
                        })?;
                        Ok(ItemAmount {
                            item,
                            amount: amount.amount,
                        })
                    })
                    .collect::<Result<_, PrototypeLoadError>>()?;

                let effects = technology
                    .effects
                    .into_iter()
                    .map(|effect| match effect {
                        RawTechnologyEffect::UnlockRecipe(recipe) => {
                            let recipe_id = *recipe_ids_by_name.get(&recipe).ok_or_else(|| {
                                PrototypeLoadError::MissingTechnologyUnlockRecipe {
                                    technology: technology.name.clone(),
                                    recipe: recipe.clone(),
                                }
                            })?;
                            Ok(TechnologyEffect::UnlockRecipe(recipe_id))
                        }
                    })
                    .collect::<Result<_, PrototypeLoadError>>()?;

                Ok(TechnologyPrototype {
                    id,
                    name: technology.name,
                    prerequisites,
                    science_packs,
                    required_units: technology.required_units,
                    effects,
                })
            })
            .collect::<Result<Vec<_>, PrototypeLoadError>>()?;
        validate_technology_prerequisite_graph(&loaded_technologies)?;

        Ok(Self {
            items: loaded_items,
            recipes: loaded_recipes,
            entities: loaded_entities,
            tiles: loaded_tiles,
            technologies: loaded_technologies,
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
            Self::MissingTechnologyPrerequisite {
                technology,
                prerequisite,
            } => write!(
                formatter,
                "technology {technology:?} references missing prerequisite {prerequisite:?}"
            ),
            Self::MissingTechnologySciencePackItem { technology, item } => write!(
                formatter,
                "technology {technology:?} references missing science pack item {item:?}"
            ),
            Self::MissingTechnologyUnlockRecipe { technology, recipe } => write!(
                formatter,
                "technology {technology:?} references missing unlock recipe {recipe:?}"
            ),
            Self::InvalidTechnologyRequiredUnits { technology } => write!(
                formatter,
                "technology {technology:?} must require at least one research unit"
            ),
            Self::TechnologySelfPrerequisite { technology } => write!(
                formatter,
                "technology {technology:?} cannot list itself as a prerequisite"
            ),
            Self::TechnologyPrerequisiteCycle { technology } => write!(
                formatter,
                "technology prerequisite graph contains a cycle at {technology:?}"
            ),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TechnologyVisitState {
    Visiting,
    Visited,
}

fn validate_technology_prerequisite_graph(
    technologies: &[TechnologyPrototype],
) -> Result<(), PrototypeLoadError> {
    let mut states = vec![None; technologies.len()];

    for technology in technologies {
        visit_technology_prerequisites(technology.id.index(), technologies, &mut states)?;
    }

    Ok(())
}

fn visit_technology_prerequisites(
    index: usize,
    technologies: &[TechnologyPrototype],
    states: &mut [Option<TechnologyVisitState>],
) -> Result<(), PrototypeLoadError> {
    match states[index] {
        Some(TechnologyVisitState::Visited) => return Ok(()),
        Some(TechnologyVisitState::Visiting) => {
            return Err(PrototypeLoadError::TechnologyPrerequisiteCycle {
                technology: technologies[index].name.clone(),
            });
        }
        None => {}
    }

    states[index] = Some(TechnologyVisitState::Visiting);

    for prerequisite in &technologies[index].prerequisites {
        let prerequisite_index = prerequisite.index();
        if prerequisite_index >= technologies.len()
            || technologies[prerequisite_index].id != *prerequisite
        {
            return Err(PrototypeLoadError::MissingTechnologyPrerequisite {
                technology: technologies[index].name.clone(),
                prerequisite: format!("<id:{}>", prerequisite.raw()),
            });
        }
        visit_technology_prerequisites(prerequisite_index, technologies, states)?;
    }

    states[index] = Some(TechnologyVisitState::Visited);
    Ok(())
}

#[derive(Debug, Deserialize)]
struct RawPrototypeCatalog {
    items: Vec<RawItemPrototype>,
    recipes: Vec<RawRecipePrototype>,
    entities: Vec<RawEntityPrototype>,
    tiles: Vec<RawTilePrototype>,
    #[serde(default)]
    technologies: Vec<RawTechnologyPrototype>,
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
    assembling_machine: Option<AssemblingMachinePrototype>,
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
struct RawTechnologyPrototype {
    id: u16,
    name: String,
    prerequisites: Vec<String>,
    science_packs: Vec<RawItemAmount>,
    required_units: u32,
    effects: Vec<RawTechnologyEffect>,
}

#[derive(Debug, Deserialize)]
enum RawTechnologyEffect {
    UnlockRecipe(String),
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

impl RawPrototype for RawTechnologyPrototype {
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
    const TECHNOLOGY_NAMES: [&str; 1] = ["automation"];

    #[test]
    fn base_catalog_loads_from_ron() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

        assert_eq!(catalog.items.len(), 19);
        assert_eq!(catalog.recipes.len(), 15);
        assert_eq!(catalog.entities.len(), 11);
        assert_eq!(catalog.tiles.len(), 3);
        assert_eq!(catalog.technologies.len(), 1);
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

        for name in TECHNOLOGY_NAMES {
            assert!(
                catalog
                    .technologies
                    .iter()
                    .any(|prototype| prototype.name == name),
                "missing technology {name}"
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

        for (expected, technology) in catalog.technologies.iter().enumerate() {
            assert_eq!(technology.id.index(), expected);
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
    fn assembling_machine_loads_metadata() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let assembler = catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == "assembling_machine")
            .expect("base catalog should contain assembling machine");

        assert_eq!(assembler.entity_kind, EntityKind::AssemblingMachine);
        assert_eq!(assembler.size, IVec2::new(3, 3));
        assert_eq!(
            assembler.assembling_machine,
            Some(AssemblingMachinePrototype {
                crafting_speed_numerator: 1,
                crafting_speed_denominator: 2,
                input_slot_count: 4,
                output_slot_count: 1,
            })
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
    fn automation_technology_loads_research_cost_and_unlock_effect() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let automation_science_pack = catalog
            .items
            .iter()
            .find(|item| item.name == "automation_science_pack")
            .expect("base catalog should contain automation science pack")
            .id;
        let assembling_machine_recipe = catalog
            .recipes
            .iter()
            .find(|recipe| recipe.name == "assembling_machine")
            .expect("base catalog should contain assembling machine recipe")
            .id;
        let automation = catalog
            .technologies
            .iter()
            .find(|technology| technology.name == "automation")
            .expect("base catalog should contain automation technology");

        assert_eq!(automation.prerequisites, Vec::<TechnologyId>::new());
        assert_eq!(
            automation.science_packs,
            vec![ItemAmount {
                item: automation_science_pack,
                amount: 1,
            }]
        );
        assert_eq!(automation.required_units, 10);
        assert_eq!(
            automation.effects,
            vec![TechnologyEffect::UnlockRecipe(assembling_machine_recipe)]
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

    #[test]
    fn missing_technology_prerequisites_fail() {
        let error = PrototypeCatalog::from_ron_str(
            r#"
            (
                items: [],
                recipes: [],
                entities: [],
                tiles: [],
                technologies: [(
                    id: 0,
                    name: "automation",
                    prerequisites: ["missing"],
                    science_packs: [],
                    required_units: 1,
                    effects: [],
                )],
            )
            "#,
        )
        .expect_err("missing technology prerequisites should fail");

        assert!(matches!(
            error,
            PrototypeLoadError::MissingTechnologyPrerequisite {
                technology,
                prerequisite,
            } if technology == "automation" && prerequisite == "missing"
        ));
    }

    #[test]
    fn missing_technology_science_pack_items_fail() {
        let error = PrototypeCatalog::from_ron_str(
            r#"
            (
                items: [],
                recipes: [],
                entities: [],
                tiles: [],
                technologies: [(
                    id: 0,
                    name: "automation",
                    prerequisites: [],
                    science_packs: [(item: "missing_pack", amount: 1)],
                    required_units: 1,
                    effects: [],
                )],
            )
            "#,
        )
        .expect_err("missing science pack item should fail");

        assert!(matches!(
            error,
            PrototypeLoadError::MissingTechnologySciencePackItem {
                technology,
                item,
            } if technology == "automation" && item == "missing_pack"
        ));
    }

    #[test]
    fn missing_technology_unlock_recipes_fail() {
        let error = PrototypeCatalog::from_ron_str(
            r#"
            (
                items: [],
                recipes: [],
                entities: [],
                tiles: [],
                technologies: [(
                    id: 0,
                    name: "automation",
                    prerequisites: [],
                    science_packs: [],
                    required_units: 1,
                    effects: [UnlockRecipe("missing_recipe")],
                )],
            )
            "#,
        )
        .expect_err("missing unlock recipe should fail");

        assert!(matches!(
            error,
            PrototypeLoadError::MissingTechnologyUnlockRecipe {
                technology,
                recipe,
            } if technology == "automation" && recipe == "missing_recipe"
        ));
    }

    #[test]
    fn duplicate_technology_ids_fail() {
        let error = PrototypeCatalog::from_ron_str(
            r#"
            (
                items: [],
                recipes: [],
                entities: [],
                tiles: [],
                technologies: [
                    (
                        id: 0,
                        name: "automation",
                        prerequisites: [],
                        science_packs: [],
                        required_units: 1,
                        effects: [],
                    ),
                    (
                        id: 0,
                        name: "logistics",
                        prerequisites: [],
                        science_packs: [],
                        required_units: 1,
                        effects: [],
                    ),
                ],
            )
            "#,
        )
        .expect_err("duplicate technology ids should fail");

        assert!(matches!(
            error,
            PrototypeLoadError::DuplicateId {
                group: "technologies",
                id: 0,
            }
        ));
    }

    #[test]
    fn duplicate_technology_names_fail() {
        let error = PrototypeCatalog::from_ron_str(
            r#"
            (
                items: [],
                recipes: [],
                entities: [],
                tiles: [],
                technologies: [
                    (
                        id: 0,
                        name: "automation",
                        prerequisites: [],
                        science_packs: [],
                        required_units: 1,
                        effects: [],
                    ),
                    (
                        id: 1,
                        name: "automation",
                        prerequisites: [],
                        science_packs: [],
                        required_units: 1,
                        effects: [],
                    ),
                ],
            )
            "#,
        )
        .expect_err("duplicate technology names should fail");

        assert!(matches!(
            error,
            PrototypeLoadError::DuplicateName {
                group: "technologies",
                name,
            } if name == "automation"
        ));
    }

    #[test]
    fn invalid_technology_required_units_fail() {
        let error = PrototypeCatalog::from_ron_str(
            r#"
            (
                items: [],
                recipes: [],
                entities: [],
                tiles: [],
                technologies: [(
                    id: 0,
                    name: "automation",
                    prerequisites: [],
                    science_packs: [],
                    required_units: 0,
                    effects: [],
                )],
            )
            "#,
        )
        .expect_err("zero required units should fail");

        assert!(matches!(
            error,
            PrototypeLoadError::InvalidTechnologyRequiredUnits { technology }
                if technology == "automation"
        ));
    }

    #[test]
    fn technology_self_prerequisites_fail() {
        let error = PrototypeCatalog::from_ron_str(
            r#"
            (
                items: [],
                recipes: [],
                entities: [],
                tiles: [],
                technologies: [(
                    id: 0,
                    name: "automation",
                    prerequisites: ["automation"],
                    science_packs: [],
                    required_units: 1,
                    effects: [],
                )],
            )
            "#,
        )
        .expect_err("self prerequisites should fail");

        assert!(matches!(
            error,
            PrototypeLoadError::TechnologySelfPrerequisite { technology }
                if technology == "automation"
        ));
    }

    #[test]
    fn technology_prerequisite_cycles_fail() {
        let error = PrototypeCatalog::from_ron_str(
            r#"
            (
                items: [],
                recipes: [],
                entities: [],
                tiles: [],
                technologies: [
                    (
                        id: 0,
                        name: "automation",
                        prerequisites: ["logistics"],
                        science_packs: [],
                        required_units: 1,
                        effects: [],
                    ),
                    (
                        id: 1,
                        name: "logistics",
                        prerequisites: ["automation"],
                        science_packs: [],
                        required_units: 1,
                        effects: [],
                    ),
                ],
            )
            "#,
        )
        .expect_err("technology prerequisite cycles should fail");

        assert!(matches!(
            error,
            PrototypeLoadError::TechnologyPrerequisiteCycle { .. }
        ));
    }
}
