use super::super::*;
use super::support::*;
use std::time::Duration;

#[test]
fn counts_report_entities_chunks_belts_items_machines_and_inserters() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (belt_id, _inserter_id, _furnace_id) = place_belt_inserter_furnace_line(&mut sim);

    sim.insert_item_onto_belt(belt_id, 0, iron_ore)
        .expect("empty belt should accept one item");

    let counts = sim.counts();
    assert_eq!(counts.entity_count, 9);
    assert_eq!(counts.chunk_count, 25);
    assert_eq!(counts.belt_count, 1);
    assert_eq!(counts.belt_item_count, 1);
    assert_eq!(counts.machine_count, 1);
    assert_eq!(counts.inserter_count, 1);
    assert_eq!(counts.active_machines, 0);
    assert_eq!(counts.idle_machines, 1);
}

#[test]
fn profiled_tick_advances_one_tick_and_reports_total_time() {
    let mut sim = Simulation::new_test_world(123);
    let before_tick = sim.tick_count();

    let profile = sim.profiled_tick();

    assert_eq!(sim.tick_count(), before_tick + 1);
    assert!(profile.total > Duration::ZERO);
}

#[test]
fn profiled_tick_preserves_deterministic_hashes_against_tick() {
    let mut ticked = Simulation::new_test_world(123);
    let mut profiled = Simulation::new_test_world(123);

    for _ in 0..120 {
        ticked.tick();
        profiled.profiled_tick();
        assert_eq!(profiled.state_hash(), ticked.state_hash());
    }
}

#[test]
fn machine_statuses_classify_power_input_and_output_blocks() {
    let mut no_power = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&no_power.world.prototypes, "assembling_machine");
    let (x, y) = first_buildable_rect(&no_power.world, 3, 3);
    let assembler_id = crate::placement::place(
        &mut no_power,
        crate::placement::EntityPlacementRequest {
            prototype_id: assembler,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("assembler should be placeable");
    add_assembler_gear_job(&mut no_power, assembler_id);
    no_power.tick();
    assert_eq!(status_count(&no_power, MachineStatus::NoPower), 1);

    let mut no_input = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut no_input);
    let recipe = recipe_id(&no_input.world.prototypes, "iron_gear_wheel");
    no_input
        .select_assembler_recipe(assembler_id, recipe)
        .expect("gear recipe should be accepted");
    no_input.tick();
    assert_eq!(status_count(&no_input, MachineStatus::NoInput), 1);

    let mut output_full = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut output_full);
    add_assembler_gear_job(&mut output_full, assembler_id);
    let gear = item_id(&output_full.world.prototypes, "iron_gear_wheel");
    set_inventory_slot(
        &mut output_full
            .entities
            .assembler_state_mut(assembler_id)
            .expect("assembler should exist")
            .output_inventory,
        0,
        gear,
        100,
    );
    output_full.tick();
    assert_eq!(status_count(&output_full, MachineStatus::OutputFull), 1);
}

#[test]
fn machine_status_for_entity_returns_working_for_active_machine() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    add_assembler_gear_job(&mut sim, assembler_id);

    sim.tick();

    assert_eq!(
        sim.machine_status_for_entity(assembler_id),
        Some(MachineStatus::Working)
    );
}

#[test]
fn machine_status_for_entity_returns_none_for_non_machine() {
    let mut sim = Simulation::new_test_world(123);
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let (belt_x, belt_y) = first_placeable_entity_tile(&sim, belt);
    let belt_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: belt,
            x: belt_x,
            y: belt_y,
            direction: Direction::North,
        },
    )
    .expect("belt should be placeable");
    let (chest_x, chest_y) = first_placeable_entity_tile(&sim, chest);
    let chest_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chest,
            x: chest_x,
            y: chest_y,
            direction: Direction::North,
        },
    )
    .expect("chest should be placeable");

    assert_eq!(sim.machine_status_for_entity(belt_id), None);
    assert_eq!(sim.machine_status_for_entity(chest_id), None);
    assert_eq!(sim.machine_status_for_entity(EntityId::new(u64::MAX)), None);
}

#[test]
fn lab_missing_logistic_science_counts_as_no_input() {
    let mut sim = Simulation::new_test_world(123);
    complete_research_by_name(&mut sim, "logistics");
    complete_research_by_name(&mut sim, "automation");
    complete_research_by_name(&mut sim, "electric_power");
    complete_research_by_name(&mut sim, "logistic_science_pack");
    let logistics_2 = technology_id(&sim.world.prototypes, "logistics_2");
    sim.select_research(logistics_2)
        .expect("logistics 2 should be selectable");
    place_lab(&mut sim);

    sim.tick();

    assert_eq!(status_count(&sim, MachineStatus::NoInput), 1);
}

#[test]
fn bottleneck_hints_report_item_science_and_steam_shortages() {
    let mut item_deficit = Simulation::new_test_world(123);
    let iron_plate = item_id(&item_deficit.world.prototypes, "iron_plate");
    item_deficit.record_item_produced(iron_plate, 1);
    item_deficit.record_item_consumed(iron_plate, 4);
    let hints = item_deficit.bottleneck_hints(5);
    assert!(
        hints
            .hints
            .iter()
            .any(|hint| hint.message == "Iron Plate consumed faster than produced")
    );

    let mut science = Simulation::new_test_world(123);
    complete_research_by_name(&mut science, "logistics");
    complete_research_by_name(&mut science, "automation");
    complete_research_by_name(&mut science, "electric_power");
    complete_research_by_name(&mut science, "logistic_science_pack");
    let logistics_2 = technology_id(&science.world.prototypes, "logistics_2");
    science
        .select_research(logistics_2)
        .expect("logistics 2 should be selectable");
    place_lab(&mut science);
    let hints = science.bottleneck_hints(5);
    assert!(
        hints
            .hints
            .iter()
            .any(|hint| hint.message == "Science labs waiting for Logistic Science Pack")
    );

    let mut steam = Simulation::new_test_world(123);
    let (x, y) = place_powered_fixture_origin(&mut steam, 3, 3, (3, 1));
    let pump_id = *steam
        .entities
        .offshore_pumps
        .keys()
        .next()
        .expect("fixture should place an offshore pump");
    crate::entity_mutation::remove(&mut steam, pump_id).expect("offshore pump should be removable");
    let assembler = entity_id_by_name(&steam.world.prototypes, "assembling_machine");
    let assembler_id = crate::placement::place(
        &mut steam,
        crate::placement::EntityPlacementRequest {
            prototype_id: assembler,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("assembler should be placeable");
    add_assembler_gear_job(&mut steam, assembler_id);
    steam.tick();
    let hints = steam.bottleneck_hints(5);
    assert!(
        hints
            .hints
            .iter()
            .any(|hint| hint.message == "Steam engines starved of steam")
    );
}

fn status_count(sim: &Simulation, status: MachineStatus) -> usize {
    sim.machine_statuses()
        .total_by_status
        .into_iter()
        .find(|count| count.status == status)
        .map(|count| count.count)
        .unwrap_or(0)
}

fn first_placeable_entity_tile(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
) -> (WorldTileCoord, WorldTileCoord) {
    all_tile_coords(&sim.world)
        .into_iter()
        .find(|(x, y)| {
            crate::placement::validate(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id,
                    x: *x,
                    y: *y,
                    direction: Direction::North,
                },
            )
            .is_ok()
        })
        .expect("expected placeable entity tile")
}
