use crate::simulation::*;

/// Builds the placement reservation for `prototype`, filling in the initial
/// state for every state map the entity participates in. This is the single
/// place that decides which per-kind state a freshly placed entity gets; the
/// exhaustive struct literal makes the compiler reject a registry entry
/// without an initializer here.
pub(in crate::simulation) fn reservation_for_prototype(
    prototype: &factory_data::EntityPrototype,
    prototype_id: EntityPrototypeId,
    x: WorldTileCoord,
    y: WorldTileCoord,
    direction: Direction,
    footprint: EntityFootprint,
) -> EntityReservation {
    EntityReservation {
        prototype_id,
        x,
        y,
        direction,
        footprint,
        entity_inventories: chest_inventory_for_prototype(prototype),
        mining_drills: mining_drill_state_for_prototype(prototype),
        furnaces: furnace_state_for_prototype(prototype),
        assembling_machines: assembling_machine_state_for_prototype(prototype),
        labs: lab_state_for_prototype(prototype),
        electric_poles: electric_pole_state_for_prototype(prototype),
        electric_consumers: electric_consumer_state_for_prototype(prototype),
        steam_engines: steam_engine_state_for_prototype(prototype),
        solar_panels: solar_panel_state_for_prototype(prototype),
        accumulators: accumulator_state_for_prototype(prototype),
        boilers: boiler_state_for_prototype(prototype),
        offshore_pumps: offshore_pump_state_for_prototype(prototype),
        fluid_boxes: fluid_box_states_for_prototype(prototype),
        transport_belts: transport_belt_segment_for_prototype(prototype, direction),
        splitters: splitter_state_for_prototype(prototype, direction),
        inserters: inserter_state_for_prototype(prototype),
        pumpjacks: pumpjack_state_for_prototype(prototype),
        gun_turrets: gun_turret_state_for_prototype(prototype),
        enemy_spawners: enemy_spawner_state_for_prototype(prototype),
        entity_health: health_state_for_prototype(prototype),
        inserter_energy: inserter_energy_for_prototype(prototype),
        laser_turrets: laser_turret_state_for_prototype(prototype),
        beacons: beacon_state_for_prototype(prototype),
        radars: radar_state_for_prototype(prototype),
        // Deliberately `None`: circuit state is created lazily when the first
        // wire or condition is configured, so an unwired belt or inserter
        // carries no per-entity circuit entry at all.
        circuit_entities: None,
        constant_combinators: constant_combinator_state_for_prototype(prototype),
        arithmetic_combinators: arithmetic_combinator_state_for_prototype(prototype),
        decider_combinators: decider_combinator_state_for_prototype(prototype),
        lamps: lamp_state_for_prototype(prototype),
        heat_buffers: heat_buffer_state_for_prototype(prototype),
        nuclear_reactors: nuclear_reactor_state_for_prototype(prototype),
        heat_pipes: heat_pipe_state_for_prototype(prototype),
        heat_exchangers: heat_exchanger_state_for_prototype(prototype),
        roboports: roboport_state_for_prototype(prototype),
        logistic_chests: logistic_chest_state_for_prototype(prototype),
    }
}

/// A freshly placed logistic chest has its rows laid out but none of them set,
/// so it joins the network asking for nothing and filtering nothing. Provider
/// chests get an empty row list, which still marks them as network members.
fn logistic_chest_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<crate::logistics::LogisticChestState> {
    if prototype.entity_kind != EntityKind::Chest {
        return None;
    }

    let logistic_chest = prototype.logistic_chest?;
    Some(crate::logistics::LogisticChestState::with_slot_count(
        usize::from(logistic_chest.request_slot_count),
    ))
}

/// A fresh roboport starts empty and cold: no robots, no repair material, and
/// an empty charging buffer it fills from the network over its first seconds.
fn roboport_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<crate::robots::RoboportState> {
    if prototype.entity_kind != EntityKind::Roboport {
        return None;
    }

    let roboport = prototype.roboport?;
    Some(crate::robots::RoboportState::new(
        roboport.robot_slot_count,
        roboport.material_slot_count,
    ))
}

/// Every heat network participant starts at ambient temperature, which is zero
/// stored energy.
fn heat_buffer_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<HeatBufferState> {
    prototype
        .heat_buffer
        .is_some()
        .then_some(HeatBufferState::default())
}

