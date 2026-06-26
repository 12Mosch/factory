use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use factory_app::{
    FactoryAppPlugin, InventoryPanel, SimResource, opened_container_after_world_click,
    transfer_open_container_slot, world_position_to_tile_coord,
};
use factory_data::{EntityPrototypeId, ItemId, PrototypeCatalog};
use factory_sim::{CHUNK_SIZE, Direction, EntityFootprint, Inventory, ItemStack, Simulation};
use std::time::Duration;

const TARGET_TICKS: u64 = 3_600;

#[test]
fn fixed_update_hash_matches_at_60_and_144_fps() {
    let at_60_fps = run_to_tick_with_frame_rate(60.0, TARGET_TICKS);
    let at_144_fps = run_to_tick_with_frame_rate(144.0, TARGET_TICKS);

    assert_eq!(at_60_fps.0, TARGET_TICKS);
    assert_eq!(at_144_fps.0, TARGET_TICKS);
    assert_eq!(at_60_fps.1, at_144_fps.1);
}

#[test]
fn zero_duration_render_pause_does_not_advance_or_corrupt_sim() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    run_until_tick(&mut app, 120);

    let before_pause = sim_tick_and_hash(&app);
    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));

    for _ in 0..240 {
        app.update();
    }

    assert_eq!(sim_tick_and_hash(&app), before_pause);

    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    run_until_tick(&mut app, TARGET_TICKS);

    let mut expected = Simulation::new_test_world(123);
    for _ in 0..TARGET_TICKS {
        expected.tick();
    }

    assert_eq!(
        sim_tick_and_hash(&app),
        (TARGET_TICKS, expected.state_hash())
    );
}

#[test]
fn world_position_to_tile_coord_floors_negative_coordinates() {
    assert_eq!(world_position_to_tile_coord(Vec2::new(0.0, 0.0)), (0, 0));
    assert_eq!(world_position_to_tile_coord(Vec2::new(7.99, 7.99)), (0, 0));
    assert_eq!(world_position_to_tile_coord(Vec2::new(8.0, 8.0)), (1, 1));
    assert_eq!(
        world_position_to_tile_coord(Vec2::new(-0.01, -0.01)),
        (-1, -1)
    );
    assert_eq!(
        world_position_to_tile_coord(Vec2::new(-8.0, -8.0)),
        (-1, -1)
    );
    assert_eq!(
        world_position_to_tile_coord(Vec2::new(-8.01, -8.01)),
        (-2, -2)
    );
}

#[test]
fn input_movement_changes_player_position_under_fixed_ticks() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    let before = app.world().resource::<SimResource>().sim.player;
    let before_tick = app.world().resource::<SimResource>().sim.tick_count();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyD);
    run_until_tick(&mut app, before_tick + 1);

    let after = app.world().resource::<SimResource>().sim.player;
    assert!(after.x_fixed() > before.x_fixed());
    assert_eq!(after.y_fixed(), before.y_fixed());
}

#[test]
fn debug_inventory_keys_insert_and_remove_selected_item() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    let selected_item = app
        .world()
        .resource::<SimResource>()
        .sim
        .world
        .prototypes
        .items[0]
        .id;
    let before = app
        .world()
        .resource::<SimResource>()
        .sim
        .player_inventory
        .count(selected_item);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyI);
    app.update();

    let after_insert = app
        .world()
        .resource::<SimResource>()
        .sim
        .player_inventory
        .count(selected_item);
    assert_eq!(after_insert, before + 1);

    {
        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.release(KeyCode::KeyI);
        keyboard.clear();
        keyboard.press(KeyCode::KeyO);
    }
    app.update();

    let after_remove = app
        .world()
        .resource::<SimResource>()
        .sim
        .player_inventory
        .count(selected_item);
    assert_eq!(after_remove, before);
}

