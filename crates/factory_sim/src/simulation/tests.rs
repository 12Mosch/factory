use super::*;
use std::collections::BTreeSet;
use std::time::Duration;

const BASIC_UNDERGROUND_BELT_MAX_OFFSET: i32 = 5;
const THROUGHPUT_TEST_TICKS: usize = 60;
const THROUGHPUT_UPSTREAM_LEN: i32 = 12;
const THROUGHPUT_DOWNSTREAM_LEN: i32 = 12;

#[derive(Clone, Copy)]
struct BeltTier {
    belt: &'static str,
    underground_entrance: &'static str,
    underground_exit: &'static str,
    splitter: &'static str,
    underground_max_distance: u8,
    items_per_second: u32,
}

const BELT_TIERS: [BeltTier; 3] = [
    BeltTier {
        belt: "transport_belt",
        underground_entrance: "underground_belt_entrance",
        underground_exit: "underground_belt_exit",
        splitter: "splitter",
        underground_max_distance: 4,
        items_per_second: 15,
    },
    BeltTier {
        belt: "fast_transport_belt",
        underground_entrance: "fast_underground_belt_entrance",
        underground_exit: "fast_underground_belt_exit",
        splitter: "fast_splitter",
        underground_max_distance: 6,
        items_per_second: 30,
    },
    BeltTier {
        belt: "express_transport_belt",
        underground_entrance: "express_underground_belt_entrance",
        underground_exit: "express_underground_belt_exit",
        splitter: "express_splitter",
        underground_max_distance: 8,
        items_per_second: 45,
    },
];

#[test]
fn world_tile_lookup_is_stable_across_chunk_boundaries() {
    let world = WorldSim::new_seeded(123);

    let left_of_origin = world.tile_at(-1, 0).expect("-1 should be in chunk -1");
    let previous_chunk_tile = world.tile_at(-33, 0).expect("-33 should be in chunk -2");
    let previous_chunk = world
        .chunks
        .get(&ChunkCoord { x: -2, y: 0 })
        .expect("previous negative chunk should exist");

    assert_eq!(
        left_of_origin,
        &world
            .chunks
            .get(&ChunkCoord { x: -1, y: 0 })
            .expect("left chunk should exist")
            .tiles[31]
    );
    assert!(world.tile_at(-32, 0).is_some());
    assert_eq!(previous_chunk_tile, &previous_chunk.tiles[31]);
}

#[test]
fn generated_chunks_have_expected_shape() {
    let world = WorldSim::new_seeded(123);

    let generated_side = (WORLD_MAX_CHUNK - WORLD_MIN_CHUNK + 1) as usize;
    assert_eq!(world.chunks.len(), generated_side * generated_side);
    for chunk in world.chunks.values() {
        assert_eq!(chunk.tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
    }
}

#[test]
fn resource_generation_is_deterministic() {
    let a = WorldSim::new_seeded(123);
    let b = WorldSim::new_seeded(123);

    assert_eq!(resource_tiles(&a), resource_tiles(&b));
}

#[test]
fn seed_123_contains_all_resource_item_types() {
    let world = WorldSim::new_seeded(123);
    let ids = WorldPrototypeIds::from_catalog(&world.prototypes);
    let resource_items = world
        .chunks
        .values()
        .flat_map(|chunk| chunk.tiles.iter())
        .filter_map(|tile| tile.resource.map(|resource| resource.resource_item))
        .collect::<BTreeSet<_>>();

    for resource_item in ids.resources {
        assert!(
            resource_items.contains(&resource_item),
            "missing generated resource item {resource_item:?}"
        );
    }
}

#[test]
fn mining_decreases_resource_amount() {
    let mut world = WorldSim::new_seeded(123);
    let (x, y, before) = first_resource_tile(&world);

    let mined = world
        .mine_resource_at(x, y, 25)
        .expect("resource tile should be minable");
    let after = world
        .tile_at(x, y)
        .expect("mined tile should still exist")
        .resource
        .expect("resource should remain after partial mining");

    assert_eq!(mined.amount, 25);
    assert_eq!(after.amount, before.amount - 25);
    assert_eq!(after.resource_item, before.resource_item);
}

#[test]
fn over_mining_clears_resource_tile() {
    let mut world = WorldSim::new_seeded(123);
    let (x, y, before) = first_resource_tile(&world);

    let mined = world
        .mine_resource_at(x, y, before.amount + 1)
        .expect("resource tile should be minable");
    let tile = world.tile_at(x, y).expect("mined tile should still exist");

    assert_eq!(mined.amount, before.amount);
    assert!(tile.resource.is_none());
    assert!(tile.collision.buildable);
    assert!(!tile.collision.minable);
}

#[test]
fn resource_dirty_revision_records_mined_tile() {
    let mut world = WorldSim::new_seeded(123);
    let before_revision = world.resource_revision();
    let (x, y, before) = first_resource_tile(&world);

    world
        .mine_resource_at(x, y, 1)
        .expect("resource tile should be minable");

    assert_eq!(world.resource_revision(), before_revision + 1);

    let changes = world
        .resource_dirty_tiles_since(before_revision)
        .expect("dirty history should include the just-mined tile")
        .collect::<Vec<_>>();
    assert_eq!(
        changes,
        vec![ResourceTileChange {
            revision: before_revision + 1,
            x,
            y,
            resource: Some(ResourceCell {
                resource_item: before.resource_item,
                amount: before.amount - 1,
            }),
        }]
    );
}

#[test]
fn manual_mining_one_ore_decreases_resource_by_one() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y, resource) = first_resource_tile(&sim.world);
    let target = ManualMiningTarget { x, y };
    sim.player = PlayerState::centered_on_tile(x, y);
    let before_count = sim.player_inventory.count(resource.resource_item);

    for _ in 0..MANUAL_MINING_TICKS_PER_ITEM {
        sim.update_manual_mining(Some(target));
    }

    let after_resource = resource_amount_at(&sim.world, x, y).expect("resource should remain");
    assert_eq!(
        sim.player_inventory.count(resource.resource_item),
        before_count + 1
    );
    assert_eq!(after_resource, resource.amount - 1);
}

#[test]
fn manual_mining_can_mine_each_generated_resource_type() {
    let mut sim = Simulation::new_test_world(123);
    let resource_names = ["iron_ore", "copper_ore", "coal", "stone"];

    for resource_name in resource_names {
        let resource_item = item_id(&sim.world.prototypes, resource_name);
        let (x, y, before_amount) = first_resource_tile_for_item(&sim.world, resource_item);
        let before_count = sim.player_inventory.count(resource_item);
        sim.player = PlayerState::centered_on_tile(x, y);

        for _ in 0..MANUAL_MINING_TICKS_PER_ITEM {
            sim.update_manual_mining(Some(ManualMiningTarget { x, y }));
        }

        assert_eq!(
            sim.player_inventory.count(resource_item),
            before_count + 1,
            "{resource_name} should be inserted into inventory"
        );
        assert_eq!(
            resource_amount_at(&sim.world, x, y),
            Some(before_amount - 1),
            "{resource_name} resource amount should decrease by one"
        );
    }
}

#[test]
fn manual_mining_does_not_decrement_resource_before_full_duration() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y, resource) = first_resource_tile(&sim.world);
    let target = ManualMiningTarget { x, y };
    sim.player = PlayerState::centered_on_tile(x, y);
    let before_count = sim.player_inventory.count(resource.resource_item);

    for _ in 0..MANUAL_MINING_TICKS_PER_ITEM - 1 {
        sim.update_manual_mining(Some(target));
    }

    assert_eq!(
        sim.player_inventory.count(resource.resource_item),
        before_count
    );
    assert_eq!(resource_amount_at(&sim.world, x, y), Some(resource.amount));
    assert_eq!(
        sim.manual_mining_progress
            .expect("manual mining should be in progress")
            .progress_ticks,
        MANUAL_MINING_TICKS_PER_ITEM - 1
    );
}

#[test]
fn manual_mining_target_change_cancels_previous_progress() {
    let mut sim = Simulation::new_test_world(123);
    let ((first_x, first_y), (second_x, second_y)) = nearby_resource_pair(&sim.world);
    let first = ManualMiningTarget {
        x: first_x,
        y: first_y,
    };
    let second = ManualMiningTarget {
        x: second_x,
        y: second_y,
    };
    sim.player = PlayerState::centered_on_tile(first_x, first_y);

    for _ in 0..10 {
        sim.update_manual_mining(Some(first));
    }
    sim.update_manual_mining(Some(second));

    assert_eq!(
        sim.manual_mining_progress,
        Some(ManualMiningProgress {
            target: second,
            progress_ticks: 1,
            required_ticks: MANUAL_MINING_TICKS_PER_ITEM,
        })
    );
}

#[test]
fn manual_mining_moving_beyond_reach_cancels_progress() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y, _) = first_resource_tile(&sim.world);
    let target = ManualMiningTarget { x, y };
    sim.player = PlayerState::centered_on_tile(x, y);

    for _ in 0..10 {
        sim.update_manual_mining(Some(target));
    }
    sim.player = PlayerState::centered_on_tile(x + 3, y);
    sim.update_manual_mining(Some(target));

    assert_eq!(sim.manual_mining_progress, None);
}

#[test]
fn manual_mining_full_inventory_prevents_completion_without_decrementing_resource() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y, resource) = first_resource_tile(&sim.world);
    let burner_mining_drill = item_id(&sim.world.prototypes, "burner_mining_drill");
    sim.player = PlayerState::centered_on_tile(x, y);
    sim.player_inventory = Inventory::with_slot_count(1);
    sim.player_inventory
        .insert(&sim.world.prototypes, burner_mining_drill, 1)
        .expect("test inventory should accept one blocking item");

    for _ in 0..MANUAL_MINING_TICKS_PER_ITEM {
        sim.update_manual_mining(Some(ManualMiningTarget { x, y }));
    }

    assert_eq!(sim.player_inventory.count(resource.resource_item), 0);
    assert_eq!(resource_amount_at(&sim.world, x, y), Some(resource.amount));
    assert_eq!(
        sim.manual_mining_progress
            .expect("full inventory should keep completed progress")
            .progress_ticks,
        MANUAL_MINING_TICKS_PER_ITEM
    );
}

#[test]
fn two_by_two_entity_cannot_overlap_another_entity() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 4, 2);

    let first = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("first furnace should be placeable");
    let error = sim
        .place_entity(furnace, x + 1, y, Direction::North)
        .expect_err("second furnace should overlap the first");

    assert!(matches!(
        error,
        BuildError::EntityOccupied {
            entity_id,
            ..
        } if entity_id == first
    ));
}

#[test]
fn entity_cannot_be_placed_on_water() {
    let mut sim = Simulation::new_test_world(123);
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let (x, y) = first_water_tile(&sim.world);

    let error = sim
        .place_entity(inserter, x, y, Direction::North)
        .expect_err("water should block entity placement");

    assert!(matches!(error, BuildError::TileBlocked { x: bx, y: by } if bx == x && by == y));
}

#[test]
fn entity_cannot_be_placed_on_player_tile() {
    let mut sim = Simulation::new_test_world(123);
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    sim.player = PlayerState::centered_on_tile(x, y);

    let error = sim
        .place_entity(inserter, x, y, Direction::North)
        .expect_err("player tile should block entity placement");

    assert!(matches!(error, BuildError::TileBlocked { x: bx, y: by } if bx == x && by == y));
}

#[test]
fn multi_tile_entity_cannot_overlap_player_tile() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    sim.player = PlayerState::centered_on_tile(x + 1, y + 1);

    let error = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect_err("entity footprint should not overlap the player tile");

    assert!(
        matches!(error, BuildError::TileBlocked { x: bx, y: by } if bx == x + 1 && by == y + 1)
    );
}

#[test]
fn entity_cannot_be_placed_outside_generated_chunks() {
    let mut sim = Simulation::new_test_world(123);
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let outside_x = (WORLD_MAX_CHUNK + 1) * CHUNK_SIZE;

    let error = sim
        .place_entity(inserter, outside_x, 0, Direction::North)
        .expect_err("unloaded chunks should block entity placement");

    assert!(matches!(
        error,
        BuildError::OutsideGeneratedChunks { x, y: 0 } if x == outside_x
    ));
}

#[test]
fn rotation_updates_entity_footprint() {
    let mut catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let inserter = entity_id_by_name(&catalog, "inserter");
    catalog.entities[inserter.index()].size.y = 2;

    let mut sim = Simulation::new(123, catalog);
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    let entity_id = sim
        .place_entity(inserter, x, y, Direction::North)
        .expect("rectangular entity should be placeable");

    assert_eq!(sim.entities.occupancy().entity_at(x, y), Some(entity_id));
    assert_eq!(
        sim.entities.occupancy().entity_at(x, y + 1),
        Some(entity_id)
    );
    assert_eq!(sim.entities.occupancy().entity_at(x + 1, y), None);

    sim.rotate_entity(entity_id, Direction::East)
        .expect("rotated rectangular entity should still be placeable");

    let entity = sim
        .entities
        .placed_entity(entity_id)
        .expect("placed entity should remain");
    assert_eq!(entity.footprint.width, 2);
    assert_eq!(entity.footprint.height, 1);
    assert_eq!(sim.entities.occupancy().entity_at(x, y), Some(entity_id));
    assert_eq!(
        sim.entities.occupancy().entity_at(x + 1, y),
        Some(entity_id)
    );
    assert_eq!(sim.entities.occupancy().entity_at(x, y + 1), None);
}

#[test]
fn rotation_cannot_overlap_player_tile() {
    let mut catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let inserter = entity_id_by_name(&catalog, "inserter");
    catalog.entities[inserter.index()].size.y = 2;

    let mut sim = Simulation::new(123, catalog);
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    let entity_id = sim
        .place_entity(inserter, x, y, Direction::North)
        .expect("rectangular entity should be placeable");
    sim.player = PlayerState::centered_on_tile(x + 1, y);

    let error = sim
        .rotate_entity(entity_id, Direction::East)
        .expect_err("rotated footprint should not overlap the player tile");

    assert!(matches!(
        error,
        BuildError::TileBlocked { x: bx, y: by } if bx == x + 1 && by == y
    ));
}

#[test]
fn chest_placement_creates_sixteen_inventory_slots() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);

    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");

    assert_eq!(
        sim.entity_inventory(entity_id)
            .expect("chest should have an inventory")
            .slots
            .len(),
        16
    );
}

#[test]
fn catalog_loads_assembler_metadata() {
    let sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let prototype = &sim.world.prototypes.entities[assembler.index()];
    let metadata = prototype
        .assembling_machine
        .as_ref()
        .expect("assembler prototype should load metadata");

    assert_eq!(prototype.entity_kind, EntityKind::AssemblingMachine);
    assert_eq!((prototype.size.x, prototype.size.y), (3, 3));
    assert_eq!(metadata.crafting_speed_numerator, 1);
    assert_eq!(metadata.crafting_speed_denominator, 2);
    assert_eq!(
        metadata.input_slot_count,
        ASSEMBLING_MACHINE_INPUT_SLOT_COUNT
    );
    assert_eq!(
        metadata.output_slot_count,
        ASSEMBLING_MACHINE_OUTPUT_SLOT_COUNT
    );
}

#[test]
fn assembler_crafts_gears_from_iron_plates() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });
    sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
        .expect("assembler should accept gear ingredients");

    for _ in 0..60 {
        sim.tick();
    }

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.input_inventory.count(iron_plate), 0);
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 1);
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(state.crafting_required_ticks, 60);
}

#[test]
fn unpowered_assembler_does_not_craft() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = first_buildable_rect(&sim.world, 3, 3);
    let assembler_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });
    sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
        .expect("assembler should accept gear ingredients");

    for _ in 0..90 {
        sim.tick();
    }

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.input_inventory.count(iron_plate), 2);
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 0);
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(
        sim.entity_power_status(assembler_id)
            .expect("assembler should report power status")
            .satisfaction_permyriad,
        0
    );
}

#[test]
fn powered_assembler_crafts() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });
    sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
        .expect("assembler should accept gear ingredients");

    for _ in 0..60 {
        sim.tick();
    }

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 1);
}

#[test]
fn insufficient_power_slows_machine_progress() {
    let mut catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let assembler = entity_id_by_name(&catalog, "assembling_machine");
    catalog.entities[assembler.index()]
        .electric_energy_source
        .as_mut()
        .expect("assembler should have electric energy source")
        .energy_usage_watts = 1_797_500;
    let mut sim = Simulation::new(123, catalog);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });
    sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
        .expect("assembler should accept gear ingredients");

    for _ in 0..60 {
        sim.tick();
    }

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.crafting_progress_ticks, 30);
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 0);
    assert_eq!(
        sim.entity_power_status(assembler_id)
            .expect("assembler should report power status")
            .satisfaction_permyriad,
        5_000
    );

    for _ in 0..60 {
        sim.tick();
    }

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 1);
}

#[test]
fn disconnected_networks_do_not_share_power() {
    let mut sim = Simulation::new_test_world(123);
    let _ = place_powered_fixture_origin(&mut sim, 1, 1, (1, 2));
    let assembler_id = place_disconnected_assembler_network(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });
    sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
        .expect("assembler should accept gear ingredients");

    for _ in 0..90 {
        sim.tick();
    }

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 0);
    assert_eq!(
        sim.entity_power_status(assembler_id)
            .expect("assembler should report power status")
            .satisfaction_permyriad,
        0
    );
    assert!(sim.power_summary().network_count >= 2);
}