fn nuclear_reactor_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<NuclearReactorState> {
    if prototype.entity_kind != EntityKind::NuclearReactor {
        return None;
    }

    let reactor = prototype.nuclear_reactor?;
    prototype.heat_buffer.as_ref()?;

    Some(NuclearReactorState {
        // A reactor's "energy usage" is the rate it turns fuel into heat, which
        // is the base output before any neighbour bonus.
        energy: BurnerEnergy {
            fuel_slot: ItemSlot::default(),
            energy_remaining_joules: 0.0,
            energy_usage_watts: reactor.heat_output_watts as f64,
        },
        output_slot: ItemSlot::default(),
    })
}

fn heat_pipe_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<HeatPipeState> {
    (prototype.entity_kind == EntityKind::HeatPipe && prototype.heat_buffer.is_some())
        .then_some(HeatPipeState)
}

fn heat_exchanger_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<HeatExchangerState> {
    (prototype.entity_kind == EntityKind::HeatExchanger
        && prototype.heat_buffer.is_some()
        && prototype.heat_energy_source.is_some()
        && prototype.boiler.is_some())
    .then_some(HeatExchangerState)
}

fn constant_combinator_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<ConstantCombinatorState> {
    let combinator = prototype.combinator?;
    (prototype.entity_kind == EntityKind::ConstantCombinator
        && combinator.kind == factory_data::CombinatorKind::Constant)
        .then(|| {
            ConstantCombinatorState::with_slot_count(usize::from(combinator.constant_slot_count))
        })
}

fn arithmetic_combinator_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<ArithmeticCombinatorState> {
    let combinator = prototype.combinator?;
    (prototype.entity_kind == EntityKind::ArithmeticCombinator
        && combinator.kind == factory_data::CombinatorKind::Arithmetic)
        .then(ArithmeticCombinatorState::default)
}

fn decider_combinator_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<DeciderCombinatorState> {
    let combinator = prototype.combinator?;
    (prototype.entity_kind == EntityKind::DeciderCombinator
        && combinator.kind == factory_data::CombinatorKind::Decider)
        .then(DeciderCombinatorState::default)
}

fn lamp_state_for_prototype(prototype: &factory_data::EntityPrototype) -> Option<LampState> {
    (prototype.entity_kind == EntityKind::Lamp && prototype.circuit_connector.is_some())
        .then_some(LampState::default())
}

fn chest_inventory_for_prototype(prototype: &factory_data::EntityPrototype) -> Option<Inventory> {
    if prototype.entity_kind != EntityKind::Chest {
        return None;
    }

    prototype
        .inventory_slot_count
        .map(Inventory::with_slot_count)
}

/// The energy source a freshly placed machine runs on, from its prototype's
/// burner or electric section. Prototype validation guarantees furnaces and
/// mining drills declare exactly one.
fn machine_energy_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<MachineEnergy> {
    if let Some(burner) = prototype.burner.as_ref() {
        return Some(MachineEnergy::Burner(BurnerEnergy {
            fuel_slot: ItemSlot::default(),
            energy_remaining_joules: 0.0,
            energy_usage_watts: burner.energy_usage_watts as f64,
        }));
    }
    prototype
        .electric_energy_source
        .is_some()
        .then_some(MachineEnergy::Electric)
}

fn mining_drill_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<MiningDrillState> {
    if prototype.entity_kind != EntityKind::MiningDrill {
        return None;
    }

    let mining_drill = prototype.mining_drill.as_ref()?;
    let energy = machine_energy_for_prototype(prototype)?;

    Some(MiningDrillState {
        modules: MachineModuleState::with_slot_count(prototype.module_slot_count),
        energy,
        mining_progress_ticks: 0,
        mining_required_ticks: mining_drill.ticks_per_item,
        resource_target: None,
        output_slot: ItemSlot::default(),
    })
}

fn furnace_state_for_prototype(prototype: &factory_data::EntityPrototype) -> Option<FurnaceState> {
    if prototype.entity_kind != EntityKind::Furnace {
        return None;
    }

    let energy = machine_energy_for_prototype(prototype)?;

    Some(FurnaceState {
        modules: MachineModuleState::with_slot_count(prototype.module_slot_count),
        input_slot: ItemSlot::default(),
        energy,
        output_slot: ItemSlot::default(),
        active_recipe: None,
        crafting_progress_ticks: 0,
        crafting_required_ticks: 0,
    })
}

