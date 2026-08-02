use std::collections::HashMap;

use glam::IVec2;

use crate::error::PrototypeLoadError;
use crate::ids::{EntityPrototypeId, FluidId, ItemId};
use crate::model::{
    ConnectionSide, EdgeConnectionPrototype, ElectricPolePrototype, EnemySpawnerPrototype,
    EntityKind, EntityPrototype, FluidBoxPrototype, HeatBufferPrototype, InserterPrototype,
    MiningDrillPrototype, POSITION_SCALE, PumpjackPrototype, RailCurvePrototype, RailHeading,
    RailPiecePrototype, RailPointPrototype,
};
use crate::raw::{
    RawEdgeConnectionPrototype, RawEntityPrototype, RawFluidBoxPrototype, RawHeatBufferPrototype,
    RawPumpjackPrototype,
};
use crate::validation::resolve_collision_mask;

pub(super) fn load_entities(
    entities: Vec<RawEntityPrototype>,
    item_ids_by_name: &HashMap<String, ItemId>,
    fluid_ids_by_name: &HashMap<String, FluidId>,
) -> Result<Vec<EntityPrototype>, PrototypeLoadError> {
    entities
        .into_iter()
        .map(|entity| {
            validate_laser_turret_metadata(&entity.name, &entity)?;
            validate_module_and_beacon_metadata(&entity.name, &entity)?;
            validate_solar_and_storage_metadata(&entity.name, &entity)?;
            validate_radar_metadata(&entity.name, &entity)?;
            validate_circuit_metadata(&entity.name, &entity)?;
            validate_roboport_metadata(&entity.name, &entity)?;
            validate_logistic_chest_metadata(&entity.name, &entity)?;
            validate_rolling_stock_metadata(&entity.name, &entity)?;
            if entity.size.x <= 0 || entity.size.y <= 0 {
                return Err(PrototypeLoadError::InvalidEntityMetadata {
                    entity: entity.name,
                    detail: "dimensions must be positive",
                });
            }
            let name = entity.name;
            let size = IVec2::new(entity.size.x, entity.size.y);
            validate_rail_metadata(&name, entity.entity_kind, size, entity.rail_piece.as_ref())?;
            validate_trackside_metadata(&name, entity.entity_kind, size)?;
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
            let fluid_boxes = resolve_fluid_boxes(
                &name,
                entity.entity_kind,
                size,
                entity.fluid_boxes,
                fluid_ids_by_name,
            )?;
            let heat_buffer = resolve_heat_buffer(&name, size, entity.heat_buffer)?;
            validate_heat_metadata(
                &name,
                entity.entity_kind,
                heat_buffer.as_ref(),
                entity.heat_energy_source.as_ref(),
                entity.nuclear_reactor.as_ref(),
                entity.boiler.as_ref(),
                entity.burner.is_some(),
                entity.electric_energy_source.is_some(),
                entity.inventory_slot_count,
            )?;
            let pumpjack =
                resolve_pumpjack(&name, entity.pumpjack, item_ids_by_name, fluid_ids_by_name)?;
            validate_machine_energy_source(
                &name,
                entity.entity_kind,
                entity.furnace.as_ref(),
                entity.mining_drill.is_some(),
                entity.burner.is_some(),
                entity.electric_energy_source.is_some(),
            )?;
            validate_lab_metadata(&name, entity.entity_kind, entity.inventory_slot_count)?;
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
                module_slot_count: entity.module_slot_count,
                beacon: entity.beacon,
                burner: entity.burner,
                furnace: entity.furnace,
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
                solar_panel: entity.solar_panel,
                accumulator: entity.accumulator,
                radar: entity.radar,
                boiler: entity.boiler,
                offshore_pump: entity.offshore_pump,
                pump: entity.pump,
                pumpjack,
                underground_pipe: entity.underground_pipe,
                fluid_boxes,
                heat_buffer,
                heat_energy_source: entity.heat_energy_source,
                nuclear_reactor: entity.nuclear_reactor,
                roboport: entity.roboport,
                logistic_chest: entity.logistic_chest,
                max_health: entity.max_health,
                pollution_per_minute_milli: entity.pollution_per_minute_milli,
                gun_turret: entity.gun_turret,
                laser_turret: entity.laser_turret,
                enemy_spawner: entity.enemy_spawner.map(|spawner| EnemySpawnerPrototype {
                    max_alive_units: spawner.max_alive_units,
                    guard_units: spawner.guard_units,
                    free_spawn_interval_ticks: spawner.free_spawn_interval_ticks,
                    unit_spawn_pollution_cost_milli: spawner.unit_spawn_pollution_cost_milli,
                    pollution_absorption_per_tick_milli: spawner
                        .pollution_absorption_per_tick_milli,
                    unit: spawner.unit,
                }),
                circuit_connector: entity.circuit_connector,
                combinator: entity.combinator,
                rail_piece: entity.rail_piece,
                rolling_stock: entity.rolling_stock,
            })
        })
        .collect()
}

