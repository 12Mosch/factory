use super::super::*;
use super::support::*;

#[test]
fn two_by_two_entity_cannot_overlap_another_entity() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 4, 2);

    let first = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: furnace,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("first furnace should be placeable");
    let error = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: furnace,
            x: x + 1,
            y,
            direction: Direction::North,
        },
    )
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

    let error = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: inserter,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect_err("water should block entity placement");

    assert!(matches!(error, BuildError::TileBlocked { x: bx, y: by } if bx == x && by == y));
}

#[test]
fn entity_topology_revision_changes_only_for_successful_topology_edits() {
    let mut sim = Simulation::new_test_world(123);
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);

    let initial_revision = sim.entity_topology_revision();
    sim.tick();
    assert_eq!(sim.entity_topology_revision(), initial_revision);

    let water = first_water_tile(&sim.world);
    crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: inserter,
            x: water.0,
            y: water.1,
            direction: Direction::North,
        },
    )
    .expect_err("water should block entity placement");
    assert_eq!(sim.entity_topology_revision(), initial_revision);

    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: inserter,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("inserter should be placeable");
    let placed_revision = initial_revision + 1;
    assert_eq!(sim.entity_topology_revision(), placed_revision);

    sim.tick();
    assert_eq!(sim.entity_topology_revision(), placed_revision);

    crate::entity_mutation::rotate(&mut sim, entity_id, Direction::North)
        .expect("same direction rotate should be a no-op");
    assert_eq!(sim.entity_topology_revision(), placed_revision);

    crate::entity_mutation::rotate(&mut sim, entity_id, Direction::East)
        .expect("direction change should be valid");
    let rotated_revision = placed_revision + 1;
    assert_eq!(sim.entity_topology_revision(), rotated_revision);

    assert!(crate::entity_mutation::remove(&mut sim, EntityId::new(999_999)).is_none());
    assert_eq!(sim.entity_topology_revision(), rotated_revision);

    crate::entity_mutation::remove(&mut sim, entity_id).expect("placed entity should be removable");
    assert_eq!(sim.entity_topology_revision(), rotated_revision + 1);
}

#[test]
fn placement_preview_reports_occupied_tiles() {
    let mut sim = Simulation::new_test_world(123);
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let belt_item = item_id_by_name(&sim.world.prototypes, "transport_belt");
    let (x, y) = first_placeable_entity_tile(&sim, belt, Direction::North);
    give_player_build_item(&mut sim, belt_item);
    let blocker = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: belt,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("blocking belt should be placeable");

    let preview = crate::placement::preview_from_player_inventory(
        &sim,
        crate::placement::PlayerPlacementRequest {
            prototype_id: belt,
            item_id: belt_item,
            x,
            y,
            direction: Direction::North,
        },
    );

    assert!(preview.issues.iter().any(|issue| {
        issue.tile == Some((x, y))
            && issue.kind == BuildPlacementIssueKind::EntityOccupied { entity_id: blocker }
    }));
}

#[test]
fn placement_preview_reports_player_tile() {
    let mut sim = Simulation::new_test_world(123);
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let inserter_item = item_id_by_name(&sim.world.prototypes, "inserter");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    sim.player = PlayerState::centered_on_tile(x, y);
    give_player_build_item(&mut sim, inserter_item);

    let preview = crate::placement::preview_from_player_inventory(
        &sim,
        crate::placement::PlayerPlacementRequest {
            prototype_id: inserter,
            item_id: inserter_item,
            x,
            y,
            direction: Direction::North,
        },
    );

    assert!(preview.issues.iter().any(|issue| {
        issue.tile == Some((x, y)) && issue.kind == BuildPlacementIssueKind::PlayerOccupied
    }));
}

#[test]
fn placement_preview_reports_blocked_terrain() {
    let mut sim = Simulation::new_test_world(123);
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let inserter_item = item_id_by_name(&sim.world.prototypes, "inserter");
    let (x, y) = first_water_tile(&sim.world);
    give_player_build_item(&mut sim, inserter_item);

    let preview = crate::placement::preview_from_player_inventory(
        &sim,
        crate::placement::PlayerPlacementRequest {
            prototype_id: inserter,
            item_id: inserter_item,
            x,
            y,
            direction: Direction::North,
        },
    );

    assert!(preview.issues.iter().any(|issue| {
        issue.tile == Some((x, y)) && issue.kind == BuildPlacementIssueKind::TerrainBlocked
    }));
}

