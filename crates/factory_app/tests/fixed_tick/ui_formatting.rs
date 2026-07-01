use super::common::{
    all_tile_coords, complete_research_by_name, entity_id_by_name, first_buildable_rect,
    first_resource_tile_for_app, format_item_name_for_test, item_id_by_name,
    place_powered_fixture_origin, recipe_id_by_name, technology_id_by_name,
};
use factory_app::resources::{RenderSyncStats, SimProfileStats};
use factory_app::ui::debug_overlay::{DebugOverlaySnapshot, format_debug_overlay};
use factory_app::ui::formatting::{
    available_crafting_recipe_choices, crafting_recipe_choices, format_assembler_detail_text,
};
use factory_app::ui::production_stats::{
    bottleneck_lines, diagnostic_lines, fluid_consumption_rows, fluid_production_rows,
    format_fluid_per_minute, power_graph_points, power_summary_lines, production_rows,
};
use factory_data::{CraftingCategory, PrototypeCatalog};
use factory_sim::{
    Direction, Inventory, ItemStack, PowerSummary, Simulation, SimulationCounts,
    SimulationTickProfile,
};
use std::time::Duration;

#[test]
fn debug_overlay_format_no_longer_mentions_debug_item_selection() {
    let sim_profile = SimProfileStats {
        last_tick: SimulationTickProfile {
            belts: Duration::from_micros(100),
            fluids: Duration::from_micros(200),
            power_rebuild: Duration::from_micros(300),
            machines: Duration::from_micros(400),
            inserters: Duration::from_micros(500),
            inventory_transfers: Duration::from_micros(600),
            chunk_lookup: Duration::from_micros(700),
            ..Default::default()
        },
        rolling_average_sim_tick_ms: 1.25,
    };
    let render_sync = RenderSyncStats {
        total: Duration::from_micros(600),
        ..Default::default()
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
        "fluids",
        "power rebuild",
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
fn fluid_stat_formatting_shows_per_minute_and_totals() {
    let mut sim = Simulation::new_test_world(123);
    let pump = entity_id_by_name(sim.catalog(), "offshore_pump");
    let (x, y) = all_tile_coords(&sim)
        .into_iter()
        .find(|(x, y)| sim.can_place_entity(pump, *x, *y, Direction::North).is_ok())
        .expect("expected placeable offshore pump");
    sim.place_entity(pump, x, y, Direction::North)
        .expect("offshore pump should place");
    sim.tick();

    let rows = fluid_production_rows(&sim);
    let row = rows
        .iter()
        .find(|row| row.item_name == "Water")
        .expect("water should appear in fluid production stats");

    assert!(row.per_minute.ends_with("/min"));
    assert_ne!(row.total, "0");
    assert_eq!(format_fluid_per_minute(12_000), "12/min");
    assert_eq!(format_fluid_per_minute(12_500), "12.5/min");
}

#[test]
fn consumption_stat_formatting_includes_fluid_rows() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let assembler = entity_id_by_name(sim.catalog(), "assembling_machine");
    let assembler_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    let recipe = recipe_id_by_name(sim.catalog(), "iron_gear_wheel");
    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("recipe should be selectable");
    sim.tick();

    let rows = fluid_consumption_rows(&sim);
    assert!(rows.iter().any(|row| row.item_name == "Water"));
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

    let samples = [
        factory_sim::PowerStatisticsSample {
            tick: 1,
            production_watts: 100,
            consumption_watts: 50,
            ..Default::default()
        },
        factory_sim::PowerStatisticsSample {
            tick: 2,
            production_watts: 200,
            consumption_watts: 150,
            ..Default::default()
        },
    ];
    let passthrough_points = power_graph_points(&samples, 4);
    assert_eq!(passthrough_points.len(), samples.len());
    assert_eq!(passthrough_points[0].production_watts, 100);
    assert_eq!(passthrough_points[0].consumption_watts, 50);
    assert_eq!(passthrough_points[1].production_watts, 200);
    assert_eq!(passthrough_points[1].consumption_watts, 150);

    let points = power_graph_points(&samples, 1);
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].production_watts, 200);
    assert_eq!(points[0].consumption_watts, 150);
}

#[test]
fn diagnostics_formatting_includes_status_counts_and_hints() {
    let mut sim = Simulation::new_test_world(123);
    let lab = entity_id_by_name(sim.catalog(), "lab");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    sim.place_entity(lab, x, y, Direction::North)
        .expect("lab should be placeable");
    sim.tick();

    assert!(
        diagnostic_lines(&sim)
            .iter()
            .any(|line| line.contains("No research"))
    );
    assert!(
        bottleneck_lines(&sim)
            .iter()
            .any(|line| line == "No active research selected")
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
