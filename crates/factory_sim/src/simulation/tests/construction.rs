use super::super::*;
use super::support::*;
use crate::construction::{ConstructionError, ConstructionJob};
use crate::simulation::construction_ops::GhostPlacementRequest;

fn ghost_request(
    prototype_id: EntityPrototypeId,
    x: WorldTileCoord,
    y: WorldTileCoord,
    direction: Direction,
) -> GhostPlacementRequest {
    GhostPlacementRequest {
        prototype_id,
        x,
        y,
        direction,
        recipe: None,
    }
}

#[test]
fn ghost_placement_reserves_tiles_and_queues_a_build_job() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);

    let ghost_id =
        construction_ops::place_ghost(&mut sim, ghost_request(furnace, x, y, Direction::North))
            .expect("furnace ghost should be placeable");

    let ghost = sim
        .construction()
        .ghost(ghost_id)
        .expect("placed ghost should be retrievable");
    assert_eq!(ghost.prototype_id, furnace);
    assert_eq!((ghost.x, ghost.y), (x, y));
    assert!(sim.construction().ghost_at(x, y).is_some());
    assert!(sim.construction().ghost_at(x + 1, y + 1).is_some());
    assert_eq!(
        sim.construction().queue().collect::<Vec<_>>(),
        vec![ConstructionJob::BuildGhost(ghost_id)]
    );
    sim.validate_state()
        .expect("simulation with a ghost should validate");
}

#[test]
fn ghost_placement_rejects_overlapping_ghosts_and_entities() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 6, 2);

    let ghost_id =
        construction_ops::place_ghost(&mut sim, ghost_request(furnace, x, y, Direction::North))
            .expect("first ghost should be placeable");
    let ghost_overlap =
        construction_ops::place_ghost(&mut sim, ghost_request(furnace, x + 1, y, Direction::North))
            .expect_err("overlapping ghost should be rejected");
    assert!(matches!(
        ghost_overlap,
        ConstructionError::GhostOccupied { ghost_id: overlapped, .. } if overlapped == ghost_id
    ));

    let entity_id = place_at(&mut sim, furnace, x + 4, y, Direction::North);
    let entity_overlap =
        construction_ops::place_ghost(&mut sim, ghost_request(furnace, x + 3, y, Direction::North))
            .expect_err("ghost overlapping a placed entity should be rejected");
    assert!(matches!(
        entity_overlap,
        ConstructionError::Build(BuildError::EntityOccupied { entity_id: occupied, .. })
            if occupied == entity_id
    ));
}

#[test]
fn ghost_placement_requires_unlocked_entity() {
    let mut sim = Simulation::new_test_world(123);
    let splitter = entity_id_by_name(&sim.world.prototypes, "splitter");
    assert!(!sim.is_entity_unlocked(splitter));
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);

    let error =
        construction_ops::place_ghost(&mut sim, ghost_request(splitter, x, y, Direction::North))
            .expect_err("locked entity ghost should be rejected");

    assert!(matches!(
        error,
        ConstructionError::EntityLocked { prototype_id } if prototype_id == splitter
    ));
}

#[test]
fn cancelling_a_ghost_releases_tiles_and_queue_job() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);

    let ghost_id =
        construction_ops::place_ghost(&mut sim, ghost_request(furnace, x, y, Direction::North))
            .expect("furnace ghost should be placeable");
    construction_ops::cancel_ghost(&mut sim, ghost_id).expect("ghost should be cancellable");

    assert_eq!(sim.construction().ghost_count(), 0);
    assert!(sim.construction().ghost_at(x, y).is_none());
    assert_eq!(sim.construction().queue_len(), 0);
    assert!(matches!(
        construction_ops::cancel_ghost(&mut sim, ghost_id),
        Err(ConstructionError::MissingGhost(missing)) if missing == ghost_id
    ));
    sim.validate_state()
        .expect("simulation after ghost cancel should validate");
}

