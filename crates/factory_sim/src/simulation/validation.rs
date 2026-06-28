use super::*;

pub fn validate_simulation(sim: &Simulation) -> Result<(), SimValidationError> {
    validate_catalog(&sim.world.prototypes)?;
    validate_world_resources(&sim.world)?;
    validate_placed_entities(sim)?;
    validate_entity_occupancy(&sim.entities)?;
    validate_entity_state_ownership_and_kind(sim)?;

    validate_inventory(&sim.world.prototypes, &sim.player_inventory)?;
    validate_crafting_queue(sim)?;
    validate_research_state(sim)?;

    for inventory in sim.entities.entity_inventories.values() {
        validate_inventory(&sim.world.prototypes, inventory)?;
    }
    for (entity_id, state) in &sim.entities.burner_mining_drills {
        validate_burner_mining_drill(sim, *entity_id, state)?;
    }
    for (entity_id, state) in &sim.entities.furnaces {
        validate_furnace(sim, *entity_id, state)?;
    }
    for (entity_id, state) in &sim.entities.assembling_machines {
        validate_assembler(sim, *entity_id, state)?;
    }
    for (entity_id, state) in &sim.entities.labs {
        validate_lab(sim, *entity_id, state)?;
    }
    for (entity_id, segment) in &sim.entities.transport_belts {
        validate_belt_segment(sim, *entity_id, segment)?;
    }
    for (entity_id, state) in &sim.entities.splitters {
        validate_splitter_state(sim, *entity_id, state)?;
    }
    for (entity_id, state) in &sim.entities.inserters {
        validate_inserter(sim, *entity_id, state)?;
    }

    Ok(())
}

impl Simulation {
    pub fn validate(&self) -> Result<(), SimValidationError> {
        validate_simulation(self)
    }

    pub fn validate_item_conservation(&self) -> bool {
        self.validate().is_ok()
    }

    pub fn validate_state(&self) -> Result<(), SimulationValidationError> {
        self.validate()
    }
}