#[test]
fn small_pole_coverage_connects_nearby_machine_and_wire_reach_connects_networks() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let assembler_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");

    sim.tick();

    let status = sim
        .entity_power_status(assembler_id)
        .expect("assembler should report power status");
    assert_eq!(status.network_id, Some(0));
    assert_eq!(sim.power_networks().len(), 1);
    assert_eq!(sim.power_networks()[0].pole_count, 2);
}

#[test]
fn pole_networks_outside_reach_do_not_connect() {
    let mut sim = Simulation::new_test_world(123);
    let pole = entity_id_by_name(&sim.world.prototypes, "small_electric_pole");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    sim.place_entity(pole, x, y, Direction::North)
        .expect("first pole should be placeable");

    for (candidate_x, candidate_y) in all_tile_coords(&sim.world) {
        if !poles_within_small_pole_reach((x, y), (candidate_x, candidate_y))
            && sim
                .can_place_entity(pole, candidate_x, candidate_y, Direction::North)
                .is_ok()
        {
            sim.place_entity(pole, candidate_x, candidate_y, Direction::North)
                .expect("second pole should be placeable");
            sim.tick();
            assert_eq!(sim.power_summary().network_count, 2);
            return;
        }
    }

    panic!("expected a second pole location outside wire reach");
}

#[test]
fn steam_engine_produces_only_with_connected_pole_and_adjacent_fueled_boiler() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let assembler_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    add_assembler_gear_job(&mut sim, assembler_id);

    sim.tick();

    let summary = sim.power_summary();
    assert_eq!(summary.available_production_watts, 900_000);
    assert_eq!(summary.production_watts, 77_500);
    assert_eq!(summary.consumption_watts, 77_500);
    assert_eq!(summary.satisfaction_permyriad, 10_000);
}

#[test]
fn boiler_does_not_burn_fuel_without_steam_demand() {
    let mut sim = Simulation::new_test_world(123);
    let (_, _, boiler_id) = place_powered_fixture_origin_with_boiler(&mut sim, 1, 1, (1, 2));
    let before = sim.boiler_state(boiler_id).unwrap().clone();

    for _ in 0..120 {
        sim.tick();
    }

    let after = sim.boiler_state(boiler_id).unwrap();
    assert_eq!(after.energy.fuel_slot, before.energy.fuel_slot);
    assert_eq!(
        after.energy.energy_remaining_joules.to_bits(),
        before.energy.energy_remaining_joules.to_bits()
    );
    assert_eq!(sim.power_summary().production_watts, 0);
}

#[test]
fn boiler_clears_insufficient_residual_energy_without_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y, boiler_id) = place_powered_fixture_origin_with_boiler(&mut sim, 3, 3, (3, 1));
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let assembler_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    add_assembler_gear_job(&mut sim, assembler_id);
    let state = sim
        .entities
        .boiler_state_mut(boiler_id)
        .expect("boiler should exist");
    state.energy.fuel_slot = None;
    state.energy.energy_remaining_joules = 1.0;

    sim.tick();

    let state = sim.boiler_state(boiler_id).unwrap();
    assert_eq!(state.energy.fuel_slot, None);
    assert_eq!(state.energy.energy_remaining_joules, 0.0);
}

#[test]
fn boiler_validation_rejects_non_fuel_in_fuel_slot() {
    let mut sim = Simulation::new_test_world(123);
    let (_, _, boiler_id) = place_powered_fixture_origin_with_boiler(&mut sim, 1, 1, (1, 2));
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    sim.entities
        .boiler_state_mut(boiler_id)
        .expect("boiler should exist")
        .energy
        .fuel_slot = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });

    assert_eq!(
        sim.validate(),
        Err(SimValidationError::InvalidMachineItem {
            entity_id: boiler_id,
            item_id: iron_ore,
        })
    );
}

#[test]
fn boiler_with_no_water_or_no_fuel_produces_no_steam_power() {
    let mut no_fuel = Simulation::new_test_world(123);
    let (x, y, boiler_id) = place_powered_fixture_origin_with_boiler(&mut no_fuel, 3, 3, (3, 1));
    no_fuel
        .entities
        .boiler_state_mut(boiler_id)
        .expect("boiler should exist")
        .energy
        .fuel_slot = None;
    let assembler = entity_id_by_name(&no_fuel.world.prototypes, "assembling_machine");
    let assembler_id = no_fuel
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    add_assembler_gear_job(&mut no_fuel, assembler_id);
    no_fuel.tick();
    assert_eq!(no_fuel.power_summary().available_production_watts, 0);

    let mut no_water = Simulation::new_test_world(123);
    let (x, y, _) = place_powered_fixture_origin_with_boiler(&mut no_water, 3, 3, (3, 1));
    let pump_id = *no_water
        .entities
        .offshore_pumps
        .keys()
        .next()
        .expect("fixture should place an offshore pump");
    no_water
        .remove_entity(pump_id)
        .expect("offshore pump should be removable");
    let assembler = entity_id_by_name(&no_water.world.prototypes, "assembling_machine");
    let assembler_id = no_water
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    add_assembler_gear_job(&mut no_water, assembler_id);
    no_water.tick();
    assert_eq!(no_water.power_summary().available_production_watts, 0);
}

#[test]
fn offshore_pump_placement_succeeds_on_shoreline_and_fails_away_from_water() {
    let mut sim = Simulation::new_test_world(123);
    let pump = entity_id_by_name(&sim.world.prototypes, "offshore_pump");
    let (shore_x, shore_y) = first_placeable_offshore_pump(&sim, pump);
    sim.place_entity(pump, shore_x, shore_y, Direction::North)
        .expect("offshore pump should place on shoreline");

    let away = first_buildable_offshore_pump_footprint_away_from_water(&sim, pump);
    assert!(matches!(
        sim.can_place_entity(pump, away.0, away.1, Direction::North),
        Err(BuildError::TileBlocked { .. })
    ));
}

#[test]
fn inserter_does_not_move_without_electricity() {
    let mut sim = Simulation::new_test_world(123);
    let (chest_id, inserter_id, furnace_id) = place_unpowered_chest_inserter_furnace_line(&mut sim);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    sim.entity_inventory_mut(chest_id)
        .expect("chest should expose inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });

    for _ in 0..inserter_cycle_tick_budget(&sim, inserter_id) * 2 {
        sim.tick();
    }

    assert_eq!(sim.entity_inventory(chest_id).unwrap().count(iron_ore), 1);
    assert_eq!(sim.furnace_state(furnace_id).unwrap().input_slot, None);
    assert_eq!(
        sim.entity_power_status(inserter_id)
            .expect("inserter should report power status")
            .satisfaction_permyriad,
        0
    );
}

#[test]
fn lab_does_not_research_without_electricity() {
    let mut sim = Simulation::new_test_world(123);
    let lab = entity_id_by_name(&sim.world.prototypes, "lab");
    let automation = technology_id(&sim.world.prototypes, "automation");
    let science_pack = item_id(&sim.world.prototypes, "automation_science_pack");
    let (x, y) = first_buildable_rect(&sim.world, 3, 3);
    let lab_id = sim
        .place_entity(lab, x, y, Direction::North)
        .expect("lab should be placeable");
    sim.select_research(automation)
        .expect("automation should be selectable");
    sim.entity_inventory_mut(lab_id)
        .expect("lab should expose inventory")
        .slots[0] = Some(ItemStack {
        item_id: science_pack,
        count: 10,
    });

    for _ in 0..1_200 {
        sim.tick();
    }

    assert_eq!(sim.technology_progress(automation), Some(0));
    assert_eq!(
        sim.entity_power_status(lab_id)
            .expect("lab should report power status")
            .satisfaction_permyriad,
        0
    );
}

#[test]
fn power_summary_reports_production_consumption_and_satisfaction() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    add_assembler_gear_job(&mut sim, assembler_id);

    sim.tick();

    assert_eq!(
        sim.power_summary(),
        PowerSummary {
            production_watts: 77_500,
            available_production_watts: 900_000,
            consumption_watts: 77_500,
            satisfaction_permyriad: 10_000,
            network_count: 1,
        }
    );
}

#[test]
fn save_load_preserves_state_hash_after_electricity_entities_exist() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    add_assembler_gear_job(&mut sim, assembler_id);
    for _ in 0..17 {
        sim.tick();
    }

    let before = sim.state_hash();
    let bytes = save_to_bytes(&sim).expect("electricity sim should save");
    let loaded = load_from_bytes(&bytes).expect("electricity sim should load");

    assert_eq!(before, loaded.state_hash());
}

#[test]
fn assembler_blocks_without_inputs() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });
    sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
        .expect("assembler should accept partial ingredients");

    for _ in 0..90 {
        sim.tick();
    }

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.input_inventory.count(iron_plate), 1);
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 0);
    assert_eq!(state.crafting_progress_ticks, 0);
}

#[test]
fn assembler_blocks_when_output_full() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let stack_size = item_stack_size(&sim.world.prototypes, iron_gear_wheel)
        .expect("gear should have stack size");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });
    sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
        .expect("assembler should accept gear ingredients");
    sim.entities
        .assembler_state_mut(assembler_id)
        .expect("assembler should expose mutable state")
        .output_inventory
        .slots[0] = Some(ItemStack {
        item_id: iron_gear_wheel,
        count: stack_size,
    });

    for _ in 0..60 {
        sim.tick();
    }

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.input_inventory.count(iron_plate), 2);
    assert_eq!(
        state.output_inventory.count(iron_gear_wheel),
        u32::from(stack_size)
    );
    assert_eq!(state.crafting_progress_ticks, 0);
}

#[test]
fn invalid_assembler_recipe_is_rejected() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let smelting_recipe = recipe_id(&sim.world.prototypes, "iron_plate");

    assert_eq!(
        sim.select_assembler_recipe(assembler_id, smelting_recipe),
        Err(AssemblerError::InvalidRecipe(smelting_recipe))
    );
    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state")
            .selected_recipe,
        None
    );
}

#[test]
fn selecting_different_assembler_recipe_on_empty_assembler_succeeds() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");

    sim.select_assembler_recipe(assembler_id, gear_recipe)
        .expect("initial recipe should be accepted");
    sim.select_assembler_recipe(assembler_id, cable_recipe)
        .expect("empty assembler should allow recipe changes");

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.selected_recipe, Some(cable_recipe));
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(state.crafting_required_ticks, 60);
}

#[test]
fn selecting_same_assembler_recipe_while_non_empty_preserves_progress() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");

    sim.select_assembler_recipe(assembler_id, gear_recipe)
        .expect("initial recipe should be accepted");
    {
        let state = sim
            .entities
            .assembler_state_mut(assembler_id)
            .expect("assembler should expose mutable state");
        state.input_inventory.slots[0] = Some(ItemStack {
            item_id: iron_plate,
            count: 1,
        });
        state.crafting_progress_ticks = 17;
    }
    let before = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state")
        .clone();

    sim.select_assembler_recipe(assembler_id, gear_recipe)
        .expect("same recipe selection should be idempotent");

    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state"),
        &before
    );
}

#[test]
fn selecting_different_assembler_recipe_with_input_items_fails_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");

    sim.select_assembler_recipe(assembler_id, gear_recipe)
        .expect("initial recipe should be accepted");
    sim.entities
        .assembler_state_mut(assembler_id)
        .expect("assembler should expose mutable state")
        .input_inventory
        .slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });
    let before = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state")
        .clone();

    assert_eq!(
        sim.select_assembler_recipe(assembler_id, cable_recipe),
        Err(AssemblerError::RecipeChangeRequiresEmpty {
            entity_id: assembler_id
        })
    );
    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state"),
        &before
    );
}

#[test]
fn selecting_different_assembler_recipe_with_output_items_fails_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, gear_recipe)
        .expect("initial recipe should be accepted");
    sim.entities
        .assembler_state_mut(assembler_id)
        .expect("assembler should expose mutable state")
        .output_inventory
        .slots[0] = Some(ItemStack {
        item_id: iron_gear_wheel,
        count: 1,
    });
    let before = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state")
        .clone();

    assert_eq!(
        sim.select_assembler_recipe(assembler_id, cable_recipe),
        Err(AssemblerError::RecipeChangeRequiresEmpty {
            entity_id: assembler_id
        })
    );
    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state"),
        &before
    );
}

#[test]
fn selecting_different_assembler_recipe_with_progress_fails_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");

    sim.select_assembler_recipe(assembler_id, gear_recipe)
        .expect("initial recipe should be accepted");
    sim.entities
        .assembler_state_mut(assembler_id)
        .expect("assembler should expose mutable state")
        .crafting_progress_ticks = 1;
    let before = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state")
        .clone();

    assert_eq!(
        sim.select_assembler_recipe(assembler_id, cable_recipe),
        Err(AssemblerError::RecipeChangeRequiresEmpty {
            entity_id: assembler_id
        })
    );
    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state"),
        &before
    );
}

#[test]
fn assembler_ingredient_status_reports_partial_ingredients() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.entities
        .assembler_state_mut(assembler_id)
        .expect("assembler should expose mutable state")
        .input_inventory
        .slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });

    assert_eq!(
        sim.assembler_ingredient_status(assembler_id)
            .expect("ingredient status should be available"),
        vec![AssemblerIngredientStatus {
            item: iron_plate,
            required: 2,
            available: 1,
            missing: 1,
        }]
    );
}

#[test]
fn inserter_moves_ingredients_from_chest_to_assembler() {
    let mut sim = Simulation::new_test_world(123);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let (chest_id, inserter_id, assembler_id) = place_chest_inserter_assembler_line(&mut sim);
    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.entity_inventory_mut(chest_id)
        .expect("chest should have inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_plate),
        0
    );
    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state")
            .input_inventory
            .count(iron_plate),
        1
    );
}

#[test]
fn inserter_does_not_place_invalid_items_into_lab() {
    let mut sim = Simulation::new_test_world(123);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let (chest_id, inserter_id, lab_id) = place_chest_inserter_lab_line(&mut sim);
    sim.entity_inventory_mut(chest_id)
        .expect("chest should expose inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });

    for _ in 0..100 {
        sim.tick();
    }

    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should expose inventory")
            .count(iron_plate),
        1
    );
    assert_eq!(
        sim.entity_inventory(lab_id)
            .expect("lab should expose inventory")
            .count(iron_plate),
        0
    );
    assert_eq!(
        sim.inserter_state(inserter_id)
            .expect("inserter should expose state"),
        &InserterState::WaitingForItem
    );
}

#[test]
fn inserter_removes_assembler_output_to_chest() {
    let mut sim = Simulation::new_test_world(123);
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let (assembler_id, inserter_id, chest_id) = place_assembler_inserter_chest_line(&mut sim);
    sim.entities
        .assembler_state_mut(assembler_id)
        .expect("assembler should expose mutable state")
        .output_inventory
        .slots[0] = Some(ItemStack {
        item_id: iron_gear_wheel,
        count: 1,
    });

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state")
            .output_inventory
            .count(iron_gear_wheel),
        0
    );
    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_gear_wheel),
        1
    );
}

#[test]
fn assembler_state_hash_remains_deterministic_for_same_seed_actions() {
    let mut first = Simulation::new_test_world(123);
    let mut second = Simulation::new_test_world(123);
    run_same_assembler_actions(&mut first);
    run_same_assembler_actions(&mut second);

    assert_eq!(first.state_hash(), second.state_hash());
}

#[test]
fn chest_inventory_accepts_items() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    let catalog = sim.world.prototypes.clone();

    sim.entity_inventory_mut(entity_id)
        .expect("chest should expose mutable inventory")
        .insert(&catalog, iron_plate, 25)
        .expect("chest should accept iron plates");

    assert_eq!(
        sim.entity_inventory(entity_id)
            .expect("chest should have inventory")
            .count(iron_plate),
        25
    );
}

#[test]
fn player_can_transfer_stack_to_chest() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[5] = Some(ItemStack {
        item_id: iron_plate,
        count: 42,
    });

    sim.transfer_player_slot_to_entity(entity_id, 5)
        .expect("stack should transfer to chest");

    assert_eq!(sim.player_inventory.slots[5], None);
    assert_eq!(
        sim.entity_inventory(entity_id)
            .expect("chest should have inventory")
            .count(iron_plate),
        42
    );
}

#[test]
fn transfer_to_full_chest_fails_without_changing_player_inventory() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[3] = Some(ItemStack {
        item_id: iron_plate,
        count: 12,
    });
    {
        let inventory = sim
            .entity_inventory_mut(entity_id)
            .expect("chest should expose inventory");
        for slot in &mut inventory.slots {
            *slot = Some(ItemStack {
                item_id: coal,
                count: 100,
            });
        }
    }
    assert!(
        !sim.entity_inventory(entity_id)
            .expect("chest should have inventory")
            .can_insert(&sim.world.prototypes, iron_plate, 12)
    );
    let player_before = sim.player_inventory.clone();

    assert_eq!(
        sim.transfer_player_slot_to_entity(entity_id, 3),
        Err(ContainerError::InsufficientSpace)
    );
    assert_eq!(sim.player_inventory, player_before);
}

#[test]
fn transfer_from_chest_to_full_player_fails_without_changing_chest_inventory() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    sim.player_inventory = Inventory::with_slot_count(1);
    sim.player_inventory
        .insert(&sim.world.prototypes, coal, 100)
        .expect("player inventory should accept blocking stack");
    let inventory = sim
        .entity_inventory_mut(entity_id)
        .expect("chest should expose inventory");
    inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 8,
    });
    let chest_before = sim
        .entity_inventory(entity_id)
        .expect("chest should have inventory")
        .clone();

    assert_eq!(
        sim.transfer_entity_slot_to_player(entity_id, 0),
        Err(ContainerError::InsufficientSpace)
    );
    assert_eq!(
        sim.entity_inventory(entity_id)
            .expect("chest should still have inventory"),
        &chest_before
    );
}

