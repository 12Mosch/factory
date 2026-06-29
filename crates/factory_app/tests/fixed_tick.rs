use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use factory_app::FactoryAppPlugin;
use factory_app::interaction::container_open::{
    container_open_input_allowed, opened_container_after_world_click,
};
use factory_app::interaction::cursor::world_position_to_tile_coord;
use factory_app::interaction::slot_transfer::transfer_open_container_slot;
use factory_app::placement::build::{
    buildable_prototype_at_slot, buildable_prototypes, place_selected_building_at_tile,
};
use factory_app::rendering::map_texture::{
    GRID_PIXEL, PLAYER_PIXEL, UNREVEALED_PIXEL, generate_map_pixels,
};
use factory_app::resources::{
    BuildPlacementState, BuildPlacementStatus, BuildSelection, CraftingWindowState,
    MapDisplaySettings, MapViewState, ProductionStatsWindowState, RenderSyncStats, SimProfileStats,
    SimResource, TechnologyWindowState,
};
use factory_app::ui::debug_overlay::{DebugOverlaySnapshot, format_debug_overlay};
use factory_app::ui::formatting::{
    available_crafting_recipe_choices, crafting_recipe_choices, format_assembler_detail_text,
};
use factory_app::ui::inventory_panel::InventoryPanel;
use factory_app::ui::production_stats::{power_summary_lines, production_rows};
use factory_app::ui::technology_panel::TechnologyStartQueueButton;
use factory_data::{CraftingCategory, EntityKind, EntityPrototypeId, ItemId, PrototypeCatalog};
use factory_sim::{
    CHUNK_SIZE, Direction, EntityFootprint, Inventory, ItemStack, PowerSummary, Simulation,
    SimulationCounts, SimulationTickProfile,
};
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
    let before = app.world().resource::<SimResource>().sim.player();
    let before_tick = app.world().resource::<SimResource>().sim.tick_count();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyD);
    run_until_tick(&mut app, before_tick + 1);

    let after = app.world().resource::<SimResource>().sim.player();
    assert!(after.x_fixed() > before.x_fixed());
    assert_eq!(after.y_fixed(), before.y_fixed());
}

#[test]
fn buildable_prototypes_include_placeable_item_backed_entities() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let buildables = buildable_prototypes(&catalog);
    let buildable_names = buildables
        .iter()
        .map(|buildable| {
            catalog.entities[buildable.prototype_id.index()]
                .name
                .as_str()
        })
        .collect::<Vec<_>>();

    for expected in [
        "chest",
        "transport_belt",
        "fast_transport_belt",
        "express_transport_belt",
        "splitter",
        "fast_splitter",
        "express_splitter",
        "inserter",
        "stone_furnace",
        "burner_mining_drill",
        "assembling_machine",
        "lab",
        "underground_belt_entrance",
        "underground_belt_exit",
        "fast_underground_belt_entrance",
        "fast_underground_belt_exit",
        "express_underground_belt_entrance",
        "express_underground_belt_exit",
        "pipe",
        "storage_tank",
    ] {
        assert!(
            buildable_names.contains(&expected),
            "missing buildable prototype {expected}"
        );
    }
    assert!(buildables.iter().all(|buildable| {
        let entity = &catalog.entities[buildable.prototype_id.index()];
        entity.entity_kind != EntityKind::ResourcePatch
            && entity.build_item == Some(buildable.item_id)
    }));
}

#[test]
fn number_key_selects_hotbar_slot_without_placing() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let before_entities = app
        .world()
        .resource::<SimResource>()
        .sim
        .entities()
        .placed_len();
    let slot = {
        let sim = &app.world().resource::<SimResource>().sim;
        buildable_prototypes(sim.catalog())
            .into_iter()
            .find(|buildable| sim.player_inventory().count(buildable.item_id) > 0)
            .expect("starting inventory should include at least one buildable item")
    };

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot.slot_index));
    app.update();

    let build_state = app.world().resource::<BuildPlacementState>();
    assert_eq!(
        build_state.selected,
        Some(BuildSelection {
            prototype_id: slot.prototype_id,
            item_id: slot.item_id,
        })
    );
    assert_eq!(
        app.world()
            .resource::<SimResource>()
            .sim
            .entities()
            .placed_len(),
        before_entities
    );
}