#[test]
fn building_a_ghost_consumes_inventory_and_places_the_entity() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let furnace_item = item_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    let items_before = sim.player_inventory().count(furnace_item);
    assert!(items_before > 0);

    let ghost_id =
        construction_ops::place_ghost(&mut sim, ghost_request(furnace, x, y, Direction::North))
            .expect("furnace ghost should be placeable");
    let entity_id = construction_ops::build_ghost_from_player_inventory(&mut sim, ghost_id)
        .expect("ghost should be manually buildable");

    let placed = sim
        .entities()
        .placed_entity(entity_id)
        .expect("built ghost should produce a placed entity");
    assert_eq!(placed.prototype_id, furnace);
    assert_eq!((placed.x, placed.y), (x, y));
    assert_eq!(sim.player_inventory().count(furnace_item), items_before - 1);
    assert_eq!(sim.construction().ghost_count(), 0);
    assert_eq!(sim.construction().queue_len(), 0);
    sim.validate_state()
        .expect("simulation after manual ghost build should validate");
}

#[test]
fn building_a_ghost_without_the_item_fails_and_keeps_the_ghost() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let furnace_item = item_id_by_name(&sim.world.prototypes, "stone_furnace");
    let count = u16::try_from(sim.player_inventory().count(furnace_item))
        .expect("starting furnace count should fit a stack");
    sim.player_inventory_mut()
        .remove(furnace_item, count)
        .expect("starting furnace items should be removable");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);

    let ghost_id =
        construction_ops::place_ghost(&mut sim, ghost_request(furnace, x, y, Direction::North))
            .expect("furnace ghost should be placeable");
    let error = construction_ops::build_ghost_from_player_inventory(&mut sim, ghost_id)
        .expect_err("building without the item should fail");

    assert!(matches!(
        error,
        ConstructionError::PlayerBuild(PlayerBuildError::InsufficientInventory { item_id })
            if item_id == furnace_item
    ));
    assert!(sim.construction().ghost(ghost_id).is_some());
    assert_eq!(sim.construction().queue_len(), 1);
}

#[test]
fn placing_a_real_entity_over_a_ghost_replaces_the_plan() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);

    construction_ops::place_ghost(&mut sim, ghost_request(furnace, x, y, Direction::North))
        .expect("furnace ghost should be placeable");
    place_at(&mut sim, furnace, x, y, Direction::North);

    assert_eq!(sim.construction().ghost_count(), 0);
    assert_eq!(sim.construction().queue_len(), 0);
    sim.validate_state()
        .expect("simulation after building over a ghost should validate");
}

#[test]
fn deconstruction_marks_are_planned_executed_and_cancelled() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let furnace_item = item_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    let entity_id = place_at(&mut sim, furnace, x, y, Direction::North);

    // Deconstructing an unmarked entity is rejected: the planner drives it.
    assert!(matches!(
        construction_ops::deconstruct_marked(&mut sim, entity_id),
        Err(ConstructionError::NotMarkedForDeconstruction(unmarked)) if unmarked == entity_id
    ));

    let (marked, _) = construction_ops::mark_area_for_deconstruction(&mut sim, x, y, x + 1, y + 1);
    assert_eq!(marked, 1);
    assert!(sim.construction().is_marked_for_deconstruction(entity_id));
    assert_eq!(
        sim.construction().queue().collect::<Vec<_>>(),
        vec![ConstructionJob::Deconstruct(entity_id)]
    );
    // Marking again is idempotent.
    let (remarked, _) = construction_ops::mark_area_for_deconstruction(&mut sim, x, y, x, y);
    assert_eq!(remarked, 0);
    assert_eq!(sim.construction().queue_len(), 1);
    sim.validate_state()
        .expect("simulation with deconstruction marks should validate");

    let cancelled = construction_ops::cancel_deconstruction_in_area(&mut sim, x, y, x, y);
    assert_eq!(cancelled, 1);
    assert!(!sim.construction().is_marked_for_deconstruction(entity_id));
    assert_eq!(sim.construction().queue_len(), 0);

    construction_ops::mark_area_for_deconstruction(&mut sim, x, y, x, y);
    let items_before = sim.player_inventory().count(furnace_item);
    construction_ops::deconstruct_marked(&mut sim, entity_id)
        .expect("marked entity should deconstruct");
    assert_eq!(sim.player_inventory().count(furnace_item), items_before + 1);
    assert!(sim.entities().placed_entity(entity_id).is_none());
    assert!(!sim.construction().is_marked_for_deconstruction(entity_id));
    assert_eq!(sim.construction().queue_len(), 0);
    sim.validate_state()
        .expect("simulation after deconstruction should validate");
}

