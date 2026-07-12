use glam::IVec2;
use std::collections::HashMap;
use std::path::Path;

use crate::catalog::PrototypeCatalog;
use crate::error::PrototypeLoadError;
use crate::ids::{EntityPrototypeId, FluidId, ItemId, RecipeId, TechnologyId, TileId};
use crate::model::{
    ElectricPolePrototype, EnemyBaseGenerationConfig, EnemySpawnerPrototype, EntityPrototype,
    FluidBoxPrototype, FluidConnectionPrototype, FluidConnectionSide, FluidPrototype,
    InserterPrototype, ItemAmount, ItemPrototype, MiningDrillPrototype, PumpjackPrototype,
    RecipePrototype, ResourceDistanceScalingConfig, ResourceGenerationConfig,
    ResourcePatchGridConfig, StartingAreaConfig, TechnologyEffect, TechnologyPrototype,
    TerrainLayerConfig, TerrainNoiseConfig, TilePrototype, WORLD_GENERATION_FORMAT_VERSION,
    WorldGenerationConfig,
};
use crate::raw::{
    RawEnemyBaseGeneration, RawEntityPrototype, RawFluidBoxPrototype, RawFluidConnectionPrototype,
    RawFluidPrototype, RawItemPrototype, RawPrototypeCatalog, RawPumpjackPrototype,
    RawRecipePrototype, RawResourceGeneration, RawTechnologyEffect, RawTechnologyPrototype,
    RawTerrainLayer, RawTilePrototype, RawWorldGenerationConfig,
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
        let world_generation =
            load_world_generation(raw.world_generation, &item_ids_by_name, &tiles, &entities)?;

        Ok(Self {
            items,
            fluids,
            recipes,
            entities,
            tiles,
            technologies,
            world_generation,
            enemy_gameplay: raw.enemy_gameplay,
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
    world_generation: Option<RawWorldGenerationConfig>,
    enemy_gameplay: Option<crate::model::EnemyGameplayConfig>,
}

impl ValidatedRawCatalog {
    fn from_raw(raw: RawPrototypeCatalog) -> Result<Self, PrototypeLoadError> {
        match raw.enemy_gameplay.as_ref() {
            Some(config) => validate_enemy_gameplay(config)?,
            // A catalog with enemy content but no gameplay section would
            // silently run without any enemy simulation; fail loudly instead.
            None => {
                let has_enemy_content = raw
                    .entities
                    .iter()
                    .any(|entity| entity.enemy_spawner.is_some())
                    || raw
                        .world_generation
                        .as_ref()
                        .is_some_and(|config| config.enemy_bases.is_some());
                if has_enemy_content {
                    return Err(PrototypeLoadError::MissingEnemyGameplayConfig);
                }
            }
        }
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
            world_generation: raw.world_generation,
            enemy_gameplay: raw.enemy_gameplay,
        })
    }
}