#[test]
fn rotate_key_updates_build_direction() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let selection = first_available_build_selection(&app);
    app.world_mut()
        .resource_mut::<BuildPlacementState>()
        .selected = Some(selection);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyR);
    app.update();

    assert_eq!(
        app.world().resource::<BuildPlacementState>().direction,
        Direction::East
    );
}

#[test]
fn escape_clears_build_selection() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let selection = first_available_build_selection(&app);
    app.world_mut()
        .resource_mut::<BuildPlacementState>()
        .selected = Some(selection);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    app.update();

    assert_eq!(app.world().resource::<BuildPlacementState>().selected, None);
}

#[test]
fn technology_screen_toggles_with_t() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyT);
    app.update();

    assert!(app.world().resource::<TechnologyWindowState>().open);
}

#[test]
fn map_screen_toggles_with_m() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyM);
    app.update();

    assert!(app.world().resource::<MapViewState>().open);
}

#[test]
fn production_stats_toggles_with_p() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyP);
    app.update();

    assert!(app.world().resource::<ProductionStatsWindowState>().open);
}

#[test]
fn crafting_screen_toggles_with_c() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyC);
    app.update();

    assert!(app.world().resource::<CraftingWindowState>().open);

    {
        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.clear_just_pressed(KeyCode::KeyC);
        keyboard.release(KeyCode::KeyC);
    }
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyC);
    app.update();

    assert!(!app.world().resource::<CraftingWindowState>().open);
}

#[test]
fn f3_toggles_map_debug_flags() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::F3);
    app.update();

    let settings = app.world().resource::<MapDisplaySettings>();
    assert!(settings.debug_reveal_all);
    assert!(settings.show_chunk_grid);
}

#[test]
fn open_map_suppresses_build_hotbar_selection() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let slot = {
        let sim = &app.world().resource::<SimResource>().sim;
        buildable_prototypes(sim.catalog())
            .into_iter()
            .find(|buildable| sim.player_inventory().count(buildable.item_id) > 0)
            .expect("starting inventory should include at least one buildable item")
    };

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyM);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::KeyM);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot.slot_index));
    app.update();

    assert_eq!(app.world().resource::<BuildPlacementState>().selected, None);
}

#[test]
fn open_crafting_suppresses_build_hotbar_selection() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let slot = {
        let sim = &app.world().resource::<SimResource>().sim;
        buildable_prototypes(sim.catalog())
            .into_iter()
            .find(|buildable| sim.player_inventory().count(buildable.item_id) > 0)
            .expect("starting inventory should include at least one buildable item")
    };

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyC);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::KeyC);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot.slot_index));
    app.update();

    assert_eq!(app.world().resource::<BuildPlacementState>().selected, None);
}

#[test]
fn technology_screen_start_button_updates_research_state() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    let logistics = {
        let sim = &app.world().resource::<SimResource>().sim;
        technology_id_by_name(sim.catalog(), "logistics")
    };
    {
        let mut window = app.world_mut().resource_mut::<TechnologyWindowState>();
        window.open = true;
        window.selected = Some(logistics);
    }
    app.update();

    let mut query = app
        .world_mut()
        .query_filtered::<&mut Interaction, With<TechnologyStartQueueButton>>();
    for mut interaction in query.iter_mut(app.world_mut()) {
        *interaction = Interaction::Pressed;
    }
    app.update();

    assert_eq!(
        app.world().resource::<SimResource>().sim.active_research(),
        Some(logistics)
    );
}

#[test]
fn build_bar_rejects_locked_buildable_and_allows_after_research() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let (assembler_entity, assembler_item, automation, slot_index) = {
        let sim = &app.world().resource::<SimResource>().sim;
        let assembler_entity = entity_id_by_name(sim.catalog(), "assembling_machine");
        let assembler_item = item_id_by_name(sim.catalog(), "assembling_machine");
        let automation = technology_id_by_name(sim.catalog(), "automation");
        let slot_index = buildable_prototypes(sim.catalog())
            .into_iter()
            .find(|buildable| buildable.prototype_id == assembler_entity)
            .expect("assembling machine should be buildable")
            .slot_index;
        (assembler_entity, assembler_item, automation, slot_index)
    };
    {
        let catalog = app.world().resource::<SimResource>().sim.catalog().clone();
        app.world_mut()
            .resource_mut::<SimResource>()
            .sim
            .player_inventory_mut()
            .insert(&catalog, assembler_item, 1)
            .expect("test inventory should accept assembler");
    }

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot_index));
    app.update();

    let build_state = app.world().resource::<BuildPlacementState>();
    assert_eq!(build_state.selected, None);
    assert!(matches!(
        build_state.last_status,
        BuildPlacementStatus::Locked(_)
    ));

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(hotbar_key_for_slot(slot_index));
    app.update();
    {
        let mut sim = app.world_mut().resource_mut::<SimResource>();
        complete_research_by_name(&mut sim.sim, "logistics");
        sim.sim
            .select_research(automation)
            .expect("automation should be selectable");
        sim.sim
            .add_research_units(20)
            .expect("automation should complete");
    }
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot_index));
    app.update();

    assert_eq!(
        app.world().resource::<BuildPlacementState>().selected,
        Some(BuildSelection {
            prototype_id: assembler_entity,
            item_id: assembler_item,
        })
    );
}

