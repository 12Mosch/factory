use super::*;

pub(super) fn consumer_power_demand_for(
    world: &WorldSim,
    entities: &EntityStore,
    research: &ResearchState,
    entity_id: EntityId,
) -> Option<(u64, u64)> {
    let energy_source = electric_consumer_power_source(&world.prototypes, entities, entity_id)?;
    let active_usage_watts = if electric_consumer_can_work(world, entities, research, entity_id) {
        energy_source.energy_usage_watts
    } else {
        0
    };
    Some((active_usage_watts, energy_source.drain_watts))
}

pub(super) fn electric_consumer_has_power_source(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    entity_id: EntityId,
) -> bool {
    electric_consumer_power_source(catalog, entities, entity_id).is_some()
}

fn electric_consumer_power_source<'a>(
    catalog: &'a PrototypeCatalog,
    entities: &EntityStore,
    entity_id: EntityId,
) -> Option<&'a factory_data::ElectricEnergySourcePrototype> {
    let placed = entities.placed_entity(entity_id)?;
    catalog
        .entity(placed.prototype_id)?
        .electric_energy_source
        .as_ref()
}

fn electric_consumer_can_work(
    world: &WorldSim,
    entities: &EntityStore,
    research: &ResearchState,
    entity_id: EntityId,
) -> bool {
    let catalog = &world.prototypes;
    if let Some(state) = entities.assembling_machines.get(&entity_id) {
        return assembler_can_work(catalog, entities, research, entity_id, state);
    }
    if let Some(state) = entities.furnaces.get(&entity_id) {
        return furnace_can_work(catalog, research, state);
    }
    if let Some(state) = entities.mining_drills.get(&entity_id) {
        return mining_drill_can_work(world, entities, entity_id, state);
    }
    if let Some(state) = entities.labs.get(&entity_id) {
        return lab_can_work(catalog, research, state);
    }
    if entities.pumpjacks.contains_key(&entity_id) {
        return pumpjack_can_work(catalog, entities, entity_id);
    }
    if let (Some(placed), Some(state)) = (
        entities.placed_entity(entity_id),
        entities.inserters.get(&entity_id),
    ) {
        return inserter_can_work(catalog, research, entities, placed, state);
    }

    false
}

fn furnace_can_work(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    state: &FurnaceState,
) -> bool {
    let Some((_, _, _, product)) = furnace_work_selection(catalog, research, state.input_slot)
    else {
        return false;
    };
    state
        .output_slot
        .can_insert_item(catalog, product.item, product.amount)
}

fn mining_drill_can_work(
    world: &WorldSim,
    entities: &EntityStore,
    entity_id: EntityId,
    state: &MiningDrillState,
) -> bool {
    let Some(placed) = entities.placed_entity(entity_id) else {
        return false;
    };
    let Some(mining_drill) = world
        .prototypes
        .entity(placed.prototype_id)
        .and_then(|prototype| prototype.mining_drill.as_ref())
    else {
        return false;
    };
    let Some((_, resource_item)) =
        first_resource_in_mining_area(world, &placed.footprint, mining_drill)
    else {
        return false;
    };
    let output_target = drill_output_target(entities, placed);
    drill_output_target_can_accept(
        &world.prototypes,
        entities,
        output_target,
        state.output_slot,
        resource_item,
        1,
    )
}

fn assembler_can_work(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    research: &ResearchState,
    entity_id: EntityId,
    state: &AssemblingMachineState,
) -> bool {
    let Some(recipe) = selected_assembler_recipe(catalog, research, state) else {
        return false;
    };

    if !assembler_has_ingredients(&state.input_inventory, &recipe.ingredients)
        || !assembler_output_can_accept(catalog, &state.output_inventory, &recipe.products)
    {
        return false;
    }
    if recipe.fluid_ingredients.is_empty() && recipe.fluid_products.is_empty() {
        return true;
    }

    let Some(prototype) = entities
        .placed_entity(entity_id)
        .and_then(|placed| catalog.entity(placed.prototype_id))
    else {
        return false;
    };
    let box_states = entities
        .fluid_boxes
        .get(&entity_id)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    fluid_ingredient_box_indices(
        &prototype.fluid_boxes,
        box_states,
        &recipe.fluid_ingredients,
    )
    .is_some()
        && fluid_product_box_indices(&prototype.fluid_boxes, box_states, &recipe.fluid_products)
            .is_some()
}

fn pumpjack_can_work(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    entity_id: EntityId,
) -> bool {
    let Some(prototype) = entities
        .placed_entity(entity_id)
        .and_then(|placed| catalog.entity(placed.prototype_id))
    else {
        return false;
    };
    let Some(capacity_milliunits) = prototype
        .fluid_boxes
        .first()
        .map(|fluid_box| fluid_box.capacity_milliunits)
    else {
        return false;
    };

    entities
        .fluid_boxes
        .get(&entity_id)
        .and_then(|boxes| boxes.first())
        .is_some_and(|state| state.amount_milliunits < capacity_milliunits)
}

fn lab_can_work(catalog: &PrototypeCatalog, research: &ResearchState, state: &LabState) -> bool {
    let Some(technology_id) = state.active_technology.or(research.active) else {
        return false;
    };
    let Some(technology) = catalog.technology(technology_id) else {
        return false;
    };

    lab_has_science_packs(&state.inventory, &technology.science_packs)
}

fn inserter_can_work(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    entities: &EntityStore,
    placed: &PlacedEntity,
    state: &InserterState,
) -> bool {
    let Some(prototype) = catalog.entity(placed.prototype_id) else {
        return false;
    };
    let Some(inserter) = prototype.inserter.as_ref() else {
        return false;
    };
    let (pickup_tile, drop_tile) = inserter_transfer_tiles_for_prototype(placed, inserter);

    match *state {
        InserterState::WaitingForItem => {
            let Some(item_id) = peek_inserter_source_item(entities, pickup_tile) else {
                return false;
            };
            inserter_target_can_accept(
                catalog,
                research,
                entities,
                drop_tile,
                ItemStack::new(catalog, item_id, 1)
                    .expect("a source item should exist in the prototype catalog"),
            )
        }
        InserterState::Picking { .. } | InserterState::Dropping { .. } => true,
        InserterState::Holding { item } => {
            inserter_target_can_accept(catalog, research, entities, drop_tile, item)
        }
    }
}
