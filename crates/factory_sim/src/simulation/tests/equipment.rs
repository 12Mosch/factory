use super::super::*;
use super::combat::spawn_test_enemy_at;
use super::support::{
    all_tile_coords, entity_id_by_name, item_id_by_name, place_at, set_inventory_slot,
};

fn add_item(sim: &mut Simulation, slot: usize, name: &str) -> ItemId {
    let item_id = item_id_by_name(sim.catalog(), name);
    set_inventory_slot(&mut sim.player_inventory, slot, item_id, 1);
    item_id
}

fn equip_modular_armor(sim: &mut Simulation) -> ItemId {
    let armor = add_item(sim, 10, "modular_armor");
    sim.equip_armor(10).unwrap();
    armor
}

/// Installs the base personal roboport and seeds one construction robot.
fn install_personal_roboport(sim: &mut Simulation) -> (ItemId, ItemId) {
    equip_modular_armor(sim);
    let equipment = add_item(sim, 11, "personal_roboport_equipment");
    sim.install_equipment(11, 0, 0).unwrap();
    let robot = item_id_by_name(sim.catalog(), "construction_robot");
    set_inventory_slot(&mut sim.player_inventory, 20, robot, 1);
    sim.player_equipment.personal_roboport_energy_joules = 3_000_000;
    (equipment, robot)
}

/// Places a valid ghost inside personal coverage, optionally requiring an
/// overlap with stationary construction coverage.
fn place_personal_ghost(
    sim: &mut Simulation,
    prototype_id: EntityPrototypeId,
    require_stationary_coverage: bool,
) -> GhostId {
    let bounds = sim
        .personal_roboport_coverage()
        .expect("fixture installs personal coverage");
    for (x, y) in all_tile_coords(&sim.world) {
        if !bounds.contains(x, y)
            || (require_stationary_coverage
                && sim.construction_network_covering_tile(x, y).is_none())
        {
            continue;
        }
        let request = construction_ops::GhostPlacementRequest {
            prototype_id,
            x,
            y,
            direction: Direction::East,
            recipe: None,
        };
        let entity_request = placement::EntityPlacementRequest {
            prototype_id,
            x,
            y,
            direction: Direction::East,
        };
        if construction_ops::validate_ghost_placement(sim, request).is_ok()
            && placement_validation_ops::validate_entity_placement(sim, entity_request).is_ok()
        {
            return construction_ops::place_ghost(sim, request).unwrap();
        }
    }
    panic!("expected a buildable tile in personal roboport coverage");
}

/// Advances a fixture until a condition holds or fails with a bounded timeout.
fn tick_until(sim: &mut Simulation, limit: usize, predicate: impl Fn(&Simulation) -> bool) {
    for _ in 0..limit {
        if predicate(sim) {
            return;
        }
        sim.tick();
    }
    panic!("condition did not hold within {limit} ticks");
}

#[test]
fn personal_laser_uses_shared_battery_and_tracks_each_grid_slot_cooldown() {
    let mut sim = Simulation::new_test_world(123);
    equip_modular_armor(&mut sim);
    let battery = add_item(&mut sim, 11, "battery_equipment");
    let laser = add_item(&mut sim, 12, "personal_laser_equipment");
    sim.install_equipment(11, 0, 0).unwrap();
    sim.install_equipment(12, 1, 0).unwrap();
    sim.player_equipment.battery_energy_joules = 100_000;
    let (x, y) = sim.player.tile_position();
    let enemy = spawn_test_enemy_at(&mut sim, x + 5, y);

    let mut commands = CombatCommandBuffer::default();
    sim.advance_defensive_turrets(&mut commands);
    sim.advance_personal_lasers(&mut commands);
    sim.resolve_combat_commands(commands);
    assert_eq!(sim.enemies.get(enemy).unwrap().health.current, 15);
    assert_eq!(sim.personal_stored_energy(), (50_000, 500_000));

    let mut commands = CombatCommandBuffer::default();
    sim.advance_defensive_turrets(&mut commands);
    sim.advance_personal_lasers(&mut commands);
    assert!(commands.is_empty(), "module must respect its own cooldown");
    for _ in 0..30 {
        sim.tick += 1;
        sim.advance_day_night_cycle();
        sim.advance_statistics_to_current_tick();
    }
    sim.advance_defensive_turrets(&mut commands);
    sim.advance_personal_lasers(&mut commands);
    sim.resolve_combat_commands(commands);
    assert!(sim.enemies.get(enemy).is_none());
    assert_eq!(sim.personal_stored_energy(), (0, 500_000));

    sim.remove_equipment(1, 0).unwrap();
    assert_eq!(sim.player_inventory.count(laser), 1);
    assert_eq!(sim.player_inventory.count(battery), 0);
    assert!(
        sim.player_equipment
            .personal_laser_next_ready_ticks
            .is_empty()
    );
    sim.validate().unwrap();
}