#[test]
fn placement_preview_reports_outside_generated_chunks() {
    let mut sim = Simulation::new_test_world(123);
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let inserter_item = item_id_by_name(&sim.world.prototypes, "inserter");
    let outside_x = (STARTING_MAX_CHUNK + 1) * CHUNK_SIZE;
    give_player_build_item(&mut sim, inserter_item);

    let preview = crate::placement::preview_from_player_inventory(
        &sim,
        crate::placement::PlayerPlacementRequest {
            prototype_id: inserter,
            item_id: inserter_item,
            x: outside_x,
            y: 0,
            direction: Direction::North,
        },
    );

    assert!(preview.issues.iter().any(|issue| {
        issue.tile == Some((outside_x, 0))
            && issue.kind == BuildPlacementIssueKind::OutsideGeneratedChunks
    }));
}

#[test]
fn placement_preview_reports_missing_drill_resource() {
    let mut sim = Simulation::new_test_world(123);
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let drill_item = item_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let prototype = &sim.world.prototypes.entities[drill.index()];
    let (x, y) =
        first_buildable_rect_without_resource(&sim.world, prototype.size.x, prototype.size.y);
    give_player_build_item(&mut sim, drill_item);

    let preview = crate::placement::preview_from_player_inventory(
        &sim,
        crate::placement::PlayerPlacementRequest {
            prototype_id: drill,
            item_id: drill_item,
            x,
            y,
            direction: Direction::North,
        },
    );

    assert!(preview.issues.iter().any(|issue| {
        issue.kind == BuildPlacementIssueKind::MissingRequiredResource
            && issue.tile.is_some_and(|tile| {
                preview
                    .footprint
                    .expect("valid drill footprint should be previewed")
                    .contains_tile(tile.0, tile.1)
            })
    }));
}

#[test]
fn placement_preview_reports_missing_offshore_pump_water() {
    let mut sim = Simulation::new_test_world(123);
    let pump = entity_id_by_name(&sim.world.prototypes, "offshore_pump");
    let pump_item = build_item_or_fallback_item(&sim, pump);
    let (x, y) = first_buildable_offshore_pump_footprint_away_from_water(&sim, pump);
    give_player_build_item(&mut sim, pump_item);

    let preview = crate::placement::preview_from_player_inventory(
        &sim,
        crate::placement::PlayerPlacementRequest {
            prototype_id: pump,
            item_id: pump_item,
            x,
            y,
            direction: Direction::North,
        },
    );

    assert!(preview.issues.iter().any(|issue| {
        issue.kind == BuildPlacementIssueKind::MissingAdjacentWater
            && issue
                .tile
                .is_some_and(|(tile_x, tile_y)| tile_y == y - 1 && tile_x >= x)
    }));
}

#[test]
fn entity_cannot_be_placed_on_player_tile() {
    let mut sim = Simulation::new_test_world(123);
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    sim.player = PlayerState::centered_on_tile(x, y);

    let error = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: inserter,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect_err("player tile should block entity placement");

    assert!(matches!(error, BuildError::TileBlocked { x: bx, y: by } if bx == x && by == y));
}

#[test]
fn multi_tile_entity_cannot_overlap_player_tile() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    sim.player = PlayerState::centered_on_tile(x + 1, y + 1);

    let error = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: furnace,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect_err("entity footprint should not overlap the player tile");

    assert!(
        matches!(error, BuildError::TileBlocked { x: bx, y: by } if bx == x + 1 && by == y + 1)
    );
}