#[test]
fn place_selected_building_consumes_inventory() {
    let mut sim = Simulation::new_test_world(123);
    let belt = entity_id_by_name(sim.catalog(), "transport_belt");
    let belt_item = item_id_by_name(sim.catalog(), "transport_belt");
    let (x, y) = first_buildable_rect(&sim, belt);
    *sim.player_inventory_mut() = Inventory::player();
    let catalog = sim.catalog().clone();
    sim.player_inventory_mut()
        .insert(&catalog, belt_item, 1)
        .expect("test inventory should accept belt");
    let before_entities = sim.entities().placed_len();

    let status = place_selected_building_at_tile(
        &mut sim,
        BuildSelection {
            prototype_id: belt,
            item_id: belt_item,
        },
        Direction::North,
        x,
        y,
    );

    assert!(matches!(status, BuildPlacementStatus::Placed(_)));
    assert_eq!(sim.player_inventory().count(belt_item), 0);
    assert_eq!(sim.entities().placed_len(), before_entities + 1);
}

#[test]
fn failed_selected_building_placement_keeps_inventory() {
    let mut sim = Simulation::new_test_world(123);
    let belt = entity_id_by_name(sim.catalog(), "transport_belt");
    let belt_item = item_id_by_name(sim.catalog(), "transport_belt");
    let (x, y) = first_buildable_rect(&sim, belt);
    *sim.player_inventory_mut() = Inventory::player();
    let catalog = sim.catalog().clone();
    sim.player_inventory_mut()
        .insert(&catalog, belt_item, 1)
        .expect("test inventory should accept belt");
    sim.place_entity(belt, x, y, Direction::North)
        .expect("blocking belt should be placeable");

    let status = place_selected_building_at_tile(
        &mut sim,
        BuildSelection {
            prototype_id: belt,
            item_id: belt_item,
        },
        Direction::North,
        x,
        y,
    );

    assert!(matches!(status, BuildPlacementStatus::CannotPlace(_)));
    assert_eq!(sim.player_inventory().count(belt_item), 1);
}

#[test]
fn debug_overlay_format_no_longer_mentions_debug_item_selection() {
    let sim_profile = SimProfileStats {
        last_tick: SimulationTickProfile {
            belts: Duration::from_micros(100),
            machines: Duration::from_micros(200),
            inserters: Duration::from_micros(300),
            inventory_transfers: Duration::from_micros(400),
            chunk_lookup: Duration::from_micros(500),
            ..default()
        },
        rolling_average_sim_tick_ms: 1.25,
    };
    let render_sync = RenderSyncStats {
        total: Duration::from_micros(600),
        ..default()
    };
    let text = format_debug_overlay(DebugOverlaySnapshot {
        tick: 7,
        ups: 60.0,
        fps: Some(59.9),
        frame_ms: Some(16.667),
        sim_profile: &sim_profile,
        render_sync: &render_sync,
        counts: SimulationCounts {
            entity_count: 10,
            chunk_count: 25,
            belt_count: 3,
            belt_item_count: 4,
            machine_count: 5,
            inserter_count: 6,
            active_machines: 2,
            idle_machines: 3,
        },
        power: PowerSummary {
            production_watts: 0,
            available_production_watts: 0,
            consumption_watts: 0,
            satisfaction_permyriad: 10_000,
            network_count: 0,
        },
    });

    for label in [
        "UPS:",
        "FPS:",
        "Sim tick:",
        "Entities:",
        "Power:",
        "render sync",
    ] {
        assert!(text.contains(label), "missing debug overlay label {label}");
    }
    assert!(!text.contains("Item:"));
    assert!(!text.contains("Count:"));
}

