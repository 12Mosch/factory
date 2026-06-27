use super::*;

impl Simulation {
    pub fn validate_item_conservation(&self) -> bool {
        self.validate_state().is_ok()
    }

    pub fn validate_state(&self) -> Result<(), SimulationValidationError> {
        validate_inventory(&self.world.prototypes, &self.player_inventory)?;
        validate_entity_occupancy(&self.entities)?;
        validate_entity_state_ownership(&self.entities)?;

        for inventory in self.entities.entity_inventories.values() {
            validate_inventory(&self.world.prototypes, inventory)?;
        }
        for (entity_id, state) in &self.entities.burner_mining_drills {
            validate_burner_mining_drill(self, *entity_id, state)?;
        }
        for (entity_id, state) in &self.entities.furnaces {
            validate_furnace(self, *entity_id, state)?;
        }
        for (entity_id, state) in &self.entities.assembling_machines {
            validate_assembler(self, *entity_id, state)?;
        }
        for (entity_id, state) in &self.entities.labs {
            validate_lab(self, *entity_id, state)?;
        }
        for (entity_id, segment) in &self.entities.transport_belts {
            validate_belt_segment(self, *entity_id, segment)?;
        }
        for (entity_id, state) in &self.entities.inserters {
            validate_inserter(self, *entity_id, state)?;
        }

        Ok(())
    }
}

fn validate_inventory(
    catalog: &PrototypeCatalog,
    inventory: &Inventory,
) -> Result<(), SimulationValidationError> {
    for stack in inventory.slots.iter().flatten() {
        validate_item_stack(catalog, *stack)?;
    }

    Ok(())
}

fn validate_item_stack(
    catalog: &PrototypeCatalog,
    stack: ItemStack,
) -> Result<(), SimulationValidationError> {
    if stack.count == 0 {
        return Err(SimulationValidationError::EmptyItemStack(stack.item_id));
    }

    let stack_size = item_stack_size(catalog, stack.item_id)
        .ok_or(SimulationValidationError::UnknownItem(stack.item_id))?;
    if stack.count > stack_size {
        return Err(SimulationValidationError::StackExceedsLimit {
            item_id: stack.item_id,
            count: stack.count,
            stack_size,
        });
    }

    Ok(())
}

fn validate_entity_occupancy(entities: &EntityStore) -> Result<(), SimulationValidationError> {
    let mut expected = BTreeMap::new();

    for placed in entities.placed_entities.values() {
        for (x, y) in placed.footprint.tiles() {
            if let Some(first) = expected.insert((x, y), placed.id) {
                return Err(SimulationValidationError::EntityOverlap {
                    x,
                    y,
                    first,
                    second: placed.id,
                });
            }
        }
    }

    if expected != entities.occupancy.occupied_tiles {
        return Err(SimulationValidationError::OccupancyMismatch);
    }

    Ok(())
}

fn validate_entity_state_ownership(
    entities: &EntityStore,
) -> Result<(), SimulationValidationError> {
    for entity_id in entities
        .entity_inventories
        .keys()
        .chain(entities.burner_mining_drills.keys())
        .chain(entities.furnaces.keys())
        .chain(entities.assembling_machines.keys())
        .chain(entities.labs.keys())
        .chain(entities.transport_belts.keys())
        .chain(entities.inserters.keys())
    {
        if !entities.placed_entities.contains_key(entity_id) {
            return Err(SimulationValidationError::OrphanEntityState(*entity_id));
        }
    }

    Ok(())
}

