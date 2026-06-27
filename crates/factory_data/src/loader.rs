use glam::IVec2;
use std::collections::HashMap;
use std::path::Path;

use crate::catalog::PrototypeCatalog;
use crate::error::PrototypeLoadError;
use crate::ids::{EntityPrototypeId, ItemId, RecipeId, TechnologyId, TileId};
use crate::model::{
    EntityPrototype, ItemAmount, ItemPrototype, MiningDrillPrototype, RecipePrototype,
    TechnologyEffect, TechnologyPrototype, TilePrototype,
};
use crate::raw::{
    RawEntityPrototype, RawItemPrototype, RawPrototypeCatalog, RawRecipePrototype,
    RawTechnologyEffect, RawTechnologyPrototype, RawTilePrototype,
};
use crate::validation::{
    resolve_collision_mask, resolve_item_amounts, validate_group,
    validate_technology_prerequisite_graph,
};

#[cfg(test)]
mod tests;

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
        let raw = ValidatedRawCatalog::from_raw(raw)?;

        let (items, item_ids_by_name) = load_items(raw.items);
        let (recipes, recipe_ids_by_name) = load_recipes(raw.recipes, &item_ids_by_name)?;
        let entities = load_entities(raw.entities, &item_ids_by_name)?;
        let tiles = load_tiles(raw.tiles)?;
        let technologies =
            load_technologies(raw.technologies, &item_ids_by_name, &recipe_ids_by_name)?;
        validate_technology_prerequisite_graph(&technologies)?;

        Ok(Self {
            items,
            recipes,
            entities,
            tiles,
            technologies,
        })
    }
}

struct ValidatedRawCatalog {
    items: Vec<RawItemPrototype>,
    recipes: Vec<RawRecipePrototype>,
    entities: Vec<RawEntityPrototype>,
    tiles: Vec<RawTilePrototype>,
    technologies: Vec<RawTechnologyPrototype>,
}

impl ValidatedRawCatalog {
    fn from_raw(raw: RawPrototypeCatalog) -> Result<Self, PrototypeLoadError> {
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

        Ok(Self {
            items,
            recipes,
            entities,
            tiles,
            technologies,
        })
    }
}

fn load_items(items: Vec<RawItemPrototype>) -> (Vec<ItemPrototype>, HashMap<String, ItemId>) {
    let mut item_ids_by_name = HashMap::with_capacity(items.len());
    let items = items
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

    (items, item_ids_by_name)
}

fn load_recipes(
    recipes: Vec<RawRecipePrototype>,
    item_ids_by_name: &HashMap<String, ItemId>,
) -> Result<(Vec<RecipePrototype>, HashMap<String, RecipeId>), PrototypeLoadError> {
    let recipes = recipes
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
                    item_ids_by_name,
                )?,
                products: resolve_item_amounts(&recipe_name, recipe.products, item_ids_by_name)?,
            })
        })
        .collect::<Result<Vec<_>, PrototypeLoadError>>()?;
    let recipe_ids_by_name = recipes
        .iter()
        .map(|recipe: &RecipePrototype| (recipe.name.clone(), recipe.id))
        .collect();

    Ok((recipes, recipe_ids_by_name))
}

fn load_entities(
    entities: Vec<RawEntityPrototype>,
    item_ids_by_name: &HashMap<String, ItemId>,
) -> Result<Vec<EntityPrototype>, PrototypeLoadError> {
    entities
        .into_iter()
        .map(|entity| {
            let name = entity.name;
            let build_item = resolve_entity_build_item(&name, entity.build_item, item_ids_by_name)?;
            Ok(EntityPrototype {
                id: EntityPrototypeId::new(entity.id),
                name: name.clone(),
                entity_kind: entity.entity_kind,
                size: IVec2::new(entity.size.x, entity.size.y),
                collision_mask: resolve_collision_mask(name, entity.collision_mask)?,
                build_item,
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
                transport_belt: entity.transport_belt,
            })
        })
        .collect()
}

