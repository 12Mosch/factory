use super::*;

/// Read-only simulation state a consumer's power demand is derived from.
#[derive(Clone, Copy)]
pub(super) struct ConsumerDemandInputs<'a> {
    pub(super) world: &'a WorldSim,
    pub(super) entities: &'a EntityStore,
    pub(super) fluids: &'a FluidSubsystem,
    pub(super) research: &'a ResearchState,
}

pub(super) fn refresh_consumer_demand_cache(
    inputs: ConsumerDemandInputs<'_>,
    topology: &PowerTopologyCache,
    entity_statuses: &mut DenseEntityMap<EntityPowerStatus>,
    cache: &mut PowerDemandCache,
    networks: &mut [NetworkPowerBalance],
) {
    if !cache.valid || cache.network_consumption_watts.len() != networks.len() {
        rebuild_consumer_demand_cache(inputs, topology, entity_statuses, cache, networks.len());
    } else {
        cache.refresh_consumers.clear();
        cache
            .refresh_consumers
            .extend_from_slice(&cache.active_consumers);
        cache.refresh_consumers.append(&mut cache.dirty_consumers);
        cache.refresh_consumers.sort_unstable();
        cache.refresh_consumers.dedup();

        for &entity_id in &cache.refresh_consumers {
            let Some(status) = entity_statuses.get_mut(&entity_id) else {
                continue;
            };
            let Some((active_usage_watts, drain_watts)) =
                consumer_power_demand_for(inputs, entity_id)
            else {
                cache.invalidate();
                break;
            };
            #[cfg(test)]
            {
                cache.demand_recomputations += 1;
            }

            let old_demand = status.active_usage_watts.saturating_add(status.drain_watts);
            let new_demand = active_usage_watts.saturating_add(drain_watts);
            if let Some(network_id) = status.network_id {
                let consumption = &mut cache.network_consumption_watts[network_id as usize];
                *consumption = consumption
                    .saturating_sub(old_demand)
                    .saturating_add(new_demand);
            }
            status.active_usage_watts = active_usage_watts;
            status.drain_watts = drain_watts;
        }

        if !cache.valid {
            rebuild_consumer_demand_cache(inputs, topology, entity_statuses, cache, networks.len());
        }
    }

    for (network_id, network) in networks.iter_mut().enumerate() {
        network.consumer_count = cache.network_consumer_counts[network_id];
        network.consumption_watts = cache.network_consumption_watts[network_id];
    }
}

fn rebuild_consumer_demand_cache(
    inputs: ConsumerDemandInputs<'_>,
    topology: &PowerTopologyCache,
    entity_statuses: &mut DenseEntityMap<EntityPowerStatus>,
    cache: &mut PowerDemandCache,
    network_count: usize,
) {
    entity_statuses.clear();
    cache.active_consumers.clear();
    cache.dirty_consumers.clear();
    cache.network_consumption_watts.clear();
    cache.network_consumption_watts.resize(network_count, 0);
    cache.network_consumer_counts.clear();
    cache.network_consumer_counts.resize(network_count, 0);
    cache.consumers_by_network.clear();
    cache
        .consumers_by_network
        .resize_with(network_count, Vec::new);
    cache.network_satisfaction_permyriad.clear();
    cache
        .network_satisfaction_permyriad
        .resize(network_count, u32::MAX);

    for &entity_id in inputs.entities.electric_consumers.keys() {
        let Some((active_usage_watts, drain_watts)) = consumer_power_demand_for(inputs, entity_id)
        else {
            continue;
        };
        #[cfg(test)]
        {
            cache.demand_recomputations += 1;
        }
        let network_id = topology.network_ids_by_entity.get(&entity_id).copied();
        entity_statuses.insert(
            entity_id,
            EntityPowerStatus {
                network_id,
                active_usage_watts,
                drain_watts,
                ..EntityPowerStatus::default()
            },
        );

        if consumer_demand_is_active(inputs.entities, inputs.world, entity_id) {
            cache.active_consumers.push(entity_id);
        }
        if let Some(network_id) = network_id {
            let network_index = network_id as usize;
            cache.network_consumer_counts[network_index] += 1;
            cache.network_consumption_watts[network_index] = cache.network_consumption_watts
                [network_index]
                .saturating_add(active_usage_watts.saturating_add(drain_watts));
            cache.consumers_by_network[network_index].push(entity_id);
        }
    }
    cache.valid = true;
}