fn validate_burner_mining_drill(
    sim: &Simulation,
    entity_id: EntityId,
    state: &BurnerMiningDrillState,
) -> Result<(), SimulationValidationError> {
    validate_single_slot(&sim.world.prototypes, state.energy.fuel_slot)?;
    validate_single_slot(&sim.world.prototypes, state.output_slot)?;
    if let Some(stack) = state.output_slot {
        let ids = WorldPrototypeIds::from_catalog(&sim.world.prototypes);
        if !ids.resources.contains(&stack.item_id) {
            return Err(SimulationValidationError::InvalidMachineItem {
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
) -> Result<(), SimulationValidationError> {
    validate_single_slot(&sim.world.prototypes, state.input_slot)?;
    validate_single_slot(&sim.world.prototypes, state.energy.fuel_slot)?;
    validate_single_slot(&sim.world.prototypes, state.output_slot)?;

    if let Some(stack) = state.input_slot
        && first_matching_smelting_recipe(&sim.world.prototypes, stack.item_id).is_none()
    {
        return Err(SimulationValidationError::InvalidMachineItem {
            entity_id,
            item_id: stack.item_id,
        });
    }

    if let Some(recipe_id) = state.active_recipe {
        let recipe = smelting_recipe_by_id(&sim.world.prototypes, recipe_id).ok_or(
            SimulationValidationError::InvalidMachineRecipe {
                entity_id,
                recipe_id,
            },
        )?;
        if let Some(stack) = state.output_slot
            && !recipe
                .products
                .iter()
                .any(|product| product.item == stack.item_id)
        {
            return Err(SimulationValidationError::InvalidMachineItem {
                entity_id,
                item_id: stack.item_id,
            });
        }
    } else if let Some(stack) = state.output_slot
        && !sim.world.prototypes.recipes.iter().any(|recipe| {
            recipe.category == CraftingCategory::Smelting
                && recipe
                    .products
                    .iter()
                    .any(|product| product.item == stack.item_id)
        })
    {
        return Err(SimulationValidationError::InvalidMachineItem {
            entity_id,
            item_id: stack.item_id,
        });
    }

    Ok(())
}

fn validate_assembler(
    sim: &Simulation,
    entity_id: EntityId,
    state: &AssemblingMachineState,
) -> Result<(), SimulationValidationError> {
    validate_inventory(&sim.world.prototypes, &state.input_inventory)?;
    validate_inventory(&sim.world.prototypes, &state.output_inventory)?;

    let Some(recipe_id) = state.selected_recipe else {
        if let Some(stack) = state
            .input_inventory
            .slots
            .iter()
            .chain(state.output_inventory.slots.iter())
            .flatten()
            .next()
        {
            return Err(SimulationValidationError::InvalidMachineItem {
                entity_id,
                item_id: stack.item_id,
            });
        }
        return Ok(());
    };

    let recipe = sim
        .world
        .prototypes
        .recipes
        .get(recipe_id.index())
        .filter(|recipe| recipe.id == recipe_id && recipe.category == CraftingCategory::Crafting)
        .ok_or(SimulationValidationError::InvalidMachineRecipe {
            entity_id,
            recipe_id,
        })?;

    for stack in state.input_inventory.slots.iter().flatten() {
        if !recipe
            .ingredients
            .iter()
            .any(|ingredient| ingredient.item == stack.item_id)
        {
            return Err(SimulationValidationError::InvalidMachineItem {
                entity_id,
                item_id: stack.item_id,
            });
        }
    }

    for stack in state.output_inventory.slots.iter().flatten() {
        if !recipe
            .products
            .iter()
            .any(|product| product.item == stack.item_id)
        {
            return Err(SimulationValidationError::InvalidMachineItem {
                entity_id,
                item_id: stack.item_id,
            });
        }
    }

    Ok(())
}

fn validate_lab(
    sim: &Simulation,
    entity_id: EntityId,
    state: &LabState,
) -> Result<(), SimulationValidationError> {
    validate_inventory(&sim.world.prototypes, &state.inventory)?;
    for stack in state.inventory.slots.iter().flatten() {
        if !lab_can_accept_item(&sim.world.prototypes, stack.item_id) {
            return Err(SimulationValidationError::InvalidMachineItem {
                entity_id,
                item_id: stack.item_id,
            });
        }
    }

    Ok(())
}

fn validate_belt_segment(
    sim: &Simulation,
    entity_id: EntityId,
    segment: &BeltSegment,
) -> Result<(), SimulationValidationError> {
    if let Some(placed) = sim.entities.placed_entity(entity_id)
        && placed.direction != segment.dir
    {
        return Err(SimulationValidationError::OccupancyMismatch);
    }

    for (lane_index, lane) in segment.lanes.iter().enumerate() {
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
                return Err(SimulationValidationError::InvalidBeltItemPosition {
                    entity_id,
                    lane_index,
                    position_subtile: item.position_subtile,
                });
            }
            if let Some(previous) = previous_position
                && u32::from(item.position_subtile)
                    < u32::from(previous) + u32::from(BELT_ITEM_SPACING_SUBTILES)
            {
                return Err(SimulationValidationError::BeltItemSpacingViolation {
                    entity_id,
                    lane_index,
                });
            }
            previous_position = Some(item.position_subtile);
        }
    }

    Ok(())
}

fn validate_inserter(
    sim: &Simulation,
    _entity_id: EntityId,
    state: &InserterState,
) -> Result<(), SimulationValidationError> {
    if let InserterState::Holding { item } = state {
        validate_item_stack(&sim.world.prototypes, *item)?;
    }

    Ok(())
}

fn validate_single_slot(
    catalog: &PrototypeCatalog,
    slot: Option<ItemStack>,
) -> Result<(), SimulationValidationError> {
    if let Some(stack) = slot {
        validate_item_stack(catalog, stack)?;
    }

    Ok(())
}

fn smelting_recipe_by_id(
    catalog: &PrototypeCatalog,
    recipe_id: RecipeId,
) -> Option<&factory_data::RecipePrototype> {
    catalog
        .recipes
        .get(recipe_id.index())
        .filter(|recipe| recipe.id == recipe_id && recipe.category == CraftingCategory::Smelting)
}
