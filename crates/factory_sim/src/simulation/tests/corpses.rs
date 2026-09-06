use super::super::*;
use super::player_death::damage_player;
use super::support::*;

fn respawn(sim: &mut Simulation) {
    sim.apply_command(&SimCommand::RespawnPlayer).unwrap();
    sim.tick();
}

fn corpse_count(sim: &Simulation, id: u64, item: ItemId) -> u64 {
    sim.corpse(id).map_or(0, |corpse| {
        corpse
            .items()
            .iter()
            .filter(|amount| amount.item_id() == item)
            .map(|amount| amount.count())
            .sum()
    })
}

#[test]
fn full_inventory_and_partial_recovery_preserve_leftovers_across_save_load() {
    let mut sim = Simulation::new_test_world(123);
    let plate = item_id_by_name(sim.catalog(), "iron_plate");
    let stone = item_id_by_name(sim.catalog(), "stone");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, plate, 5);
    damage_player(&mut sim, &[u32::MAX]);
    let location = sim.corpse(1).unwrap().tile_position();
    assert_eq!(location, sim.player.tile_position());
    respawn(&mut sim);
    let stone_stack = sim.catalog().item(stone).unwrap().stack_size;
    for slot in 0..sim.player_inventory.slots().len() {
        set_inventory_slot(&mut sim.player_inventory, slot, stone, stone_stack);
    }
    let before = sim.state_hash();
    assert_eq!(sim.recover_corpse(1), Err(CorpseRecoveryError::NoCapacity));
    assert_eq!(before, sim.state_hash());
    let plate_stack = sim.catalog().item(plate).unwrap().stack_size;
    set_inventory_slot(&mut sim.player_inventory, 0, plate, plate_stack - 1);
    sim.recover_corpse(1).unwrap();
    assert_eq!(corpse_count(&sim, 1, plate), 4);
    let mut replay = load_from_bytes(&save_to_bytes(&sim).unwrap()).unwrap();
    for simulation in [&mut sim, &mut replay] {
        simulation
            .player_inventory
            .remove(plate, plate_stack)
            .unwrap();
        simulation
            .apply_command(&SimCommand::RecoverCorpse { corpse_id: 1 })
            .unwrap();
        assert_eq!(simulation.player_inventory.count(plate), 4);
        assert!(simulation.corpse(1).is_none());
        assert_eq!(
            simulation.recover_corpse(1),
            Err(CorpseRecoveryError::MissingCorpse)
        );
        simulation.validate().unwrap();
    }
    assert_eq!(sim.state_hash(), replay.state_hash());
}

#[test]
fn separate_corpses_survive_repeated_deaths_and_can_be_recovered_by_id() {
    let mut sim = Simulation::new_test_world(123);
    let plate = item_id_by_name(sim.catalog(), "iron_plate");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, plate, 5);
    damage_player(&mut sim, &[u32::MAX]);
    let first = sim.corpse(1).unwrap().clone();
    respawn(&mut sim);
    set_inventory_slot(&mut sim.player_inventory, 0, plate, 7);
    damage_player(&mut sim, &[u32::MAX]);
    assert_eq!(sim.corpse(1), Some(&first));
    assert_eq!(sim.corpses().count(), 2);
    let mut sim = load_from_bytes(&save_to_bytes(&sim).unwrap()).unwrap();
    respawn(&mut sim);
    sim.recover_corpse(2).unwrap();
    assert_eq!(sim.player_inventory.count(plate), 7);
    assert_eq!(sim.corpse(1), Some(&first));
    sim.recover_corpse(1).unwrap();
    assert_eq!(sim.player_inventory.count(plate), 12);
    assert_eq!(sim.corpses().count(), 0);
    sim.validate().unwrap();
}

#[test]
fn recovery_rejects_dead_or_distant_players_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    damage_player(&mut sim, &[u32::MAX]);
    let before = sim.state_hash();
    assert_eq!(sim.recover_corpse(1), Err(CorpseRecoveryError::PlayerDead));
    assert_eq!(
        sim.apply_command(&SimCommand::RecoverCorpse { corpse_id: 1 }),
        Err(SimCommandError::PlayerDead)
    );
    assert_eq!(before, sim.state_hash());
    respawn(&mut sim);
    sim.player.x += 50 * PLAYER_POSITION_SCALE;
    let before = sim.state_hash();
    assert_eq!(sim.recover_corpse(1), Err(CorpseRecoveryError::OutOfReach));
    assert_eq!(before, sim.state_hash());
}

#[test]
fn conflicting_opened_repair_pack_remains_in_corpse_until_player_uses_theirs() {
    let mut sim = Simulation::new_test_world(123);
    sim.player.repair_remaining_health = 17;
    damage_player(&mut sim, &[u32::MAX]);
    respawn(&mut sim);
    sim.player.repair_remaining_health = 20;
    sim.recover_corpse(1).unwrap();
    assert_eq!(sim.player.repair_remaining_health, 20);
    assert_eq!(sim.corpse(1).unwrap().remaining_repair_health(), 17);
    assert_eq!(sim.recover_corpse(1), Err(CorpseRecoveryError::NoCapacity));
    let mut sim = load_from_bytes(&save_to_bytes(&sim).unwrap()).unwrap();
    sim.player.repair_remaining_health = 0;
    sim.recover_corpse(1).unwrap();
    assert_eq!(sim.player.repair_remaining_health, 17);
    assert!(sim.corpse(1).is_none());
}

#[test]
fn corpse_identity_location_and_contents_are_hashed_and_validated() {
    let mut sim = Simulation::new_test_world(123);
    damage_player(&mut sim, &[u32::MAX]);
    let before = sim.state_hash();
    sim.corpses.get_mut(&1).unwrap().x += 1;
    assert_ne!(before, sim.state_hash());
    sim.corpses.get_mut(&1).unwrap().id = 2;
    assert_eq!(
        sim.validate(),
        Err(SimValidationError::InvalidPlayerCorpse { corpse_id: 1 })
    );
}