#[test]
fn armor_and_equipment_commands_conserve_items_and_place_canonically() {
    let mut sim = Simulation::new_test_world(123);
    let armor = equip_modular_armor(&mut sim);
    let battery = add_item(&mut sim, 11, "battery_equipment");
    let solar = add_item(&mut sim, 12, "portable_solar_panel");

    sim.install_equipment(11, 3, 2).unwrap();
    sim.install_equipment(12, 0, 0).unwrap();

    assert_eq!(sim.equipped_armor(), Some(armor));
    assert_eq!(
        sim.installed_equipment(),
        &[
            InstalledEquipment {
                item_id: solar,
                x: 0,
                y: 0,
            },
            InstalledEquipment {
                item_id: battery,
                x: 3,
                y: 2,
            },
        ]
    );
    assert_eq!(
        sim.unequip_armor(),
        Err(PlayerEquipmentError::ArmorGridNotEmpty)
    );
    assert_eq!(sim.player_inventory().count(armor), 0);
    assert!(sim.validate().is_ok());
}

#[test]
fn placement_rejects_overlap_and_bounds_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    equip_modular_armor(&mut sim);
    let shield = add_item(&mut sim, 11, "energy_shield_equipment");
    let battery = add_item(&mut sim, 12, "battery_equipment");
    sim.install_equipment(11, 1, 1).unwrap();
    let before = sim.state_hash();

    assert_eq!(
        sim.install_equipment(12, 2, 2),
        Err(PlayerEquipmentError::PlacementOverlaps)
    );
    assert_eq!(sim.state_hash(), before);
    assert_eq!(
        sim.install_equipment(12, 4, 4),
        Err(PlayerEquipmentError::PlacementOutOfBounds)
    );
    assert_eq!(sim.state_hash(), before);
    assert_eq!(sim.player_inventory().count(battery), 1);
    assert_eq!(sim.player_inventory().count(shield), 0);
}

#[test]
fn personal_power_recharges_shields_then_battery_with_exact_integer_energy() {
    let mut sim = Simulation::new_test_world(123);
    equip_modular_armor(&mut sim);
    add_item(&mut sim, 11, "portable_solar_panel");
    add_item(&mut sim, 12, "battery_equipment");
    add_item(&mut sim, 13, "energy_shield_equipment");
    sim.install_equipment(11, 0, 0).unwrap();
    sim.install_equipment(12, 1, 0).unwrap();
    sim.install_equipment(13, 2, 0).unwrap();

    for _ in 0..50 {
        sim.advance_player_equipment();
    }
    assert_eq!(sim.personal_shield_points(), (50, 50));
    assert_eq!(sim.personal_stored_energy(), (0, 500_000));

    sim.advance_player_equipment();
    assert_eq!(sim.personal_stored_energy(), (1_000, 500_000));
    assert_eq!(sim.player_equipment.generation_remainder_watt_ticks, 0);
    assert_eq!(sim.player_equipment.recharge_remainder_watt_ticks, 0);
}