#[test]
fn deconstruction_planner_cancels_ghosts_in_the_area() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    construction_ops::place_ghost(&mut sim, ghost_request(furnace, x, y, Direction::North))
        .expect("furnace ghost should be placeable");

    let (marked, ghosts_removed) =
        construction_ops::mark_area_for_deconstruction(&mut sim, x, y, x + 1, y + 1);

    assert_eq!(marked, 0);
    assert_eq!(ghosts_removed, 1);
    assert_eq!(sim.construction().ghost_count(), 0);
    assert_eq!(sim.construction().queue_len(), 0);
}

#[test]
fn manually_mining_a_marked_entity_clears_its_mark() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    let entity_id = place_at(&mut sim, furnace, x, y, Direction::North);
    construction_ops::mark_area_for_deconstruction(&mut sim, x, y, x, y);

    crate::entity_mutation::destroy_to_player_inventory(&mut sim, entity_id)
        .expect("marked entity should be manually minable");

    assert!(!sim.construction().is_marked_for_deconstruction(entity_id));
    assert_eq!(sim.construction().queue_len(), 0);
    sim.validate_state()
        .expect("simulation after mining a marked entity should validate");
}

#[test]
fn blueprint_capture_and_paste_replans_the_area() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 8, 4);
    place_at(&mut sim, furnace, x, y, Direction::North);
    place_at(&mut sim, furnace, x + 2, y + 1, Direction::North);

    let blueprint = sim
        .capture_blueprint("test", x, y, x + 3, y + 2)
        .expect("area with entities should capture");
    assert_eq!(blueprint.entities.len(), 2);
    assert_eq!(
        blueprint
            .entities
            .iter()
            .map(|entity| (entity.dx, entity.dy))
            .collect::<Vec<_>>(),
        vec![(0, 0), (2, 1)]
    );

    let paste_x = x + 4;
    let paste_y = y;
    let (placed, skipped) =
        construction_ops::paste_blueprint_ghosts(&mut sim, &blueprint.entities, paste_x, paste_y);
    assert_eq!((placed, skipped), (2, 0));
    assert_eq!(sim.construction().ghost_count(), 2);
    assert!(sim.construction().ghost_at(paste_x, paste_y).is_some());
    assert!(
        sim.construction()
            .ghost_at(paste_x + 2, paste_y + 1)
            .is_some()
    );
    sim.validate_state()
        .expect("simulation after blueprint paste should validate");

    // Pasting the same blueprint onto its own ghosts skips every entry.
    let (replaced, reskipped) =
        construction_ops::paste_blueprint_ghosts(&mut sim, &blueprint.entities, paste_x, paste_y);
    assert_eq!((replaced, reskipped), (0, 2));
}

#[test]
fn empty_blueprint_area_is_rejected() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 2, 2);

    let error = sim
        .capture_blueprint("empty", x, y, x + 1, y + 1)
        .expect_err("empty area should not capture");

    assert!(matches!(error, ConstructionError::EmptyBlueprintArea));
    // Save-to-library goes through the same validation.
    assert!(
        construction_ops::save_blueprint_from_area(&mut sim, "empty", x, y, x + 1, y + 1).is_err()
    );
}

#[test]
fn blueprint_library_saves_and_deletes_entries() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    place_at(&mut sim, furnace, x, y, Direction::North);

    let index = construction_ops::save_blueprint_from_area(&mut sim, "smelter", x, y, x + 1, y + 1)
        .expect("area with a furnace should save");
    assert_eq!(index, 0);
    assert_eq!(sim.construction().blueprints().len(), 1);
    assert_eq!(sim.construction().blueprints()[0].name, "smelter");
    sim.validate_state()
        .expect("simulation with a saved blueprint should validate");

    assert!(matches!(
        construction_ops::delete_blueprint(&mut sim, 5),
        Err(ConstructionError::MissingBlueprint { index: 5 })
    ));
    let deleted = construction_ops::delete_blueprint(&mut sim, 0).expect("blueprint should delete");
    assert_eq!(deleted.name, "smelter");
    assert!(sim.construction().blueprints().is_empty());
}