#[test]
fn burner_drill_without_fuel_remains_idle() {
    let mut sim = Simulation::new_test_world(123);
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, x, y, before) = place_burner_drill_on_resource(&mut sim, coal);

    for _ in 0..240 {
        sim.tick();
    }

    let state = sim
        .burner_drill_state(entity_id)
        .expect("burner drill should expose state");
    assert_eq!(state.energy.energy_remaining_joules, 0.0);
    assert_eq!(state.mining_progress_ticks, 0);
    assert_eq!(state.output_slot, None);
    assert_eq!(resource_amount_at(&sim.world, x, y), Some(before));
}

#[test]
fn burner_drill_with_coal_mines_output() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, x, y, before) = place_burner_drill_on_resource(&mut sim, iron_ore);
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: coal,
        count: 1,
    });
    sim.transfer_player_slot_to_burner_drill_fuel(entity_id, 0)
        .expect("coal should transfer to drill fuel");

    for _ in 0..240 {
        sim.tick();
    }

    let state = sim
        .burner_drill_state(entity_id)
        .expect("burner drill should expose state");
    assert_eq!(
        state.output_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        })
    );
    assert_eq!(state.mining_progress_ticks, 0);
    assert_eq!(state.energy.energy_remaining_joules, 3_400_000.0);
    assert_eq!(resource_amount_at(&sim.world, x, y), Some(before - 1));
}

#[test]
fn one_coal_powers_burner_drill_for_exactly_1600_ticks() {
    let mut sim = Simulation::new_test_world(123);
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, _, _, _) = place_burner_drill_on_resource(&mut sim, coal);
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: coal,
        count: 1,
    });
    sim.transfer_player_slot_to_burner_drill_fuel(entity_id, 0)
        .expect("coal should transfer to drill fuel");

    for _ in 0..1600 {
        sim.tick();
    }

    let state = sim
        .burner_drill_state(entity_id)
        .expect("burner drill should expose state");
    assert_eq!(state.energy.fuel_slot, None);
    assert_eq!(state.energy.energy_remaining_joules, 0.0);
    assert_eq!(state.output_slot.map(|stack| stack.count), Some(6));
    assert_eq!(state.mining_progress_ticks, 160);

    sim.tick();

    let state = sim
        .burner_drill_state(entity_id)
        .expect("burner drill should expose state");
    assert_eq!(state.energy.energy_remaining_joules, 0.0);
    assert_eq!(state.mining_progress_ticks, 160);
}

#[test]
fn blocked_burner_drill_output_pauses_without_consuming_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, x, y, before) = place_burner_drill_on_resource(&mut sim, iron_ore);
    let state = sim
        .entities
        .burner_drill_state_mut(entity_id)
        .expect("burner drill should expose state");
    state.energy.fuel_slot = Some(ItemStack {
        item_id: coal,
        count: 1,
    });
    state.output_slot = Some(ItemStack {
        item_id: coal,
        count: 1,
    });

    for _ in 0..10 {
        sim.tick();
    }

    let state = sim
        .burner_drill_state(entity_id)
        .expect("burner drill should expose state");
    assert_eq!(
        state.energy.fuel_slot,
        Some(ItemStack {
            item_id: coal,
            count: 1,
        })
    );
    assert_eq!(state.energy.energy_remaining_joules, 0.0);
    assert_eq!(state.mining_progress_ticks, 0);
    assert_eq!(resource_amount_at(&sim.world, x, y), Some(before));
}

#[test]
fn invalid_burner_drill_fuel_is_rejected() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (entity_id, _, _, _) = place_burner_drill_on_resource(&mut sim, iron_ore);
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });

    assert_eq!(
        sim.transfer_player_slot_to_burner_drill_fuel(entity_id, 0),
        Err(BurnerDrillError::InvalidFuel(iron_ore))
    );
    assert_eq!(
        sim.burner_drill_state(entity_id)
            .expect("burner drill should expose state")
            .energy
            .fuel_slot,
        None
    );
    assert_eq!(
        sim.player_inventory.slots[0],
        Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        })
    );
}

#[test]
fn burner_drill_outputs_ore_after_required_ticks() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, _, _, _) = place_burner_drill_on_resource(&mut sim, iron_ore);
    add_fuel_to_burner_drill(&mut sim, entity_id, coal, 1);

    for _ in 0..240 {
        sim.tick();
    }

    assert_eq!(
        sim.burner_drill_state(entity_id)
            .expect("burner drill should expose state")
            .output_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        })
    );
}

#[test]
fn burner_drill_consumes_resource_tile() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, x, y, before) = place_burner_drill_on_resource(&mut sim, iron_ore);
    add_fuel_to_burner_drill(&mut sim, entity_id, coal, 1);

    for _ in 0..240 {
        sim.tick();
    }

    assert_eq!(resource_amount_at(&sim.world, x, y), Some(before - 1));
}

#[test]
fn belt_moves_item_to_next_segment() {
    let mut sim = Simulation::new_test_world(123);
    let belts = place_belt_line(&mut sim, 2);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    sim.insert_item_onto_belt(belts[0], 0, iron_ore)
        .expect("empty belt entry should accept an item");

    for _ in 0..32 {
        sim.tick();
    }

    assert!(
        sim.belt_segment(belts[0]).unwrap().lanes[0]
            .items
            .is_empty()
    );
    let second_lane = &sim.belt_segment(belts[1]).unwrap().lanes[0].items;
    assert_eq!(second_lane.len(), 1);
    assert_eq!(second_lane[0].item_id, iron_ore);
}

#[test]
fn belt_does_not_duplicate_items() {
    let mut sim = Simulation::new_test_world(123);
    let belts = place_belt_line(&mut sim, 20);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    feed_belt_items(&mut sim, belts[0], iron_ore, 100);

    for _ in 0..2_000 {
        sim.tick();
    }

    assert_eq!(total_belt_item_count(&sim), 100);
}

#[test]
fn straight_belt_tier_throughput_matches_prototype_speed() {
    for tier in BELT_TIERS {
        let mut sim = Simulation::new_test_world(123);

        assert_eq!(
            straight_belt_throughput_over_one_second(&mut sim, tier.belt),
            tier.items_per_second,
            "{} should move {} items per second",
            tier.belt,
            tier.items_per_second
        );
    }
}

#[test]
fn underground_belt_tier_throughput_matches_prototype_speed() {
    for tier in BELT_TIERS {
        let mut sim = Simulation::new_test_world(123);

        assert_eq!(
            underground_belt_throughput_over_one_second(
                &mut sim,
                tier.belt,
                tier.underground_entrance,
                tier.underground_exit,
                tier.underground_max_distance,
            ),
            tier.items_per_second,
            "{} pair should move {} items per second",
            tier.underground_entrance,
            tier.items_per_second
        );
    }
}

#[test]
fn splitter_tier_throughput_matches_prototype_speed() {
    for tier in BELT_TIERS {
        let mut sim = Simulation::new_test_world(123);

        assert_eq!(
            splitter_throughput_over_one_second(&mut sim, tier.belt, tier.splitter),
            tier.items_per_second,
            "{} should move {} items per second",
            tier.splitter,
            tier.items_per_second
        );
    }
}

#[test]
fn splitter_balances_one_full_input_across_two_outputs() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let fixture = place_splitter_fixture(&mut sim, 20, true);
    let inserted = 40;

    feed_belt_items(&mut sim, fixture.input0, iron_ore, inserted);
    for _ in 0..2_000 {
        sim.tick();
    }

    let output0 = total_item_count_on_belts(&sim, &fixture.output0, iron_ore);
    let output1 = total_item_count_on_belts(&sim, &fixture.output1, iron_ore);

    assert_eq!(output0 + output1, inserted as u32);
    assert!(output0.abs_diff(output1) <= 1);
}

#[test]
fn splitter_merges_two_inputs_into_one_output_without_loss() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let copper_ore = item_id(&sim.world.prototypes, "copper_ore");
    let fixture = place_splitter_fixture(&mut sim, 30, false);
    let inserted_per_input = 20;

    feed_belt_items(&mut sim, fixture.input0, iron_ore, inserted_per_input);
    feed_belt_items(&mut sim, fixture.input1, copper_ore, inserted_per_input);
    for _ in 0..3_000 {
        sim.tick();
    }

    let iron_output = total_item_count_on_belts(&sim, &fixture.output0, iron_ore);
    let copper_output = total_item_count_on_belts(&sim, &fixture.output0, copper_ore);

    assert_eq!(
        total_belt_count_for_item(&sim, iron_ore),
        inserted_per_input as u32
    );
    assert_eq!(
        total_belt_count_for_item(&sim, copper_ore),
        inserted_per_input as u32
    );
    assert!(iron_output > 0);
    assert!(copper_output > 0);
}

#[test]
fn splitter_blocked_outputs_do_not_delete_items() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let fixture = place_splitter_fixture(&mut sim, 0, false);
    let inserted = 10;

    feed_belt_items(&mut sim, fixture.input0, iron_ore, inserted);
    for _ in 0..1_000 {
        sim.tick();
    }

    assert_eq!(total_belt_count_for_item(&sim, iron_ore), inserted as u32);
    sim.validate_state()
        .expect("blocked splitter fixture should remain valid");
}

#[test]
fn splitter_conserves_items_over_10000_ticks() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let copper_ore = item_id(&sim.world.prototypes, "copper_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let fixture = place_splitter_fixture(&mut sim, 12, false);

    feed_belt_items(&mut sim, fixture.input0, iron_ore, 12);
    feed_belt_items(&mut sim, fixture.input1, copper_ore, 12);
    feed_belt_items(&mut sim, fixture.input0, coal, 8);
    let before = [
        (iron_ore, total_belt_count_for_item(&sim, iron_ore)),
        (copper_ore, total_belt_count_for_item(&sim, copper_ore)),
        (coal, total_belt_count_for_item(&sim, coal)),
    ];

    for _ in 0..10_000 {
        sim.tick();
    }

    for (item_id, before_count) in before {
        assert_eq!(total_belt_count_for_item(&sim, item_id), before_count);
    }
    sim.validate_state()
        .expect("long-running splitter fixture should remain valid");
}

#[test]
fn save_load_round_trip_preserves_splitter_internal_state_hash() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let copper_ore = item_id(&sim.world.prototypes, "copper_ore");
    let fixture = place_splitter_fixture(&mut sim, 1, true);
    let state = sim
        .entities
        .splitters
        .get_mut(&fixture.splitter)
        .expect("placed splitter should have state");
    state.input_lanes[0][0].items.push(BeltItem {
        item_id: iron_ore,
        position_subtile: 64,
    });
    state.input_lanes[1][1].items.push(BeltItem {
        item_id: copper_ore,
        position_subtile: 128,
    });
    state.next_output_by_lane = [1, 0];

    let before_hash = sim.state_hash();
    let bytes = save_to_bytes(&sim).expect("splitter state should save");
    let loaded = load_from_bytes(&bytes).expect("splitter state should load");

    assert_eq!(before_hash, loaded.state_hash());
}

#[test]
fn blocked_belt_preserves_item_order() {
    let mut sim = Simulation::new_test_world(123);
    let belts = place_belt_line(&mut sim, 1);
    let inserted = [
        item_id(&sim.world.prototypes, "iron_ore"),
        item_id(&sim.world.prototypes, "copper_ore"),
        item_id(&sim.world.prototypes, "coal"),
        item_id(&sim.world.prototypes, "stone"),
    ];

    for item_id in inserted {
        loop {
            if sim.insert_item_onto_belt(belts[0], 0, item_id).is_ok() {
                break;
            }
            sim.tick();
        }
        for _ in 0..8 {
            sim.tick();
        }
    }
    for _ in 0..200 {
        sim.tick();
    }

    let lane = &sim.belt_segment(belts[0]).unwrap().lanes[0].items;
    let downstream_to_upstream = lane
        .iter()
        .rev()
        .map(|item| item.item_id)
        .collect::<Vec<_>>();
    assert_eq!(downstream_to_upstream, inserted);
    for pair in lane.windows(2) {
        assert!(pair[1].position_subtile - pair[0].position_subtile >= BELT_ITEM_SPACING_SUBTILES);
    }
}

#[test]
fn underground_belt_pair_transfers_items_to_exit_preserving_order() {
    let mut sim = Simulation::new_test_world(123);
    let (entrance_id, exit_id) =
        place_underground_belt_pair(&mut sim, BASIC_UNDERGROUND_BELT_MAX_OFFSET, Direction::East);
    let inserted = [
        item_id(&sim.world.prototypes, "iron_ore"),
        item_id(&sim.world.prototypes, "copper_ore"),
        item_id(&sim.world.prototypes, "coal"),
    ];

    for item_id in inserted {
        loop {
            if sim.insert_item_onto_belt(entrance_id, 0, item_id).is_ok() {
                break;
            }
            sim.tick();
        }
        for _ in 0..8 {
            sim.tick();
        }
    }
    for _ in 0..200 {
        sim.tick();
    }

    assert!(
        sim.belt_segment(entrance_id).unwrap().lanes[0]
            .items
            .is_empty()
    );
    let lane = &sim.belt_segment(exit_id).unwrap().lanes[0].items;
    let downstream_to_upstream = lane
        .iter()
        .rev()
        .map(|item| item.item_id)
        .collect::<Vec<_>>();
    assert_eq!(downstream_to_upstream, inserted);
}

#[test]
fn underground_belt_does_not_pair_beyond_max_distance() {
    let mut sim = Simulation::new_test_world(123);
    let (entrance_id, exit_id) = place_underground_belt_pair(
        &mut sim,
        BASIC_UNDERGROUND_BELT_MAX_OFFSET + 1,
        Direction::East,
    );
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");

    sim.insert_item_onto_belt(entrance_id, 0, iron_ore)
        .expect("empty underground entrance should accept an item");
    for _ in 0..100 {
        sim.tick();
    }

    let entrance_lane = &sim.belt_segment(entrance_id).unwrap().lanes[0].items;
    assert_eq!(entrance_lane.len(), 1);
    assert_eq!(entrance_lane[0].item_id, iron_ore);
    assert_eq!(
        entrance_lane[0].position_subtile,
        BELT_SUBTILES_PER_TILE - 1
    );
    assert!(sim.belt_segment(exit_id).unwrap().lanes[0].items.is_empty());
}

#[test]
fn underground_belt_requires_exit_to_face_same_direction() {
    let mut sim = Simulation::new_test_world(123);
    let (entrance_id, exit_id) = place_underground_belt_pair(
        &mut sim,
        BASIC_UNDERGROUND_BELT_MAX_OFFSET,
        Direction::North,
    );
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");

    sim.insert_item_onto_belt(entrance_id, 0, iron_ore)
        .expect("empty underground entrance should accept an item");
    for _ in 0..100 {
        sim.tick();
    }

    let entrance_lane = &sim.belt_segment(entrance_id).unwrap().lanes[0].items;
    assert_eq!(entrance_lane.len(), 1);
    assert_eq!(entrance_lane[0].item_id, iron_ore);
    assert!(sim.belt_segment(exit_id).unwrap().lanes[0].items.is_empty());
}

#[test]
fn underground_belt_requires_exit_endpoint() {
    let mut sim = Simulation::new_test_world(123);
    let (entrance_id, other_entrance_id) = place_underground_belt_endpoint_pair(
        &mut sim,
        "underground_belt_entrance",
        BASIC_UNDERGROUND_BELT_MAX_OFFSET,
        Direction::East,
    );
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");

    sim.insert_item_onto_belt(entrance_id, 0, iron_ore)
        .expect("empty underground entrance should accept an item");
    for _ in 0..100 {
        sim.tick();
    }

    let entrance_lane = &sim.belt_segment(entrance_id).unwrap().lanes[0].items;
    assert_eq!(entrance_lane.len(), 1);
    assert_eq!(entrance_lane[0].item_id, iron_ore);
    assert!(
        sim.belt_segment(other_entrance_id).unwrap().lanes[0]
            .items
            .is_empty()
    );
}

#[test]
fn underground_belt_blocks_when_exit_lane_is_full() {
    let mut sim = Simulation::new_test_world(123);
    let (entrance_id, exit_id) =
        place_underground_belt_pair(&mut sim, BASIC_UNDERGROUND_BELT_MAX_OFFSET, Direction::East);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let copper_ore = item_id(&sim.world.prototypes, "copper_ore");

    {
        let exit = sim
            .entities
            .transport_belts
            .get_mut(&exit_id)
            .expect("placed underground exit should have belt state");
        for position_subtile in [0, 64, 128, 192] {
            exit.lanes[0].items.push(BeltItem {
                item_id: copper_ore,
                position_subtile,
            });
        }
    }

    sim.insert_item_onto_belt(entrance_id, 0, iron_ore)
        .expect("empty underground entrance should accept an item");
    for _ in 0..100 {
        sim.tick();
    }

    let entrance_lane = &sim.belt_segment(entrance_id).unwrap().lanes[0].items;
    assert_eq!(entrance_lane.len(), 1);
    assert_eq!(entrance_lane[0].item_id, iron_ore);
    assert_eq!(total_belt_count_for_item(&sim, copper_ore), 4);
    assert_eq!(total_belt_count_for_item(&sim, iron_ore), 1);
}