/// Circuit metadata is only coherent when the entity kind, the connector
/// layout, and the combinator section agree. Rejecting mismatches here keeps
/// the simulation free of "combinator without an output port" style checks.
fn validate_circuit_metadata(
    name: &str,
    entity: &RawEntityPrototype,
) -> Result<(), PrototypeLoadError> {
    use crate::model::{CircuitPortLayout, CombinatorKind, EntityKind};

    let invalid = |detail: &'static str| {
        Err(PrototypeLoadError::InvalidCircuitMetadata {
            entity: name.to_string(),
            detail,
        })
    };

    let combinator_kind = match entity.entity_kind {
        EntityKind::ConstantCombinator => Some(CombinatorKind::Constant),
        EntityKind::ArithmeticCombinator => Some(CombinatorKind::Arithmetic),
        EntityKind::DeciderCombinator => Some(CombinatorKind::Decider),
        _ => None,
    };

    match (combinator_kind, entity.combinator) {
        (Some(expected), Some(combinator)) => {
            if combinator.kind != expected {
                return invalid("combinator kind must match the entity kind");
            }
            match expected {
                CombinatorKind::Constant if combinator.constant_slot_count == 0 => {
                    return invalid("constant combinators require at least one signal slot");
                }
                CombinatorKind::Arithmetic | CombinatorKind::Decider
                    if combinator.constant_slot_count != 0 =>
                {
                    return invalid("only constant combinators declare signal slots");
                }
                _ => {}
            }
        }
        (Some(_), None) => return invalid("combinator entities require combinator metadata"),
        (None, Some(_)) => return invalid("combinator metadata is only valid on combinators"),
        (None, None) => {}
    }

    let Some(connector) = entity.circuit_connector else {
        if combinator_kind.is_some() {
            return invalid("combinators require a circuit connector");
        }
        if entity.entity_kind == EntityKind::Lamp {
            return invalid("lamps require a circuit connector");
        }
        return Ok(());
    };

    if connector.wire_reach_tiles_x2 == 0 {
        return invalid("circuit wire reach must be positive");
    }
    let expected_layout = if combinator_kind.is_some() {
        CircuitPortLayout::InputOutput
    } else {
        CircuitPortLayout::Single
    };
    if connector.ports != expected_layout {
        return invalid("only combinators use separate input and output connectors");
    }
    // A constant combinator publishes its configured rows, not stored goods,
    // and no combinator is gated by a condition of its own.
    if combinator_kind.is_some() && (connector.reads_contents || connector.controllable) {
        return invalid("combinators neither read contents nor take an enable condition");
    }
    if entity.entity_kind == EntityKind::Lamp && !connector.controllable {
        return invalid("lamps require a controllable circuit connector");
    }

    Ok(())
}

/// A roboport is a powered building whose whole purpose is its two coverage
/// radii and its robot storage, so every one of those has to be present and
/// positive. It also runs on electricity alone: any other energy source would
/// leave two competing answers for what powers robot charging.
fn validate_roboport_metadata(
    name: &str,
    entity: &RawEntityPrototype,
) -> Result<(), PrototypeLoadError> {
    use crate::model::EntityKind;

    let invalid = |detail| {
        Err(PrototypeLoadError::InvalidRoboportMetadata {
            entity: name.to_string(),
            detail,
        })
    };

    match (entity.entity_kind, entity.roboport) {
        (EntityKind::Roboport, Some(roboport)) => {
            if roboport.construction_radius_tiles == 0 || roboport.logistic_radius_tiles == 0 {
                return invalid("construction and logistic radii must be positive");
            }
            if roboport.robot_slot_count == 0 || roboport.material_slot_count == 0 {
                return invalid("roboports require robot and material slots");
            }
            if roboport.charging_energy_buffer_joules == 0 {
                return invalid("roboport charging buffer must be positive");
            }
            // Without a pad no robot could ever finish charging, and a pad that
            // delivers nothing is the same stall by another route.
            if roboport.charging_pad_count == 0 || roboport.charging_pad_watts == 0 {
                return invalid("roboports require at least one charging pad with positive power");
            }
            if entity
                .electric_energy_source
                .as_ref()
                .is_none_or(|source| source.energy_usage_watts == 0 || source.drain_watts == 0)
            {
                return invalid(
                    "roboports require an electric energy source with positive charging power and idle drain",
                );
            }
            if entity.burner.is_some()
                || entity.heat_energy_source.is_some()
                || entity.heat_buffer.is_some()
                || !entity.fluid_boxes.is_empty()
            {
                return invalid("roboports run on electricity alone");
            }
            // The robot and material slots are the roboport's inventory; a
            // second, unfiltered one would let it double as a chest.
            if entity.inventory_slot_count.is_some() {
                return invalid("roboports use their robot and material slots, not an inventory");
            }
            if entity.max_health.is_none_or(|health| health == 0) {
                return invalid("roboports require positive maximum health");
            }
        }
        (EntityKind::Roboport, None) => {
            return invalid("roboport entities require roboport metadata");
        }
        (_, Some(_)) => return invalid("roboport metadata is only valid on roboport entities"),
        (_, None) => {}
    }

    Ok(())
}