#[test]
fn opening_clicked_chest_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let (x, y) = first_buildable_rect(&sim, chest);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn opening_clicked_burner_drill_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let coal = item_id_by_name(&sim.world.prototypes, "coal");
    let (x, y) = first_placeable_resource_rect(&sim, drill, coal);
    let entity_id = sim
        .place_entity(drill, x, y, Direction::North)
        .expect("burner drill should be placeable over resources");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn slot_click_transfer_delegates_to_sim_transfer_api() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let iron_plate = item_id_by_name(&sim.world.prototypes, "iron_plate");
    let (x, y) = first_buildable_rect(&sim, chest);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[2] = Some(ItemStack {
        item_id: iron_plate,
        count: 9,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("slot click should transfer stack to chest");

    assert_eq!(sim.player_inventory.slots[2], None);
    assert_eq!(
        sim.entity_inventory(entity_id)
            .expect("chest should have inventory")
            .count(iron_plate),
        9
    );
}

#[test]
fn slot_click_transfer_handles_burner_drill_fuel_and_output() {
    let mut sim = Simulation::new_test_world(123);
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let coal = item_id_by_name(&sim.world.prototypes, "coal");
    let (x, y) = first_placeable_resource_rect(&sim, drill, coal);
    let entity_id = sim
        .place_entity(drill, x, y, Direction::North)
        .expect("burner drill should be placeable over resources");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[2] = Some(ItemStack {
        item_id: coal,
        count: 1,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("player coal should transfer to burner drill fuel");

    assert_eq!(sim.player_inventory.slots[2], None);
    assert_eq!(
        sim.burner_drill_state(entity_id)
            .expect("burner drill should expose state")
            .energy
            .fuel_slot,
        Some(ItemStack {
            item_id: coal,
            count: 1,
        })
    );

    for _ in 0..240 {
        sim.tick();
    }

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::BurnerOutput, 0)
        .expect("drill output should transfer to player");

    assert_eq!(sim.player_inventory.count(coal), 1);
    assert_eq!(
        sim.burner_drill_state(entity_id)
            .expect("burner drill should expose state")
            .output_slot,
        None
    );
}

fn run_to_tick_with_frame_rate(frame_rate: f64, target_tick: u64) -> (u64, u64) {
    let mut app = test_app(Duration::from_secs_f64(1.0 / frame_rate));
    run_until_tick(&mut app, target_tick);
    sim_tick_and_hash(&app)
}

fn test_app(frame_duration: Duration) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FactoryAppPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame_duration));
    app
}

fn run_until_tick(app: &mut App, target_tick: u64) {
    while app.world().resource::<SimResource>().sim.tick_count() < target_tick {
        app.update();
    }
}

fn sim_tick_and_hash(app: &App) -> (u64, u64) {
    let sim = &app.world().resource::<SimResource>().sim;
    (sim.tick_count(), sim.state_hash())
}

fn first_buildable_rect(sim: &Simulation, prototype_id: EntityPrototypeId) -> (i32, i32) {
    let prototype = &sim.world.prototypes.entities[prototype_id.index()];

    for chunk in sim.world.chunks.values() {
        for (index, _) in chunk.tiles.iter().enumerate() {
            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let x = chunk.coord.x * CHUNK_SIZE + local_x;
            let y = chunk.coord.y * CHUNK_SIZE + local_y;
            let footprint = EntityFootprint {
                x,
                y,
                width: prototype.size.x,
                height: prototype.size.y,
            };

            if sim.world.validate_entity_footprint(&footprint).is_ok()
                && sim
                    .entities
                    .occupancy()
                    .validate_available(&footprint, None)
                    .is_ok()
            {
                return (x, y);
            }
        }
    }

    panic!("expected at least one buildable area");
}

fn first_placeable_resource_rect(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    resource_item: ItemId,
) -> (i32, i32) {
    for chunk in sim.world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            let Some(resource) = tile.resource else {
                continue;
            };
            if resource.resource_item != resource_item {
                continue;
            }

            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let x = chunk.coord.x * CHUNK_SIZE + local_x;
            let y = chunk.coord.y * CHUNK_SIZE + local_y;

            if sim
                .can_place_entity(prototype_id, x, y, Direction::North)
                .is_ok()
            {
                return (x, y);
            }
        }
    }

    panic!("expected at least one placeable resource area");
}

fn entity_id_by_name(catalog: &PrototypeCatalog, name: &str) -> EntityPrototypeId {
    catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required entity prototype {name:?}"))
}

fn item_id_by_name(catalog: &PrototypeCatalog, name: &str) -> ItemId {
    catalog
        .items
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required item prototype {name:?}"))
}