fn resolve_entity_build_item(
    entity_name: &str,
    raw_build_item: Option<String>,
    item_ids_by_name: &HashMap<String, ItemId>,
) -> Result<Option<ItemId>, PrototypeLoadError> {
    match raw_build_item {
        Some(item_name) => {
            let item_id = *item_ids_by_name.get(&item_name).ok_or_else(|| {
                PrototypeLoadError::MissingEntityBuildItem {
                    entity: entity_name.to_string(),
                    item: item_name.clone(),
                }
            })?;
            Ok(Some(item_id))
        }
        None => Ok(item_ids_by_name.get(entity_name).copied()),
    }
}

fn load_tiles(tiles: Vec<RawTilePrototype>) -> Result<Vec<TilePrototype>, PrototypeLoadError> {
    tiles
        .into_iter()
        .map(|tile| {
            let name = tile.name;
            Ok(TilePrototype {
                id: TileId::new(tile.id),
                name: name.clone(),
                collision_mask: resolve_collision_mask(name, tile.collision_mask)?,
            })
        })
        .collect()
}

fn load_technologies(
    technologies: Vec<RawTechnologyPrototype>,
    item_ids_by_name: &HashMap<String, ItemId>,
    recipe_ids_by_name: &HashMap<String, RecipeId>,
) -> Result<Vec<TechnologyPrototype>, PrototypeLoadError> {
    let technology_ids_by_name = technologies
        .iter()
        .map(|technology| (technology.name.clone(), TechnologyId::new(technology.id)))
        .collect::<HashMap<_, _>>();

    technologies
        .into_iter()
        .map(|technology| {
            if technology.required_units == 0 {
                return Err(PrototypeLoadError::InvalidTechnologyRequiredUnits {
                    technology: technology.name,
                });
            }
            if technology.research_time_ticks == 0 {
                return Err(PrototypeLoadError::InvalidTechnologyResearchTime {
                    technology: technology.name,
                });
            }

            let id = TechnologyId::new(technology.id);
            let prerequisites =
                resolve_technology_prerequisites(&technology, id, &technology_ids_by_name)?;
            let science_packs = resolve_technology_science_packs(&technology, item_ids_by_name)?;
            let effects = resolve_technology_effects(&technology, recipe_ids_by_name)?;

            Ok(TechnologyPrototype {
                id,
                name: technology.name,
                prerequisites,
                science_packs,
                required_units: technology.required_units,
                research_time_ticks: technology.research_time_ticks,
                effects,
            })
        })
        .collect()
}

fn resolve_technology_prerequisites(
    technology: &RawTechnologyPrototype,
    technology_id: TechnologyId,
    technology_ids_by_name: &HashMap<String, TechnologyId>,
) -> Result<Vec<TechnologyId>, PrototypeLoadError> {
    technology
        .prerequisites
        .iter()
        .map(|prerequisite| {
            let prerequisite_id = *technology_ids_by_name.get(prerequisite).ok_or_else(|| {
                PrototypeLoadError::MissingTechnologyPrerequisite {
                    technology: technology.name.clone(),
                    prerequisite: prerequisite.clone(),
                }
            })?;
            if prerequisite_id == technology_id {
                return Err(PrototypeLoadError::TechnologySelfPrerequisite {
                    technology: technology.name.clone(),
                });
            }
            Ok(prerequisite_id)
        })
        .collect()
}

fn resolve_technology_science_packs(
    technology: &RawTechnologyPrototype,
    item_ids_by_name: &HashMap<String, ItemId>,
) -> Result<Vec<ItemAmount>, PrototypeLoadError> {
    technology
        .science_packs
        .iter()
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
        .collect()
}

fn resolve_technology_effects(
    technology: &RawTechnologyPrototype,
    recipe_ids_by_name: &HashMap<String, RecipeId>,
) -> Result<Vec<TechnologyEffect>, PrototypeLoadError> {
    technology
        .effects
        .iter()
        .map(|effect| match effect {
            RawTechnologyEffect::UnlockRecipe(recipe) => {
                let recipe_id = *recipe_ids_by_name.get(recipe).ok_or_else(|| {
                    PrototypeLoadError::MissingTechnologyUnlockRecipe {
                        technology: technology.name.clone(),
                        recipe: recipe.clone(),
                    }
                })?;
                Ok(TechnologyEffect::UnlockRecipe(recipe_id))
            }
        })
        .collect()
}