/// A logistic chest is an ordinary chest with a network role, so it has to keep
/// the chest half intact (kind and inventory) and declare a row count its mode
/// can actually use. Rejecting a mismatch here is what lets the simulation read
/// `requests[i]` positionally without ever asking whether the mode has rows.
fn validate_logistic_chest_metadata(
    name: &str,
    entity: &RawEntityPrototype,
) -> Result<(), PrototypeLoadError> {
    use crate::model::{EntityKind, LogisticChestMode};

    let invalid = |detail| {
        Err(PrototypeLoadError::InvalidLogisticChestMetadata {
            entity: name.to_string(),
            detail,
        })
    };

    let Some(logistic_chest) = entity.logistic_chest else {
        return Ok(());
    };
    if entity.entity_kind != EntityKind::Chest {
        return invalid("logistic chest metadata is only valid on chest entities");
    }
    if entity.inventory_slot_count.is_none_or(|count| count == 0) {
        return invalid("logistic chests require inventory slots");
    }

    let rows = logistic_chest.request_slot_count;
    match logistic_chest.mode {
        LogisticChestMode::PassiveProvider | LogisticChestMode::ActiveProvider if rows != 0 => {
            invalid("provider chests supply what they hold and declare no request rows")
        }
        LogisticChestMode::Storage if rows != 1 => {
            invalid("storage chests declare exactly one row, their filter")
        }
        LogisticChestMode::Buffer | LogisticChestMode::Requester if rows == 0 => {
            invalid("buffer and requester chests require at least one request row")
        }
        _ => Ok(()),
    }
}

fn validate_module_and_beacon_metadata(
    name: &str,
    entity: &RawEntityPrototype,
) -> Result<(), PrototypeLoadError> {
    use crate::model::EntityKind;

    let supports_modules = matches!(
        entity.entity_kind,
        EntityKind::AssemblingMachine
            | EntityKind::Furnace
            | EntityKind::MiningDrill
            | EntityKind::Lab
            | EntityKind::Beacon
    );
    if entity.module_slot_count > 0 && !supports_modules {
        return Err(PrototypeLoadError::InvalidModuleSlotMetadata {
            entity: name.to_string(),
            detail: "this entity kind cannot declare module slots",
        });
    }
    if entity.module_slot_count > u16::MAX as usize {
        return Err(PrototypeLoadError::InvalidModuleSlotMetadata {
            entity: name.to_string(),
            detail: "module slot count exceeds supported fixed-point aggregation",
        });
    }

    match (entity.entity_kind, entity.beacon) {
        (EntityKind::Beacon, Some(beacon)) => {
            if entity.module_slot_count == 0 {
                return Err(PrototypeLoadError::InvalidBeaconMetadata {
                    entity: name.to_string(),
                    detail: "beacons require at least one module slot",
                });
            }
            if beacon.effect_radius_tiles == 0 || beacon.transmission_permyriad == 0 {
                return Err(PrototypeLoadError::InvalidBeaconMetadata {
                    entity: name.to_string(),
                    detail: "effect radius and transmission must be positive",
                });
            }
            if entity.assembling_machine.is_some()
                || entity.furnace.is_some()
                || entity.mining_drill.is_some()
                || entity.burner.is_some()
                || entity.electric_energy_source.is_some()
            {
                return Err(PrototypeLoadError::InvalidBeaconMetadata {
                    entity: name.to_string(),
                    detail: "passive beacons cannot carry machine or energy-source metadata",
                });
            }
        }
        (EntityKind::Beacon, None) => {
            return Err(PrototypeLoadError::InvalidBeaconMetadata {
                entity: name.to_string(),
                detail: "beacon entities require beacon metadata",
            });
        }
        (_, Some(_)) => {
            return Err(PrototypeLoadError::InvalidBeaconMetadata {
                entity: name.to_string(),
                detail: "beacon metadata is only valid on beacon entities",
            });
        }
        (_, None) => {}
    }
    Ok(())
}