fn assembling_machine_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<AssemblingMachineState> {
    if prototype.entity_kind != EntityKind::AssemblingMachine {
        return None;
    }

    let assembling_machine = prototype.assembling_machine.as_ref()?;

    Some(AssemblingMachineState {
        modules: MachineModuleState::with_slot_count(prototype.module_slot_count),
        selected_recipe: None,
        input_inventory: Inventory::with_slot_count(assembling_machine.input_slot_count),
        output_inventory: Inventory::with_slot_count(assembling_machine.output_slot_count),
        crafting_progress_ticks: 0,
        crafting_required_ticks: 0,
        crafting_speed_numerator: assembling_machine.crafting_speed_numerator,
        crafting_speed_denominator: assembling_machine.crafting_speed_denominator,
    })
}

fn lab_state_for_prototype(prototype: &factory_data::EntityPrototype) -> Option<LabState> {
    (prototype.entity_kind == EntityKind::Lab).then(|| LabState {
        modules: MachineModuleState::with_slot_count(prototype.module_slot_count),
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

fn beacon_state_for_prototype(prototype: &factory_data::EntityPrototype) -> Option<BeaconState> {
    (prototype.entity_kind == EntityKind::Beacon)
        .then(|| BeaconState::with_slot_count(prototype.module_slot_count))
}

fn electric_pole_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<ElectricPoleState> {
    (prototype.entity_kind == EntityKind::ElectricPole && prototype.electric_pole.is_some())
        .then_some(ElectricPoleState)
}

fn electric_consumer_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<ElectricConsumerState> {
    prototype
        .electric_energy_source
        .is_some()
        .then_some(ElectricConsumerState::default())
}

fn steam_engine_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<SteamEngineState> {
    (prototype.entity_kind == EntityKind::SteamEngine && prototype.steam_engine.is_some())
        .then_some(SteamEngineState)
}

fn solar_panel_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<SolarPanelState> {
    (prototype.entity_kind == EntityKind::SolarPanel && prototype.solar_panel.is_some())
        .then_some(SolarPanelState)
}

fn accumulator_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<AccumulatorState> {
    (prototype.entity_kind == EntityKind::Accumulator && prototype.accumulator.is_some())
        .then_some(AccumulatorState::default())
}

fn radar_state_for_prototype(prototype: &factory_data::EntityPrototype) -> Option<RadarState> {
    (prototype.entity_kind == EntityKind::Radar && prototype.radar.is_some())
        .then_some(RadarState::default())
}

fn boiler_state_for_prototype(prototype: &factory_data::EntityPrototype) -> Option<BoilerState> {
    if prototype.entity_kind != EntityKind::Boiler {
        return None;
    }

    let burner = prototype.burner.as_ref()?;
    prototype.boiler.as_ref()?;

    Some(BoilerState {
        energy: BurnerEnergy {
            fuel_slot: ItemSlot::default(),
            energy_remaining_joules: 0.0,
            energy_usage_watts: burner.energy_usage_watts as f64,
        },
    })
}

fn offshore_pump_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<OffshorePumpState> {
    (prototype.entity_kind == EntityKind::OffshorePump && prototype.offshore_pump.is_some())
        .then_some(OffshorePumpState)
}

fn pumpjack_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<PumpjackState> {
    (prototype.entity_kind == EntityKind::Pumpjack && prototype.pumpjack.is_some())
        .then_some(PumpjackState)
}

fn fluid_box_states_for_prototype(
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

fn transport_belt_segment_for_prototype(
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

fn splitter_state_for_prototype(
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

fn inserter_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<InserterState> {
    (prototype.entity_kind == EntityKind::Inserter && prototype.inserter.is_some())
        .then_some(InserterState::WaitingForItem)
}

fn inserter_energy_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<MachineEnergy> {
    (prototype.entity_kind == EntityKind::Inserter)
        .then(|| machine_energy_for_prototype(prototype))
        .flatten()
}

fn gun_turret_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<GunTurretState> {
    (prototype.entity_kind == EntityKind::GunTurret && prototype.gun_turret.is_some())
        .then(GunTurretState::new)
}

fn laser_turret_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<LaserTurretState> {
    (prototype.entity_kind == EntityKind::LaserTurret && prototype.laser_turret.is_some())
        .then_some(LaserTurretState::default())
}

fn enemy_spawner_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<EnemySpawnerState> {
    (prototype.entity_kind == EntityKind::EnemySpawner && prototype.enemy_spawner.is_some())
        .then_some(EnemySpawnerState::default())
}

fn health_state_for_prototype(prototype: &factory_data::EntityPrototype) -> Option<HealthState> {
    let faction = if prototype.entity_kind == EntityKind::EnemySpawner {
        Faction::Enemy
    } else {
        Faction::Player
    };
    prototype
        .max_health
        .map(|max_health| HealthState::new(max_health, faction))
}