#[test]
fn production_stat_formatting_shows_per_minute_and_totals() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y, resource) = first_resource_tile_for_app(&sim);
    sim.move_player_by_tiles(
        x as f32 - sim.player().position_tiles().0,
        y as f32 - sim.player().position_tiles().1,
    );
    for _ in 0..factory_sim::MANUAL_MINING_TICKS_PER_ITEM {
        sim.update_manual_mining(Some(factory_sim::ManualMiningTarget { x, y }));
    }

    let rows = production_rows(&sim);
    let row = rows
        .iter()
        .find(|row| row.item_name == format_item_name_for_test(&sim, resource.resource_item))
        .expect("mined resource should appear in production stats");

    assert_eq!(row.per_minute, "1/min");
    assert_eq!(row.total, "1");
}

#[test]
fn power_stat_formatting_uses_summary_and_network_rows() {
    let summary = PowerSummary {
        production_watts: 500,
        available_production_watts: 1_000,
        consumption_watts: 500,
        satisfaction_permyriad: 10_000,
        network_count: 1,
    };
    let networks = [factory_sim::PowerNetworkSnapshot {
        network_id: 7,
        pole_count: 2,
        producer_count: 1,
        consumer_count: 3,
        production_watts: 500,
        available_production_watts: 1_000,
        consumption_watts: 500,
        satisfaction_permyriad: 10_000,
    }];

    let lines = power_summary_lines(summary, &networks);

    assert!(lines.iter().any(|line| line == "Production: 500 W"));
    assert!(lines.iter().any(|line| line.contains("Network 7")));
    assert!(lines.iter().any(|line| line.contains("poles 2")));
}

#[test]
fn map_pixel_generation_draws_reveal_player_and_debug_grid() {
    let sim = Simulation::new_test_world(123);
    let normal = generate_map_pixels(&sim, &MapDisplaySettings::default());
    let player_tile = sim.player().tile_position();

    assert_eq!(pixel_at(&normal, player_tile), PLAYER_PIXEL);

    let unrevealed_chunk = sim
        .world()
        .chunks
        .keys()
        .copied()
        .find(|coord| !sim.is_chunk_revealed(*coord))
        .expect("initial chart should leave distant chunks unrevealed");
    assert_eq!(
        pixel_at(
            &normal,
            (
                unrevealed_chunk.x * CHUNK_SIZE + 1,
                unrevealed_chunk.y * CHUNK_SIZE + 1
            )
        ),
        UNREVEALED_PIXEL
    );

    let debug = generate_map_pixels(
        &sim,
        &MapDisplaySettings {
            debug_reveal_all: true,
            show_chunk_grid: true,
        },
    );
    assert_eq!(pixel_at(&debug, (0, 0)), GRID_PIXEL);
}

#[test]
fn container_open_ignores_click_when_building_selected() {
    let mut build_state = BuildPlacementState::default();
    assert!(container_open_input_allowed(&build_state));

    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let slot = buildable_prototype_at_slot(&catalog, 0).expect("slot 0 should be buildable");
    build_state.selected = Some(BuildSelection {
        prototype_id: slot.prototype_id,
        item_id: slot.item_id,
    });

    assert!(!container_open_input_allowed(&build_state));
}

#[test]
fn opening_clicked_chest_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(sim.catalog(), "chest");
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
    let drill = entity_id_by_name(sim.catalog(), "burner_mining_drill");
    let coal = item_id_by_name(sim.catalog(), "coal");
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
fn opening_clicked_furnace_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let (x, y) = first_buildable_rect(&sim, furnace);
    let entity_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn opening_clicked_assembler_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(sim.catalog(), "assembling_machine");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let entity_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn opening_clicked_lab_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let lab = entity_id_by_name(sim.catalog(), "lab");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let entity_id = sim
        .place_entity(lab, x, y, Direction::North)
        .expect("lab should be placeable");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn assembler_recipe_choices_are_all_and_only_crafting_recipes() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let choices = crafting_recipe_choices(&catalog);
    let expected_count = catalog
        .recipes
        .iter()
        .filter(|recipe| recipe.category == CraftingCategory::Crafting)
        .count();

    assert_eq!(choices.len(), expected_count);
    assert!(
        choices
            .iter()
            .all(|recipe| recipe.category == CraftingCategory::Crafting)
    );
    assert!(
        catalog
            .recipes
            .iter()
            .filter(|recipe| recipe.category != CraftingCategory::Crafting)
            .all(|recipe| !choices.iter().any(|choice| choice.id == recipe.id))
    );
}

