use super::super::*;
use super::support::*;

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
fn manual_mining_records_item_production() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y, resource) = first_resource_tile(&sim.world);
    let target = ManualMiningTarget { x, y };
    sim.player = PlayerState::centered_on_tile(x, y);

    for _ in 0..MANUAL_MINING_TICKS_PER_ITEM {
        sim.update_manual_mining(Some(target));
    }

    let row = sim
        .item_statistics()
        .rows
        .into_iter()
        .find(|row| row.item_id == resource.resource_item)
        .expect("mined item should be recorded");
    assert_eq!(row.produced_last_minute, 1);
    assert_eq!(row.produced_total, 1);
}

#[test]
fn item_rolling_window_expires_after_sixty_seconds() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_ore");
    sim.record_item_produced(iron, 3);

    for _ in 0..ITEM_STATISTICS_WINDOW_TICKS {
        sim.tick();
    }

    let row = sim
        .item_statistics()
        .rows
        .into_iter()
        .find(|row| row.item_id == iron)
        .expect("all-time item should remain visible");
    assert_eq!(row.produced_last_minute, 0);
    assert_eq!(row.produced_total, 3);
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
fn manual_mining_right_click_destroys_building_before_mining_resource_under_it() {
    let mut sim = Simulation::new_test_world(123);
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let drill_item = item_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let coal = item_id_by_name(&sim.world.prototypes, "coal");
    let (x, y, before_amount) = first_placeable_resource_tile(&sim, drill, coal);
    sim.player_inventory = Inventory::player();
    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: drill,
            x,
            y,
            direction: Direction::North,
        },
    )
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