fn validate_enemy_gameplay(
    config: &crate::model::EnemyGameplayConfig,
) -> Result<(), PrototypeLoadError> {
    let valid = config.generated_colony_min_spawners > 0
        && config.generated_colony_min_spawners <= config.generated_colony_max_spawners
        && config.generated_colony_max_spawners <= config.max_spawners_per_colony
        && config.colony_spawner_radius_tiles > 0
        && config.outpost_growth_interval_ticks > 0
        && config.raid_staging_timeout_ticks > 0
        && config.raid_cooldown_ticks > 0
        && config.expansion_minimum_age_ticks > 0
        && config.expansion_interval_ticks > 0
        && config.expansion_retry_ticks > 0
        && config.expansion_min_distance_chunks > 0
        && config.expansion_min_distance_chunks <= config.expansion_max_distance_chunks
        && config.expansion_candidate_limit > 0
        && config.expansion_colony_spacing_chunks > 0
        && config.expansion_player_spacing_tiles > 0
        && config.evolution_time_interval_ticks > 0
        && config.evolution_time_points > 0
        && config.evolution_pollution_units_per_point > 0
        && config.evolution_spawner_destroyed_points > 0
        && config.evolution_colony_destroyed_points > 0;
    if valid {
        Ok(())
    } else {
        Err(PrototypeLoadError::InvalidEnemyGameplayConfig {
            detail: "enemy gameplay intervals and ranges must be non-zero and ordered",
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
                ammo: item.ammo,
                repair: item.repair,
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
            match (
                build_item.is_some(),
                entity.building_category,
                entity.building_menu_order,
            ) {
                (true, Some(_), Some(_)) | (false, None, None) => {}
                (true, _, _) => {
                    return Err(PrototypeLoadError::InvalidBuildingMenuMetadata {
                        entity: name,
                        detail: "buildable entities require category and menu order",
                    });
                }
                (false, _, _) => {
                    return Err(PrototypeLoadError::InvalidBuildingMenuMetadata {
                        entity: name,
                        detail: "non-buildable entities must not define category or menu order",
                    });
                }
            }
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
                building_category: entity.building_category,
                building_menu_order: entity.building_menu_order,
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
                max_health: entity.max_health,
                pollution_per_minute_milli: entity.pollution_per_minute_milli,
                gun_turret: entity.gun_turret,
                enemy_spawner: entity.enemy_spawner.map(|spawner| EnemySpawnerPrototype {
                    max_alive_units: spawner.max_alive_units,
                    guard_units: spawner.guard_units,
                    free_spawn_interval_ticks: spawner.free_spawn_interval_ticks,
                    unit_spawn_pollution_cost_milli: spawner.unit_spawn_pollution_cost_milli,
                    pollution_absorption_per_tick_milli: spawner
                        .pollution_absorption_per_tick_milli,
                    unit: spawner.unit,
                }),
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

fn load_world_generation(
    raw: Option<RawWorldGenerationConfig>,
    item_ids_by_name: &HashMap<String, ItemId>,
    tiles: &[TilePrototype],
    entities: &[EntityPrototype],
) -> Result<WorldGenerationConfig, PrototypeLoadError> {
    let Some(raw) = raw else {
        return Ok(WorldGenerationConfig::default());
    };

    validate_world_generation(&raw)?;

    let terrain = resolve_terrain_layers(raw.terrain, tiles)?;
    let terrain_noise = raw
        .terrain_noise
        .map(|noise| TerrainNoiseConfig {
            scale: noise.scale,
            octaves: noise.octaves,
        })
        .unwrap_or_default();
    let resources = resolve_resources(raw.resources, item_ids_by_name)?;
    let enemy_bases = raw
        .enemy_bases
        .map(|bases| resolve_enemy_bases(bases, entities))
        .transpose()?;

    Ok(WorldGenerationConfig {
        version: raw.version,
        starting_area: StartingAreaConfig {
            min_chunk: raw.starting_area.min_chunk,
            max_chunk: raw.starting_area.max_chunk,
        },
        terrain,
        terrain_noise,
        patch_grid: ResourcePatchGridConfig {
            cell_size: raw.patch_grid.cell_size,
            jitter: raw.patch_grid.jitter,
            edge_noise: raw.patch_grid.edge_noise,
        },
        distance_scaling: raw
            .distance_scaling
            .map(|scaling| ResourceDistanceScalingConfig {
                interval_tiles: scaling.interval_tiles,
                richness_bonus_percent: scaling.richness_bonus_percent,
                radius_bonus_tiles: scaling.radius_bonus_tiles,
                max_radius_bonus_tiles: scaling.max_radius_bonus_tiles,
            }),
        resources,
        enemy_bases,
    })
}

/// Resolve the enemy base spawner entity name and validate placement rules.
fn resolve_enemy_bases(
    bases: RawEnemyBaseGeneration,
    entities: &[EntityPrototype],
) -> Result<EnemyBaseGenerationConfig, PrototypeLoadError> {
    let spawner_entity = entities
        .iter()
        .find(|entity| entity.name == bases.spawner_entity)
        .ok_or(PrototypeLoadError::MissingWorldGenerationSpawnerEntity {
            entity: bases.spawner_entity.clone(),
        })?;
    if spawner_entity.enemy_spawner.is_none() {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "enemy base spawner entity must declare an enemy_spawner section",
        });
    }
    if bases.frequency_percent > 100 {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "enemy base frequency_percent must not exceed 100",
        });
    }

    Ok(EnemyBaseGenerationConfig {
        spawner_entity: spawner_entity.id,
        frequency_percent: bases.frequency_percent,
        min_distance_tiles: bases.min_distance_tiles,
    })
}