fn validate_catalog(catalog: &PrototypeCatalog) -> Result<(), SimValidationError> {
    for (index, item) in catalog.items.iter().enumerate() {
        if item.id.index() != index {
            return Err(SimValidationError::UnknownItem(item.id));
        }
    }

    for (index, recipe) in catalog.recipes.iter().enumerate() {
        if recipe.id.index() != index {
            return Err(SimValidationError::InvalidCraftingRecipe {
                recipe_id: recipe.id,
            });
        }

        for amount in recipe.ingredients.iter().chain(recipe.products.iter()) {
            if !item_exists(catalog, amount.item) {
                return Err(SimValidationError::InvalidRecipeItem {
                    recipe_id: recipe.id,
                    item_id: amount.item,
                });
            }
        }
    }

    for (index, technology) in catalog.technologies.iter().enumerate() {
        if technology.id.index() != index {
            return Err(SimValidationError::InvalidResearchTechnology {
                technology_id: technology.id,
            });
        }

        for science_pack in &technology.science_packs {
            if !item_exists(catalog, science_pack.item) {
                return Err(SimValidationError::InvalidTechnologyItem {
                    technology_id: technology.id,
                    item_id: science_pack.item,
                });
            }
        }
        for prerequisite_id in &technology.prerequisites {
            if technology_by_id(catalog, *prerequisite_id).is_none() {
                return Err(SimValidationError::InvalidTechnologyPrerequisite {
                    technology_id: technology.id,
                    prerequisite_id: *prerequisite_id,
                });
            }
        }
        for effect in &technology.effects {
            let TechnologyEffect::UnlockRecipe(recipe_id) = *effect;
            if recipe_by_id(catalog, recipe_id).is_none() {
                return Err(SimValidationError::InvalidTechnologyRecipe {
                    technology_id: technology.id,
                    recipe_id,
                });
            }
        }
    }

    for prototype in &catalog.entities {
        if let Some(item_id) = prototype.build_item
            && !item_exists(catalog, item_id)
        {
            return Err(SimValidationError::UnknownItem(item_id));
        }

        match prototype.entity_kind {
            EntityKind::TransportBelt => {
                let Some(transport_belt) = prototype.transport_belt.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if transport_belt.speed_subtiles_per_tick == 0 {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::Splitter => {
                let Some(splitter) = prototype.splitter.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if splitter.speed_subtiles_per_tick == 0 {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn validate_world_resources(world: &WorldSim) -> Result<(), SimValidationError> {
    for chunk in world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            if world
                .prototypes
                .tiles
                .get(tile.tile_id.index())
                .is_none_or(|prototype| prototype.id != tile.tile_id)
            {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                return Err(SimValidationError::MissingTile {
                    x: chunk.coord.x * CHUNK_SIZE + local_x,
                    y: chunk.coord.y * CHUNK_SIZE + local_y,
                });
            }

            if let Some(resource) = tile.resource
                && !item_exists(&world.prototypes, resource.resource_item)
            {
                return Err(SimValidationError::UnknownItem(resource.resource_item));
            }
        }
    }

    Ok(())
}

fn validate_placed_entities(sim: &Simulation) -> Result<(), SimValidationError> {
    for placed in sim.entities.placed_entities.values() {
        let prototype = entity_prototype_by_id(&sim.world.prototypes, placed.prototype_id).ok_or(
            SimValidationError::InvalidEntityPrototype {
                entity_id: placed.id,
                prototype_id: placed.prototype_id,
            },
        )?;
        let expected_footprint = EntityFootprint::from_size(
            placed.x,
            placed.y,
            prototype.size.x,
            prototype.size.y,
            placed.direction,
        );

        if placed.footprint != expected_footprint || placed.footprint.validate().is_err() {
            return Err(SimValidationError::InvalidEntityFootprint {
                entity_id: placed.id,
            });
        }

        for (x, y) in placed.footprint.tiles() {
            let tile = sim
                .world
                .tile_at(x, y)
                .ok_or(SimValidationError::InvalidEntityTile {
                    entity_id: placed.id,
                    x,
                    y,
                })?;
            if !entity_can_occupy_tile(prototype, tile) {
                return Err(SimValidationError::InvalidEntityTile {
                    entity_id: placed.id,
                    x,
                    y,
                });
            }
        }
    }

    Ok(())
}

fn entity_can_occupy_tile(prototype: &factory_data::EntityPrototype, tile: &TileCell) -> bool {
    if prototype.entity_kind == EntityKind::MiningDrill {
        tile.collision.walkable
    } else {
        tile.collision.buildable
    }
}

fn validate_inventory(
    catalog: &PrototypeCatalog,
    inventory: &Inventory,
) -> Result<(), SimValidationError> {
    for stack in inventory.slots.iter().flatten() {
        validate_item_stack(catalog, *stack)?;
    }

    Ok(())
}

fn validate_item_stack(
    catalog: &PrototypeCatalog,
    stack: ItemStack,
) -> Result<(), SimValidationError> {
    if stack.count == 0 {
        return Err(SimValidationError::EmptyItemStack(stack.item_id));
    }

    let stack_size = item_stack_size(catalog, stack.item_id)
        .ok_or(SimValidationError::UnknownItem(stack.item_id))?;
    if stack.count > stack_size {
        return Err(SimValidationError::StackExceedsLimit {
            item_id: stack.item_id,
            count: stack.count,
            stack_size,
        });
    }

    Ok(())
}

fn validate_entity_occupancy(entities: &EntityStore) -> Result<(), SimValidationError> {
    let mut expected = BTreeMap::new();

    for placed in entities.placed_entities.values() {
        for (x, y) in placed.footprint.tiles() {
            if let Some(first) = expected.insert((x, y), placed.id) {
                return Err(SimValidationError::EntityOverlap {
                    x,
                    y,
                    first,
                    second: placed.id,
                });
            }
        }
    }

    if expected != entities.occupancy.occupied_tiles {
        return Err(SimValidationError::OccupancyMismatch);
    }

    Ok(())
}

fn validate_entity_state_ownership_and_kind(sim: &Simulation) -> Result<(), SimValidationError> {
    for entity_id in sim.entities.entity_inventories.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::Chest)?;
    }
    for entity_id in sim.entities.burner_mining_drills.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::MiningDrill)?;
    }
    for entity_id in sim.entities.furnaces.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::Furnace)?;
    }
    for entity_id in sim.entities.assembling_machines.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::AssemblingMachine)?;
    }
    for entity_id in sim.entities.labs.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::Lab)?;
    }
    for entity_id in sim.entities.transport_belts.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::TransportBelt)?;
    }
    for entity_id in sim.entities.splitters.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::Splitter)?;
    }
    for entity_id in sim.entities.inserters.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::Inserter)?;
    }

    Ok(())
}

