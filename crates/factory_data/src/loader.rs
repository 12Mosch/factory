use glam::IVec2;
use std::collections::HashMap;
use std::path::Path;

use crate::catalog::PrototypeCatalog;
use crate::error::PrototypeLoadError;
use crate::ids::{EntityPrototypeId, FluidId, ItemId, RecipeId, TechnologyId, TileId};
use crate::model::{
    ElectricPolePrototype, EntityPrototype, FluidBoxPrototype, FluidConnectionPrototype,
    FluidConnectionSide, FluidPrototype, InserterPrototype, ItemAmount, ItemPrototype,
    MiningDrillPrototype, PumpjackPrototype, RecipePrototype, TechnologyEffect,
    TechnologyPrototype, TilePrototype,
};
use crate::raw::{
    RawEntityPrototype, RawFluidBoxPrototype, RawFluidConnectionPrototype, RawFluidPrototype,
    RawItemPrototype, RawPrototypeCatalog, RawPumpjackPrototype, RawRecipePrototype,
    RawTechnologyEffect, RawTechnologyPrototype, RawTilePrototype,
};
use crate::validation::{
    resolve_collision_mask, resolve_fluid_amounts, resolve_item_amounts, validate_group,
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
        let (fluids, fluid_ids_by_name) = load_fluids(raw.fluids);
        let (recipes, recipe_ids_by_name) =
            load_recipes(raw.recipes, &item_ids_by_name, &fluid_ids_by_name)?;
        let entities = load_entities(raw.entities, &item_ids_by_name, &fluid_ids_by_name)?;
        let tiles = load_tiles(raw.tiles)?;
        let technologies =
            load_technologies(raw.technologies, &item_ids_by_name, &recipe_ids_by_name)?;
        validate_technology_prerequisite_graph(&technologies)?;

        Ok(Self {
            items,
            fluids,
            recipes,
            entities,
            tiles,
            technologies,
        })
    }
}

struct ValidatedRawCatalog {
    items: Vec<RawItemPrototype>,
    fluids: Vec<RawFluidPrototype>,
    recipes: Vec<RawRecipePrototype>,
    entities: Vec<RawEntityPrototype>,
    tiles: Vec<RawTilePrototype>,
    technologies: Vec<RawTechnologyPrototype>,
}

