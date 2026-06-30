use super::super::*;
use super::ids::*;
use super::inventory::*;

pub(super) fn validate_burner_mining_drill(
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

pub(super) fn validate_furnace(
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

pub(super) fn validate_boiler(
    sim: &Simulation,
    entity_id: EntityId,
    state: &BoilerState,
) -> Result<(), SimValidationError> {
    validate_single_slot(&sim.world.prototypes, state.energy.fuel_slot)?;

    if let Some(stack) = state.energy.fuel_slot
        && fuel_value_joules(&sim.world.prototypes, stack.item_id).is_none()
    {
        return Err(SimValidationError::InvalidMachineItem {
            entity_id,
            item_id: stack.item_id,
        });
    }

    Ok(())
}

pub(super) fn validate_assembler(
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

pub(super) fn validate_lab(
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

pub(super) fn validate_belt_segment(
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

pub(super) fn validate_splitter_state(
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

pub(super) fn validate_inserter(
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
    let (pickup_tile, drop_tile) = inserter_transfer_tiles(&sim.world.prototypes, placed).ok_or(
        SimValidationError::InvalidCatalogEntityPrototype {
            prototype_id: placed.prototype_id,
        },
    )?;
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