fn consumer_demand_is_active(
    entities: &EntityStore,
    world: &WorldSim,
    entity_id: EntityId,
) -> bool {
    if entities.radars.contains_key(&entity_id)
        || entities.inserters.contains_key(&entity_id)
        || entities.mining_drills.contains_key(&entity_id)
        || entities.pumpjacks.contains_key(&entity_id)
    {
        return true;
    }

    let Some(prototype) = entities
        .placed_entity(entity_id)
        .and_then(|placed| world.prototypes.entity(placed.prototype_id))
    else {
        return false;
    };
    // A pump's demand depends on the fluid networks it bridges, which change without the
    // pump itself being marked dirty.
    prototype.pump.is_some()
        || (entities.assembling_machines.contains_key(&entity_id)
            && !prototype.fluid_boxes.is_empty())
}

pub(super) fn consumer_power_demand_for(
    inputs: ConsumerDemandInputs<'_>,
    entity_id: EntityId,
) -> Option<(u64, u64)> {
    let energy_source =
        electric_consumer_power_source(&inputs.world.prototypes, inputs.entities, entity_id)?;
    let active_usage_watts = if electric_consumer_can_work(inputs, entity_id) {
        let multiplier = entity_module_energy_multiplier(inputs.entities, entity_id);
        energy_source
            .energy_usage_watts
            .saturating_mul(multiplier)
            .saturating_add(9_999)
            / 10_000
    } else {
        0
    };
    Some((active_usage_watts, energy_source.drain_watts))
}

fn entity_module_energy_multiplier(entities: &EntityStore, entity_id: EntityId) -> u64 {
    entities
        .machine_module_state(entity_id)
        .map_or(10_000, |modules| {
            modules.resolved_effects.energy_multiplier_permyriad()
        })
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

fn electric_consumer_can_work(inputs: ConsumerDemandInputs<'_>, entity_id: EntityId) -> bool {
    let ConsumerDemandInputs {
        world,
        entities,
        fluids,
        research,
    } = inputs;
    let catalog = &world.prototypes;
    if entities.radars.contains_key(&entity_id) {
        return true;
    }
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
    if let Some(state) = entities.laser_turrets.get(&entity_id) {
        return state.engaged;
    }
    if entities.pumpjacks.contains_key(&entity_id) {
        return pumpjack_can_work(catalog, entities, entity_id);
    }
    if entities
        .placed_entity(entity_id)
        .and_then(|placed| catalog.entity(placed.prototype_id))
        .is_some_and(|prototype| prototype.pump.is_some())
    {
        return pump_fluid_transfer(catalog, entities, fluids, entity_id).is_some();
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
    state.output_slot.can_insert_item(
        catalog,
        product.item,
        u16::try_from(u64::from(product.amount).saturating_mul(state.modules.output_copies_due()))
            .unwrap_or(u16::MAX),
    )
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
    drill_productivity_output_can_fit(
        &world.prototypes,
        entities,
        output_target,
        state.output_slot,
        resource_item,
        state.modules.output_copies_due(),
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
        || !assembler_output_can_accept_copies(
            catalog,
            &state.output_inventory,
            &recipe.products,
            state.modules.output_copies_due(),
        )
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
        && fluid_product_box_indices_for_copies(
            &prototype.fluid_boxes,
            box_states,
            &recipe.fluid_products,
            state.modules.output_copies_due(),
        )
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
