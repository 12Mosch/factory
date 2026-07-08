use super::*;

pub(super) fn consumer_power_demand_for(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    research: &ResearchState,
    entity_id: EntityId,
) -> Option<(Option<u32>, u64, u64)> {
    let placed = entities.placed_entity(entity_id)?;
    let energy_source = catalog
        .entity(placed.prototype_id)?
        .electric_energy_source
        .as_ref()?;
    let active_usage_watts = if electric_consumer_can_work(catalog, entities, research, entity_id) {
        energy_source.energy_usage_watts
    } else {
        0
    };
    Some((None, active_usage_watts, energy_source.drain_watts))
}

fn electric_consumer_can_work(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    research: &ResearchState,
    entity_id: EntityId,
) -> bool {
    if let Some(state) = entities.assembling_machines.get(&entity_id) {
        return assembler_can_work(catalog, research, state);
    }
    if let Some(state) = entities.labs.get(&entity_id) {
        return lab_can_work(catalog, research, state);
    }
    if let (Some(placed), Some(state)) = (
        entities.placed_entity(entity_id),
        entities.inserters.get(&entity_id),
    ) {
        return inserter_can_work(catalog, research, entities, placed, state);
    }

    false
}

fn assembler_can_work(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    state: &AssemblingMachineState,
) -> bool {
    let Some(recipe) = selected_assembler_recipe(catalog, research, state) else {
        return false;
    };

    assembler_has_ingredients(&state.input_inventory, &recipe.ingredients)
        && assembler_output_can_accept(catalog, &state.output_inventory, &recipe.products)
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
                ItemStack { item_id, count: 1 },
            )
        }
        InserterState::Picking { .. } | InserterState::Dropping { .. } => true,
        InserterState::Holding { item } => {
            inserter_target_can_accept(catalog, research, entities, drop_tile, item)
        }
    }
}