#[test]
fn removing_underground_exit_invalidates_pair_without_losing_items() {
    let mut sim = Simulation::new_test_world(123);
    let (entrance_id, exit_id) =
        place_underground_belt_pair(&mut sim, BASIC_UNDERGROUND_BELT_MAX_OFFSET, Direction::East);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");

    sim.insert_item_onto_belt(entrance_id, 0, iron_ore)
        .expect("empty underground entrance should accept an item");
    sim.remove_entity(exit_id)
        .expect("placed underground exit should be removable");
    for _ in 0..100 {
        sim.tick();
    }

    let entrance_lane = &sim.belt_segment(entrance_id).unwrap().lanes[0].items;
    assert_eq!(entrance_lane.len(), 1);
    assert_eq!(entrance_lane[0].item_id, iron_ore);
    assert_eq!(
        entrance_lane[0].position_subtile,
        BELT_SUBTILES_PER_TILE - 1
    );
    assert_eq!(total_belt_count_for_item(&sim, iron_ore), 1);
}

#[test]
fn belt_pickup_uses_front_most_item_across_lanes() {
    let iron_ore = ItemId::new(0);
    let copper_ore = ItemId::new(1);
    let mut segment = BeltSegment::new(Direction::East, 8);
    segment.lanes[0].items.push(BeltItem {
        item_id: iron_ore,
        position_subtile: 100,
    });
    segment.lanes[1].items.push(BeltItem {
        item_id: copper_ore,
        position_subtile: 200,
    });

    assert_eq!(belt_pickup_item(&segment), Some(copper_ore));
}

#[test]
fn belt_removal_uses_front_most_matching_item_across_lanes() {
    let iron_ore = ItemId::new(0);
    let mut segment = BeltSegment::new(Direction::East, 8);
    segment.lanes[0].items.push(BeltItem {
        item_id: iron_ore,
        position_subtile: 100,
    });
    segment.lanes[1].items.push(BeltItem {
        item_id: iron_ore,
        position_subtile: 200,
    });

    assert!(remove_one_item_from_belt(&mut segment, iron_ore));
    assert_eq!(segment.lanes[0].items.len(), 1);
    assert!(segment.lanes[1].items.is_empty());
}

#[test]
fn burner_drill_outputs_ore_onto_belt() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (drill_id, belt_id, _, _, _) = place_burner_drill_outputting_to_belt(&mut sim, iron_ore);
    add_fuel_to_burner_drill(&mut sim, drill_id, coal, 1);

    for _ in 0..240 {
        sim.tick();
    }

    assert_eq!(
        sim.burner_drill_state(drill_id)
            .expect("drill should expose state")
            .output_slot,
        None
    );
    assert!(
        sim.belt_segment(belt_id)
            .expect("belt should expose state")
            .lanes
            .iter()
            .any(|lane| lane.items.iter().any(|item| item.item_id == iron_ore))
    );
}

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
fn burner_drill_exports_stored_output_onto_belt_without_new_production() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (drill_id, _, x, y, before) = place_burner_drill_outputting_to_belt(&mut sim, iron_ore);
    let state = sim
        .entities
        .burner_drill_state_mut(drill_id)
        .expect("burner drill should expose state");
    state.output_slot = Some(ItemStack {
        item_id: iron_ore,
        count: 3,
    });

    sim.tick();

    assert_eq!(
        sim.burner_drill_state(drill_id)
            .expect("burner drill should expose state")
            .output_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 2,
        })
    );
    assert_eq!(total_belt_count_for_item(&sim, iron_ore), 1);
    assert_eq!(resource_amount_at(&sim.world, x, y), Some(before));
}

#[test]
fn belt_line_moves_100_items_across_20_tiles() {
    let mut sim = Simulation::new_test_world(123);
    let belts = place_belt_line(&mut sim, 20);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    feed_belt_items(&mut sim, belts[0], iron_ore, 100);

    for _ in 0..1_000 {
        sim.tick();
    }

    assert_eq!(total_belt_item_count(&sim), 100);
    assert!(
        sim.belt_segment(*belts.last().unwrap())
            .unwrap()
            .lanes
            .iter()
            .any(|lane| !lane.items.is_empty())
    );
}

#[test]
fn burner_drill_blocks_when_output_inventory_full() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (drill_id, chest_id, x, y, before) =
        place_burner_drill_outputting_to_chest(&mut sim, iron_ore);
    add_fuel_to_burner_drill(&mut sim, drill_id, coal, 1);
    fill_inventory_with(&mut sim, chest_id, coal);

    for _ in 0..240 {
        sim.tick();
    }

    let state = sim
        .burner_drill_state(drill_id)
        .expect("burner drill should expose state");
    assert_eq!(state.energy.energy_remaining_joules, 0.0);
    assert_eq!(
        state.energy.fuel_slot,
        Some(ItemStack {
            item_id: coal,
            count: 1,
        })
    );
    assert_eq!(state.mining_progress_ticks, 0);
    assert_eq!(resource_amount_at(&sim.world, x, y), Some(before));
    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        0
    );
}

#[test]
fn burner_drill_outputs_into_adjacent_chest() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (drill_id, chest_id, _, _, _) = place_burner_drill_outputting_to_chest(&mut sim, iron_ore);
    add_fuel_to_burner_drill(&mut sim, drill_id, coal, 1);

    for _ in 0..240 {
        sim.tick();
    }

    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        1
    );
    assert_eq!(
        sim.burner_drill_state(drill_id)
            .expect("burner drill should expose state")
            .output_slot,
        None
    );
}

#[test]
fn burner_drill_placed_on_coal_produces_coal() {
    let mut sim = Simulation::new_test_world(123);
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, _, _, _) = place_burner_drill_on_resource(&mut sim, coal);
    add_fuel_to_burner_drill(&mut sim, entity_id, coal, 1);

    for _ in 0..240 {
        sim.tick();
    }

    assert_eq!(
        sim.burner_drill_state(entity_id)
            .expect("burner drill should expose state")
            .output_slot,
        Some(ItemStack {
            item_id: coal,
            count: 1,
        })
    );
}

#[test]
fn burner_drill_without_resource_in_mining_area_refuses_placement() {
    let sim = Simulation::new_test_world(123);
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 2, 2);

    assert!(matches!(
        sim.can_place_entity(drill, x, y, Direction::North),
        Err(BuildError::TileBlocked { .. })
    ));
}

#[test]
fn burner_drill_hash_is_deterministic_for_same_seed_and_inputs() {
    let mut a = Simulation::new_test_world(123);
    let mut b = Simulation::new_test_world(123);
    let coal = item_id(&a.world.prototypes, "coal");
    let a_entity = place_burner_drill_on_resource(&mut a, coal).0;
    let b_entity = place_burner_drill_on_resource(&mut b, coal).0;

    for (sim, entity_id) in [(&mut a, a_entity), (&mut b, b_entity)] {
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[0] = Some(ItemStack {
            item_id: coal,
            count: 2,
        });
        sim.transfer_player_slot_to_burner_drill_fuel(entity_id, 0)
            .expect("coal should transfer to drill fuel");
    }

    for _ in 0..1000 {
        a.tick();
        b.tick();
    }

    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn furnace_smelts_iron_ore_to_iron_plate() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, iron_ore, coal);

    for _ in 0..210 {
        sim.tick();
    }

    let state = sim
        .furnace_state(entity_id)
        .expect("furnace should expose state");
    assert_eq!(state.input_slot, None);
    assert_eq!(
        state.output_slot,
        Some(ItemStack {
            item_id: iron_plate,
            count: 1,
        })
    );
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(state.energy.energy_remaining_joules, 3_685_000.0);
}

#[test]
fn furnace_does_not_smelts_without_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let entity_id = place_stone_furnace(&mut sim);
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });
    sim.transfer_player_slot_to_furnace_input(entity_id, 0)
        .expect("ore should transfer to furnace input");

    for _ in 0..210 {
        sim.tick();
    }

    let state = sim
        .furnace_state(entity_id)
        .expect("furnace should expose state");
    assert_eq!(
        state.input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        })
    );
    assert_eq!(state.output_slot, None);
    assert_eq!(state.energy.energy_remaining_joules, 0.0);
    assert_eq!(state.crafting_progress_ticks, 0);
}

#[test]
fn furnace_blocks_when_output_full() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let copper_plate = item_id(&sim.world.prototypes, "copper_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, iron_ore, coal);
    let state = sim
        .entities
        .furnace_state_mut(entity_id)
        .expect("furnace should expose state");
    state.output_slot = Some(ItemStack {
        item_id: copper_plate,
        count: 1,
    });

    for _ in 0..210 {
        sim.tick();
    }

    let state = sim
        .furnace_state(entity_id)
        .expect("furnace should expose state");
    assert_eq!(
        state.input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        })
    );
    assert_eq!(
        state.energy.fuel_slot,
        Some(ItemStack {
            item_id: coal,
            count: 1,
        })
    );
    assert_eq!(state.energy.energy_remaining_joules, 0.0);
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(
        state.output_slot.map(|stack| stack.item_id),
        Some(copper_plate)
    );
    assert_eq!(
        state.output_slot.map(|stack| stack.item_id == iron_plate),
        Some(false)
    );
}

#[test]
fn furnace_smelts_copper_ore_to_copper_plate() {
    let mut sim = Simulation::new_test_world(123);
    let copper_ore = item_id(&sim.world.prototypes, "copper_ore");
    let copper_plate = item_id(&sim.world.prototypes, "copper_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, copper_ore, coal);

    for _ in 0..210 {
        sim.tick();
    }

    assert_eq!(
        sim.furnace_state(entity_id)
            .expect("furnace should expose state")
            .output_slot,
        Some(ItemStack {
            item_id: copper_plate,
            count: 1,
        })
    );
}

#[test]
fn furnace_smelts_stone_to_stone_brick() {
    let mut sim = Simulation::new_test_world(123);
    let stone = item_id(&sim.world.prototypes, "stone");
    let stone_brick = item_id(&sim.world.prototypes, "stone_brick");
    let coal = item_id(&sim.world.prototypes, "coal");
    let recipe = recipe_id(&sim.world.prototypes, "stone_brick");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, stone, coal);

    for _ in 0..210 {
        sim.tick();
    }

    let state = sim
        .furnace_state(entity_id)
        .expect("furnace should expose state");
    assert_eq!(state.active_recipe, Some(recipe));
    assert_eq!(
        state.output_slot,
        Some(ItemStack {
            item_id: stone_brick,
            count: 1,
        })
    );
}

#[test]
fn invalid_furnace_input_is_rejected() {
    let mut sim = Simulation::new_test_world(123);
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_stone_furnace(&mut sim);
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: coal,
        count: 1,
    });

    assert_eq!(
        sim.transfer_player_slot_to_furnace_input(entity_id, 0),
        Err(FurnaceError::InvalidInput(coal))
    );
    assert_eq!(
        sim.furnace_state(entity_id)
            .expect("furnace should expose state")
            .input_slot,
        None
    );
    assert_eq!(
        sim.player_inventory.slots[0],
        Some(ItemStack {
            item_id: coal,
            count: 1,
        })
    );
}

#[test]
fn non_container_entities_reject_inventory_access() {
    let mut sim = Simulation::new_test_world(123);
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let entity_id = sim
        .place_entity(inserter, x, y, Direction::North)
        .expect("inserter should be placeable");

    assert_eq!(
        sim.entity_inventory(entity_id),
        Err(ContainerError::NotContainer(entity_id))
    );
}

#[test]
fn lab_rejects_non_science_pack_player_transfer_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let lab_id = place_lab(&mut sim);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });

    assert_eq!(
        sim.transfer_player_slot_to_entity(lab_id, 0),
        Err(ContainerError::InvalidItem(iron_plate))
    );
    assert_eq!(
        sim.player_inventory.slots[0],
        Some(ItemStack {
            item_id: iron_plate,
            count: 1,
        })
    );
    assert_eq!(
        sim.entity_inventory(lab_id)
            .expect("lab should expose inventory")
            .count(iron_plate),
        0
    );
}

#[test]
fn player_starts_on_walkable_generated_tile() {
    let sim = Simulation::new_test_world(123);
    let (x, y) = sim.player.tile_position();
    let tile = sim
        .world
        .tile_at(x, y)
        .expect("player start should be in a generated chunk");

    assert!(tile.collision.walkable);
    assert!(sim.can_player_occupy_tile(x, y));
}

#[test]
fn player_cannot_move_into_water() {
    let mut sim = Simulation::new_test_world(123);
    let (start, delta) = first_player_approach_to_water(&sim);
    let before = PlayerState::centered_on_tile(start.0, start.1);
    sim.player = before;

    sim.move_player_by_tiles(delta.0, delta.1);

    assert_eq!(sim.player, before);
}

#[test]
fn player_cannot_move_into_unloaded_tiles() {
    let mut sim = Simulation::new_test_world(123);
    let (start, delta) = first_player_approach_to_unloaded_tile(&sim);
    let before = PlayerState::centered_on_tile(start.0, start.1);
    sim.player = before;

    sim.move_player_by_tiles(delta.0, delta.1);

    assert_eq!(sim.player, before);
}

#[test]
fn player_cannot_move_into_occupied_entity_tile() {
    let mut sim = Simulation::new_test_world(123);
    let (start, delta) = first_player_approach_to_occupied_tile(&mut sim);
    let before = PlayerState::centered_on_tile(start.0, start.1);
    sim.player = before;

    sim.move_player_by_tiles(delta.0, delta.1);

    assert_eq!(sim.player, before);
}

#[test]
fn player_axis_separated_movement_slides_along_blocked_edges() {
    let mut sim = Simulation::new_test_world(123);
    let (start, expected) = first_player_slide_fixture(&mut sim);
    sim.player = PlayerState::centered_on_tile(start.0, start.1);

    sim.move_player_by_tiles(1.0, 1.0);

    assert_eq!(sim.player.tile_position(), expected);
}

#[test]
fn inventory_merges_stacks_until_stack_size() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let iron_plate = item_id(&catalog, "iron_plate");
    let mut inventory = Inventory::with_slot_count(2);

    inventory
        .insert(&catalog, iron_plate, 99)
        .expect("first insert should fit");
    inventory
        .insert(&catalog, iron_plate, 2)
        .expect("second insert should fill existing stack first");

    assert_eq!(
        inventory.slots,
        vec![
            Some(ItemStack {
                item_id: iron_plate,
                count: 100,
            }),
            Some(ItemStack {
                item_id: iron_plate,
                count: 1,
            }),
        ]
    );
}

#[test]
fn inventory_rejects_insert_when_full() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let iron_plate = item_id(&catalog, "iron_plate");
    let coal = item_id(&catalog, "coal");
    let mut inventory = Inventory::with_slot_count(1);

    inventory
        .insert(&catalog, iron_plate, 100)
        .expect("initial stack should fit");
    let before = inventory.clone();

    assert_eq!(
        inventory.insert(&catalog, coal, 1),
        Err(InventoryError::InsufficientSpace)
    );
    assert_eq!(inventory, before);
}

#[test]
fn inventory_acceptance_reports_unknown_items() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let inventory = Inventory::with_slot_count(1);
    let unknown_item = ItemId::new(catalog.items.len() as u16);

    assert_eq!(
        ensure_inventory_can_accept(
            &catalog,
            &inventory,
            ItemStack {
                item_id: unknown_item,
                count: 1,
            },
        ),
        Err(ContainerError::UnknownItem)
    );
}

#[test]
fn inventory_remove_is_atomic() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let iron_plate = item_id(&catalog, "iron_plate");
    let mut inventory = Inventory::with_slot_count(1);

    inventory
        .insert(&catalog, iron_plate, 3)
        .expect("initial stack should fit");
    let before = inventory.clone();

    assert_eq!(
        inventory.remove(iron_plate, 4),
        Err(InventoryError::InsufficientItems)
    );
    assert_eq!(inventory, before);
    assert_eq!(inventory.count(iron_plate), 3);
}

#[test]
fn new_simulations_start_with_automation_locked_and_no_progress() {
    let sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");

    assert!(!sim.is_technology_unlocked(automation));
    assert_eq!(sim.technology_progress(automation), Some(0));
    assert_eq!(sim.research.active, None);
}

#[test]
fn technology_unlocked_recipes_are_unavailable_until_researched() {
    let sim = Simulation::new_test_world(123);
    let assembling_machine = recipe_id(&sim.world.prototypes, "assembling_machine");
    let iron_gear_wheel = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let available_crafting = sim
        .available_recipes(CraftingCategory::Crafting)
        .into_iter()
        .map(|recipe| recipe.id)
        .collect::<Vec<_>>();

    assert!(!sim.is_recipe_unlocked(assembling_machine));
    assert!(sim.is_recipe_unlocked(iron_gear_wheel));
    assert!(!available_crafting.contains(&assembling_machine));
    assert!(available_crafting.contains(&iron_gear_wheel));
}

#[test]
fn locked_manual_craft_fails_without_consuming_ingredients() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "assembling_machine");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let electronic_circuit = item_id(&sim.world.prototypes, "electronic_circuit");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 9)
        .expect("test inventory should accept iron plates");
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_gear_wheel, 5)
        .expect("test inventory should accept gears");
    sim.player_inventory
        .insert(&sim.world.prototypes, electronic_circuit, 3)
        .expect("test inventory should accept circuits");
    let before = sim.player_inventory.clone();

    assert_eq!(
        sim.start_manual_craft(recipe),
        Err(CraftingError::RecipeLocked(recipe))
    );
    assert_eq!(sim.player_inventory, before);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn locked_assembler_recipe_selection_fails_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "assembling_machine");
    let before = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state")
        .clone();

    assert_eq!(
        sim.select_assembler_recipe(assembler_id, recipe),
        Err(AssemblerError::RecipeLocked(recipe))
    );
    assert_eq!(
        sim.can_select_assembler_recipe(assembler_id, recipe),
        Ok(false)
    );
    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state"),
        &before
    );
}