#[test]
fn blueprint_library_renames_entries() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    place_at(&mut sim, furnace, x, y, Direction::North);

    let index = construction_ops::save_blueprint_from_area(&mut sim, "smelter", x, y, x + 1, y + 1)
        .expect("area with a furnace should save");

    assert!(matches!(
        construction_ops::rename_blueprint(&mut sim, 5, "renamed".to_string()),
        Err(ConstructionError::MissingBlueprint { index: 5 })
    ));

    construction_ops::rename_blueprint(&mut sim, index, "renamed".to_string())
        .expect("blueprint should rename");
    assert_eq!(sim.construction().blueprints()[index].name, "renamed");
    sim.validate_state()
        .expect("simulation with a renamed blueprint should validate");
}

#[test]
fn blueprint_captures_assembler_recipes_onto_ghosts() {
    let mut sim = Simulation::new_test_world(123);
    sim.apply_command(&SimCommand::BuildRedScienceResearchFixture)
        .expect("research fixture should build");
    while sim.active_research().is_some() {
        sim.tick();
    }
    let assembler_prototype =
        factory_data::entity_prototype_id_by_name(&sim.world.prototypes, "assembling_machine");
    assert!(sim.is_entity_unlocked(assembler_prototype));

    let (x, y) = all_tile_coords(&sim.world)
        .into_iter()
        .find(|&(x, y)| {
            [x, x + 3].into_iter().all(|x| {
                crate::placement::validate(
                    &sim,
                    crate::placement::EntityPlacementRequest {
                        prototype_id: assembler_prototype,
                        x,
                        y,
                        direction: Direction::North,
                    },
                )
                .is_ok()
            })
        })
        .expect("expected two adjacent clear assembler placement areas");
    let assembler_id = place_at(&mut sim, assembler_prototype, x, y, Direction::North);
    let recipe = sim
        .world
        .prototypes
        .recipes
        .iter()
        .find(|recipe| {
            recipe.category == CraftingCategory::Crafting && sim.is_recipe_unlocked(recipe.id)
        })
        .map(|recipe| recipe.id)
        .expect("an unlocked crafting recipe should exist");
    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("assembler recipe should be selectable");

    let blueprint = sim
        .capture_blueprint("assembly", x, y, x + 2, y + 2)
        .expect("assembler area should capture");
    assert_eq!(blueprint.entities.len(), 1);
    assert_eq!(blueprint.entities[0].recipe, Some(recipe));

    let (placed, _) =
        construction_ops::paste_blueprint_ghosts(&mut sim, &blueprint.entities, x + 3, y);
    assert_eq!(placed, 1);
    let ghost = sim
        .construction()
        .ghost_at(x + 3, y)
        .expect("pasted assembler ghost should exist");
    assert_eq!(ghost.recipe, Some(recipe));
    let ghost_id = ghost.id;

    let item = sim
        .world
        .prototypes
        .entity(assembler_prototype)
        .and_then(|prototype| prototype.build_item)
        .expect("assembler should have a build item");
    let prototypes = sim.world.prototypes.clone();
    sim.player_inventory_mut()
        .insert(&prototypes, item, 1)
        .expect("player inventory should accept an assembler");
    let built_id = construction_ops::build_ghost_from_player_inventory(&mut sim, ghost_id)
        .expect("assembler ghost should build");
    assert_eq!(
        crate::entity_access::assembler_state(&sim, built_id)
            .expect("built assembler should have state")
            .selected_recipe,
        Some(recipe)
    );
}

#[test]
fn construction_state_round_trips_through_save() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 6, 2);
    construction_ops::place_ghost(&mut sim, ghost_request(furnace, x, y, Direction::North))
        .expect("furnace ghost should be placeable");
    let entity_id = place_at(&mut sim, furnace, x + 3, y, Direction::North);
    construction_ops::mark_area_for_deconstruction(&mut sim, x + 3, y, x + 3, y);
    construction_ops::save_blueprint_from_area(&mut sim, "plan", x, y, x + 4, y + 1)
        .expect("area should save as blueprint");

    let bytes = save_to_bytes(&sim).expect("simulation should save");
    let loaded = load_from_bytes(&bytes).expect("simulation should load");

    assert_eq!(sim.state_hash(), loaded.state_hash());
    assert_eq!(loaded.construction().ghost_count(), 1);
    assert!(
        loaded
            .construction()
            .is_marked_for_deconstruction(entity_id)
    );
    assert_eq!(loaded.construction().blueprints().len(), 1);
    assert_eq!(loaded.construction().queue_len(), 2);
}
