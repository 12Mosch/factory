use super::super::*;
use super::support::*;

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
fn repeated_belt_ticks_reuse_cached_lane_graph() {
    let mut sim = Simulation::new_test_world(123);
    place_belt_line(&mut sim, 8);

    sim.advance_transport_belts();
    assert_eq!(sim.transport_lane_graph_rebuild_count(), 1);

    sim.advance_transport_belts();
    sim.advance_transport_belts();
    assert_eq!(sim.transport_lane_graph_rebuild_count(), 1);
}

#[test]
fn transport_topology_edits_dirty_cached_lane_graph() {
    let mut sim = Simulation::new_test_world(123);
    let belts = place_belt_line(&mut sim, 2);

    sim.advance_transport_belts();
    assert_eq!(sim.transport_lane_graph_rebuild_count(), 1);

    sim.rotate_entity(belts[0], Direction::North)
        .expect("placed belt should rotate");
    sim.advance_transport_belts();
    assert_eq!(sim.transport_lane_graph_rebuild_count(), 2);

    sim.remove_entity(belts[1])
        .expect("placed belt should be removable");
    sim.advance_transport_belts();
    assert_eq!(sim.transport_lane_graph_rebuild_count(), 3);
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