impl ValidatedRawCatalog {
    fn from_raw(raw: RawPrototypeCatalog) -> Result<Self, PrototypeLoadError> {
        let mut items = raw.items;
        validate_group(&mut items, "items")?;

        let mut fluids = raw.fluids;
        validate_group(&mut fluids, "fluids")?;

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
            fluids,
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

fn load_fluids(fluids: Vec<RawFluidPrototype>) -> (Vec<FluidPrototype>, HashMap<String, FluidId>) {
    let mut fluid_ids_by_name = HashMap::with_capacity(fluids.len());
    let fluids = fluids
        .into_iter()
        .map(|fluid| {
            let id = FluidId::new(fluid.id);
            fluid_ids_by_name.insert(fluid.name.clone(), id);
            FluidPrototype {
                id,
                name: fluid.name,
            }
        })
        .collect();

    (fluids, fluid_ids_by_name)
}

fn load_recipes(
    recipes: Vec<RawRecipePrototype>,
    item_ids_by_name: &HashMap<String, ItemId>,
    fluid_ids_by_name: &HashMap<String, FluidId>,
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
                fluid_ingredients: resolve_fluid_amounts(
                    &recipe_name,
                    recipe.fluid_ingredients,
                    fluid_ids_by_name,
                )?,
                fluid_products: resolve_fluid_amounts(
                    &recipe_name,
                    recipe.fluid_products,
                    fluid_ids_by_name,
                )?,
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
    fluid_ids_by_name: &HashMap<String, FluidId>,
) -> Result<Vec<EntityPrototype>, PrototypeLoadError> {
    entities
        .into_iter()
        .map(|entity| {
            let name = entity.name;
            let size = IVec2::new(entity.size.x, entity.size.y);
            let build_item = resolve_entity_build_item(&name, entity.build_item, item_ids_by_name)?;
            let fluid_boxes =
                resolve_fluid_boxes(&name, size, entity.fluid_boxes, fluid_ids_by_name)?;
            let pumpjack =
                resolve_pumpjack(&name, entity.pumpjack, item_ids_by_name, fluid_ids_by_name)?;
            validate_machine_fluid_roles(
                &name,
                entity.entity_kind,
                &fluid_boxes,
                pumpjack.as_ref(),
                fluid_ids_by_name,
            )?;
            Ok(EntityPrototype {
                id: EntityPrototypeId::new(entity.id),
                name: name.clone(),
                entity_kind: entity.entity_kind,
                size,
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
                splitter: entity.splitter,
                inserter: entity.inserter.map(|inserter| InserterPrototype {
                    pickup_offset: IVec2::new(inserter.pickup_offset.x, inserter.pickup_offset.y),
                    drop_offset: IVec2::new(inserter.drop_offset.x, inserter.drop_offset.y),
                    pickup_ticks: inserter.pickup_ticks,
                    drop_ticks: inserter.drop_ticks,
                }),
                electric_pole: entity
                    .electric_pole
                    .map(|electric_pole| ElectricPolePrototype {
                        supply_area_tiles: IVec2::new(
                            electric_pole.supply_area_tiles.x,
                            electric_pole.supply_area_tiles.y,
                        ),
                        wire_reach_tiles_x2: electric_pole.wire_reach_tiles_x2,
                    }),
                electric_energy_source: entity.electric_energy_source,
                steam_engine: entity.steam_engine,
                boiler: entity.boiler,
                offshore_pump: entity.offshore_pump,
                pumpjack,
                fluid_boxes,
            })
        })
        .collect()
}

fn resolve_fluid_boxes(
    entity_name: &str,
    entity_size: IVec2,
    fluid_boxes: Vec<RawFluidBoxPrototype>,
    fluid_ids_by_name: &HashMap<String, FluidId>,
) -> Result<Vec<FluidBoxPrototype>, PrototypeLoadError> {
    fluid_boxes
        .into_iter()
        .enumerate()
        .map(|(box_index, fluid_box)| {
            if fluid_box.capacity_milliunits == 0 || fluid_box.connections.is_empty() {
                return Err(PrototypeLoadError::InvalidFluidBox {
                    entity: entity_name.to_string(),
                    box_index,
                });
            }
            let filter = fluid_box
                .filter
                .map(|fluid_name| {
                    fluid_ids_by_name.get(&fluid_name).copied().ok_or_else(|| {
                        PrototypeLoadError::MissingFluidReference {
                            owner: entity_name.to_string(),
                            fluid: fluid_name,
                        }
                    })
                })
                .transpose()?;
            let connections = fluid_box
                .connections
                .into_iter()
                .enumerate()
                .map(|(connection_index, connection)| {
                    validate_fluid_connection_geometry(
                        entity_name,
                        box_index,
                        connection_index,
                        entity_size,
                        &connection,
                    )?;
                    Ok(FluidConnectionPrototype {
                        local_offset: IVec2::new(
                            connection.local_offset.x,
                            connection.local_offset.y,
                        ),
                        side: connection.side,
                    })
                })
                .collect::<Result<_, PrototypeLoadError>>()?;

            Ok(FluidBoxPrototype {
                capacity_milliunits: fluid_box.capacity_milliunits,
                filter,
                io: fluid_box.io,
                connections,
            })
        })
        .collect()
}

fn resolve_pumpjack(
    entity_name: &str,
    pumpjack: Option<RawPumpjackPrototype>,
    item_ids_by_name: &HashMap<String, ItemId>,
    fluid_ids_by_name: &HashMap<String, FluidId>,
) -> Result<Option<PumpjackPrototype>, PrototypeLoadError> {
    let Some(pumpjack) = pumpjack else {
        return Ok(None);
    };

    let resource_item = *item_ids_by_name
        .get(&pumpjack.resource_item)
        .ok_or_else(|| PrototypeLoadError::MissingPumpjackResourceItem {
            entity: entity_name.to_string(),
            item: pumpjack.resource_item.clone(),
        })?;
    let output_fluid = *fluid_ids_by_name
        .get(&pumpjack.output_fluid)
        .ok_or_else(|| PrototypeLoadError::MissingFluidReference {
            owner: entity_name.to_string(),
            fluid: pumpjack.output_fluid.clone(),
        })?;

    Ok(Some(PumpjackPrototype {
        pumping_speed_per_second_milliunits: pumpjack.pumping_speed_per_second_milliunits,
        resource_item,
        output_fluid,
    }))
}

fn validate_fluid_connection_geometry(
    entity_name: &str,
    box_index: usize,
    connection_index: usize,
    entity_size: IVec2,
    connection: &RawFluidConnectionPrototype,
) -> Result<(), PrototypeLoadError> {
    let x = connection.local_offset.x;
    let y = connection.local_offset.y;
    let on_entity = x >= 0 && y >= 0 && x < entity_size.x && y < entity_size.y;
    let on_side = match connection.side {
        FluidConnectionSide::North => y == 0,
        FluidConnectionSide::East => x == entity_size.x - 1,
        FluidConnectionSide::South => y == entity_size.y - 1,
        FluidConnectionSide::West => x == 0,
    };

    if on_entity && on_side {
        Ok(())
    } else {
        Err(PrototypeLoadError::InvalidFluidConnection {
            entity: entity_name.to_string(),
            box_index,
            connection_index,
        })
    }
}

fn validate_machine_fluid_roles(
    entity_name: &str,
    entity_kind: crate::model::EntityKind,
    fluid_boxes: &[FluidBoxPrototype],
    pumpjack: Option<&PumpjackPrototype>,
    fluid_ids_by_name: &HashMap<String, FluidId>,
) -> Result<(), PrototypeLoadError> {
    let required_fluid = |fluid_name: &str| {
        fluid_ids_by_name.get(fluid_name).copied().ok_or_else(|| {
            PrototypeLoadError::MissingFluidReference {
                owner: entity_name.to_string(),
                fluid: fluid_name.to_string(),
            }
        })
    };

    match entity_kind {
        crate::model::EntityKind::OffshorePump => {
            require_fluid_box_filters(entity_name, fluid_boxes, &[Some(required_fluid("water")?)])
        }
        crate::model::EntityKind::Boiler => require_fluid_box_filters(
            entity_name,
            fluid_boxes,
            &[
                Some(required_fluid("water")?),
                Some(required_fluid("steam")?),
            ],
        ),
        crate::model::EntityKind::SteamEngine => {
            require_fluid_box_filters(entity_name, fluid_boxes, &[Some(required_fluid("steam")?)])
        }
        crate::model::EntityKind::Pumpjack => {
            let Some(pumpjack) = pumpjack else {
                return Err(PrototypeLoadError::InvalidFluidBox {
                    entity: entity_name.to_string(),
                    box_index: 0,
                });
            };
            require_fluid_box_filters(entity_name, fluid_boxes, &[Some(pumpjack.output_fluid)])
        }
        _ => Ok(()),
    }
}

fn require_fluid_box_filters(
    entity_name: &str,
    fluid_boxes: &[FluidBoxPrototype],
    expected_filters: &[Option<FluidId>],
) -> Result<(), PrototypeLoadError> {
    if fluid_boxes.len() != expected_filters.len() {
        return Err(PrototypeLoadError::InvalidFluidBox {
            entity: entity_name.to_string(),
            box_index: fluid_boxes.len(),
        });
    }

    for (box_index, (fluid_box, expected_filter)) in
        fluid_boxes.iter().zip(expected_filters.iter()).enumerate()
    {
        if fluid_box.filter != *expected_filter {
            return Err(PrototypeLoadError::InvalidFluidBox {
                entity: entity_name.to_string(),
                box_index,
            });
        }
    }

    Ok(())
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