#[test]
fn research_progress_unlocks_automation_recipe_effects() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let assembling_machine = recipe_id(&sim.world.prototypes, "assembling_machine");

    sim.select_research(automation)
        .expect("automation should be selectable");
    assert_eq!(
        sim.add_research_units(9),
        Ok(ResearchProgressResult::InProgress {
            technology_id: automation,
            progress_units: 9,
            required_units: 10,
        })
    );
    assert!(!sim.is_technology_unlocked(automation));
    assert!(!sim.is_recipe_unlocked(assembling_machine));
    assert_eq!(
        sim.add_research_units(1),
        Ok(ResearchProgressResult::Completed {
            technology_id: automation
        })
    );

    assert!(sim.is_technology_unlocked(automation));
    assert_eq!(sim.technology_progress(automation), Some(10));
    assert_eq!(sim.research.active, None);
    assert!(sim.is_recipe_unlocked(assembling_machine));
}

#[test]
fn zero_research_units_return_current_progress_without_advancing() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");

    sim.select_research(automation)
        .expect("automation should be selectable");

    assert_eq!(
        sim.add_research_units(0),
        Ok(ResearchProgressResult::InProgress {
            technology_id: automation,
            progress_units: 0,
            required_units: 10,
        })
    );
    assert_eq!(sim.technology_progress(automation), Some(0));
    assert!(!sim.is_technology_unlocked(automation));
}

#[test]
fn lab_consumes_science_and_increases_research_progress() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let science_pack = item_id(&sim.world.prototypes, "automation_science_pack");
    let (chest_id, inserter_id, lab_id) = place_chest_inserter_lab_line(&mut sim);
    sim.select_research(automation)
        .expect("automation should be selectable");
    sim.entity_inventory_mut(chest_id)
        .expect("chest should expose inventory")
        .slots[0] = Some(ItemStack {
        item_id: science_pack,
        count: 1,
    });

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        sim.entity_inventory(lab_id)
            .expect("lab should expose inventory")
            .count(science_pack),
        1
    );

    let progress_after_insert = sim
        .lab_state(lab_id)
        .expect("lab should expose state")
        .progress_ticks;
    for _ in progress_after_insert..599 {
        sim.tick();
    }

    assert_eq!(sim.technology_progress(automation), Some(0));
    assert_eq!(
        sim.entity_inventory(lab_id)
            .expect("lab should expose inventory")
            .count(science_pack),
        1
    );

    sim.tick();

    assert_eq!(
        sim.entity_inventory(lab_id)
            .expect("lab should expose inventory")
            .count(science_pack),
        0
    );
    assert_eq!(sim.technology_progress(automation), Some(1));
    assert!(!sim.is_technology_unlocked(automation));
}

#[test]
fn multiple_labs_contribute_research_units_in_parallel() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let science_pack = item_id(&sim.world.prototypes, "automation_science_pack");
    let first_lab = place_lab(&mut sim);
    let second_lab = place_lab(&mut sim);
    sim.select_research(automation)
        .expect("automation should be selectable");
    for lab_id in [first_lab, second_lab] {
        sim.entity_inventory_mut(lab_id)
            .expect("lab should expose inventory")
            .slots[0] = Some(ItemStack {
            item_id: science_pack,
            count: 1,
        });
    }

    for _ in 0..600 {
        sim.tick();
    }

    assert_eq!(sim.technology_progress(automation), Some(2));
    assert_eq!(
        sim.entity_inventory(first_lab)
            .expect("lab should expose inventory")
            .count(science_pack),
        0
    );
    assert_eq!(
        sim.entity_inventory(second_lab)
            .expect("lab should expose inventory")
            .count(science_pack),
        0
    );
}

#[test]
fn no_active_research_leaves_labs_idle() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let science_pack = item_id(&sim.world.prototypes, "automation_science_pack");
    let lab_id = place_lab(&mut sim);
    sim.entity_inventory_mut(lab_id)
        .expect("lab should expose inventory")
        .slots[0] = Some(ItemStack {
        item_id: science_pack,
        count: 1,
    });

    for _ in 0..1_000 {
        sim.tick();
    }

    let lab = sim.lab_state(lab_id).expect("lab should expose state");
    assert_eq!(lab.active_technology, None);
    assert_eq!(lab.progress_ticks, 0);
    assert_eq!(lab.required_ticks, 0);
    assert_eq!(lab.inventory.count(science_pack), 1);
    assert_eq!(sim.technology_progress(automation), Some(0));
}

#[test]
fn lab_completed_research_unlocks_recipe() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let science_pack = item_id(&sim.world.prototypes, "automation_science_pack");
    let assembling_machine = recipe_id(&sim.world.prototypes, "assembling_machine");
    let lab_id = place_lab(&mut sim);
    sim.select_research(automation)
        .expect("automation should be selectable");
    sim.entity_inventory_mut(lab_id)
        .expect("lab should expose inventory")
        .slots[0] = Some(ItemStack {
        item_id: science_pack,
        count: 10,
    });

    for _ in 0..6_000 {
        sim.tick();
    }

    assert!(sim.is_technology_unlocked(automation));
    assert!(sim.is_recipe_unlocked(assembling_machine));
    assert_eq!(sim.research.active, None);
    assert_eq!(sim.technology_progress(automation), Some(10));
    assert_eq!(
        sim.entity_inventory(lab_id)
            .expect("lab should expose inventory")
            .count(science_pack),
        0
    );
}

#[test]
fn after_automation_unlock_assembling_machine_can_be_manually_crafted() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let recipe = recipe_id(&sim.world.prototypes, "assembling_machine");
    let assembling_machine = item_id(&sim.world.prototypes, "assembling_machine");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let electronic_circuit = item_id(&sim.world.prototypes, "electronic_circuit");
    sim.select_research(automation)
        .expect("automation should be selectable");
    sim.add_research_units(10)
        .expect("automation research should complete");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 9)
        .expect("test inventory should accept iron plates");
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_gear_wheel, 5)
        .expect("test inventory should accept gears");
    sim.player_inventory
        .insert(&sim.world.prototypes, electronic_circuit, 3)
        .expect("test inventory should accept circuits");

    sim.start_manual_craft(recipe)
        .expect("unlocked recipe should craft with enough ingredients");
    for _ in 0..30 {
        sim.tick();
    }

    assert_eq!(sim.player_inventory.count(assembling_machine), 1);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn research_progress_participates_in_state_hash_deterministically() {
    let mut first = Simulation::new_test_world(123);
    let mut second = Simulation::new_test_world(123);
    let automation = technology_id(&first.world.prototypes, "automation");
    let initial_hash = first.state_hash();

    first
        .select_research(automation)
        .expect("automation should be selectable");
    first
        .add_research_units(4)
        .expect("research should accept units");
    second
        .select_research(automation)
        .expect("automation should be selectable");
    second
        .add_research_units(4)
        .expect("research should accept units");

    assert_ne!(first.state_hash(), initial_hash);
    assert_eq!(first.state_hash(), second.state_hash());
}

#[test]
fn crafting_consumes_ingredients_and_outputs_product() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 2)
        .expect("test inventory should accept ingredients");

    sim.start_manual_craft(recipe)
        .expect("craft should start with enough ingredients");

    assert_eq!(sim.player_inventory.count(iron_plate), 0);
    assert_eq!(sim.player_inventory.count(iron_gear_wheel), 0);
    assert_eq!(
        sim.crafting_queue.entries.front(),
        Some(&CraftingJob {
            recipe_id: recipe,
            remaining_ticks: 30,
        })
    );

    for _ in 0..30 {
        sim.tick();
    }

    assert_eq!(sim.player_inventory.count(iron_gear_wheel), 1);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn crafting_does_not_start_without_ingredients() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 1)
        .expect("test inventory should accept partial ingredients");
    let before = sim.player_inventory.clone();

    assert_eq!(
        sim.start_manual_craft(recipe),
        Err(CraftingError::InsufficientIngredients)
    );
    assert_eq!(sim.player_inventory, before);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn crafting_product_appears_only_after_configured_ticks() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "transport_belt");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let transport_belt = item_id(&sim.world.prototypes, "transport_belt");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 1)
        .expect("test inventory should accept iron plate");
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_gear_wheel, 1)
        .expect("test inventory should accept gear");

    sim.start_manual_craft(recipe)
        .expect("craft should start with enough ingredients");
    for _ in 0..29 {
        sim.tick();
    }

    assert_eq!(sim.player_inventory.count(transport_belt), 0);
    assert_eq!(
        sim.crafting_queue
            .entries
            .front()
            .map(|job| job.remaining_ticks),
        Some(1)
    );

    sim.tick();

    assert_eq!(sim.player_inventory.count(transport_belt), 2);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn full_inventory_pauses_completed_craft_until_space_is_freed() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let coal = item_id(&sim.world.prototypes, "coal");
    sim.player_inventory = Inventory::with_slot_count(1);
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 2)
        .expect("single stack should fit ingredients");
    sim.start_manual_craft(recipe)
        .expect("craft should start with enough ingredients");
    sim.player_inventory
        .insert(&sim.world.prototypes, coal, 100)
        .expect("blocking stack should fill inventory");

    for _ in 0..30 {
        sim.tick();
    }

    assert_eq!(sim.player_inventory.count(iron_gear_wheel), 0);
    assert_eq!(sim.crafting_queue.entries.len(), 1);
    assert_eq!(
        sim.crafting_queue
            .entries
            .front()
            .map(|job| job.remaining_ticks),
        Some(0)
    );

    sim.tick();
    assert_eq!(sim.player_inventory.count(iron_gear_wheel), 0);
    assert_eq!(sim.crafting_queue.entries.len(), 1);

    sim.player_inventory
        .remove(coal, 100)
        .expect("test should be able to free blocking stack");
    sim.tick();

    assert_eq!(sim.player_inventory.count(iron_gear_wheel), 1);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn smelting_recipes_cannot_be_manually_crafted() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_plate");
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_ore, 1)
        .expect("test inventory should accept ore");

    assert_eq!(
        sim.start_manual_craft(recipe),
        Err(CraftingError::NotManualRecipe(recipe))
    );
    assert_eq!(sim.player_inventory.count(iron_ore), 1);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn base_catalog_contains_expected_manually_craftable_recipes() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let recipe_names = [
        "stone_furnace",
        "burner_mining_drill",
        "transport_belt",
        "inserter",
        "assembling_machine",
        "lab",
        "automation_science_pack",
    ];

    for recipe_name in recipe_names {
        let recipe = catalog
            .recipes
            .iter()
            .find(|recipe| recipe.name == recipe_name)
            .unwrap_or_else(|| panic!("missing recipe {recipe_name:?}"));
        assert!(
            matches!(
                recipe.category,
                CraftingCategory::Crafting | CraftingCategory::Manual
            ),
            "{recipe_name} should be manually craftable"
        );
    }
}

#[test]
fn player_starts_with_drill_and_furnace_only() {
    let sim = Simulation::new_test_world(123);
    let burner_mining_drill = item_id(&sim.world.prototypes, "burner_mining_drill");
    let stone_furnace = item_id(&sim.world.prototypes, "stone_furnace");
    let occupied_slots = sim
        .player_inventory
        .slots
        .iter()
        .filter_map(|slot| *slot)
        .collect::<Vec<_>>();

    assert_eq!(
        sim.player_inventory.slots.len(),
        PLAYER_INVENTORY_SLOT_COUNT
    );
    assert_eq!(sim.player_inventory.count(burner_mining_drill), 1);
    assert_eq!(sim.player_inventory.count(stone_furnace), 1);
    assert_eq!(occupied_slots.len(), 2);
    assert_eq!(
        occupied_slots.iter().map(|stack| stack.count).sum::<u16>(),
        2
    );
}

#[test]
fn inventory_insert_never_exceeds_item_stack_size() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let copper_cable = item_id(&catalog, "copper_cable");
    let mut inventory = Inventory::with_slot_count(2);

    inventory
        .insert(&catalog, copper_cable, 201)
        .expect("two cable stacks should fit");

    assert_eq!(inventory.count(copper_cable), 201);
    for stack in inventory.slots.iter().flatten() {
        assert!(stack.count <= 200);
    }
}

#[test]
fn zero_count_insert_and_remove_are_no_ops() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let unknown_item = ItemId::new(u16::MAX);
    let mut inventory = Inventory::with_slot_count(1);

    inventory
        .insert(&catalog, unknown_item, 0)
        .expect("zero-count insert should be a no-op");
    inventory
        .remove(unknown_item, 0)
        .expect("zero-count remove should be a no-op");

    assert_eq!(inventory.slots, vec![None]);
}

#[test]
fn inserter_moves_item_from_chest_to_furnace() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (chest_id, inserter_id, furnace_id) = place_chest_inserter_furnace_line(&mut sim);

    sim.entity_inventory_mut(chest_id)
        .expect("chest should have inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        0
    );
    assert_eq!(
        sim.furnace_state(furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1
        })
    );
    assert!(matches!(
        sim.inserter_state(inserter_id)
            .expect("inserter should have state"),
        InserterState::WaitingForItem | InserterState::Dropping { .. }
    ));
    assert!(!matches!(
        sim.inserter_state(inserter_id)
            .expect("inserter should have state"),
        InserterState::Holding { .. }
    ));
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
}

#[test]
fn inserter_moves_fuel_from_chest_to_furnace_fuel_slot() {
    let mut sim = Simulation::new_test_world(123);
    let coal = item_id(&sim.world.prototypes, "coal");
    let (chest_id, inserter_id, furnace_id) = place_chest_inserter_furnace_line(&mut sim);

    sim.entity_inventory_mut(chest_id)
        .expect("chest should have inventory")
        .slots[0] = Some(ItemStack {
        item_id: coal,
        count: 1,
    });

    run_inserter_until_idle(&mut sim, inserter_id);

    let furnace = sim
        .furnace_state(furnace_id)
        .expect("furnace should have state");
    assert_eq!(furnace.input_slot, None);
    assert_eq!(
        furnace.energy.fuel_slot,
        Some(ItemStack {
            item_id: coal,
            count: 1,
        })
    );
}

#[test]
fn inserter_waits_when_target_full() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let stack_size =
        item_stack_size(&sim.world.prototypes, iron_ore).expect("iron ore should have stack size");
    let (chest_id, inserter_id, furnace_id) = place_chest_inserter_furnace_line(&mut sim);

    sim.entity_inventory_mut(chest_id)
        .expect("chest should have inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });
    sim.entities
        .furnace_state_mut(furnace_id)
        .expect("furnace should have state")
        .input_slot = Some(ItemStack {
        item_id: iron_ore,
        count: stack_size,
    });

    for _ in 0..BASIC_INSERTER_PICKUP_TICKS + BASIC_INSERTER_DROP_TICKS + 10 {
        sim.tick();
    }

    assert_eq!(
        sim.inserter_state(inserter_id)
            .expect("inserter should have state"),
        &InserterState::WaitingForItem
    );
    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        1
    );
    assert_eq!(
        sim.furnace_state(furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: stack_size
        })
    );
    assert!(!matches!(
        sim.inserter_state(inserter_id)
            .expect("inserter should have state"),
        InserterState::Holding { .. }
    ));
    assert_eq!(
        total_item_count_in_sim(&sim, iron_ore),
        u32::from(stack_size) + 1
    );
}

#[test]
fn inserter_preserves_item_count() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (chest_id, _inserter_id, _furnace_id) = place_chest_inserter_furnace_line(&mut sim);

    sim.entity_inventory_mut(chest_id)
        .expect("chest should have inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 3,
    });

    let ticks = (BASIC_INSERTER_PICKUP_TICKS + BASIC_INSERTER_DROP_TICKS + 5) * 3;
    for _ in 0..ticks {
        sim.tick();
        assert_eq!(total_item_count_in_sim(&sim, iron_ore), 3);
    }
}

#[test]
fn inserter_moves_item_from_belt_to_furnace() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (belt_id, inserter_id, furnace_id) = place_belt_inserter_furnace_line(&mut sim);

    sim.insert_item_onto_belt(belt_id, 0, iron_ore)
        .expect("belt should accept ore");

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(total_belt_count_for_item(&sim, iron_ore), 0);
    assert_eq!(
        sim.furnace_state(furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1
        })
    );
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
}

#[test]
fn inserter_moves_furnace_output_to_chest() {
    let mut sim = Simulation::new_test_world(123);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let (furnace_id, inserter_id, chest_id) = place_furnace_inserter_chest_line(&mut sim);

    sim.entities
        .furnace_state_mut(furnace_id)
        .expect("furnace should have state")
        .output_slot = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        sim.furnace_state(furnace_id)
            .expect("furnace should have state")
            .output_slot,
        None
    );
    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_plate),
        1
    );
    assert_eq!(total_item_count_in_sim(&sim, iron_plate), 1);
}

#[test]
fn inserter_moves_furnace_output_to_belt() {
    let mut sim = Simulation::new_test_world(123);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let (furnace_id, inserter_id, _belt_id) = place_furnace_inserter_belt_line(&mut sim);

    sim.entities
        .furnace_state_mut(furnace_id)
        .expect("furnace should have state")
        .output_slot = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        sim.furnace_state(furnace_id)
            .expect("furnace should have state")
            .output_slot,
        None
    );
    assert_eq!(total_belt_count_for_item(&sim, iron_plate), 1);
    assert_eq!(total_item_count_in_sim(&sim, iron_plate), 1);
}

