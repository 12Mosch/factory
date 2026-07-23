use super::super::*;
use super::ids::*;
use super::inventory::*;

/// A machine's energy state must use the variant its prototype declares: a
/// burner prototype owns burner fuel state, an electric prototype has none.
pub(in crate::simulation) fn validate_machine_energy_matches_prototype(
    sim: &Simulation,
    entity_id: EntityId,
    energy: &MachineEnergy,
) -> Result<(), SimValidationError> {
    let prototype = sim
        .entities
        .placed_entity(entity_id)
        .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
        .ok_or(SimValidationError::OrphanEntityState(entity_id))?;
    let matches_prototype = match energy {
        MachineEnergy::Burner(_) => prototype.burner.is_some(),
        MachineEnergy::Electric => {
            prototype.burner.is_none() && prototype.electric_energy_source.is_some()
        }
    };
    if !matches_prototype {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }

    Ok(())
}

pub(in crate::simulation) fn validate_inserter_energy(
    sim: &Simulation,
    entity_id: EntityId,
    energy: &MachineEnergy,
) -> Result<(), SimValidationError> {
    let prototype = sim
        .entities
        .placed_entity(entity_id)
        .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
        .ok_or(SimValidationError::OrphanEntityState(entity_id))?;
    if prototype.entity_kind != EntityKind::Inserter {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }
    validate_machine_energy_matches_prototype(sim, entity_id, energy)?;
    if let Some(fuel_slot) = energy.fuel_slot() {
        validate_item_slot(&sim.world.prototypes, fuel_slot)?;
        validate_slot_policy(sim, entity_id, fuel_slot, ItemSlotPolicy::Fuel)?;
    }
    Ok(())
}

pub(in crate::simulation) fn validate_mining_drill(
    sim: &Simulation,
    entity_id: EntityId,
    state: &MiningDrillState,
) -> Result<(), SimValidationError> {
    validate_machine_energy_matches_prototype(sim, entity_id, &state.energy)?;
    if let Some(fuel_slot) = state.energy.fuel_slot() {
        validate_item_slot(&sim.world.prototypes, fuel_slot)?;
        validate_slot_policy(sim, entity_id, fuel_slot, ItemSlotPolicy::Fuel)?;
    }
    validate_item_slot(&sim.world.prototypes, state.output_slot)?;
    if let Some(stack) = state.output_slot.stack() {
        let is_solid_resource =
            sim.world
                .prototypes
                .world_generation
                .resources
                .iter()
                .any(|resource| {
                    resource.resource_item == stack.item_id()
                        && resource.extraction == ResourceExtraction::Solid
                });
        if !is_solid_resource {
            return Err(SimValidationError::InvalidMachineItem {
                entity_id,
                item_id: stack.item_id(),
            });
        }
    }

    Ok(())
}

pub(in crate::simulation) fn validate_furnace(
    sim: &Simulation,
    entity_id: EntityId,
    state: &FurnaceState,
) -> Result<(), SimValidationError> {
    validate_machine_energy_matches_prototype(sim, entity_id, &state.energy)?;
    validate_item_slot(&sim.world.prototypes, state.input_slot)?;
    validate_item_slot(&sim.world.prototypes, state.output_slot)?;
    validate_slot_policy(
        sim,
        entity_id,
        state.input_slot,
        ItemSlotPolicy::FurnaceIngredient,
    )?;
    if let Some(fuel_slot) = state.energy.fuel_slot() {
        validate_item_slot(&sim.world.prototypes, fuel_slot)?;
        validate_slot_policy(sim, entity_id, fuel_slot, ItemSlotPolicy::Fuel)?;
    }

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

pub(in crate::simulation) fn validate_boiler(
    sim: &Simulation,
    entity_id: EntityId,
    state: &BoilerState,
) -> Result<(), SimValidationError> {
    validate_item_slot(&sim.world.prototypes, state.energy.fuel_slot)?;
    validate_slot_policy(sim, entity_id, state.energy.fuel_slot, ItemSlotPolicy::Fuel)?;

    Ok(())
}

pub(in crate::simulation) fn validate_assembler(
    sim: &Simulation,
    entity_id: EntityId,
    state: &AssemblingMachineState,
) -> Result<(), SimValidationError> {
    validate_inventory(&sim.world.prototypes, &state.input_inventory)?;
    validate_inventory(&sim.world.prototypes, &state.output_inventory)?;

    for slot in state.input_inventory.slots() {
        validate_slot_policy(
            sim,
            entity_id,
            *slot,
            ItemSlotPolicy::AssemblerIngredient(entity_id),
        )?;
    }

    let Some(recipe_id) = state.selected_recipe else {
        return Ok(());
    };

    let machine_category =
        assembler_machine_category(&sim.world.prototypes, &sim.entities, entity_id);
    sim.world
        .prototypes
        .recipe(recipe_id)
        .filter(|recipe| recipe.category == machine_category)
        .ok_or(SimValidationError::InvalidMachineRecipe {
            entity_id,
            recipe_id,
        })?;

    Ok(())
}

pub(in crate::simulation) fn validate_lab(
    sim: &Simulation,
    entity_id: EntityId,
    state: &LabState,
) -> Result<(), SimValidationError> {
    validate_inventory(&sim.world.prototypes, &state.inventory)?;
    for slot in state.inventory.slots() {
        validate_slot_policy(sim, entity_id, *slot, ItemSlotPolicy::SciencePack)?;
    }

    if let Some(technology_id) = state.active_technology
        && sim.world.prototypes.technology(technology_id).is_none()
    {
        return Err(SimValidationError::InvalidActiveResearch { technology_id });
    }

    Ok(())
}

pub(in crate::simulation) fn validate_belt_segment(
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
    let prototype = sim.world.prototypes.entity(placed.prototype_id).ok_or(
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

pub(in crate::simulation) fn validate_splitter_state(
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
    let prototype = sim.world.prototypes.entity(placed.prototype_id).ok_or(
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
        ItemStack::new(&sim.world.prototypes, item.item_id, 1).map_err(|error| match error {
            InventoryError::UnknownItem(item_id) => SimValidationError::UnknownItem(item_id),
            InventoryError::EmptyItemStack(item_id) => SimValidationError::EmptyItemStack(item_id),
            InventoryError::StackExceedsLimit {
                item_id,
                count,
                stack_size,
            } => SimValidationError::StackExceedsLimit {
                item_id,
                count,
                stack_size,
            },
            _ => unreachable!("stack construction cannot report inventory capacity errors"),
        })?;
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

pub(in crate::simulation) fn validate_inserter(
    sim: &Simulation,
    entity_id: EntityId,
    state: &InserterState,
) -> Result<(), SimValidationError> {
    if !sim.entities.inserter_energy.contains_key(&entity_id) {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }
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
    target: (WorldTileCoord, WorldTileCoord),
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

fn validate_slot_policy(
    sim: &Simulation,
    entity_id: EntityId,
    slot: ItemSlot,
    policy: ItemSlotPolicy,
) -> Result<(), SimValidationError> {
    if let Some(stack) = slot.stack()
        && !item_slot_policy_accepts(
            &sim.world.prototypes,
            &sim.research,
            &sim.entities,
            policy,
            ItemSlotOperation::MachineInsert,
            stack.item_id(),
        )
    {
        return Err(SimValidationError::InvalidMachineItem {
            entity_id,
            item_id: stack.item_id(),
        });
    }

    Ok(())
}