#[test]
fn armor_mitigates_before_shields_and_shields_protect_health() {
    let mut sim = Simulation::new_test_world(123);
    equip_modular_armor(&mut sim);
    add_item(&mut sim, 11, "energy_shield_equipment");
    sim.install_equipment(11, 0, 0).unwrap();
    sim.player_equipment.shield_energy_joules = 10_000;

    let mut commands = CombatCommandBuffer::default();
    commands.push(CombatCommand {
        source: CombatSource::new(CombatantId::Enemy(EnemyId::new(1)), Faction::Enemy),
        target: CombatantId::Player,
        damage: Damage::physical(12),
    });
    sim.resolve_combat_commands(commands);

    // (12 - 2) * 80% = 8, all absorbed by the shield.
    assert_eq!(sim.player_health(), (PLAYER_MAX_HEALTH, PLAYER_MAX_HEALTH));
    assert_eq!(sim.player_equipment.shield_energy_joules, 2_000);
}

#[test]
fn removing_through_any_occupied_cell_returns_one_item_and_clamps_capacity() {
    let mut sim = Simulation::new_test_world(123);
    equip_modular_armor(&mut sim);
    let shield = add_item(&mut sim, 11, "energy_shield_equipment");
    sim.install_equipment(11, 1, 1).unwrap();
    sim.player_equipment.shield_energy_joules = 50_000;

    sim.remove_equipment(2, 2).unwrap();

    assert!(sim.installed_equipment().is_empty());
    assert_eq!(sim.player_inventory().count(shield), 1);
    assert_eq!(sim.personal_shield_points(), (0, 0));
}

#[test]
fn equipment_state_round_trips_and_remains_lockstep() {
    let mut sim = Simulation::new_test_world(123);
    equip_modular_armor(&mut sim);
    add_item(&mut sim, 11, "portable_solar_panel");
    add_item(&mut sim, 12, "battery_equipment");
    sim.install_equipment(11, 0, 0).unwrap();
    sim.install_equipment(12, 1, 0).unwrap();
    for _ in 0..17 {
        sim.tick();
    }

    let bytes = save_to_bytes(&sim).unwrap();
    let mut loaded = load_from_bytes(&bytes).unwrap();
    assert_eq!(sim.state_hash(), loaded.state_hash());
    for _ in 0..30 {
        sim.tick();
        loaded.tick();
        assert_eq!(sim.state_hash(), loaded.state_hash());
    }
}

#[test]
fn validation_rejects_noncanonical_and_over_capacity_equipment_state() {
    let mut sim = Simulation::new_test_world(123);
    equip_modular_armor(&mut sim);
    let solar = item_id_by_name(sim.catalog(), "portable_solar_panel");
    sim.player_equipment.installed = vec![
        InstalledEquipment {
            item_id: solar,
            x: 1,
            y: 0,
        },
        InstalledEquipment {
            item_id: solar,
            x: 0,
            y: 0,
        },
    ];
    assert_eq!(
        sim.validate(),
        Err(SimValidationError::InvalidPlayerEquipment)
    );

    sim.player_equipment.installed.clear();
    sim.player_equipment.battery_energy_joules = 1;
    assert_eq!(
        sim.validate(),
        Err(SimValidationError::InvalidPlayerEquipment)
    );
}

#[test]
/// Personal dispatch consumes player-owned inputs and returns the robot after
/// completing its build.
fn personal_roboport_dispatches_from_player_inventory_and_builds() {
    let mut sim = Simulation::new_test_world(123);
    let (_, robot_item) = install_personal_roboport(&mut sim);
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let build_item = sim.catalog().entity(furnace).unwrap().build_item.unwrap();
    set_inventory_slot(&mut sim.player_inventory, 21, build_item, 1);
    let material_before = sim.player_inventory.count(build_item);
    let ghost_id = place_personal_ghost(&mut sim, furnace, false);

    sim.tick();

    let (_, robot_id) = sim
        .construction
        .reservations()
        .find(|(job, _)| *job == ConstructionJob::BuildGhost(ghost_id))
        .expect("personal roboport should reserve covered work");
    let robot = sim.robot(robot_id).expect("dispatched robot should fly");
    assert!(robot.personal);
    assert_eq!(robot.home_roboport, None);
    assert_eq!(sim.player_inventory.count(robot_item), 0);
    assert_eq!(sim.player_inventory.count(build_item), material_before - 1);

    tick_until(&mut sim, 2_000, |sim| {
        sim.construction.ghost(ghost_id).is_none() && sim.robot_count() == 0
    });
    assert_eq!(sim.player_inventory.count(robot_item), 1);
    sim.validate()
        .expect("personal construction should remain valid");
}