fn validate_entity_state_kind(
    sim: &Simulation,
    entity_id: EntityId,
    expected_kind: EntityKind,
) -> Result<(), SimValidationError> {
    let placed = sim
        .entities
        .placed_entities
        .get(&entity_id)
        .ok_or(SimValidationError::OrphanEntityState(entity_id))?;
    let prototype = entity_prototype_by_id(&sim.world.prototypes, placed.prototype_id).ok_or(
        SimValidationError::InvalidEntityPrototype {
            entity_id,
            prototype_id: placed.prototype_id,
        },
    )?;

    if prototype.entity_kind != expected_kind {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }

    Ok(())
}

fn validate_crafting_queue(sim: &Simulation) -> Result<(), SimValidationError> {
    for job in &sim.crafting_queue.entries {
        let Some(recipe) = recipe_by_id(&sim.world.prototypes, job.recipe_id) else {
            return Err(SimValidationError::InvalidCraftingRecipe {
                recipe_id: job.recipe_id,
            });
        };
        if !matches!(
            recipe.category,
            CraftingCategory::Crafting | CraftingCategory::Manual
        ) {
            return Err(SimValidationError::InvalidCraftingRecipe {
                recipe_id: job.recipe_id,
            });
        }
    }

    Ok(())
}

fn validate_research_state(sim: &Simulation) -> Result<(), SimValidationError> {
    for technology in &sim.world.prototypes.technologies {
        let state = sim
            .research
            .technologies
            .get(technology.id.index())
            .filter(|state| state.technology_id == technology.id)
            .ok_or(SimValidationError::InvalidResearchTechnology {
                technology_id: technology.id,
            })?;

        if state.progress_units > technology.required_units {
            return Err(SimValidationError::InvalidResearchProgress {
                technology_id: technology.id,
                progress_units: state.progress_units,
                required_units: technology.required_units,
            });
        }
    }

    for state in &sim.research.technologies {
        if technology_by_id(&sim.world.prototypes, state.technology_id).is_none() {
            return Err(SimValidationError::InvalidResearchTechnology {
                technology_id: state.technology_id,
            });
        }
    }

    if let Some(technology_id) = sim.research.active {
        let state = sim
            .research
            .technologies
            .get(technology_id.index())
            .filter(|state| state.technology_id == technology_id)
            .ok_or(SimValidationError::InvalidActiveResearch { technology_id })?;
        if state.unlocked {
            return Err(SimValidationError::InvalidActiveResearch { technology_id });
        }
    }

    Ok(())
}

fn validate_burner_mining_drill(
    sim: &Simulation,
    entity_id: EntityId,
    state: &BurnerMiningDrillState,
) -> Result<(), SimValidationError> {
    validate_single_slot(&sim.world.prototypes, state.energy.fuel_slot)?;
    validate_single_slot(&sim.world.prototypes, state.output_slot)?;
    if let Some(stack) = state.output_slot {
        let ids = WorldPrototypeIds::from_catalog(&sim.world.prototypes);
        if !ids.resources.contains(&stack.item_id) {
            return Err(SimValidationError::InvalidMachineItem {
                entity_id,
                item_id: stack.item_id,
            });
        }
    }

    Ok(())
}

