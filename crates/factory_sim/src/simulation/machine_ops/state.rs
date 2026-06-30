use crate::simulation::*;

pub(in crate::simulation) fn burner_mining_drill_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<BurnerMiningDrillState> {
    if prototype.entity_kind != EntityKind::MiningDrill {
        return None;
    }

    let burner = prototype.burner.as_ref()?;
    let mining_drill = prototype.mining_drill.as_ref()?;

    Some(BurnerMiningDrillState {
        energy: BurnerEnergy {
            fuel_slot: None,
            energy_remaining_joules: 0.0,
            energy_usage_watts: burner.energy_usage_watts as f64,
        },
        mining_progress_ticks: 0,
        mining_required_ticks: mining_drill.ticks_per_item,
        resource_target: None,
        output_slot: None,
    })
}

pub(in crate::simulation) fn furnace_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<FurnaceState> {
    if prototype.entity_kind != EntityKind::Furnace {
        return None;
    }

    let burner = prototype.burner.as_ref()?;

    Some(FurnaceState {
        input_slot: None,
        energy: BurnerEnergy {
            fuel_slot: None,
            energy_remaining_joules: 0.0,
            energy_usage_watts: burner.energy_usage_watts as f64,
        },
        output_slot: None,
        active_recipe: None,
        crafting_progress_ticks: 0,
        crafting_required_ticks: 0,
    })
}

pub(in crate::simulation) fn assembling_machine_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<AssemblingMachineState> {
    if prototype.entity_kind != EntityKind::AssemblingMachine {
        return None;
    }

    let assembling_machine = prototype.assembling_machine.as_ref()?;

    Some(AssemblingMachineState {
        selected_recipe: None,
        input_inventory: Inventory::with_slot_count(assembling_machine.input_slot_count),
        output_inventory: Inventory::with_slot_count(assembling_machine.output_slot_count),
        crafting_progress_ticks: 0,
        crafting_required_ticks: 0,
        crafting_speed_numerator: assembling_machine.crafting_speed_numerator,
        crafting_speed_denominator: assembling_machine.crafting_speed_denominator,
    })
}

pub(in crate::simulation) fn lab_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<LabState> {
    (prototype.entity_kind == EntityKind::Lab).then(|| LabState {
        inventory: Inventory::with_slot_count(
            prototype
                .inventory_slot_count
                .expect("lab prototype should define inventory slots"),
        ),
        active_technology: None,
        progress_ticks: 0,
        required_ticks: 0,
    })
}

pub(in crate::simulation) fn electric_pole_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<ElectricPoleState> {
    (prototype.entity_kind == EntityKind::ElectricPole && prototype.electric_pole.is_some())
        .then_some(ElectricPoleState)
}

pub(in crate::simulation) fn electric_consumer_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<ElectricConsumerState> {
    prototype
        .electric_energy_source
        .is_some()
        .then_some(ElectricConsumerState::default())
}

pub(in crate::simulation) fn steam_engine_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<SteamEngineState> {
    (prototype.entity_kind == EntityKind::SteamEngine && prototype.steam_engine.is_some())
        .then_some(SteamEngineState)
}

pub(in crate::simulation) fn boiler_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<BoilerState> {
    if prototype.entity_kind != EntityKind::Boiler {
        return None;
    }

    let burner = prototype.burner.as_ref()?;
    prototype.boiler.as_ref()?;

    Some(BoilerState {
        energy: BurnerEnergy {
            fuel_slot: None,
            energy_remaining_joules: 0.0,
            energy_usage_watts: burner.energy_usage_watts as f64,
        },
    })
}

pub(in crate::simulation) fn offshore_pump_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<OffshorePumpState> {
    (prototype.entity_kind == EntityKind::OffshorePump && prototype.offshore_pump.is_some())
        .then_some(OffshorePumpState)
}

pub(in crate::simulation) fn fluid_box_states_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<Vec<FluidBoxState>> {
    (!prototype.fluid_boxes.is_empty()).then(|| {
        prototype
            .fluid_boxes
            .iter()
            .map(|_| FluidBoxState::default())
            .collect()
    })
}

pub(in crate::simulation) fn transport_belt_segment_for_prototype(
    prototype: &factory_data::EntityPrototype,
    direction: Direction,
) -> Option<BeltSegment> {
    if prototype.entity_kind != EntityKind::TransportBelt {
        return None;
    }

    let transport_belt = prototype.transport_belt.as_ref()?;
    let Some(underground) = transport_belt.underground.as_ref() else {
        return Some(BeltSegment::new(
            direction,
            transport_belt.speed_subtiles_per_tick,
        ));
    };

    Some(BeltSegment::underground(
        direction,
        transport_belt.speed_subtiles_per_tick,
        underground.part,
        underground.max_distance,
    ))
}

pub(in crate::simulation) fn splitter_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
    direction: Direction,
) -> Option<SplitterState> {
    if prototype.entity_kind != EntityKind::Splitter {
        return None;
    }

    let splitter = prototype.splitter.as_ref()?;
    Some(SplitterState::new(
        direction,
        splitter.speed_subtiles_per_tick,
    ))
}

pub(in crate::simulation) fn inserter_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<InserterState> {
    (prototype.entity_kind == EntityKind::Inserter && prototype.inserter.is_some())
        .then_some(InserterState::WaitingForItem)
}