#[test]
/// Dispatch waits below the prototype energy threshold without consuming any
/// player-owned items, then resumes exactly at the threshold.
fn personal_roboport_low_power_blocks_dispatch_without_consuming_items() {
    let mut sim = Simulation::new_test_world(123);
    let (_, robot_item) = install_personal_roboport(&mut sim);
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let build_item = sim.catalog().entity(furnace).unwrap().build_item.unwrap();
    let robot_capacity = sim
        .catalog()
        .item(robot_item)
        .and_then(|item| item.robot)
        .expect("construction robot declares a flight profile")
        .energy_capacity_joules;
    set_inventory_slot(&mut sim.player_inventory, 21, build_item, 1);
    let material_before = sim.player_inventory.count(build_item);
    let ghost_id = place_personal_ghost(&mut sim, furnace, false);
    sim.player_equipment.personal_roboport_energy_joules = robot_capacity - 1;

    sim.tick();

    assert_eq!(sim.robot_count(), 0);
    assert_eq!(sim.player_inventory.count(robot_item), 1);
    assert_eq!(sim.player_inventory.count(build_item), material_before);
    assert!(
        sim.construction
            .queue()
            .any(|job| job == ConstructionJob::BuildGhost(ghost_id))
    );

    sim.player_equipment.personal_roboport_energy_joules = robot_capacity;
    sim.tick();
    assert_eq!(sim.robot_count(), 1);
    assert_eq!(sim.player_inventory.count(robot_item), 0);
    assert_eq!(sim.player_inventory.count(build_item), material_before - 1);

    // Draining the equipment after dispatch cannot erase the unit. It waits on
    // its personal pad with its exact remaining energy until power returns.
    sim.player_equipment.personal_roboport_energy_joules = 0;
    tick_until(&mut sim, 2_000, |sim| {
        sim.robots()
            .any(|robot| robot.activity == RobotActivity::PersonalCharging)
    });
    assert_eq!(sim.robot_count(), 1);
    assert_eq!(sim.player_inventory.count(robot_item), 0);
    assert_eq!(sim.player_equipment.personal_roboport_energy_joules, 0);

    sim.player_equipment.personal_roboport_energy_joules = 3_000_000;
    tick_until(&mut sim, 1_000, |sim| sim.robot_count() == 0);
    assert_eq!(sim.player_inventory.count(robot_item), 1);
}