/// Validate the top-level world generation fields that do not require
/// resolving names against loaded prototypes.
fn validate_world_generation(raw: &RawWorldGenerationConfig) -> Result<(), PrototypeLoadError> {
    if raw.version != WORLD_GENERATION_FORMAT_VERSION {
        return Err(PrototypeLoadError::UnsupportedWorldGenerationVersion {
            found: raw.version,
            supported: WORLD_GENERATION_FORMAT_VERSION,
        });
    }
    if raw.starting_area.min_chunk > raw.starting_area.max_chunk {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "starting area min_chunk must not exceed max_chunk",
        });
    }
    if raw.patch_grid.cell_size < 1 {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "patch grid cell_size must be at least 1",
        });
    }
    if raw.patch_grid.jitter < 0 || raw.patch_grid.edge_noise < 0 {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "patch grid jitter and edge_noise must not be negative",
        });
    }
    if raw.terrain.is_empty() {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "terrain must declare at least one layer",
        });
    }
    if let Some(noise) = &raw.terrain_noise {
        if noise.scale < 1 {
            return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                detail: "terrain noise scale must be at least 1",
            });
        }
        if noise.octaves < 1 || noise.octaves > 8 {
            return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                detail: "terrain noise octaves must be between 1 and 8",
            });
        }
    }
    if let Some(scaling) = &raw.distance_scaling {
        if scaling.interval_tiles < 1 {
            return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                detail: "distance scaling interval_tiles must be at least 1",
            });
        }
        if scaling.radius_bonus_tiles > scaling.max_radius_bonus_tiles {
            return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                detail: "distance scaling radius_bonus_tiles must not exceed \
                         max_radius_bonus_tiles",
            });
        }
    }
    Ok(())
}

/// Resolve terrain layer tile names against loaded tiles and validate weights.
fn resolve_terrain_layers(
    terrain: Vec<RawTerrainLayer>,
    tiles: &[TilePrototype],
) -> Result<Vec<TerrainLayerConfig>, PrototypeLoadError> {
    let terrain = terrain
        .into_iter()
        .map(|layer| {
            let tile = tiles
                .iter()
                .find(|tile| tile.name == layer.tile)
                .map(|tile| tile.id)
                .ok_or(PrototypeLoadError::MissingWorldGenerationTile { tile: layer.tile })?;
            Ok(TerrainLayerConfig {
                tile,
                weight: layer.weight,
            })
        })
        .collect::<Result<Vec<_>, PrototypeLoadError>>()?;
    if terrain.iter().all(|layer| layer.weight == 0) {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "terrain layer weights must not all be zero",
        });
    }
    Ok(terrain)
}

/// Resolve resource item names against loaded items and validate each entry.
fn resolve_resources(
    resources: Vec<RawResourceGeneration>,
    item_ids_by_name: &HashMap<String, ItemId>,
) -> Result<Vec<ResourceGenerationConfig>, PrototypeLoadError> {
    let mut seen_resource_items = std::collections::HashSet::new();
    resources
        .into_iter()
        .map(|resource| {
            let resource_item = *item_ids_by_name.get(&resource.item).ok_or_else(|| {
                PrototypeLoadError::MissingWorldGenerationResourceItem {
                    item: resource.item.clone(),
                }
            })?;
            if !seen_resource_items.insert(resource_item) {
                return Err(PrototypeLoadError::DuplicateWorldGenerationResource {
                    item: resource.item,
                });
            }
            if resource.frequency_percent > 100 {
                return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                    detail: "resource frequency_percent must not exceed 100",
                });
            }
            if resource.radius < 1 {
                return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                    detail: "resource radius must be at least 1",
                });
            }
            if resource.richness == 0 {
                return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                    detail: "resource richness must be at least 1",
                });
            }
            Ok(ResourceGenerationConfig {
                resource_item,
                extraction: resource.extraction,
                frequency_percent: resource.frequency_percent,
                radius: resource.radius,
                richness: resource.richness,
                starting_patch: resource
                    .starting_patch
                    .map(|offset| IVec2::new(offset.x, offset.y)),
            })
        })
        .collect::<Result<Vec<_>, PrototypeLoadError>>()
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
                pollution_absorption_per_minute_milli: tile.pollution_absorption_per_minute_milli,
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