#[test]
fn available_crafting_recipe_choices_follow_research_unlocks() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id_by_name(sim.catalog(), "automation");
    let assembling_machine = recipe_id_by_name(sim.catalog(), "assembling_machine");

    let initial_choices = available_crafting_recipe_choices(&sim);
    assert!(
        !initial_choices
            .iter()
            .any(|recipe| recipe.id == assembling_machine)
    );

    complete_research_by_name(&mut sim, "logistics");
    sim.select_research(automation)
        .expect("automation should be selectable");
    sim.add_research_units(20)
        .expect("automation research should complete");

    let unlocked_choices = available_crafting_recipe_choices(&sim);
    assert!(
        unlocked_choices
            .iter()
            .any(|recipe| recipe.id == assembling_machine)
    );
}

#[test]
fn available_crafting_recipe_choices_include_express_only_after_logistics_3() {
    let mut sim = Simulation::new_test_world(123);
    let express_recipes = [
        recipe_id_by_name(sim.catalog(), "express_transport_belt"),
        recipe_id_by_name(sim.catalog(), "express_underground_belt"),
        recipe_id_by_name(sim.catalog(), "express_splitter"),
    ];

    for recipe_id in express_recipes {
        assert!(
            !available_crafting_recipe_choices(&sim)
                .iter()
                .any(|recipe| recipe.id == recipe_id)
        );
    }

    complete_research_by_name(&mut sim, "logistics");
    complete_research_by_name(&mut sim, "automation");
    complete_research_by_name(&mut sim, "electric_power");
    complete_research_by_name(&mut sim, "logistic_science_pack");
    complete_research_by_name(&mut sim, "logistics_2");

    for recipe_id in express_recipes {
        assert!(
            !available_crafting_recipe_choices(&sim)
                .iter()
                .any(|recipe| recipe.id == recipe_id)
        );
    }

    complete_research_by_name(&mut sim, "fluid_handling");
    complete_research_by_name(&mut sim, "logistics_3");

    for recipe_id in express_recipes {
        assert!(
            available_crafting_recipe_choices(&sim)
                .iter()
                .any(|recipe| recipe.id == recipe_id)
        );
    }
}

#[test]
fn completed_research_unlocks_recipe() {
    let mut sim = Simulation::new_test_world(123);
    let lab = entity_id_by_name(sim.catalog(), "lab");
    let automation = technology_id_by_name(sim.catalog(), "automation");
    let science_pack = item_id_by_name(sim.catalog(), "automation_science_pack");
    let assembling_machine = recipe_id_by_name(sim.catalog(), "assembling_machine");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let lab_id = sim
        .place_entity(lab, x, y, Direction::North)
        .expect("lab should be placeable");
    complete_research_by_name(&mut sim, "logistics");
    sim.select_research(automation)
        .expect("automation should be selectable");
    sim.entity_inventory_mut(lab_id)
        .expect("lab should expose inventory")
        .slots[0] = Some(ItemStack {
        item_id: science_pack,
        count: 20,
    });

    assert!(
        !available_crafting_recipe_choices(&sim)
            .iter()
            .any(|recipe| recipe.id == assembling_machine)
    );

    for _ in 0..12_000 {
        sim.tick();
    }

    assert!(
        available_crafting_recipe_choices(&sim)
            .iter()
            .any(|recipe| recipe.id == assembling_machine)
    );
}

#[test]
fn locked_assembler_recipe_buttons_are_unavailable_without_error() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(sim.catalog(), "assembling_machine");
    let recipe = recipe_id_by_name(sim.catalog(), "assembling_machine");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let entity_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");

    assert_eq!(
        sim.can_select_assembler_recipe(entity_id, recipe),
        Ok(false)
    );
}

#[test]
fn assembler_detail_formatting_reports_partial_ingredients() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(sim.catalog(), "assembling_machine");
    let recipe = recipe_id_by_name(sim.catalog(), "iron_gear_wheel");
    let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");
    let (x, y) = first_buildable_rect(&sim, assembler);
    let entity_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    sim.select_assembler_recipe(entity_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });
    sim.transfer_player_slot_to_assembler_input(entity_id, 2)
        .expect("partial ingredients should transfer to assembler input");

    let details =
        format_assembler_detail_text(&sim, entity_id).expect("assembler details should format");

    assert_eq!(details.recipe, "Recipe: Iron Gear Wheel");
    assert_eq!(
        details.ingredients,
        "Ingredients:\nIron Plate: need 2, have 1, missing 1"
    );
    assert_eq!(details.products, "Output: Iron Gear Wheel x1");
    assert_eq!(details.progress, "Progress: 0/60");
}