#[test]
fn inserter_uses_rotated_direction_for_pickup_and_drop() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (x, y) = place_powered_fixture_origin(&mut sim, 4, 2, (1, 2));

    let chest_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 1, y, Direction::North)
        .expect("inserter should be placeable");
    let furnace_id = sim
        .place_entity(furnace, x + 2, y, Direction::North)
        .expect("furnace should be placeable");
    sim.entity_inventory_mut(chest_id)
        .expect("chest should have inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });

    for _ in 0..BASIC_INSERTER_PICKUP_TICKS + 2 {
        sim.tick();
    }
    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        1
    );
    assert_eq!(
        sim.furnace_state(furnace_id)
            .expect("furnace should have state")
            .input_slot,
        None
    );

    sim.rotate_entity(inserter_id, Direction::East)
        .expect("inserter should rotate");
    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        0
    );
    assert_eq!(
        sim.furnace_state(furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1
        })
    );
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
}

#[test]
fn fast_inserter_transfers_faster_than_basic() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (x, y) = place_powered_fixture_origin(&mut sim, 4, 5, (1, 2));

    let (basic_source, _basic_inserter, basic_target) =
        place_chest_inserter_furnace_line_at(&mut sim, "inserter", x, y);
    let (fast_source, _fast_inserter, fast_target) =
        place_chest_inserter_furnace_line_at(&mut sim, "fast_inserter", x, y + 3);

    sim.entity_inventory_mut(basic_source)
        .expect("basic source chest should have inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });
    sim.entity_inventory_mut(fast_source)
        .expect("fast source chest should have inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });

    for _ in 0..20 {
        sim.tick();
    }

    assert_eq!(
        sim.furnace_state(fast_target)
            .expect("fast target should be a furnace")
            .input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        })
    );
    assert_eq!(
        sim.furnace_state(basic_target)
            .expect("basic target should be a furnace")
            .input_slot,
        None
    );
    assert_eq!(
        sim.entity_inventory(basic_source)
            .expect("basic source chest should have inventory")
            .count(iron_ore),
        1
    );
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 2);
}

#[test]
fn long_handed_inserter_uses_two_tile_pickup_and_drop() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (chest_id, inserter_id, furnace_id) =
        place_two_tile_chest_inserter_furnace_line(&mut sim, "long_handed_inserter");

    sim.entity_inventory_mut(chest_id)
        .expect("chest should have inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        0
    );
    assert_eq!(
        sim.furnace_state(furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        })
    );
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
}

#[test]
fn basic_inserter_does_not_reach_long_handed_positions() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (chest_id, inserter_id, furnace_id) =
        place_two_tile_chest_inserter_furnace_line(&mut sim, "inserter");

    sim.entity_inventory_mut(chest_id)
        .expect("chest should have inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });

    for _ in 0..inserter_cycle_tick_budget(&sim, inserter_id) {
        sim.tick();
    }

    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        1
    );
    assert_eq!(
        sim.furnace_state(furnace_id)
            .expect("furnace should have state")
            .input_slot,
        None
    );
    assert_eq!(
        sim.inserter_state(inserter_id)
            .expect("inserter should have state"),
        &InserterState::WaitingForItem
    );
}

#[test]
fn inserter_holding_item_does_not_duplicate_when_target_becomes_blocked() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let copper_ore = item_id(&sim.world.prototypes, "copper_ore");
    let stack_size = item_stack_size(&sim.world.prototypes, copper_ore)
        .expect("copper ore should have stack size");
    let (chest_id, inserter_id, furnace_id) = place_chest_inserter_furnace_line(&mut sim);

    sim.entity_inventory_mut(chest_id)
        .expect("chest should have inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });

    run_inserter_until_holding(&mut sim, inserter_id);
    sim.entities
        .furnace_state_mut(furnace_id)
        .expect("furnace should have state")
        .input_slot = Some(ItemStack {
        item_id: copper_ore,
        count: stack_size,
    });

    for _ in 0..inserter_cycle_tick_budget(&sim, inserter_id) * 3 {
        sim.tick();
    }

    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        0
    );
    assert_eq!(
        sim.furnace_state(furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(ItemStack {
            item_id: copper_ore,
            count: stack_size,
        })
    );
    assert_eq!(
        sim.inserter_state(inserter_id)
            .expect("inserter should have state"),
        &InserterState::Holding {
            item: ItemStack {
                item_id: iron_ore,
                count: 1,
            },
        }
    );
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
    assert_eq!(
        total_item_count_in_sim(&sim, copper_ore),
        u32::from(stack_size)
    );

    sim.entities
        .furnace_state_mut(furnace_id)
        .expect("furnace should have state")
        .input_slot = None;
    sim.tick();

    assert_eq!(
        sim.furnace_state(furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        })
    );
    assert!(!matches!(
        sim.inserter_state(inserter_id)
            .expect("inserter should have state"),
        InserterState::Holding { .. }
    ));
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
}

fn first_resource_tile(world: &WorldSim) -> (i32, i32, ResourceCell) {
    for chunk in world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            if let Some(resource) = tile.resource {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                return (
                    chunk.coord.x * CHUNK_SIZE + local_x,
                    chunk.coord.y * CHUNK_SIZE + local_y,
                    resource,
                );
            }
        }
    }

    panic!("expected at least one resource tile");
}

fn first_resource_tile_for_item(world: &WorldSim, resource_item: ItemId) -> (i32, i32, u32) {
    for chunk in world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            let Some(resource) = tile.resource else {
                continue;
            };

            if resource.resource_item != resource_item {
                continue;
            }

            let (x, y) = tile_coord(chunk, index);
            return (x, y, resource.amount);
        }
    }

    panic!("expected at least one resource tile for {resource_item:?}");
}

fn straight_belt_throughput_over_one_second(sim: &mut Simulation, belt_name: &str) -> u32 {
    let belts = place_named_belt_line(
        sim,
        belt_name,
        THROUGHPUT_UPSTREAM_LEN + THROUGHPUT_DOWNSTREAM_LEN,
    );
    let split = THROUGHPUT_UPSTREAM_LEN as usize;
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    seed_saturated_belts(sim, &belts[..split], iron_ore);

    for _ in 0..THROUGHPUT_TEST_TICKS {
        sim.tick();
    }

    total_item_count_on_belts(sim, &belts[split..], iron_ore)
}

fn underground_belt_throughput_over_one_second(
    sim: &mut Simulation,
    belt_name: &str,
    entrance_name: &str,
    exit_name: &str,
    max_distance: u8,
) -> u32 {
    let (seeded, measured) =
        place_throughput_underground_pair(sim, belt_name, entrance_name, exit_name, max_distance);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    seed_saturated_belts(sim, &seeded, iron_ore);

    for _ in 0..THROUGHPUT_TEST_TICKS {
        sim.tick();
    }

    total_item_count_on_belts(sim, &measured, iron_ore)
}

fn splitter_throughput_over_one_second(
    sim: &mut Simulation,
    belt_name: &str,
    splitter_name: &str,
) -> u32 {
    let (input, splitter_id, outputs) =
        place_throughput_splitter_fixture(sim, belt_name, splitter_name);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    seed_saturated_belts(sim, &input, iron_ore);
    seed_saturated_splitter_input(sim, splitter_id, 0, iron_ore);

    for _ in 0..THROUGHPUT_TEST_TICKS {
        sim.tick();
    }

    total_item_count_on_belts(sim, &outputs, iron_ore)
}

fn place_belt_line(sim: &mut Simulation, length: i32) -> Vec<EntityId> {
    place_named_belt_line(sim, "transport_belt", length)
}

fn place_named_belt_line(sim: &mut Simulation, belt_name: &str, length: i32) -> Vec<EntityId> {
    let belt = entity_id_by_name(&sim.world.prototypes, belt_name);
    for (x, y) in all_tile_coords(&sim.world) {
        if (0..length).all(|offset| {
            sim.can_place_entity(belt, x + offset, y, Direction::East)
                .is_ok()
        }) {
            return (0..length)
                .map(|offset| {
                    sim.place_entity(belt, x + offset, y, Direction::East)
                        .expect("validated belt line tile should be placeable")
                })
                .collect();
        }
    }

    panic!("expected placeable belt line of length {length}");
}

fn place_throughput_underground_pair(
    sim: &mut Simulation,
    belt_name: &str,
    entrance_name: &str,
    exit_name: &str,
    max_distance: u8,
) -> (Vec<EntityId>, Vec<EntityId>) {
    let belt = entity_id_by_name(&sim.world.prototypes, belt_name);
    let entrance = entity_id_by_name(&sim.world.prototypes, entrance_name);
    let exit = entity_id_by_name(&sim.world.prototypes, exit_name);
    let underground_offset = i32::from(max_distance) + 1;

    for (x, y) in all_tile_coords(&sim.world) {
        let entrance_x = x + THROUGHPUT_UPSTREAM_LEN;
        let exit_x = entrance_x + underground_offset;
        let input_tiles = (0..THROUGHPUT_UPSTREAM_LEN)
            .map(|offset| (x + offset, y))
            .collect::<Vec<_>>();
        let output_tiles = (1..=THROUGHPUT_DOWNSTREAM_LEN)
            .map(|offset| (exit_x + offset, y))
            .collect::<Vec<_>>();

        if input_tiles.iter().any(|(tile_x, tile_y)| {
            sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                .is_err()
        }) || sim
            .can_place_entity(entrance, entrance_x, y, Direction::East)
            .is_err()
            || sim
                .can_place_entity(exit, exit_x, y, Direction::East)
                .is_err()
            || output_tiles.iter().any(|(tile_x, tile_y)| {
                sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .is_err()
            })
        {
            continue;
        }

        let mut seeded = input_tiles
            .iter()
            .map(|(tile_x, tile_y)| {
                sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .expect("validated throughput input belt should be placeable")
            })
            .collect::<Vec<_>>();
        seeded.push(
            sim.place_entity(entrance, entrance_x, y, Direction::East)
                .expect("validated throughput underground entrance should be placeable"),
        );
        let mut measured = vec![
            sim.place_entity(exit, exit_x, y, Direction::East)
                .expect("validated throughput underground exit should be placeable"),
        ];
        measured.extend(output_tiles.iter().map(|(tile_x, tile_y)| {
            sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                .expect("validated throughput output belt should be placeable")
        }));

        return (seeded, measured);
    }

    panic!("expected placeable throughput underground fixture for {entrance_name}");
}

fn place_throughput_splitter_fixture(
    sim: &mut Simulation,
    belt_name: &str,
    splitter_name: &str,
) -> (Vec<EntityId>, EntityId, Vec<EntityId>) {
    let belt = entity_id_by_name(&sim.world.prototypes, belt_name);
    let splitter = entity_id_by_name(&sim.world.prototypes, splitter_name);

    for (x, y) in all_tile_coords(&sim.world) {
        let splitter_x = x + THROUGHPUT_UPSTREAM_LEN;
        let input_tiles = (0..THROUGHPUT_UPSTREAM_LEN)
            .map(|offset| (x + offset, y))
            .collect::<Vec<_>>();
        let output0_tiles = (1..=THROUGHPUT_DOWNSTREAM_LEN)
            .map(|offset| (splitter_x + offset, y))
            .collect::<Vec<_>>();
        let output1_tiles = (1..=THROUGHPUT_DOWNSTREAM_LEN)
            .map(|offset| (splitter_x + offset, y + 1))
            .collect::<Vec<_>>();

        if input_tiles.iter().any(|(tile_x, tile_y)| {
            sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                .is_err()
        }) || sim
            .can_place_entity(splitter, splitter_x, y, Direction::East)
            .is_err()
            || output0_tiles.iter().any(|(tile_x, tile_y)| {
                sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .is_err()
            })
            || output1_tiles.iter().any(|(tile_x, tile_y)| {
                sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .is_err()
            })
        {
            continue;
        }

        let input = input_tiles
            .iter()
            .map(|(tile_x, tile_y)| {
                sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .expect("validated splitter throughput input belt should be placeable")
            })
            .collect::<Vec<_>>();
        let splitter_id = sim
            .place_entity(splitter, splitter_x, y, Direction::East)
            .expect("validated throughput splitter should be placeable");
        let mut outputs = output0_tiles
            .iter()
            .map(|(tile_x, tile_y)| {
                sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .expect("validated splitter throughput output belt should be placeable")
            })
            .collect::<Vec<_>>();
        outputs.extend(output1_tiles.iter().map(|(tile_x, tile_y)| {
            sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                .expect("validated splitter throughput output belt should be placeable")
        }));

        return (input, splitter_id, outputs);
    }

    panic!("expected placeable throughput splitter fixture for {splitter_name}");
}

fn seed_saturated_belts(sim: &mut Simulation, belts: &[EntityId], item_id: ItemId) {
    for entity_id in belts {
        let segment = sim
            .entities
            .transport_belts
            .get_mut(entity_id)
            .expect("throughput fixture should contain belt state");
        seed_saturated_lane(&mut segment.lanes[0], item_id, &[0, 64, 128, 192]);
        seed_saturated_lane(&mut segment.lanes[1], item_id, &[32, 96, 160, 224]);
    }
}

fn seed_saturated_splitter_input(
    sim: &mut Simulation,
    splitter_id: EntityId,
    input_port: usize,
    item_id: ItemId,
) {
    let state = sim
        .entities
        .splitters
        .get_mut(&splitter_id)
        .expect("throughput fixture should contain splitter state");
    let input_lanes = state
        .input_lanes
        .get_mut(input_port)
        .expect("throughput splitter input port should exist");
    seed_saturated_lane(&mut input_lanes[0], item_id, &[0, 64, 128, 192]);
    seed_saturated_lane(&mut input_lanes[1], item_id, &[32, 96, 160, 224]);
}

fn seed_saturated_lane(lane: &mut BeltLane, item_id: ItemId, positions: &[u16]) {
    lane.items.clear();
    lane.items
        .extend(positions.iter().map(|position_subtile| BeltItem {
            item_id,
            position_subtile: *position_subtile,
        }));
}

struct SplitterFixture {
    input0: EntityId,
    input1: EntityId,
    splitter: EntityId,
    output0: Vec<EntityId>,
    output1: Vec<EntityId>,
}