#[test]
/// Personal coverage wins deterministically when personal and stationary
/// construction networks overlap.
fn personal_roboport_precedes_stationary_network_in_overlapping_coverage() {
    use super::support::{place_roboport, station_robots};

    let mut sim = Simulation::new_test_world(123);
    let (stationary, (x, y)) = place_roboport(&mut sim);
    station_robots(&mut sim, stationary, "construction_robot", 1);
    let player_tile = all_tile_coords(&sim.world)
        .into_iter()
        .filter(|(tile_x, tile_y)| sim.can_player_occupy_tile(*tile_x, *tile_y))
        .min_by_key(|(tile_x, tile_y)| (tile_x - x).pow(2) + (tile_y - y).pow(2))
        .expect("a walkable player tile should exist near the roboport");
    sim.player.x = tile_center_fixed(player_tile.0);
    sim.player.y = tile_center_fixed(player_tile.1);
    let (_, personal_robot) = install_personal_roboport(&mut sim);
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let build_item = sim.catalog().entity(furnace).unwrap().build_item.unwrap();
    set_inventory_slot(&mut sim.player_inventory, 21, build_item, 1);
    sim.entities
        .roboport_state_mut(stationary)
        .unwrap()
        .materials
        .insert(&sim.world.prototypes.clone(), build_item, 1)
        .unwrap();
    let ghost_id = place_personal_ghost(&mut sim, furnace, true);

    sim.tick();

    let (_, robot_id) = sim
        .construction
        .reservations()
        .find(|(job, _)| *job == ConstructionJob::BuildGhost(ghost_id))
        .unwrap();
    assert!(sim.robot(robot_id).unwrap().personal);
    assert_eq!(sim.player_inventory.count(personal_robot), 0);
    let stationary_state = sim.entities.roboport_state(stationary).unwrap();
    assert_eq!(stationary_state.robots.count(personal_robot), 1);
    assert_eq!(stationary_state.materials.count(build_item), 1);
}

#[test]
/// Leaving personal coverage aborts the reservation and recovers the robot and
/// its payload without loss.
fn moving_out_of_range_recovers_owned_job_robot_and_payload() {
    let mut sim = Simulation::new_test_world(123);
    let (_, robot_item) = install_personal_roboport(&mut sim);
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let build_item = sim.catalog().entity(furnace).unwrap().build_item.unwrap();
    set_inventory_slot(&mut sim.player_inventory, 21, build_item, 1);
    let material_before = sim.player_inventory.count(build_item);
    let ghost_id = place_personal_ghost(&mut sim, furnace, false);
    sim.tick();
    assert_eq!(sim.construction.reservations().count(), 1);

    sim.player.x += 100 * POSITION_SCALE;
    sim.tick();

    assert_eq!(sim.construction.reservations().count(), 0);
    assert!(
        sim.construction
            .queue()
            .any(|job| job == ConstructionJob::BuildGhost(ghost_id))
    );
    assert!(sim.robots().all(|robot| robot.construction_job.is_none()));
    tick_until(&mut sim, 3_000, |sim| sim.robot_count() == 0);
    assert_eq!(sim.player_inventory.count(robot_item), 1);
    assert_eq!(sim.player_inventory.count(build_item), material_before);
}

#[test]
/// Personal robots use player repair materials and return deconstruction
/// recoveries to the player inventory.
fn personal_robot_repairs_and_deconstructs_with_player_owned_materials() {
    let mut sim = Simulation::new_test_world(123);
    let (_, robot_item) = install_personal_roboport(&mut sim);
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let ghost_id = place_personal_ghost(&mut sim, furnace, false);
    let ghost = sim.construction.ghost(ghost_id).cloned().unwrap();
    construction_ops::cancel_ghost(&mut sim, ghost_id).unwrap();
    let entity_id = place_at(&mut sim, furnace, ghost.x, ghost.y, Direction::North);
    let build_item = sim.catalog().entity(furnace).unwrap().build_item.unwrap();
    let material_before = sim.player_inventory.count(build_item);
    let maximum = sim.entity_health(entity_id).unwrap().1;
    assert!(!sim.damage_entity(entity_id, maximum / 2));
    let repair_pack = item_id_by_name(sim.catalog(), "repair_pack");
    set_inventory_slot(&mut sim.player_inventory, 21, repair_pack, 1);

    sim.tick();
    assert!(sim.robots().any(|robot| {
        robot.personal && robot.construction_job == Some(ConstructionJob::Repair(entity_id))
    }));
    tick_until(&mut sim, 2_000, |sim| {
        sim.entity_health(entity_id) == Some((maximum, maximum)) && sim.robot_count() == 0
    });

    construction_ops::mark_area_for_deconstruction(&mut sim, ghost.x, ghost.y, ghost.x, ghost.y);
    sim.player_equipment.personal_roboport_energy_joules = 3_000_000;
    sim.tick();
    assert!(sim.robots().any(|robot| {
        robot.personal && robot.construction_job == Some(ConstructionJob::Deconstruct(entity_id))
    }));
    tick_until(&mut sim, 2_000, |sim| {
        sim.entities.placed_entity(entity_id).is_none() && sim.robot_count() == 0
    });
    assert_eq!(sim.player_inventory.count(robot_item), 1);
    assert_eq!(sim.player_inventory.count(build_item), material_before + 1);
}