#[test]
fn entity_cannot_be_placed_outside_generated_chunks() {
    let mut sim = Simulation::new_test_world(123);
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let outside_x = (STARTING_MAX_CHUNK + 1) * CHUNK_SIZE;

    let error = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: inserter,
            x: outside_x,
            y: 0,
            direction: Direction::North,
        },
    )
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
    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: inserter,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("rectangular entity should be placeable");

    assert_eq!(sim.entities.occupancy().entity_at(x, y), Some(entity_id));
    assert_eq!(
        sim.entities.occupancy().entity_at(x, y + 1),
        Some(entity_id)
    );
    assert_eq!(sim.entities.occupancy().entity_at(x + 1, y), None);

    crate::entity_mutation::rotate(&mut sim, entity_id, Direction::East)
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
    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: inserter,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("rectangular entity should be placeable");
    sim.player = PlayerState::centered_on_tile(x + 1, y);

    let error = crate::entity_mutation::rotate(&mut sim, entity_id, Direction::East)
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

    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chest,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("chest should be placeable");

    assert_eq!(
        crate::entity_access::inventory(&sim, entity_id)
            .expect("chest should have an inventory")
            .slots
            .len(),
        16
    );
}

#[test]
fn player_cannot_place_locked_entity_even_with_inventory_item() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_entity = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let assembler_item = item_id(&sim.world.prototypes, "assembling_machine");
    let (x, y) = first_placeable_entity_tile(&sim, assembler_entity, Direction::North);
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, assembler_item, 1)
        .expect("test inventory should accept assembler");

    assert_eq!(
        crate::placement::place_from_player_inventory(
            &mut sim,
            crate::placement::PlayerPlacementRequest {
                prototype_id: assembler_entity,
                item_id: assembler_item,
                x,
                y,
                direction: Direction::North
            }
        ),
        Err(PlayerBuildError::EntityLocked {
            prototype_id: assembler_entity,
        })
    );
    assert_eq!(sim.player_inventory.count(assembler_item), 1);
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

    let entity_id = crate::placement::place_from_player_inventory(
        &mut sim,
        crate::placement::PlayerPlacementRequest {
            prototype_id: belt,
            item_id: belt_item,
            x,
            y,
            direction: Direction::North,
        },
    )
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

    let result = crate::placement::place_from_player_inventory(
        &mut sim,
        crate::placement::PlayerPlacementRequest {
            prototype_id: belt,
            item_id: belt_item,
            x,
            y,
            direction: Direction::North,
        },
    );

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
    let blocker = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: belt,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("blocking belt should be placeable");

    let result = crate::placement::place_from_player_inventory(
        &mut sim,
        crate::placement::PlayerPlacementRequest {
            prototype_id: belt,
            item_id: belt_item,
            x,
            y,
            direction: Direction::North,
        },
    );

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

    let result = crate::placement::place_from_player_inventory(
        &mut sim,
        crate::placement::PlayerPlacementRequest {
            prototype_id: belt,
            item_id: chest_item,
            x,
            y,
            direction: Direction::North,
        },
    );

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

    let result = crate::placement::place_from_player_inventory(
        &mut sim,
        crate::placement::PlayerPlacementRequest {
            prototype_id: resource_patch,
            item_id: belt_item,
            x: 0,
            y: 0,
            direction: Direction::North,
        },
    );

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
    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chest,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("chest should be placeable");
    crate::entity_access::inventory_mut(&mut sim, entity_id)
        .expect("chest should expose inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 7,
    });
    sim.player_inventory = Inventory::player();

    let removed = crate::entity_mutation::destroy_to_player_inventory(&mut sim, entity_id)
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
    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chest,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("chest should be placeable");
    sim.player_inventory = Inventory::with_slot_count(1);
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: iron_stack_size,
    });

    let result = crate::entity_mutation::destroy_to_player_inventory(&mut sim, entity_id);

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

fn give_player_build_item(sim: &mut Simulation, item_id: ItemId) {
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, item_id, 1)
        .expect("test inventory should accept build item");
}

fn build_item_or_fallback_item(sim: &Simulation, prototype_id: EntityPrototypeId) -> ItemId {
    sim.world.prototypes.entities[prototype_id.index()]
        .build_item
        .unwrap_or_else(|| item_id_by_name(&sim.world.prototypes, "transport_belt"))
}
