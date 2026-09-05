use super::super::*;
use super::support::*;

pub(super) fn damage_player(sim: &mut Simulation, amounts: &[u32]) {
    let mut commands = CombatCommandBuffer::default();
    for &amount in amounts {
        commands.push(CombatCommand {
            source: CombatSource {
                owner: CombatantId::Enemy(EnemyId::new(u64::MAX)),
                faction: Faction::Enemy,
            },
            target: CombatantId::Player,
            damage: Damage::physical(amount),
        });
    }
    sim.resolve_combat_commands(commands);
}

#[test]
fn simultaneous_lethal_damage_transitions_once_and_rejects_commands() {
    let mut sim = Simulation::new_test_world(123);
    damage_player(&mut sim, &[40]);
    assert!(!sim.player.is_dead());
    damage_player(&mut sim, &[30, 30]);
    assert!(sim.player.is_dead());
    assert_eq!(sim.player_deaths(), 1);
    assert_eq!(sim.player.dead_since(), Some(0));
    let before = sim.state_hash();
    damage_player(&mut sim, &[u32::MAX, u32::MAX]);
    assert_eq!(sim.state_hash(), before);
    for command in [
        SimCommand::MovePlayer {
            direction_x: 1.0,
            direction_y: 0.0,
            delta_seconds: 1.0,
        },
        SimCommand::SetManualMiningTarget(None),
        SimCommand::CyclePlayerWeapon,
        SimCommand::AttackWithPlayerWeapon { x: 0, y: 0 },
        SimCommand::UnequipArmor,
        SimCommand::StartManualCraft(recipe_id(&sim.world.prototypes, "iron_gear_wheel")),
        SimCommand::BuildRedScienceResearchFixture,
    ] {
        assert_eq!(
            sim.apply_command(&command),
            Err(SimCommandError::PlayerDead)
        );
        assert_eq!(sim.state_hash(), before);
    }
    let position = sim.player.position_tiles();
    sim.move_player_by_tiles(1.0, 1.0);
    assert_eq!(sim.player.position_tiles(), position);
    sim.validate().unwrap();
}

#[test]
fn respawn_is_deferred_and_preserves_items_crafting_and_replay() {
    let mut sim = Simulation::new_test_world(123);
    let plate = item_id_by_name(sim.catalog(), "iron_plate");
    let gear = item_id_by_name(sim.catalog(), "iron_gear_wheel");
    set_inventory_slot(&mut sim.player_inventory, 10, plate, 2);
    sim.start_manual_craft(recipe_id(sim.catalog(), "iron_gear_wheel"))
        .unwrap();
    sim.player.repair_remaining_health = 17;
    damage_player(&mut sim, &[u32::MAX]);
    let inventory = sim.player_inventory.clone();
    let crafting = sim.crafting_queue.clone();
    for _ in 0..5 {
        sim.tick();
    }
    assert_eq!(sim.crafting_queue, crafting);
    assert_eq!(sim.player_inventory, inventory);
    let mut replay = load_from_bytes(&save_to_bytes(&sim).unwrap()).unwrap();
    assert_eq!(sim.state_hash(), replay.state_hash());
    for simulation in [&mut sim, &mut replay] {
        simulation
            .apply_command(&SimCommand::RespawnPlayer)
            .unwrap();
        simulation
            .apply_command(&SimCommand::RespawnPlayer)
            .unwrap();
        assert!(simulation.player.is_dead());
        assert_eq!(
            simulation.apply_command(&SimCommand::CyclePlayerWeapon),
            Err(SimCommandError::PlayerDead)
        );
    }
    replay = load_from_bytes(&save_to_bytes(&replay).unwrap()).unwrap();
    for _ in 0..40 {
        sim.tick();
        replay.profiled_tick();
        assert_eq!(sim.state_hash(), replay.state_hash());
    }
    assert!(!sim.player.is_dead());
    assert_eq!(sim.player_health(), (PLAYER_MAX_HEALTH, PLAYER_MAX_HEALTH));
    assert_eq!(sim.player.repair_remaining_health, 17);
    assert_eq!(sim.player_inventory.count(gear), 1);
    assert_eq!(sim.player_deaths(), 1);
    let restored = load_from_bytes(&save_to_bytes(&sim).unwrap()).unwrap();
    assert_eq!(sim.state_hash(), restored.state_hash());
    assert_eq!(
        sim.apply_command(&SimCommand::RespawnPlayer),
        Err(SimCommandError::PlayerAlive)
    );
    sim.validate().unwrap();
}

#[test]
fn respawn_avoids_occupied_start_and_invalid_death_state_is_rejected() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = sim.player.tile_position();
    sim.player.x += PLAYER_POSITION_SCALE;
    let chest = entity_id_by_name(sim.catalog(), "chest");
    place_at(&mut sim, chest, x, y, Direction::North);
    damage_player(&mut sim, &[u32::MAX]);
    sim.apply_command(&SimCommand::RespawnPlayer).unwrap();
    sim.tick();
    assert_ne!(sim.player.tile_position(), (x, y));
    let (x, y) = sim.player.tile_position();
    assert!(sim.can_player_occupy_tile(x, y));
    sim.validate().unwrap();
    sim.player.dead_since = Some(sim.tick);
    assert_eq!(sim.validate(), Err(SimValidationError::InvalidPlayerState));
}

#[test]
fn death_counter_is_hashed_and_repeated_lives_count_once_each() {
    let mut sim = Simulation::new_test_world(123);
    for deaths in 1..=3 {
        damage_player(&mut sim, &[u32::MAX]);
        assert_eq!(sim.player_deaths(), deaths);
        sim.apply_command(&SimCommand::RespawnPlayer).unwrap();
        sim.tick();
    }
    let before = sim.state_hash();
    sim.statistics.player_deaths += 1;
    assert_ne!(before, sim.state_hash());
}

#[test]
fn respawn_waits_when_no_generated_tile_is_walkable() {
    let mut sim = Simulation::new_test_world(123);
    let base = factory_data::BasePrototypeIds::from_catalog(sim.catalog());
    for (x, y) in all_tile_coords(&sim.world) {
        if sim.world.tile_at(x, y).unwrap().tile_id != base.tiles.water {
            sim.set_tile(x, y, base.tiles.water).unwrap();
        }
    }
    damage_player(&mut sim, &[u32::MAX]);
    sim.apply_command(&SimCommand::RespawnPlayer).unwrap();
    let before = sim.state_hash();
    sim.advance_player_respawn();
    assert!(sim.player.is_dead());
    assert_eq!(sim.state_hash(), before);
    let mut restored = load_from_bytes(&save_to_bytes(&sim).unwrap()).unwrap();
    for simulation in [&mut sim, &mut restored] {
        simulation.set_tile(2, 3, base.tiles.grass).unwrap();
        simulation.advance_player_respawn();
        assert_eq!(simulation.player.tile_position(), (2, 3));
        simulation.validate().unwrap();
    }
    assert_eq!(sim.state_hash(), restored.state_hash());
}