fn validate_laser_turret_metadata(
    name: &str,
    entity: &RawEntityPrototype,
) -> Result<(), PrototypeLoadError> {
    let is_laser = entity.entity_kind == crate::model::EntityKind::LaserTurret;
    if is_laser
        && (entity.max_health.is_none()
            || entity.electric_energy_source.is_none()
            || entity.laser_turret.is_none())
    {
        return Err(PrototypeLoadError::InvalidLaserTurretMetadata {
            entity: name.to_string(),
            detail: "laser turrets require health, electric, and laser-turret metadata",
        });
    }
    if !is_laser && entity.laser_turret.is_some() {
        return Err(PrototypeLoadError::InvalidLaserTurretMetadata {
            entity: name.to_string(),
            detail: "laser-turret metadata is only valid on laser turret entities",
        });
    }
    if let Some(laser) = entity.laser_turret {
        if laser.range_tiles == 0 || laser.damage == 0 || laser.cooldown_ticks == 0 {
            return Err(PrototypeLoadError::InvalidLaserTurretMetadata {
                entity: name.to_string(),
                detail: "range, damage, and cooldown must be positive",
            });
        }
        let electric = entity
            .electric_energy_source
            .as_ref()
            .expect("presence checked above");
        if electric.energy_usage_watts == 0 || electric.drain_watts == 0 {
            return Err(PrototypeLoadError::InvalidLaserTurretMetadata {
                entity: name.to_string(),
                detail: "active power and idle drain must be positive",
            });
        }
        if entity.max_health == Some(0) {
            return Err(PrototypeLoadError::InvalidLaserTurretMetadata {
                entity: name.to_string(),
                detail: "maximum health must be positive",
            });
        }
    }
    Ok(())
}

/// Solar panels and accumulators are passive, fuel-free power participants.
/// Their prototypes must carry the matching metadata, keep it off every other
/// entity kind, and never mix in the ordinary machine energy sources (electric
/// consumer, burner, steam, or fluid boxes) that would wire them into the
/// consumer-demand model instead.
fn validate_solar_and_storage_metadata(
    name: &str,
    entity: &RawEntityPrototype,
) -> Result<(), PrototypeLoadError> {
    use crate::model::EntityKind;

    let invalid = |detail| {
        Err(PrototypeLoadError::InvalidSolarStorageMetadata {
            entity: name.to_string(),
            detail,
        })
    };
    let no_energy_or_fluid = entity.electric_energy_source.is_none()
        && entity.burner.is_none()
        && entity.steam_engine.is_none()
        && entity.boiler.is_none()
        && entity.heat_buffer.is_none()
        && entity.heat_energy_source.is_none()
        && entity.fluid_boxes.is_empty();

    match entity.entity_kind {
        EntityKind::SolarPanel => {
            let Some(solar) = entity.solar_panel else {
                return invalid("solar panel entities require solar_panel metadata");
            };
            if solar.max_power_output_watts == 0 {
                return invalid("solar panel maximum output must be positive");
            }
            if entity.accumulator.is_some() {
                return invalid("solar panels cannot declare accumulator metadata");
            }
            if !no_energy_or_fluid {
                return invalid(
                    "solar panels cannot declare electric, burner, steam, boiler, or fluid metadata",
                );
            }
        }
        EntityKind::Accumulator => {
            let Some(accumulator) = entity.accumulator else {
                return invalid("accumulator entities require accumulator metadata");
            };
            if accumulator.capacity_joules == 0
                || accumulator.max_charge_watts == 0
                || accumulator.max_discharge_watts == 0
            {
                return invalid(
                    "accumulator capacity, charge, and discharge rates must be positive",
                );
            }
            if entity.solar_panel.is_some() {
                return invalid("accumulators cannot declare solar_panel metadata");
            }
            if !no_energy_or_fluid {
                return invalid(
                    "accumulators cannot declare electric, burner, steam, boiler, or fluid metadata",
                );
            }
        }
        _ => {
            if entity.solar_panel.is_some() {
                return invalid("solar_panel metadata is only valid on solar panel entities");
            }
            if entity.accumulator.is_some() {
                return invalid("accumulator metadata is only valid on accumulator entities");
            }
        }
    }
    Ok(())
}

fn validate_radar_metadata(
    name: &str,
    entity: &RawEntityPrototype,
) -> Result<(), PrototypeLoadError> {
    use crate::model::EntityKind;

    let invalid = |detail| {
        Err(PrototypeLoadError::InvalidRadarMetadata {
            entity: name.to_string(),
            detail,
        })
    };

    match (entity.entity_kind, entity.radar) {
        (EntityKind::Radar, Some(radar)) => {
            if radar.nearby_reveal_radius_chunks == 0
                || radar.nearby_scan_interval_ticks == 0
                || radar.far_scan_radius_chunks <= radar.nearby_reveal_radius_chunks
                || radar.far_scan_interval_ticks == 0
            {
                return invalid(
                    "scan radii and intervals must be positive, and far radius must exceed nearby radius",
                );
            }
            if entity
                .electric_energy_source
                .as_ref()
                .is_none_or(|source| source.energy_usage_watts == 0)
            {
                return invalid("radars require a positive electric energy source");
            }
            if entity.burner.is_some() {
                return invalid("radars cannot declare a burner energy source");
            }
            if entity.max_health.is_none_or(|health| health == 0) {
                return invalid("radars require positive maximum health");
            }
        }
        (EntityKind::Radar, None) => return invalid("radar entities require radar metadata"),
        (_, Some(_)) => return invalid("radar metadata is only valid on radar entities"),
        (_, None) => {}
    }

    Ok(())
}