fn place_splitter_fixture(
    sim: &mut Simulation,
    output_len: i32,
    connect_second_output: bool,
) -> SplitterFixture {
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let splitter = entity_id_by_name(&sim.world.prototypes, "splitter");

    for (x, y) in all_tile_coords(&sim.world) {
        let input0 = (x, y);
        let input1 = (x, y + 1);
        let splitter_tile = (x + 1, y);
        let output0_start = (x + 2, y);
        let output1_start = (x + 2, y + 1);

        let output0_tiles = (0..output_len)
            .map(|offset| (output0_start.0 + offset, output0_start.1))
            .collect::<Vec<_>>();
        let output1_tiles = if connect_second_output {
            (0..output_len)
                .map(|offset| (output1_start.0 + offset, output1_start.1))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        if sim
            .can_place_entity(belt, input0.0, input0.1, Direction::East)
            .is_err()
            || sim
                .can_place_entity(belt, input1.0, input1.1, Direction::East)
                .is_err()
            || sim
                .can_place_entity(splitter, splitter_tile.0, splitter_tile.1, Direction::East)
                .is_err()
            || output0_tiles.iter().any(|(tile_x, tile_y)| {
                sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .is_err()
            })
            || output1_tiles.iter().any(|(tile_x, tile_y)| {
                sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .is_err()
            })
        {
            continue;
        }

        let input0_id = sim
            .place_entity(belt, input0.0, input0.1, Direction::East)
            .expect("validated splitter input belt should be placeable");
        let input1_id = sim
            .place_entity(belt, input1.0, input1.1, Direction::East)
            .expect("validated splitter input belt should be placeable");
        let splitter_id = sim
            .place_entity(splitter, splitter_tile.0, splitter_tile.1, Direction::East)
            .expect("validated splitter should be placeable");
        let output0 = output0_tiles
            .iter()
            .map(|(tile_x, tile_y)| {
                sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .expect("validated splitter output belt should be placeable")
            })
            .collect();
        let output1 = output1_tiles
            .iter()
            .map(|(tile_x, tile_y)| {
                sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .expect("validated splitter output belt should be placeable")
            })
            .collect();

        return SplitterFixture {
            input0: input0_id,
            input1: input1_id,
            splitter: splitter_id,
            output0,
            output1,
        };
    }

    panic!("expected placeable splitter fixture");
}

fn total_item_count_on_belts(sim: &Simulation, belts: &[EntityId], item_id: ItemId) -> u32 {
    belts
        .iter()
        .filter_map(|entity_id| sim.belt_segment(*entity_id).ok())
        .map(|segment| {
            segment
                .lanes
                .iter()
                .flat_map(|lane| lane.items.iter())
                .filter(|item| item.item_id == item_id)
                .count() as u32
        })
        .sum()
}

fn place_underground_belt_pair(
    sim: &mut Simulation,
    offset: i32,
    exit_direction: Direction,
) -> (EntityId, EntityId) {
    place_underground_belt_endpoint_pair(sim, "underground_belt_exit", offset, exit_direction)
}

fn place_underground_belt_endpoint_pair(
    sim: &mut Simulation,
    output_endpoint_name: &str,
    offset: i32,
    output_direction: Direction,
) -> (EntityId, EntityId) {
    let entrance = entity_id_by_name(&sim.world.prototypes, "underground_belt_entrance");
    let output = entity_id_by_name(&sim.world.prototypes, output_endpoint_name);

    for (x, y) in all_tile_coords(&sim.world) {
        let output_x = x + offset;
        if sim
            .can_place_entity(entrance, x, y, Direction::East)
            .is_ok()
            && sim
                .can_place_entity(output, output_x, y, output_direction)
                .is_ok()
        {
            let entrance_id = sim
                .place_entity(entrance, x, y, Direction::East)
                .expect("validated underground entrance tile should be placeable");
            let output_id = sim
                .place_entity(output, output_x, y, output_direction)
                .expect("validated underground endpoint tile should be placeable");
            return (entrance_id, output_id);
        }
    }

    panic!("expected placeable underground belt pair with offset {offset}");
}

fn feed_belt_items(sim: &mut Simulation, belt_id: EntityId, item_id: ItemId, count: usize) {
    let mut inserted = 0;
    let mut lane_index = 0;

    while inserted < count {
        if sim
            .insert_item_onto_belt(belt_id, lane_index, item_id)
            .is_ok()
        {
            inserted += 1;
            lane_index = 1 - lane_index;
        }
        sim.tick();
    }
}

fn total_belt_item_count(sim: &Simulation) -> usize {
    let belt_items = sim
        .entities
        .placed_entities()
        .filter_map(|placed| sim.belt_segment(placed.id).ok())
        .map(|segment| {
            segment
                .lanes
                .iter()
                .map(|lane| lane.items.len())
                .sum::<usize>()
        })
        .sum::<usize>();
    let splitter_items = sim
        .entities
        .splitters
        .values()
        .map(|state| {
            state
                .input_lanes
                .iter()
                .flat_map(|input_lanes| input_lanes.iter())
                .map(|lane| lane.items.len())
                .sum::<usize>()
        })
        .sum::<usize>();

    belt_items + splitter_items
}

fn place_burner_drill_on_resource(
    sim: &mut Simulation,
    resource_item: ItemId,
) -> (EntityId, i32, i32, u32) {
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    for (x, y) in all_tile_coords(&sim.world) {
        let Some(resource) = sim.world.tile_at(x, y).and_then(|tile| tile.resource) else {
            continue;
        };
        if resource.resource_item != resource_item {
            continue;
        }
        if sim.can_place_entity(drill, x, y, Direction::North).is_err() {
            continue;
        }

        let entity_id = sim
            .place_entity(drill, x, y, Direction::North)
            .expect("validated drill target should be placeable");
        return (entity_id, x, y, resource.amount);
    }

    panic!("expected placeable resource tile for burner drill");
}

fn place_burner_drill_outputting_to_chest(
    sim: &mut Simulation,
    resource_item: ItemId,
) -> (EntityId, EntityId, i32, i32, u32) {
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    for direction in [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ] {
        for (x, y) in all_tile_coords(&sim.world) {
            let Some(resource) = sim.world.tile_at(x, y).and_then(|tile| tile.resource) else {
                continue;
            };
            if resource.resource_item != resource_item {
                continue;
            }
            if sim.can_place_entity(drill, x, y, direction).is_err() {
                continue;
            }

            let footprint = sim
                .world
                .entity_footprint(drill, x, y, direction)
                .expect("validated drill prototype should have a footprint");
            let placed = PlacedEntity {
                id: EntityId::new(0),
                prototype_id: drill,
                x,
                y,
                direction,
                footprint,
            };
            let (output_x, output_y) = drill_output_tile(&placed);
            if sim
                .can_place_entity(chest, output_x, output_y, Direction::North)
                .is_err()
            {
                continue;
            }

            let drill_id = sim
                .place_entity(drill, x, y, direction)
                .expect("validated drill target should be placeable");
            let chest_id = sim
                .place_entity(chest, output_x, output_y, Direction::North)
                .expect("validated chest output target should be placeable");
            return (drill_id, chest_id, x, y, resource.amount);
        }
    }

    panic!("expected burner drill fixture with adjacent chest output");
}

fn place_burner_drill_outputting_to_belt(
    sim: &mut Simulation,
    resource_item: ItemId,
) -> (EntityId, EntityId, i32, i32, u32) {
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    for direction in [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ] {
        for (x, y) in all_tile_coords(&sim.world) {
            let Some(resource) = sim.world.tile_at(x, y).and_then(|tile| tile.resource) else {
                continue;
            };
            if resource.resource_item != resource_item {
                continue;
            }
            if sim.can_place_entity(drill, x, y, direction).is_err() {
                continue;
            }

            let footprint = sim
                .world
                .entity_footprint(drill, x, y, direction)
                .expect("validated drill prototype should have a footprint");
            let placed = PlacedEntity {
                id: EntityId::new(0),
                prototype_id: drill,
                x,
                y,
                direction,
                footprint,
            };
            let (output_x, output_y) = drill_output_tile(&placed);
            if sim
                .can_place_entity(belt, output_x, output_y, direction)
                .is_err()
            {
                continue;
            }

            let drill_id = sim
                .place_entity(drill, x, y, direction)
                .expect("validated drill target should be placeable");
            let belt_id = sim
                .place_entity(belt, output_x, output_y, direction)
                .expect("validated belt output target should be placeable");
            return (drill_id, belt_id, x, y, resource.amount);
        }
    }

    panic!("expected burner drill fixture with adjacent belt output");
}

fn add_fuel_to_burner_drill(
    sim: &mut Simulation,
    entity_id: EntityId,
    fuel_item: ItemId,
    count: u16,
) {
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: fuel_item,
        count,
    });
    sim.transfer_player_slot_to_burner_drill_fuel(entity_id, 0)
        .expect("fuel should transfer to burner drill");
}

fn place_stone_furnace(sim: &mut Simulation) -> EntityId {
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    sim.place_entity(furnace, x, y, Direction::North)
        .expect("stone furnace should be placeable")
}

fn place_assembling_machine(sim: &mut Simulation) -> EntityId {
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = place_powered_fixture_origin(sim, 3, 3, (3, 1));
    sim.place_entity(assembler, x, y, Direction::North)
        .expect("assembling machine should be placeable")
}

fn add_furnace_input_and_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    input_item: ItemId,
    fuel_item: ItemId,
) {
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: input_item,
        count: 1,
    });
    sim.player_inventory.slots[1] = Some(ItemStack {
        item_id: fuel_item,
        count: 1,
    });
    sim.transfer_player_slot_to_furnace_input(entity_id, 0)
        .expect("input should transfer to furnace");
    sim.transfer_player_slot_to_furnace_fuel(entity_id, 1)
        .expect("fuel should transfer to furnace");
}

#[test]
fn inventory_backed_placement_consumes_one_item() {
    let mut sim = Simulation::new_test_world(123);
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let belt_item = item_id_by_name(&sim.world.prototypes, "transport_belt");
    let (x, y) = first_placeable_entity_tile(&sim, belt, Direction::North);
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, belt_item, 1)
        .expect("test inventory should accept belt");

    let entity_id = sim
        .place_entity_from_player_inventory(belt, belt_item, x, y, Direction::North)
        .expect("inventory-backed belt placement should succeed");

    assert!(sim.entities.placed_entity(entity_id).is_some());
    assert_eq!(sim.player_inventory.count(belt_item), 0);
}

#[test]
fn inventory_backed_placement_fails_without_item() {
    let mut sim = Simulation::new_test_world(123);
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let belt_item = item_id_by_name(&sim.world.prototypes, "transport_belt");
    let (x, y) = first_placeable_entity_tile(&sim, belt, Direction::North);
    sim.player_inventory = Inventory::player();
    let before_entities = sim.entities.placed_len();

    let result = sim.place_entity_from_player_inventory(belt, belt_item, x, y, Direction::North);

    assert_eq!(
        result,
        Err(PlayerBuildError::InsufficientInventory { item_id: belt_item })
    );
    assert_eq!(sim.entities.placed_len(), before_entities);
}

#[test]
fn inventory_backed_placement_does_not_consume_on_blocked_tile() {
    let mut sim = Simulation::new_test_world(123);
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let belt_item = item_id_by_name(&sim.world.prototypes, "transport_belt");
    let (x, y) = first_placeable_entity_tile(&sim, belt, Direction::North);
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, belt_item, 1)
        .expect("test inventory should accept belt");
    let blocker = sim
        .place_entity(belt, x, y, Direction::North)
        .expect("blocking belt should be placeable");

    let result = sim.place_entity_from_player_inventory(belt, belt_item, x, y, Direction::North);

    assert_eq!(
        result,
        Err(PlayerBuildError::Build(BuildError::EntityOccupied {
            x,
            y,
            entity_id: blocker,
        }))
    );
    assert_eq!(sim.player_inventory.count(belt_item), 1);
}

#[test]
fn inventory_backed_placement_rejects_item_entity_mismatch() {
    let mut sim = Simulation::new_test_world(123);
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let chest_item = item_id_by_name(&sim.world.prototypes, "chest");
    let (x, y) = first_placeable_entity_tile(&sim, belt, Direction::North);
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, chest_item, 1)
        .expect("test inventory should accept chest");
    let before_entities = sim.entities.placed_len();

    let result = sim.place_entity_from_player_inventory(belt, chest_item, x, y, Direction::North);

    assert_eq!(
        result,
        Err(PlayerBuildError::ItemDoesNotBuildEntity {
            item_id: chest_item,
            prototype_id: belt,
        })
    );
    assert_eq!(sim.entities.placed_len(), before_entities);
    assert_eq!(sim.player_inventory.count(chest_item), 1);
}

#[test]
fn inventory_backed_placement_rejects_resource_patch() {
    let mut sim = Simulation::new_test_world(123);
    let resource_patch = sim
        .world
        .prototypes
        .entities
        .iter()
        .find(|prototype| prototype.entity_kind == EntityKind::ResourcePatch)
        .expect("base catalog should include resource patch prototypes")
        .id;
    let belt_item = item_id_by_name(&sim.world.prototypes, "transport_belt");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, belt_item, 1)
        .expect("test inventory should accept belt");
    let before_entities = sim.entities.placed_len();

    let result =
        sim.place_entity_from_player_inventory(resource_patch, belt_item, 0, 0, Direction::North);

    assert_eq!(
        result,
        Err(PlayerBuildError::MissingBuildItem {
            prototype_id: resource_patch,
        })
    );
    assert_eq!(sim.entities.placed_len(), before_entities);
    assert_eq!(sim.player_inventory.count(belt_item), 1);
}

#[test]
fn destroying_entity_returns_building_and_contents_to_player() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let chest_item = item_id_by_name(&sim.world.prototypes, "chest");
    let iron_plate = item_id_by_name(&sim.world.prototypes, "iron_plate");
    let (x, y) = first_placeable_entity_tile(&sim, chest, Direction::North);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    sim.entity_inventory_mut(entity_id)
        .expect("chest should expose inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 7,
    });
    sim.player_inventory = Inventory::player();

    let removed = sim
        .destroy_entity_to_player_inventory(entity_id)
        .expect("player should have room to recover entity");

    assert_eq!(removed.id, entity_id);
    assert!(sim.entities.placed_entity(entity_id).is_none());
    assert_eq!(sim.entities.occupancy().entity_at(x, y), None);
    assert_eq!(sim.player_inventory.count(chest_item), 1);
    assert_eq!(sim.player_inventory.count(iron_plate), 7);
}

#[test]
fn destroying_entity_keeps_world_unchanged_when_inventory_is_full() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let chest_item = item_id_by_name(&sim.world.prototypes, "chest");
    let iron_plate = item_id_by_name(&sim.world.prototypes, "iron_plate");
    let iron_stack_size =
        item_stack_size(&sim.world.prototypes, iron_plate).expect("iron plate should stack");
    let (x, y) = first_placeable_entity_tile(&sim, chest, Direction::North);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    sim.player_inventory = Inventory::with_slot_count(1);
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: iron_stack_size,
    });

    let result = sim.destroy_entity_to_player_inventory(entity_id);

    assert_eq!(
        result,
        Err(EntityDestroyError::InsufficientInventory {
            item_id: chest_item,
        })
    );
    assert!(sim.entities.placed_entity(entity_id).is_some());
    assert_eq!(sim.entities.occupancy().entity_at(x, y), Some(entity_id));
    assert_eq!(sim.player_inventory.count(chest_item), 0);
}

#[test]
fn manual_mining_right_click_destroys_building_before_mining_resource_under_it() {
    let mut sim = Simulation::new_test_world(123);
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let drill_item = item_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let coal = item_id_by_name(&sim.world.prototypes, "coal");
    let (x, y, before_amount) = first_placeable_resource_tile(&sim, drill, coal);
    sim.player_inventory = Inventory::player();
    let entity_id = sim
        .place_entity(drill, x, y, Direction::North)
        .expect("burner drill should be placeable over resources");
    let player_tile = first_manual_mining_reach_tile(&sim, x, y);
    sim.player = PlayerState::centered_on_tile(player_tile.0, player_tile.1);

    for _ in 0..MANUAL_MINING_TICKS_PER_ITEM {
        sim.update_manual_mining(Some(ManualMiningTarget { x, y }));
    }

    assert!(sim.entities.placed_entity(entity_id).is_none());
    assert_eq!(sim.entities.occupancy().entity_at(x, y), None);
    assert_eq!(
        resource_amount_at(&sim.world, x, y),
        Some(before_amount),
        "underlying resource should not be mined on the same completion tick"
    );
    assert_eq!(sim.player_inventory.count(drill_item), 1);
}

fn first_placeable_resource_tile(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    resource_item: ItemId,
) -> (i32, i32, u32) {
    for (x, y) in all_tile_coords(&sim.world) {
        let Some(resource) = sim.world.tile_at(x, y).and_then(|tile| tile.resource) else {
            continue;
        };
        if resource.resource_item == resource_item
            && sim
                .can_place_entity(prototype_id, x, y, Direction::North)
                .is_ok()
        {
            return (x, y, resource.amount);
        }
    }

    panic!("expected at least one placeable resource tile");
}

fn first_manual_mining_reach_tile(sim: &Simulation, target_x: i32, target_y: i32) -> (i32, i32) {
    for dy in -2..=2 {
        for dx in -2..=2 {
            if dx * dx + dy * dy > 4 {
                continue;
            }

            let x = target_x + dx;
            let y = target_y + dy;
            if sim.can_player_occupy_tile(x, y) {
                return (x, y);
            }
        }
    }

    panic!("expected a reachable walkable tile near manual mining target");
}

fn fill_inventory_with(sim: &mut Simulation, entity_id: EntityId, item_id: ItemId) {
    let stack_size = item_stack_size(&sim.world.prototypes, item_id)
        .expect("test item should have a stack size");
    let inventory = sim
        .entity_inventory_mut(entity_id)
        .expect("test entity should have inventory");
    for slot in &mut inventory.slots {
        *slot = Some(ItemStack {
            item_id,
            count: stack_size,
        });
    }
}

fn first_buildable_rect_without_resource(world: &WorldSim, width: i32, height: i32) -> (i32, i32) {
    for chunk in world.chunks.values() {
        for (index, _) in chunk.tiles.iter().enumerate() {
            let (x, y) = tile_coord(chunk, index);
            let footprint = EntityFootprint {
                x,
                y,
                width,
                height,
            };

            if world.validate_entity_footprint(&footprint).is_ok()
                && footprint.tiles().iter().all(|(tile_x, tile_y)| {
                    world
                        .tile_at(*tile_x, *tile_y)
                        .and_then(|tile| tile.resource)
                        .is_none()
                })
            {
                return (x, y);
            }
        }
    }

    panic!("expected buildable area without resources");
}

fn place_powered_fixture_origin(
    sim: &mut Simulation,
    fixture_width: i32,
    fixture_height: i32,
    pole_offset: (i32, i32),
) -> (i32, i32) {
    let (x, y, _) =
        place_powered_fixture_origin_with_boiler(sim, fixture_width, fixture_height, pole_offset);
    (x, y)
}

fn place_powered_fixture_origin_with_boiler(
    sim: &mut Simulation,
    fixture_width: i32,
    fixture_height: i32,
    pole_offset: (i32, i32),
) -> (i32, i32, EntityId) {
    let pump = entity_id_by_name(&sim.world.prototypes, "offshore_pump");
    let boiler = entity_id_by_name(&sim.world.prototypes, "boiler");
    let steam_engine = entity_id_by_name(&sim.world.prototypes, "steam_engine");
    let pole = entity_id_by_name(&sim.world.prototypes, "small_electric_pole");
    let coal = item_id(&sim.world.prototypes, "coal");

    for (x, y) in all_tile_coords(&sim.world) {
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
        sim.entities
            .boiler_state_mut(boiler_id)
            .expect("placed boiler should expose boiler state")
            .energy
            .fuel_slot = Some(ItemStack {
            item_id: coal,
            count: 50,
        });

        return (fixture_x, fixture_y, boiler_id);
    }

    panic!("expected powered fixture area");
}

fn fixture_is_clear_buildable(sim: &Simulation, footprint: &EntityFootprint) -> bool {
    footprint.tiles().into_iter().all(|(x, y)| {
        sim.world
            .tile_at(x, y)
            .is_some_and(|tile| tile.collision.buildable && tile.resource.is_none())
            && sim.entities.occupancy().entity_at(x, y).is_none()
    })
}

fn poles_within_small_pole_reach(first: (i32, i32), second: (i32, i32)) -> bool {
    let dx_x2 = i64::from((first.0 - second.0) * 2);
    let dy_x2 = i64::from((first.1 - second.1) * 2);
    dx_x2 * dx_x2 + dy_x2 * dy_x2 <= 15 * 15
}