#[test]
/// Personal robot flight, job, charging, and equipment state serialize and
/// replay deterministically.
fn deployed_personal_robot_energy_job_and_equipment_round_trip_deterministically() {
    let mut sim = Simulation::new_test_world(123);
    install_personal_roboport(&mut sim);
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let build_item = sim.catalog().entity(furnace).unwrap().build_item.unwrap();
    set_inventory_slot(&mut sim.player_inventory, 21, build_item, 1);
    let ghost_id = place_personal_ghost(&mut sim, furnace, false);
    sim.tick();
    let robot_before = sim.robots().next().cloned().unwrap();
    let equipment_energy_before = sim.personal_roboport_energy();

    let bytes = save_to_bytes(&sim).unwrap();
    let mut loaded = load_from_bytes(&bytes).unwrap();
    assert_eq!(loaded.robots().next(), Some(&robot_before));
    assert_eq!(loaded.personal_roboport_energy(), equipment_energy_before);
    assert!(
        loaded
            .construction
            .reservations()
            .any(|(job, _)| { job == ConstructionJob::BuildGhost(ghost_id) })
    );
    assert_eq!(sim.state_hash(), loaded.state_hash());

    for _ in 0..500 {
        sim.tick();
        loaded.tick();
        assert_eq!(sim.state_hash(), loaded.state_hash());
    }
}

#[test]
fn death_freezes_personal_robot_and_recovers_exactly_once_after_load() {
    let mut sim = Simulation::new_test_world(123);
    let (_, robot_item) = install_personal_roboport(&mut sim);
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let build_item = sim.catalog().entity(furnace).unwrap().build_item.unwrap();
    set_inventory_slot(&mut sim.player_inventory, 21, build_item, 1);
    let material_before = sim.player_inventory.count(build_item);
    let ghost_id = place_personal_ghost(&mut sim, furnace, false);
    sim.tick();
    assert_eq!(sim.robot_count(), 1);
    let robot = sim.robots().next().unwrap().clone();
    let installed = sim.installed_equipment().to_vec();
    let armor = sim.equipped_armor();
    super::player_death::damage_player(&mut sim, &[u32::MAX]);
    assert_eq!(sim.personal_roboport_energy().0, 0);
    for _ in 0..30 {
        sim.tick();
        sim.validate().unwrap();
    }
    assert_eq!(sim.robot(robot.id), Some(&robot));
    assert_eq!(sim.installed_equipment(), installed);
    assert_eq!(sim.equipped_armor(), armor);
    let mut restored = load_from_bytes(&save_to_bytes(&sim).unwrap()).unwrap();
    assert_eq!(sim.state_hash(), restored.state_hash());
    // Cancelled work returns both the payload and robot; retained craft inputs
    // and a full inventory never require a lossy death-time refund.
    for simulation in [&mut sim, &mut restored] {
        simulation
            .apply_command(&SimCommand::RespawnPlayer)
            .unwrap();
        simulation.tick();
        simulation
            .apply_command(&SimCommand::CancelGhost { ghost_id })
            .unwrap();
        simulation.player_equipment.personal_roboport_energy_joules = 3_000_000;
    }
    for _ in 0..2000 {
        sim.tick();
        restored.profiled_tick();
        assert_eq!(sim.state_hash(), restored.state_hash());
        if sim.robot_count() == 0 {
            break;
        }
    }
    assert_eq!(sim.robot_count(), 0);
    assert_eq!(sim.player_inventory.count(robot_item), 1);
    assert_eq!(sim.player_inventory.count(build_item), material_before);
    sim.validate().unwrap();
}