fn validate_furnace(
    sim: &Simulation,
    entity_id: EntityId,
    state: &FurnaceState,
) -> Result<(), SimValidationError> {
    validate_single_slot(&sim.world.prototypes, state.input_slot)?;
    validate_single_slot(&sim.world.prototypes, state.energy.fuel_slot)?;
    validate_single_slot(&sim.world.prototypes, state.output_slot)?;

    if let Some(recipe_id) = state.active_recipe {
        smelting_recipe_by_id(&sim.world.prototypes, recipe_id).ok_or(
            SimValidationError::InvalidMachineRecipe {
                entity_id,
                recipe_id,
            },
        )?;
    }

    Ok(())
}

fn validate_assembler(
    sim: &Simulation,
    entity_id: EntityId,
    state: &AssemblingMachineState,
) -> Result<(), SimValidationError> {
    validate_inventory(&sim.world.prototypes, &state.input_inventory)?;
    validate_inventory(&sim.world.prototypes, &state.output_inventory)?;

    let Some(recipe_id) = state.selected_recipe else {
        return Ok(());
    };

    sim.world
        .prototypes
        .recipes
        .get(recipe_id.index())
        .filter(|recipe| recipe.id == recipe_id && recipe.category == CraftingCategory::Crafting)
        .ok_or(SimValidationError::InvalidMachineRecipe {
            entity_id,
            recipe_id,
        })?;

    Ok(())
}

fn validate_lab(
    sim: &Simulation,
    entity_id: EntityId,
    state: &LabState,
) -> Result<(), SimValidationError> {
    validate_inventory(&sim.world.prototypes, &state.inventory)?;
    for stack in state.inventory.slots.iter().flatten() {
        if !lab_can_accept_item(&sim.world.prototypes, stack.item_id) {
            return Err(SimValidationError::InvalidMachineItem {
                entity_id,
                item_id: stack.item_id,
            });
        }
    }

    if let Some(technology_id) = state.active_technology
        && technology_by_id(&sim.world.prototypes, technology_id).is_none()
    {
        return Err(SimValidationError::InvalidActiveResearch { technology_id });
    }

    Ok(())
}

fn validate_belt_segment(
    sim: &Simulation,
    entity_id: EntityId,
    segment: &BeltSegment,
) -> Result<(), SimValidationError> {
    if let Some(placed) = sim.entities.placed_entity(entity_id)
        && placed.direction != segment.dir
    {
        return Err(SimValidationError::OccupancyMismatch);
    }
    let placed = sim
        .entities
        .placed_entity(entity_id)
        .ok_or(SimValidationError::OrphanEntityState(entity_id))?;
    let prototype = entity_prototype_by_id(&sim.world.prototypes, placed.prototype_id).ok_or(
        SimValidationError::InvalidEntityPrototype {
            entity_id,
            prototype_id: placed.prototype_id,
        },
    )?;
    if prototype
        .transport_belt
        .as_ref()
        .is_none_or(|transport_belt| {
            transport_belt.speed_subtiles_per_tick != segment.speed_subtiles_per_tick
        })
    {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }

    for (lane_index, lane) in segment.lanes.iter().enumerate() {
        validate_transport_lane_items(sim, entity_id, lane_index, lane)?;
    }

    Ok(())
}

fn validate_splitter_state(
    sim: &Simulation,
    entity_id: EntityId,
    state: &SplitterState,
) -> Result<(), SimValidationError> {
    if let Some(placed) = sim.entities.placed_entity(entity_id)
        && placed.direction != state.dir
    {
        return Err(SimValidationError::OccupancyMismatch);
    }
    let placed = sim
        .entities
        .placed_entity(entity_id)
        .ok_or(SimValidationError::OrphanEntityState(entity_id))?;
    let prototype = entity_prototype_by_id(&sim.world.prototypes, placed.prototype_id).ok_or(
        SimValidationError::InvalidEntityPrototype {
            entity_id,
            prototype_id: placed.prototype_id,
        },
    )?;
    if prototype
        .splitter
        .as_ref()
        .is_none_or(|splitter| splitter.speed_subtiles_per_tick != state.speed_subtiles_per_tick)
    {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }

    for (lane_index, output_port) in state.next_output_by_lane.iter().copied().enumerate() {
        if output_port >= 2 {
            return Err(SimValidationError::InvalidSplitterOutputCursor {
                entity_id,
                lane_index,
                output_port,
            });
        }
    }

    for (input_port, input_lanes) in state.input_lanes.iter().enumerate() {
        for (lane_index, lane) in input_lanes.iter().enumerate() {
            validate_transport_lane_items(sim, entity_id, input_port * 2 + lane_index, lane)?;
        }
    }

    Ok(())
}