fn place_disconnected_assembler_network(sim: &mut Simulation) -> EntityId {
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let pole = entity_id_by_name(&sim.world.prototypes, "small_electric_pole");

    for (x, y) in all_tile_coords(&sim.world) {
        let pole_pos = (x + 3, y + 1);
        if sim
            .can_place_entity(assembler, x, y, Direction::North)
            .is_err()
            || sim
                .can_place_entity(pole, pole_pos.0, pole_pos.1, Direction::North)
                .is_err()
            || !pole_is_disconnected_from_existing_poles(sim, pole_pos)
        {
            continue;
        }

        let pole_id = sim
            .place_entity(pole, pole_pos.0, pole_pos.1, Direction::North)
            .expect("validated disconnected pole should be placeable");
        debug_assert!(sim.entities.electric_poles.contains_key(&pole_id));
        return sim
            .place_entity(assembler, x, y, Direction::North)
            .expect("validated disconnected assembler should be placeable");
    }

    panic!("expected disconnected assembler network fixture");
}

fn add_assembler_gear_job(sim: &mut Simulation, assembler_id: EntityId) {
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("gear recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });
    sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
        .expect("assembler should accept gear ingredients");
}

fn first_placeable_offshore_pump(sim: &Simulation, pump: EntityPrototypeId) -> (i32, i32) {
    all_tile_coords(&sim.world)
        .into_iter()
        .find(|(x, y)| sim.can_place_entity(pump, *x, *y, Direction::North).is_ok())
        .expect("expected placeable offshore pump shoreline")
}

fn first_buildable_offshore_pump_footprint_away_from_water(
    sim: &Simulation,
    pump: EntityPrototypeId,
) -> (i32, i32) {
    let prototype = &sim.world.prototypes.entities[pump.index()];
    for (x, y) in all_tile_coords(&sim.world) {
        let footprint =
            EntityFootprint::from_size(x, y, prototype.size.x, prototype.size.y, Direction::North);
        if sim.world.validate_entity_footprint(&footprint).is_err()
            || sim
                .entities
                .occupancy()
                .validate_available(&footprint, None)
                .is_err()
        {
            continue;
        }
        let north_edge_is_water = (x..x + footprint.width).any(|tile_x| {
            sim.world
                .tile_at(tile_x, y - 1)
                .is_some_and(|tile| !tile.collision.walkable && !tile.collision.buildable)
        });
        if !north_edge_is_water {
            return (x, y);
        }
    }

    panic!("expected buildable offshore pump footprint away from water");
}

fn place_unpowered_chest_inserter_furnace_line(
    sim: &mut Simulation,
) -> (EntityId, EntityId, EntityId) {
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 4, 2);
    place_chest_inserter_furnace_line_at(sim, "inserter", x, y)
}

fn pole_is_disconnected_from_existing_poles(sim: &Simulation, pole_pos: (i32, i32)) -> bool {
    sim.entities.electric_poles.keys().all(|entity_id| {
        let placed = sim
            .entities
            .placed_entity(*entity_id)
            .expect("electric pole state should belong to a placed entity");
        !poles_within_small_pole_reach((placed.x, placed.y), pole_pos)
    })
}

fn place_lab(sim: &mut Simulation) -> EntityId {
    let lab = entity_id_by_name(&sim.world.prototypes, "lab");
    let (x, y) = place_powered_fixture_origin(sim, 3, 3, (3, 1));

    sim.place_entity(lab, x, y, Direction::North)
        .expect("lab should be placeable")
}

fn place_chest_inserter_furnace_line_at(
    sim: &mut Simulation,
    inserter_name: &str,
    x: i32,
    y: i32,
) -> (EntityId, EntityId, EntityId) {
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let inserter = entity_id_by_name(&sim.world.prototypes, inserter_name);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let chest_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 1, y, Direction::East)
        .expect("inserter should be placeable");
    let furnace_id = sim
        .place_entity(furnace, x + 2, y, Direction::North)
        .expect("furnace should be placeable");

    (chest_id, inserter_id, furnace_id)
}

fn place_chest_inserter_furnace_line(sim: &mut Simulation) -> (EntityId, EntityId, EntityId) {
    let (x, y) = place_powered_fixture_origin(sim, 4, 2, (1, 2));
    place_chest_inserter_furnace_line_at(sim, "inserter", x, y)
}

fn place_two_tile_chest_inserter_furnace_line(
    sim: &mut Simulation,
    inserter_name: &str,
) -> (EntityId, EntityId, EntityId) {
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let inserter = entity_id_by_name(&sim.world.prototypes, inserter_name);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = place_powered_fixture_origin(sim, 6, 2, (2, 2));
    let chest_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 2, y, Direction::East)
        .expect("inserter should be placeable");
    let furnace_id = sim
        .place_entity(furnace, x + 4, y, Direction::North)
        .expect("furnace should be placeable");

    (chest_id, inserter_id, furnace_id)
}

fn place_chest_inserter_assembler_line(sim: &mut Simulation) -> (EntityId, EntityId, EntityId) {
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = place_powered_fixture_origin(sim, 5, 3, (1, 3));
    let chest_id = sim
        .place_entity(chest, x, y + 1, Direction::North)
        .expect("chest should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 1, y + 1, Direction::East)
        .expect("inserter should be placeable");
    let assembler_id = sim
        .place_entity(assembler, x + 2, y, Direction::North)
        .expect("assembler should be placeable");

    (chest_id, inserter_id, assembler_id)
}

fn place_chest_inserter_lab_line(sim: &mut Simulation) -> (EntityId, EntityId, EntityId) {
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let lab = entity_id_by_name(&sim.world.prototypes, "lab");
    let (x, y) = place_powered_fixture_origin(sim, 5, 3, (1, 3));
    let chest_id = sim
        .place_entity(chest, x, y + 1, Direction::North)
        .expect("chest should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 1, y + 1, Direction::East)
        .expect("inserter should be placeable");
    let lab_id = sim
        .place_entity(lab, x + 2, y, Direction::North)
        .expect("lab should be placeable");

    (chest_id, inserter_id, lab_id)
}

fn place_belt_inserter_furnace_line(sim: &mut Simulation) -> (EntityId, EntityId, EntityId) {
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = place_powered_fixture_origin(sim, 4, 2, (1, 2));
    let belt_id = sim
        .place_entity(belt, x, y, Direction::East)
        .expect("belt should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 1, y, Direction::East)
        .expect("inserter should be placeable");
    let furnace_id = sim
        .place_entity(furnace, x + 2, y, Direction::North)
        .expect("furnace should be placeable");

    (belt_id, inserter_id, furnace_id)
}

fn place_furnace_inserter_chest_line(sim: &mut Simulation) -> (EntityId, EntityId, EntityId) {
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let (x, y) = place_powered_fixture_origin(sim, 4, 2, (2, 2));
    let furnace_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 2, y, Direction::East)
        .expect("inserter should be placeable");
    let chest_id = sim
        .place_entity(chest, x + 3, y, Direction::North)
        .expect("chest should be placeable");

    (furnace_id, inserter_id, chest_id)
}

fn place_assembler_inserter_chest_line(sim: &mut Simulation) -> (EntityId, EntityId, EntityId) {
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let (x, y) = place_powered_fixture_origin(sim, 5, 3, (1, 3));
    let assembler_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 3, y + 1, Direction::East)
        .expect("inserter should be placeable");
    let chest_id = sim
        .place_entity(chest, x + 4, y + 1, Direction::North)
        .expect("chest should be placeable");

    (assembler_id, inserter_id, chest_id)
}

fn place_furnace_inserter_belt_line(sim: &mut Simulation) -> (EntityId, EntityId, EntityId) {
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let (x, y) = place_powered_fixture_origin(sim, 4, 2, (2, 2));
    let furnace_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 2, y, Direction::East)
        .expect("inserter should be placeable");
    let belt_id = sim
        .place_entity(belt, x + 3, y, Direction::East)
        .expect("belt should be placeable");

    (furnace_id, inserter_id, belt_id)
}

fn run_inserter_until_idle(sim: &mut Simulation, inserter_id: EntityId) {
    for _ in 0..inserter_cycle_tick_budget(sim, inserter_id) {
        sim.tick();
        if matches!(
            sim.inserter_state(inserter_id)
                .expect("inserter should have state"),
            InserterState::WaitingForItem
        ) {
            return;
        }
    }

    panic!("inserter did not return to idle");
}

fn run_inserter_until_holding(sim: &mut Simulation, inserter_id: EntityId) {
    for _ in 0..inserter_cycle_tick_budget(sim, inserter_id) {
        sim.tick();
        if matches!(
            sim.inserter_state(inserter_id)
                .expect("inserter should have state"),
            InserterState::Holding { .. }
        ) {
            return;
        }
    }

    panic!("inserter did not pick up an item");
}

fn inserter_cycle_tick_budget(sim: &Simulation, inserter_id: EntityId) -> u32 {
    let placed = sim
        .entities
        .placed_entity(inserter_id)
        .expect("inserter should be placed");
    let prototype = sim
        .world
        .prototypes
        .entities
        .get(placed.prototype_id.index())
        .filter(|prototype| prototype.id == placed.prototype_id)
        .expect("inserter prototype should exist");
    let inserter = prototype
        .inserter
        .as_ref()
        .expect("inserter prototype should define metadata");

    inserter.pickup_ticks + inserter.drop_ticks + 20
}

fn total_item_count_in_sim(sim: &Simulation, item_id: ItemId) -> u32 {
    sim.player_inventory.count(item_id)
        + sim
            .entities
            .entity_inventories
            .values()
            .map(|inventory| inventory.count(item_id))
            .sum::<u32>()
        + sim
            .entities
            .labs
            .values()
            .map(|lab| lab.inventory.count(item_id))
            .sum::<u32>()
        + sim
            .entities
            .furnaces
            .values()
            .map(|furnace| {
                count_slot_item(furnace.input_slot, item_id)
                    + count_slot_item(furnace.energy.fuel_slot, item_id)
                    + count_slot_item(furnace.output_slot, item_id)
            })
            .sum::<u32>()
        + sim
            .entities
            .burner_mining_drills
            .values()
            .map(|drill| {
                count_slot_item(drill.energy.fuel_slot, item_id)
                    + count_slot_item(drill.output_slot, item_id)
            })
            .sum::<u32>()
        + sim
            .entities
            .assembling_machines
            .values()
            .map(|assembler| {
                assembler.input_inventory.count(item_id) + assembler.output_inventory.count(item_id)
            })
            .sum::<u32>()
        + total_belt_count_for_item(sim, item_id)
        + sim
            .entities
            .inserters
            .values()
            .map(|state| match state {
                InserterState::Holding { item } if item.item_id == item_id => u32::from(item.count),
                _ => 0,
            })
            .sum::<u32>()
}

fn total_belt_count_for_item(sim: &Simulation, item_id: ItemId) -> u32 {
    let belt_count = sim
        .entities
        .transport_belts
        .values()
        .map(|segment| {
            segment
                .lanes
                .iter()
                .flat_map(|lane| lane.items.iter())
                .filter(|item| item.item_id == item_id)
                .count() as u32
        })
        .sum::<u32>();
    let splitter_count = sim
        .entities
        .splitters
        .values()
        .map(|state| {
            state
                .input_lanes
                .iter()
                .flat_map(|input_lanes| input_lanes.iter())
                .flat_map(|lane| lane.items.iter())
                .filter(|item| item.item_id == item_id)
                .count() as u32
        })
        .sum::<u32>();

    belt_count + splitter_count
}

fn count_slot_item(slot: Option<ItemStack>, item_id: ItemId) -> u32 {
    match slot {
        Some(stack) if stack.item_id == item_id => u32::from(stack.count),
        _ => 0,
    }
}

fn run_same_assembler_actions(sim: &mut Simulation) {
    let assembler_id = place_assembling_machine(sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 4,
    });
    sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
        .expect("assembler should accept gear ingredients");
    for _ in 0..125 {
        sim.tick();
    }
}

fn resource_amount_at(world: &WorldSim, x: i32, y: i32) -> Option<u32> {
    world
        .tile_at(x, y)
        .and_then(|tile| tile.resource.map(|resource| resource.amount))
}

fn nearby_resource_pair(world: &WorldSim) -> ((i32, i32), (i32, i32)) {
    let resources = all_tile_coords(world)
        .into_iter()
        .filter(|(x, y)| {
            world
                .tile_at(*x, *y)
                .and_then(|tile| tile.resource)
                .is_some()
        })
        .collect::<Vec<_>>();

    for first in &resources {
        for second in &resources {
            if first == second {
                continue;
            }

            let dx = first.0 - second.0;
            let dy = first.1 - second.1;
            if dx * dx + dy * dy <= 6 {
                return (*first, *second);
            }
        }
    }

    panic!("expected two resource tiles close enough to mine from one position");
}

fn first_water_tile(world: &WorldSim) -> (i32, i32) {
    for chunk in world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            if !tile.collision.buildable {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                return (
                    chunk.coord.x * CHUNK_SIZE + local_x,
                    chunk.coord.y * CHUNK_SIZE + local_y,
                );
            }
        }
    }

    panic!("expected at least one water tile");
}

fn first_buildable_rect(world: &WorldSim, width: i32, height: i32) -> (i32, i32) {
    for chunk in world.chunks.values() {
        for (index, _) in chunk.tiles.iter().enumerate() {
            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let x = chunk.coord.x * CHUNK_SIZE + local_x;
            let y = chunk.coord.y * CHUNK_SIZE + local_y;
            let footprint = EntityFootprint {
                x,
                y,
                width,
                height,
            };

            if world.validate_entity_footprint(&footprint).is_ok() {
                return (x, y);
            }
        }
    }

    panic!("expected at least one buildable {width}x{height} area");
}

fn first_player_approach_to_water(sim: &Simulation) -> ((i32, i32), (f32, f32)) {
    for chunk in sim.world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            if tile.collision.walkable {
                continue;
            }

            let (x, y) = tile_coord(chunk, index);
            for (dx, dy) in CARDINAL_DIRECTIONS {
                let start = (x - dx, y - dy);
                if sim.can_player_occupy_tile(start.0, start.1) {
                    return (start, (dx as f32, dy as f32));
                }
            }
        }
    }

    panic!("expected a water tile with a walkable adjacent approach");
}

fn first_player_approach_to_unloaded_tile(sim: &Simulation) -> ((i32, i32), (f32, f32)) {
    for chunk in sim.world.chunks.values() {
        for (index, _) in chunk.tiles.iter().enumerate() {
            let (x, y) = tile_coord(chunk, index);
            if !sim.can_player_occupy_tile(x, y) {
                continue;
            }

            for (dx, dy) in CARDINAL_DIRECTIONS {
                if sim.world.tile_at(x + dx, y + dy).is_none() {
                    return ((x, y), (dx as f32, dy as f32));
                }
            }
        }
    }

    panic!("expected a walkable boundary tile next to an unloaded chunk");
}

fn first_player_approach_to_occupied_tile(sim: &mut Simulation) -> ((i32, i32), (f32, f32)) {
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");

    for (x, y) in all_tile_coords(&sim.world) {
        if sim
            .can_place_entity(inserter, x, y, Direction::North)
            .is_err()
        {
            continue;
        }

        for (dx, dy) in CARDINAL_DIRECTIONS {
            let start = (x - dx, y - dy);
            if sim.can_player_occupy_tile(start.0, start.1) {
                sim.place_entity(inserter, x, y, Direction::North)
                    .expect("validated occupied target should be placeable");
                return (start, (dx as f32, dy as f32));
            }
        }
    }

    panic!("expected a placeable entity tile with a walkable adjacent approach");
}

fn first_player_slide_fixture(sim: &mut Simulation) -> ((i32, i32), (i32, i32)) {
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");

    for (x, y) in all_tile_coords(&sim.world) {
        let start = (x - 1, y);
        let expected = (x - 1, y + 1);

        if sim
            .can_place_entity(inserter, x, y, Direction::North)
            .is_ok()
            && sim.can_player_occupy_tile(start.0, start.1)
            && sim.can_player_occupy_tile(expected.0, expected.1)
        {
            sim.place_entity(inserter, x, y, Direction::North)
                .expect("validated slide blocker should be placeable");
            return (start, expected);
        }
    }

    panic!("expected a slide fixture with an occupied x-axis target and open y-axis target");
}

fn tile_coord(chunk: &Chunk, index: usize) -> (i32, i32) {
    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
    (
        chunk.coord.x * CHUNK_SIZE + local_x,
        chunk.coord.y * CHUNK_SIZE + local_y,
    )
}

fn all_tile_coords(world: &WorldSim) -> Vec<(i32, i32)> {
    world
        .chunks
        .values()
        .flat_map(|chunk| {
            chunk
                .tiles
                .iter()
                .enumerate()
                .map(move |(index, _)| tile_coord(chunk, index))
        })
        .collect()
}

fn first_placeable_entity_tile(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    direction: Direction,
) -> (i32, i32) {
    for (x, y) in all_tile_coords(&sim.world) {
        if sim.can_place_entity(prototype_id, x, y, direction).is_ok() {
            return (x, y);
        }
    }

    panic!("expected at least one placeable entity tile");
}

fn resource_tiles(world: &WorldSim) -> Vec<(i32, i32, ResourceCell)> {
    world
        .chunks
        .values()
        .flat_map(|chunk| {
            chunk
                .tiles
                .iter()
                .enumerate()
                .filter_map(move |(index, tile)| {
                    let resource = tile.resource?;
                    let (x, y) = tile_coord(chunk, index);
                    Some((x, y, resource))
                })
        })
        .collect()
}

fn entity_id_by_name(catalog: &PrototypeCatalog, name: &str) -> EntityPrototypeId {
    factory_data::entity_prototype_id_by_name(catalog, name)
}

fn item_id_by_name(catalog: &PrototypeCatalog, name: &str) -> ItemId {
    factory_data::item_id_by_name(catalog, name)
}

const CARDINAL_DIRECTIONS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