#[test]
fn slot_click_transfer_delegates_to_sim_transfer_api() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(sim.catalog(), "chest");
    let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");
    let (x, y) = first_buildable_rect(&sim, chest);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: iron_plate,
        count: 9,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("slot click should transfer stack to chest");

    assert_eq!(sim.player_inventory_mut().slots[2], None);
    assert_eq!(
        sim.entity_inventory(entity_id)
            .expect("chest should have inventory")
            .count(iron_plate),
        9
    );
}

#[test]
fn slot_click_transfer_routes_science_to_lab_inventory() {
    let mut sim = Simulation::new_test_world(123);
    let lab = entity_id_by_name(sim.catalog(), "lab");
    let science_pack = item_id_by_name(sim.catalog(), "automation_science_pack");
    let (x, y) = first_buildable_rect(&sim, lab);
    let entity_id = sim
        .place_entity(lab, x, y, Direction::North)
        .expect("lab should be placeable");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: science_pack,
        count: 3,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("slot click should transfer science packs to lab");

    assert_eq!(sim.player_inventory_mut().slots[2], None);
    assert_eq!(
        sim.entity_inventory(entity_id)
            .expect("lab should expose inventory")
            .count(science_pack),
        3
    );
}

#[test]
fn slot_click_transfer_routes_furnace_input_fuel_and_output() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let iron_ore = item_id_by_name(sim.catalog(), "iron_ore");
    let coal = item_id_by_name(sim.catalog(), "coal");
    let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");
    let (x, y) = first_buildable_rect(&sim, furnace);
    let entity_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });
    sim.player_inventory_mut().slots[3] = Some(ItemStack {
        item_id: coal,
        count: 1,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("player ore should transfer to furnace input");
    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 3)
        .expect("player coal should transfer to furnace fuel");

    assert_eq!(sim.player_inventory_mut().slots[2], None);
    assert_eq!(sim.player_inventory_mut().slots[3], None);
    assert_eq!(
        sim.furnace_state(entity_id)
            .expect("furnace should expose state")
            .input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        })
    );
    assert_eq!(
        sim.furnace_state(entity_id)
            .expect("furnace should expose state")
            .energy
            .fuel_slot,
        Some(ItemStack {
            item_id: coal,
            count: 1,
        })
    );

    for _ in 0..210 {
        sim.tick();
    }

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::FurnaceOutput, 0)
        .expect("furnace output should transfer to player");

    assert_eq!(sim.player_inventory().count(iron_plate), 1);
    assert_eq!(
        sim.furnace_state(entity_id)
            .expect("furnace should expose state")
            .output_slot,
        None
    );
}

#[test]
fn slot_click_transfer_routes_assembler_input_and_output() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(sim.catalog(), "assembling_machine");
    let recipe = recipe_id_by_name(sim.catalog(), "iron_gear_wheel");
    let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");
    let iron_gear_wheel = item_id_by_name(sim.catalog(), "iron_gear_wheel");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let entity_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    sim.select_assembler_recipe(entity_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("player ingredients should transfer to assembler input");

    assert_eq!(sim.player_inventory_mut().slots[2], None);
    assert_eq!(
        sim.assembler_state(entity_id)
            .expect("assembler should expose state")
            .input_inventory
            .count(iron_plate),
        2
    );

    for _ in 0..60 {
        sim.tick();
    }

    transfer_open_container_slot(
        &mut sim,
        Some(entity_id),
        InventoryPanel::AssemblerOutput,
        0,
    )
    .expect("assembler output should transfer to player");

    assert_eq!(sim.player_inventory().count(iron_gear_wheel), 1);
    assert_eq!(
        sim.assembler_state(entity_id)
            .expect("assembler should expose state")
            .output_inventory
            .count(iron_gear_wheel),
        0
    );
}