fn validate_transport_lane_items(
    sim: &Simulation,
    entity_id: EntityId,
    lane_index: usize,
    lane: &BeltLane,
) -> Result<(), SimValidationError> {
    let mut previous_position = None;
    for item in &lane.items {
        validate_item_stack(
            &sim.world.prototypes,
            ItemStack {
                item_id: item.item_id,
                count: 1,
            },
        )?;
        if item.position_subtile >= BELT_SUBTILES_PER_TILE {
            return Err(SimValidationError::InvalidBeltItemPosition {
                entity_id,
                lane_index,
                position_subtile: item.position_subtile,
            });
        }
        if let Some(previous) = previous_position
            && u32::from(item.position_subtile)
                < u32::from(previous) + u32::from(BELT_ITEM_SPACING_SUBTILES)
        {
            return Err(SimValidationError::BeltItemSpacingViolation {
                entity_id,
                lane_index,
            });
        }
        previous_position = Some(item.position_subtile);
    }

    Ok(())
}

fn validate_inserter(
    sim: &Simulation,
    entity_id: EntityId,
    state: &InserterState,
) -> Result<(), SimValidationError> {
    if let InserterState::Holding { item } = state {
        validate_item_stack(&sim.world.prototypes, *item)?;
    }

    let Some(placed) = sim.entities.placed_entity(entity_id) else {
        return Err(SimValidationError::OrphanEntityState(entity_id));
    };
    let (pickup_tile, drop_tile) = inserter_transfer_tiles(placed);
    validate_inserter_target(sim, entity_id, pickup_tile)?;
    validate_inserter_target(sim, entity_id, drop_tile)?;

    Ok(())
}

fn validate_inserter_target(
    sim: &Simulation,
    entity_id: EntityId,
    target: (i32, i32),
) -> Result<(), SimValidationError> {
    if let Some(target_entity_id) = sim.entities.occupancy.entity_at(target.0, target.1)
        && !sim.entities.placed_entities.contains_key(&target_entity_id)
    {
        return Err(SimValidationError::InvalidInserterTarget {
            entity_id,
            x: target.0,
            y: target.1,
        });
    }

    Ok(())
}

fn validate_single_slot(
    catalog: &PrototypeCatalog,
    slot: Option<ItemStack>,
) -> Result<(), SimValidationError> {
    if let Some(stack) = slot {
        validate_item_stack(catalog, stack)?;
    }

    Ok(())
}

fn item_exists(catalog: &PrototypeCatalog, item_id: ItemId) -> bool {
    catalog
        .items
        .get(item_id.index())
        .is_some_and(|item| item.id == item_id)
}

fn recipe_by_id(
    catalog: &PrototypeCatalog,
    recipe_id: RecipeId,
) -> Option<&factory_data::RecipePrototype> {
    catalog
        .recipes
        .get(recipe_id.index())
        .filter(|recipe| recipe.id == recipe_id)
}

fn smelting_recipe_by_id(
    catalog: &PrototypeCatalog,
    recipe_id: RecipeId,
) -> Option<&factory_data::RecipePrototype> {
    recipe_by_id(catalog, recipe_id).filter(|recipe| recipe.category == CraftingCategory::Smelting)
}

fn technology_by_id(
    catalog: &PrototypeCatalog,
    technology_id: TechnologyId,
) -> Option<&factory_data::TechnologyPrototype> {
    catalog
        .technologies
        .get(technology_id.index())
        .filter(|technology| technology.id == technology_id)
}

fn entity_prototype_by_id(
    catalog: &PrototypeCatalog,
    prototype_id: EntityPrototypeId,
) -> Option<&factory_data::EntityPrototype> {
    catalog
        .entities
        .get(prototype_id.index())
        .filter(|prototype| prototype.id == prototype_id)
}