/// A lab holds the science packs it consumes, so its prototype has to declare
/// the inventory it holds them in.
///
/// The simulation builds a lab's inventory straight from this count when the
/// lab is placed, and a lab with no slots could never be fed. Rejecting it here
/// keeps the omission a named load failure rather than a placement-time panic
/// deep in machine state construction.
fn validate_lab_metadata(
    entity_name: &str,
    entity_kind: EntityKind,
    inventory_slot_count: Option<usize>,
) -> Result<(), PrototypeLoadError> {
    if entity_kind == EntityKind::Lab && inventory_slot_count.is_none_or(|count| count == 0) {
        return Err(PrototypeLoadError::InvalidEntityMetadata {
            entity: entity_name.to_string(),
            detail: "lab entities require inventory slots to hold science packs",
        });
    }

    Ok(())
}

/// Furnaces and mining drills work from exactly one energy source, so their
/// prototypes must declare either a burner or an electric energy source (not
/// both, not neither). Furnaces additionally need a `furnace` section with a
/// positive crafting speed so smelting times are always well-defined.
fn validate_machine_energy_source(
    entity_name: &str,
    entity_kind: crate::model::EntityKind,
    furnace: Option<&crate::model::FurnacePrototype>,
    has_mining_drill: bool,
    has_burner: bool,
    has_electric: bool,
) -> Result<(), PrototypeLoadError> {
    let invalid = |detail| {
        Err(PrototypeLoadError::InvalidMachineEnergySource {
            entity: entity_name.to_string(),
            detail,
        })
    };

    match entity_kind {
        crate::model::EntityKind::Furnace => {
            let Some(furnace) = furnace else {
                return invalid("furnace entities require a furnace section");
            };
            if furnace.crafting_speed_numerator == 0 || furnace.crafting_speed_denominator == 0 {
                return invalid("furnace crafting speed fraction must be positive");
            }
            if has_burner == has_electric {
                return invalid(
                    "furnace entities require exactly one of burner or electric_energy_source",
                );
            }
        }
        crate::model::EntityKind::MiningDrill => {
            if !has_mining_drill {
                return invalid("mining drill entities require a mining_drill section");
            }
            if has_burner == has_electric {
                return invalid(
                    "mining drill entities require exactly one of burner or electric_energy_source",
                );
            }
        }
        crate::model::EntityKind::Inserter => {
            if has_burner == has_electric {
                return invalid(
                    "inserter entities require exactly one of burner or electric_energy_source",
                );
            }
        }
        _ => {
            if furnace.is_some() {
                return invalid("only furnace entities may declare a furnace section");
            }
        }
    }

    Ok(())
}