#[test]
fn slot_click_rejects_invalid_furnace_input_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let inserter = item_id_by_name(sim.catalog(), "inserter");
    let (x, y) = first_buildable_rect(&sim, furnace);
    let entity_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: inserter,
        count: 1,
    });

    assert!(
        transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2).is_err()
    );
    assert_eq!(
        sim.player_inventory_mut().slots[2],
        Some(ItemStack {
            item_id: inserter,
            count: 1,
        })
    );
    assert_eq!(
        sim.furnace_state(entity_id)
            .expect("furnace should expose state")
            .input_slot,
        None
    );
}

#[test]
fn slot_click_transfer_handles_burner_drill_fuel_and_output() {
    let mut sim = Simulation::new_test_world(123);
    let drill = entity_id_by_name(sim.catalog(), "burner_mining_drill");
    let coal = item_id_by_name(sim.catalog(), "coal");
    let (x, y) = first_placeable_resource_rect(&sim, drill, coal);
    let entity_id = sim
        .place_entity(drill, x, y, Direction::North)
        .expect("burner drill should be placeable over resources");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: coal,
        count: 1,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("player coal should transfer to burner drill fuel");

    assert_eq!(sim.player_inventory_mut().slots[2], None);
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

    assert_eq!(sim.player_inventory().count(coal), 1);
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

fn pixel_at(map: &factory_app::rendering::map_texture::MapPixels, tile: (i32, i32)) -> [u8; 4] {
    let local_x = (tile.0 - map.bounds.min_x) as u32;
    let local_y = (tile.1 - map.bounds.min_y) as u32;
    let flipped_y = map.bounds.height - 1 - local_y;
    let offset = ((flipped_y * map.bounds.width + local_x) * 4) as usize;
    [
        map.data[offset],
        map.data[offset + 1],
        map.data[offset + 2],
        map.data[offset + 3],
    ]
}

fn first_resource_tile_for_app(sim: &Simulation) -> (i32, i32, factory_sim::ResourceCell) {
    sim.world()
        .chunks
        .values()
        .flat_map(|chunk| {
            chunk
                .tiles
                .iter()
                .enumerate()
                .filter_map(move |(index, tile)| {
                    let resource = tile.resource?;
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    Some((
                        chunk.coord.x * CHUNK_SIZE + local_x,
                        chunk.coord.y * CHUNK_SIZE + local_y,
                        resource,
                    ))
                })
        })
        .next()
        .expect("generated world should contain resource tiles")
}

fn format_item_name_for_test(sim: &Simulation, item_id: ItemId) -> String {
    let name = &sim.catalog().items[item_id.index()].name;
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn first_available_build_selection(app: &App) -> BuildSelection {
    let sim = &app.world().resource::<SimResource>().sim;
    let buildable = buildable_prototypes(sim.catalog())
        .into_iter()
        .find(|buildable| sim.player_inventory().count(buildable.item_id) > 0)
        .expect("starting inventory should include at least one buildable item");
    BuildSelection {
        prototype_id: buildable.prototype_id,
        item_id: buildable.item_id,
    }
}

fn hotbar_key_for_slot(slot_index: usize) -> KeyCode {
    match slot_index {
        0 => KeyCode::Digit1,
        1 => KeyCode::Digit2,
        2 => KeyCode::Digit3,
        3 => KeyCode::Digit4,
        4 => KeyCode::Digit5,
        5 => KeyCode::Digit6,
        6 => KeyCode::Digit7,
        7 => KeyCode::Digit8,
        8 => KeyCode::Digit9,
        _ => panic!("test hotbar slot should be addressable by number key"),
    }
}

fn place_powered_fixture_origin(
    sim: &mut Simulation,
    fixture_width: i32,
    fixture_height: i32,
    pole_offset: (i32, i32),
) -> (i32, i32) {
    let pump = entity_id_by_name(sim.catalog(), "offshore_pump");
    let boiler = entity_id_by_name(sim.catalog(), "boiler");
    let steam_engine = entity_id_by_name(sim.catalog(), "steam_engine");
    let pole = entity_id_by_name(sim.catalog(), "small_electric_pole");
    let coal = item_id_by_name(sim.catalog(), "coal");

    for (x, y) in all_tile_coords(sim) {
        let fixture_x = x + 8;
        let fixture_y = y + 1;
        let source_pole = (x + 5, y + 4);
        let target_pole = (fixture_x + pole_offset.0, fixture_y + pole_offset.1);
        let fixture = EntityFootprint {
            x: fixture_x,
            y: fixture_y,
            width: fixture_width,
            height: fixture_height,
        };

        if !fixture_is_clear_buildable(sim, &fixture)
            || !poles_within_small_pole_reach(source_pole, target_pole)
            || sim.can_place_entity(pump, x, y, Direction::North).is_err()
            || sim
                .can_place_entity(boiler, x, y + 1, Direction::North)
                .is_err()
            || sim
                .can_place_entity(steam_engine, x + 2, y + 1, Direction::North)
                .is_err()
            || sim
                .can_place_entity(pole, source_pole.0, source_pole.1, Direction::North)
                .is_err()
            || sim
                .can_place_entity(pole, target_pole.0, target_pole.1, Direction::North)
                .is_err()
        {
            continue;
        }

        sim.place_entity(pump, x, y, Direction::North)
            .expect("validated offshore pump fixture should be placeable");
        let boiler_id = sim
            .place_entity(boiler, x, y + 1, Direction::North)
            .expect("validated boiler fixture should be placeable");
        sim.place_entity(steam_engine, x + 2, y + 1, Direction::North)
            .expect("validated steam engine fixture should be placeable");
        sim.place_entity(pole, source_pole.0, source_pole.1, Direction::North)
            .expect("validated source pole fixture should be placeable");
        sim.place_entity(pole, target_pole.0, target_pole.1, Direction::North)
            .expect("validated target pole fixture should be placeable");

        *sim.player_inventory_mut() = Inventory::player();
        sim.player_inventory_mut().slots[0] = Some(ItemStack {
            item_id: coal,
            count: 50,
        });
        sim.transfer_player_slot_to_boiler_fuel(boiler_id, 0)
            .expect("boiler should accept coal fuel");

        return (fixture_x, fixture_y);
    }

    panic!("expected powered fixture area");
}

fn all_tile_coords(sim: &Simulation) -> Vec<(i32, i32)> {
    sim.world()
        .chunks
        .values()
        .flat_map(|chunk| {
            chunk.tiles.iter().enumerate().map(move |(index, _)| {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                (
                    chunk.coord.x * CHUNK_SIZE + local_x,
                    chunk.coord.y * CHUNK_SIZE + local_y,
                )
            })
        })
        .collect()
}

fn fixture_is_clear_buildable(sim: &Simulation, footprint: &EntityFootprint) -> bool {
    sim.world().validate_entity_footprint(footprint).is_ok()
        && sim
            .entities()
            .occupancy()
            .validate_available(footprint, None)
            .is_ok()
        && footprint.tiles().into_iter().all(|(x, y)| {
            sim.world()
                .tile_at(x, y)
                .is_some_and(|tile| tile.resource.is_none())
        })
}

fn poles_within_small_pole_reach(first: (i32, i32), second: (i32, i32)) -> bool {
    let dx_x2 = i64::from((first.0 - second.0) * 2);
    let dy_x2 = i64::from((first.1 - second.1) * 2);
    dx_x2 * dx_x2 + dy_x2 * dy_x2 <= 15 * 15
}

fn first_buildable_rect(sim: &Simulation, prototype_id: EntityPrototypeId) -> (i32, i32) {
    let prototype = &sim.catalog().entities[prototype_id.index()];

    for chunk in sim.world().chunks.values() {
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

            if sim.world().validate_entity_footprint(&footprint).is_ok()
                && sim
                    .entities()
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
    for chunk in sim.world().chunks.values() {
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
    factory_data::entity_prototype_id_by_name(catalog, name)
}

fn item_id_by_name(catalog: &PrototypeCatalog, name: &str) -> ItemId {
    factory_data::item_id_by_name(catalog, name)
}

fn recipe_id_by_name(catalog: &PrototypeCatalog, name: &str) -> factory_data::RecipeId {
    factory_data::recipe_id_by_name(catalog, name)
}

fn complete_research_by_name(sim: &mut Simulation, technology_name: &str) {
    let technology_id = technology_id_by_name(sim.catalog(), technology_name);
    let required_units = sim.catalog().technologies[technology_id.index()].required_units;

    sim.select_research(technology_id)
        .unwrap_or_else(|_| panic!("{technology_name} should be selectable"));
    sim.add_research_units(required_units)
        .unwrap_or_else(|_| panic!("{technology_name} should complete"));
}

fn technology_id_by_name(catalog: &PrototypeCatalog, name: &str) -> factory_data::TechnologyId {
    factory_data::technology_id_by_name(catalog, name)
}
