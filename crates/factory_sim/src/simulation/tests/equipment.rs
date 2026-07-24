use super::super::*;
use super::support::{item_id_by_name, set_inventory_slot};

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