/// Resolves an entity's declared fluid boxes into world-relative openings.
///
/// A placed entity's box must declare at least one opening: a box no pipe can
/// ever reach is a box nothing could fill, and that is a catalog mistake rather
/// than a design. Rolling stock is the one exception, and it is an exception
/// about placement rather than about fluids — a wagon is never in the occupancy
/// grid the pipe networks are built from, so its tank is filled at a station
/// and an opening on its footprint would be a promise the simulation could not
/// keep.
fn resolve_fluid_boxes(
    entity_name: &str,
    entity_kind: EntityKind,
    entity_size: IVec2,
    fluid_boxes: Vec<RawFluidBoxPrototype>,
    fluid_ids_by_name: &HashMap<String, FluidId>,
) -> Result<Vec<FluidBoxPrototype>, PrototypeLoadError> {
    let connections_required = !entity_kind.is_rolling_stock();
    fluid_boxes
        .into_iter()
        .enumerate()
        .map(|(box_index, fluid_box)| {
            if fluid_box.capacity_milliunits == 0
                || (connections_required && fluid_box.connections.is_empty())
            {
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
            let connections =
                resolve_edge_connections(fluid_box.connections, entity_size, |index| {
                    PrototypeLoadError::InvalidFluidConnection {
                        entity: entity_name.to_string(),
                        box_index,
                        connection_index: index,
                    }
                })?;

            Ok(FluidBoxPrototype {
                capacity_milliunits: fluid_box.capacity_milliunits,
                filter,
                io: fluid_box.io,
                connections,
            })
        })
        .collect()
}

fn resolve_heat_buffer(
    entity_name: &str,
    entity_size: IVec2,
    heat_buffer: Option<RawHeatBufferPrototype>,
) -> Result<Option<HeatBufferPrototype>, PrototypeLoadError> {
    let Some(heat_buffer) = heat_buffer else {
        return Ok(None);
    };

    let invalid = |detail| PrototypeLoadError::InvalidHeatMetadata {
        entity: entity_name.to_string(),
        detail,
    };
    if heat_buffer.specific_heat_joules_per_degree == 0 {
        return Err(invalid("heat buffer specific heat must be positive"));
    }
    if heat_buffer.max_temperature_degrees <= crate::model::HEAT_AMBIENT_TEMPERATURE_DEGREES {
        return Err(invalid(
            "heat buffer maximum temperature must exceed the ambient temperature",
        ));
    }
    if heat_buffer.connections.is_empty() {
        return Err(invalid("heat buffers require at least one connection"));
    }

    let connections = resolve_edge_connections(heat_buffer.connections, entity_size, |index| {
        PrototypeLoadError::InvalidHeatConnection {
            entity: entity_name.to_string(),
            connection_index: index,
        }
    })?;

    Ok(Some(HeatBufferPrototype {
        specific_heat_joules_per_degree: heat_buffer.specific_heat_joules_per_degree,
        max_temperature_degrees: heat_buffer.max_temperature_degrees,
        connections,
    }))
}

/// Resolves tile-edge openings shared by fluid boxes and heat buffers, checking
/// that every opening sits on the footprint edge it claims to face.
fn resolve_edge_connections(
    connections: Vec<RawEdgeConnectionPrototype>,
    entity_size: IVec2,
    invalid_connection: impl Fn(usize) -> PrototypeLoadError,
) -> Result<Vec<EdgeConnectionPrototype>, PrototypeLoadError> {
    connections
        .into_iter()
        .enumerate()
        .map(|(connection_index, connection)| {
            if !edge_connection_is_on_footprint_edge(entity_size, &connection) {
                return Err(invalid_connection(connection_index));
            }
            Ok(EdgeConnectionPrototype {
                local_offset: IVec2::new(connection.local_offset.x, connection.local_offset.y),
                side: connection.side,
            })
        })
        .collect()
}

/// Heat metadata is only coherent when the kind, the buffer, and the energy
/// source agree: reactors produce into a buffer, heat exchangers consume from
/// one, heat pipes only carry heat, and no other kind touches heat at all.
#[allow(clippy::too_many_arguments)]
fn validate_heat_metadata(
    name: &str,
    entity_kind: crate::model::EntityKind,
    heat_buffer: Option<&HeatBufferPrototype>,
    heat_energy_source: Option<&crate::model::HeatEnergySourcePrototype>,
    nuclear_reactor: Option<&crate::model::NuclearReactorPrototype>,
    boiler: Option<&crate::model::BoilerPrototype>,
    has_burner: bool,
    has_electric: bool,
    inventory_slot_count: Option<usize>,
) -> Result<(), PrototypeLoadError> {
    use crate::model::EntityKind;

    let invalid = |detail| {
        Err(PrototypeLoadError::InvalidHeatMetadata {
            entity: name.to_string(),
            detail,
        })
    };

    match entity_kind {
        EntityKind::NuclearReactor => {
            let Some(reactor) = nuclear_reactor else {
                return invalid("nuclear reactors require nuclear_reactor metadata");
            };
            if heat_buffer.is_none() {
                return invalid("nuclear reactors require a heat buffer to produce into");
            }
            if reactor.heat_output_watts == 0 {
                return invalid("nuclear reactor heat output must be positive");
            }
            if has_burner || has_electric || heat_energy_source.is_some() {
                return invalid(
                    "nuclear reactors consume fuel cells directly and declare no other energy source",
                );
            }
            if inventory_slot_count.is_some() {
                return invalid(
                    "nuclear reactors use their own fuel and spent-fuel slots, not an inventory",
                );
            }
        }
        EntityKind::HeatPipe => {
            if heat_buffer.is_none() {
                return invalid("heat pipes require a heat buffer");
            }
            if heat_energy_source.is_some() || has_burner || has_electric || boiler.is_some() {
                return invalid("heat pipes only carry heat and consume no energy");
            }
            if nuclear_reactor.is_some() {
                return invalid("heat pipes carry heat but never produce it");
            }
        }
        EntityKind::HeatExchanger => {
            let Some(heat_energy_source) = heat_energy_source else {
                return invalid("heat exchangers require a heat energy source");
            };
            let Some(heat_buffer) = heat_buffer else {
                return invalid("heat exchangers require a heat buffer to draw from");
            };
            if boiler.is_none() {
                return invalid("heat exchangers require boiler water and steam rates");
            }
            if has_burner || has_electric {
                return invalid("heat exchangers run on heat alone");
            }
            if nuclear_reactor.is_some() {
                return invalid("heat exchangers consume heat but never produce it");
            }
            if heat_energy_source.energy_usage_watts == 0 {
                return invalid("heat exchanger energy usage must be positive");
            }
            if heat_energy_source.min_working_temperature_degrees
                > heat_buffer.max_temperature_degrees
            {
                return invalid(
                    "heat exchanger minimum working temperature exceeds its buffer maximum",
                );
            }
        }
        _ => {
            if heat_buffer.is_some() {
                return invalid("heat buffers are only valid on heat network entities");
            }
            if heat_energy_source.is_some() {
                return invalid("heat energy sources are only valid on heat network entities");
            }
            if nuclear_reactor.is_some() {
                return invalid("nuclear_reactor metadata is only valid on nuclear reactors");
            }
        }
    }

    Ok(())
}

/// Checks that a rail piece's declared geometry describes track a train can
/// actually run on, and that no other entity kind claims to be track.
///
/// Everything here is a property the rail graph, the placement rules, and the
/// renderer all assume without re-checking: ends sit on the footprint edge they
/// face, the path between them matches the headings, and the whole piece stays
/// inside its own footprint so tile-locked occupancy really does reserve every
/// tile the track crosses.
fn validate_rail_metadata(
    name: &str,
    entity_kind: EntityKind,
    size: IVec2,
    rail_piece: Option<&RailPiecePrototype>,
) -> Result<(), PrototypeLoadError> {
    let invalid = |detail| {
        Err(PrototypeLoadError::InvalidRailMetadata {
            entity: name.to_string(),
            detail,
        })
    };

    let Some(rail_piece) = rail_piece else {
        if entity_kind.is_rail() {
            return invalid("rail entities require rail_piece geometry");
        }
        return Ok(());
    };
    if !entity_kind.is_rail() {
        return invalid("rail_piece geometry is only valid on rail entities");
    }

    let width = i64::from(size.x) * i64::from(POSITION_SCALE);
    let height = i64::from(size.y) * i64::from(POSITION_SCALE);
    // Ends inside the footprint is what keeps the whole piece inside it, and so
    // what makes tile-locked occupancy honest. A straight is the segment between
    // its ends and the footprint is convex, so it cannot escape. An arc's center
    // shares one coordinate with each end (each radius is axis-aligned, checked
    // below), so the center is inside too, and every point of a quarter arc lies
    // between the center and the ends on both axes.
    for end in rail_piece.ends() {
        if !point_is_inside(end.position, width, height) {
            return invalid("rail ends must lie inside the footprint");
        }
        let (x, y) = (i64::from(end.position.x), i64::from(end.position.y));
        let on_edge = match end.heading {
            RailHeading::North => y == height,
            RailHeading::East => x == width,
            RailHeading::South => y == 0,
            RailHeading::West => x == 0,
        };
        if !on_edge {
            return invalid("a rail end must sit on the footprint edge its heading leaves through");
        }
    }
    if rail_piece.start.position == rail_piece.end.position {
        return invalid("a rail piece needs two distinct ends");
    }

    match rail_piece.curve {
        RailCurvePrototype::Straight => {
            if rail_piece.start.heading != rail_piece.end.heading.opposite() {
                return invalid("a straight rail's ends must face opposite headings");
            }
            let (step_x, step_y) = rail_piece.end.heading.step();
            let (dx, dy) = offset(rail_piece.start.position, rail_piece.end.position);
            if dx * i64::from(step_y) - dy * i64::from(step_x) != 0
                || dx * i64::from(step_x) + dy * i64::from(step_y) <= 0
            {
                return invalid(
                    "a straight rail must run from one end to the other along its own heading",
                );
            }
        }
        RailCurvePrototype::QuarterArc { center } => {
            if !rail_piece
                .start
                .heading
                .is_perpendicular_to(rail_piece.end.heading)
            {
                return invalid("a curved rail's ends must face perpendicular headings");
            }
            let radius = rail_piece.radius();
            if radius == 0 {
                return invalid("a curved rail needs a positive turning radius");
            }
            // The arc's length is derived from the radius as a whole number of
            // fixed-point units, so a radius that is not one would make the
            // declared curve and its measured length disagree.
            let squared_radius = i128::from(radius) * i128::from(radius);
            if squared_radius != center.squared_distance_to(rail_piece.start.position) {
                return invalid("a curved rail's radius must be a whole number of units");
            }
            for end in rail_piece.ends() {
                if center.squared_distance_to(end.position) != squared_radius {
                    return invalid("both ends of a curved rail must sit on one circle");
                }
                let (dx, dy) = offset(center, end.position);
                let (step_x, step_y) = end.heading.step();
                if dx * i64::from(step_x) + dy * i64::from(step_y) != 0 {
                    return invalid("a curved rail must leave each end along the tangent");
                }
            }
        }
    }

    Ok(())
}

/// Checks that a trackside entity is something the simulation can bind to a
/// single piece of track.
///
/// A signal or a train stop is placed *beside* a rail rather than on it, and
/// which rail it belongs to is answered from the tile it stands on. A footprint
/// wider than one tile would make that question ambiguous — two rails could be
/// equally near two different tiles of the same entity — and the whole binding
/// rule assumes it cannot be.
///
/// Checked here so a catalog that breaks it fails to load at all rather than
/// producing a world whose signals bind arbitrarily. The simulation checks the
/// same shape again when it validates a loaded catalog, which is what covers a
/// save carrying a catalog this loader never saw.
fn validate_trackside_metadata(
    name: &str,
    entity_kind: EntityKind,
    size: IVec2,
) -> Result<(), PrototypeLoadError> {
    if entity_kind.binds_to_nearby_rail() && (size.x != 1 || size.y != 1) {
        return Err(PrototypeLoadError::InvalidRailMetadata {
            entity: name.to_string(),
            detail: "an entity beside the track stands on exactly one tile",
        });
    }

    Ok(())
}

/// Checks that rolling stock declares a body a train can be built from, and
/// that no other entity kind claims to run on rails.
///
/// The motion model divides by the train's mass and compares against its top
/// speed every tick, and coupling spaces stock by its length, so a zero in any
/// of the three is a division by zero or a train of zero-length pieces stacked
/// on one point. Rejecting them here is what lets the simulation treat those
/// numbers as facts.
///
/// The cargo declarations are checked against the kind for the same reason the
/// rail geometry is: a cargo wagon with no inventory and a fluid wagon with no
/// fluid box are entities whose whole purpose is missing, and nothing later
/// would report it.
fn validate_rolling_stock_metadata(
    name: &str,
    entity: &RawEntityPrototype,
) -> Result<(), PrototypeLoadError> {
    let invalid = |detail| {
        Err(PrototypeLoadError::InvalidRollingStockMetadata {
            entity: name.to_string(),
            detail,
        })
    };

    let Some(rolling_stock) = entity.rolling_stock else {
        if entity.entity_kind.is_rolling_stock() {
            return invalid("rolling stock requires rolling_stock metadata");
        }
        return Ok(());
    };
    if !entity.entity_kind.is_rolling_stock() {
        return invalid("rolling_stock metadata is only valid on rolling stock");
    }

    if rolling_stock.length_fixed <= 0 {
        return invalid("rolling stock needs a positive length");
    }
    if rolling_stock.weight_kilograms == 0 {
        return invalid("rolling stock needs a positive weight");
    }
    if rolling_stock.max_speed_fixed_per_tick == 0 {
        return invalid("rolling stock needs a positive top speed");
    }

    match (entity.entity_kind, rolling_stock.locomotive) {
        (EntityKind::Locomotive, Some(locomotive)) => {
            if locomotive.tractive_force_newtons == 0 {
                return invalid("a locomotive needs a positive tractive force");
            }
            // Tractive force is what the burnt fuel buys; without a burner the
            // locomotive would pull for free.
            if entity.burner.is_none() {
                return invalid("a locomotive needs a burner to fuel its tractive force");
            }
        }
        (EntityKind::Locomotive, None) => {
            return invalid("a locomotive needs locomotive metadata");
        }
        (_, Some(_)) => return invalid("locomotive metadata is only valid on locomotives"),
        (_, None) => {}
    }

    match entity.entity_kind {
        EntityKind::CargoWagon if entity.inventory_slot_count.unwrap_or(0) == 0 => {
            invalid("a cargo wagon needs inventory slots")
        }
        EntityKind::FluidWagon if entity.fluid_boxes.is_empty() => {
            invalid("a fluid wagon needs a fluid box")
        }
        _ => Ok(()),
    }
}

fn point_is_inside(point: RailPointPrototype, width: i64, height: i64) -> bool {
    let (x, y) = (i64::from(point.x), i64::from(point.y));
    x >= 0 && y >= 0 && x <= width && y <= height
}

/// Vector between two sub-tile points. Each coordinate is widened before the
/// subtraction, so a difference wider than an `i32` cannot wrap; the result is
/// only ever used for the dot and cross products that check an axis, both of
/// which stay well inside an `i64`.
fn offset(from: RailPointPrototype, to: RailPointPrototype) -> (i64, i64) {
    (
        i64::from(to.x) - i64::from(from.x),
        i64::from(to.y) - i64::from(from.y),
    )
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

/// Whether an opening sits inside the footprint and on the very edge it faces,
/// so it can only ever join a neighbour across that edge.
fn edge_connection_is_on_footprint_edge(
    entity_size: IVec2,
    connection: &RawEdgeConnectionPrototype,
) -> bool {
    let x = connection.local_offset.x;
    let y = connection.local_offset.y;
    let on_entity = x >= 0 && y >= 0 && x < entity_size.x && y < entity_size.y;
    let on_side = match connection.side {
        ConnectionSide::North => y == 0,
        ConnectionSide::East => x == entity_size.x - 1,
        ConnectionSide::South => y == entity_size.y - 1,
        ConnectionSide::West => x == 0,
    };

    on_entity && on_side
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
        // Heat exchangers are boilers with a heat energy source, so they carry
        // the same water-in / steam-out box layout.
        crate::model::EntityKind::Boiler | crate::model::EntityKind::HeatExchanger => {
            require_fluid_box_filters(
                entity_name,
                fluid_boxes,
                &[
                    Some(required_fluid("water")?),
                    Some(required_fluid("steam")?),
                ],
            )
        }
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
