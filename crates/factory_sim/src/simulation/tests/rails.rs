use super::super::*;
use super::support::*;
use crate::rails::{RailCurveGeometry, RailPoint};

/// A patch of clear ground big enough for the fixtures below, with the origin
/// far enough inside it that a curve and its approaches all fit.
fn rail_test_area(sim: &Simulation) -> (WorldTileCoord, WorldTileCoord) {
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 8, 8);
    (x + 2, y + 2)
}

fn place_rail(
    sim: &mut Simulation,
    name: &str,
    x: WorldTileCoord,
    y: WorldTileCoord,
    direction: Direction,
) -> EntityId {
    let prototype_id = entity_id_by_name(&sim.world.prototypes, name);
    place_at(sim, prototype_id, x, y, direction)
}

fn network_members(sim: &Simulation, network_id: u32) -> Vec<EntityId> {
    sim.rail_networks()
        .into_iter()
        .find(|network| network.network_id == network_id)
        .map(|network| network.entities)
        .unwrap_or_default()
}

#[test]
fn end_to_end_straights_form_one_network_and_a_gap_splits_them() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = rail_test_area(&sim);

    let first = place_rail(&mut sim, "rail_straight", x, y, Direction::North);
    let second = place_rail(&mut sim, "rail_straight", x, y + 2, Direction::North);
    // Leaves the two tiles at y + 4 empty, so this piece cannot reach the pair.
    let detached = place_rail(&mut sim, "rail_straight", x, y + 6, Direction::North);

    sim.tick();

    assert_eq!(sim.rail_networks().len(), 2);
    let joined = sim
        .rail_network_id_for_entity(first)
        .expect("a placed rail belongs to a network");
    assert_eq!(sim.rail_network_id_for_entity(second), Some(joined));
    assert_eq!(network_members(&sim, joined), vec![first, second]);
    assert_ne!(sim.rail_network_id_for_entity(detached), Some(joined));
}

/// A vertical run and a horizontal one that pass close by each other are still
/// two separate runs: track joins where its centre lines meet, not where its
/// footprints happen to be neighbours.
#[test]
fn track_that_only_passes_close_by_does_not_join() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = rail_test_area(&sim);

    // The vertical piece ends at (x + 0.5, y + 2); the horizontal one runs along
    // y + 2.5, a half tile above it and offset half a tile across.
    let vertical = place_rail(&mut sim, "rail_straight", x, y, Direction::North);
    let horizontal = place_rail(&mut sim, "rail_straight", x, y + 2, Direction::East);

    sim.tick();

    assert_ne!(
        sim.rail_network_id_for_entity(vertical),
        sim.rail_network_id_for_entity(horizontal)
    );
    assert_eq!(sim.rail_networks().len(), 2);
}

#[test]
fn a_curve_joins_the_straights_at_both_of_its_ends() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = rail_test_area(&sim);

    // Vertical run below the curve, curve at (x, y), horizontal run to its
    // right — the arrangement the curve's declared endpoints describe.
    let approach = place_rail(&mut sim, "rail_straight", x, y - 2, Direction::North);
    let curve = place_rail(&mut sim, "rail_curved", x, y, Direction::North);
    let exit = place_rail(&mut sim, "rail_straight", x + 2, y + 1, Direction::East);

    sim.tick();

    assert_eq!(sim.rail_networks().len(), 1);
    let network_id = sim
        .rail_network_id_for_entity(curve)
        .expect("the curve belongs to a network");
    let mut members = network_members(&sim, network_id);
    members.sort_unstable();
    let mut expected = vec![approach, curve, exit];
    expected.sort_unstable();
    assert_eq!(members, expected);
}

#[test]
fn endpoint_connections_distinguish_a_join_from_a_dead_end() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = rail_test_area(&sim);

    let lower = place_rail(&mut sim, "rail_straight", x, y, Direction::North);
    place_rail(&mut sim, "rail_straight", x, y + 2, Direction::North);

    sim.tick();

    let connections = sim
        .rail_endpoint_connections(lower)
        .expect("a placed rail reports its endpoints");
    // The south end is open ground; the north end continues into the piece
    // above.
    assert_eq!(connections[0].endpoint.direction, Direction::South);
    assert!(connections[0].connected.is_empty());
    assert_eq!(connections[1].endpoint.direction, Direction::North);
    assert_eq!(connections[1].connected.len(), 1);
}

#[test]
fn placement_preview_reports_the_connection_it_would_form() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = rail_test_area(&sim);
    let straight = entity_id_by_name(&sim.world.prototypes, "rail_straight");

    let existing = place_rail(&mut sim, "rail_straight", x, y, Direction::North);
    sim.tick();

    let joining = sim
        .rail_placement_preview(straight, x, y + 2, Direction::North)
        .expect("a rail prototype previews its geometry");
    assert!(joining.joins_existing_track());
    assert_eq!(joining.endpoints[0].connected, vec![existing]);
    assert!(joining.endpoints[1].connected.is_empty());

    // One tile further on the piece no longer reaches, and the preview says so
    // before anything is placed.
    let detached = sim
        .rail_placement_preview(straight, x, y + 3, Direction::North)
        .expect("a rail prototype previews its geometry");
    assert!(!detached.joins_existing_track());
}

/// Occupancy is what keeps rails from overlapping: a curve reserves its whole
/// footprint, so nothing else — track included — can be built under it.
#[test]
fn overlapping_rail_is_rejected() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = rail_test_area(&sim);
    let straight = entity_id_by_name(&sim.world.prototypes, "rail_straight");
    let curved = entity_id_by_name(&sim.world.prototypes, "rail_curved");

    place_rail(&mut sim, "rail_curved", x, y, Direction::North);

    for (prototype_id, tile_x, tile_y) in [
        (straight, x, y),
        (straight, x + 1, y + 1),
        (curved, x + 1, y),
    ] {
        let result = crate::placement::validate(
            &sim,
            crate::placement::EntityPlacementRequest {
                prototype_id,
                x: tile_x,
                y: tile_y,
                direction: Direction::North,
            },
        );
        assert!(
            matches!(result, Err(BuildError::EntityOccupied { .. })),
            "a rail overlapping the curve at ({tile_x}, {tile_y}) should be rejected, got {result:?}"
        );
    }
}

#[test]
fn destroying_a_piece_splits_the_run_it_was_holding_together() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = rail_test_area(&sim);

    let first = place_rail(&mut sim, "rail_straight", x, y, Direction::North);
    let middle = place_rail(&mut sim, "rail_straight", x, y + 2, Direction::North);
    let last = place_rail(&mut sim, "rail_straight", x, y + 4, Direction::North);
    sim.tick();
    assert_eq!(sim.rail_networks().len(), 1);

    crate::entity_mutation::remove(&mut sim, middle).expect("a placed rail can be removed");
    sim.tick();

    assert_eq!(sim.rail_networks().len(), 2);
    assert_ne!(
        sim.rail_network_id_for_entity(first),
        sim.rail_network_id_for_entity(last)
    );
    assert_eq!(sim.rail_network_id_for_entity(middle), None);
}

/// The graph is derived, so it is not saved — which means a loaded world has to
/// rebuild it rather than come back without one.
#[test]
fn a_loaded_world_rebuilds_the_rail_graph() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = rail_test_area(&sim);
    let first = place_rail(&mut sim, "rail_straight", x, y, Direction::North);
    let second = place_rail(&mut sim, "rail_straight", x, y + 2, Direction::North);
    sim.tick();

    let bytes = save_to_bytes(&sim).expect("a world with track should save");
    let loaded = load_from_bytes(&bytes).expect("a world with track should load");

    assert_eq!(loaded.state_hash(), sim.state_hash());
    assert_eq!(loaded.rail_networks(), sim.rail_networks());
    assert_eq!(
        loaded.rail_network_id_for_entity(first),
        loaded.rail_network_id_for_entity(second)
    );
}

/// A rebuild walks every piece in the world, so it must happen when track
/// changes and not when anything else does.
#[test]
fn the_graph_is_only_rebuilt_when_track_changes() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = rail_test_area(&sim);
    place_rail(&mut sim, "rail_straight", x, y, Direction::North);
    sim.tick();

    let rebuilds = sim.rails.graph_rebuilds;
    sim.tick();
    assert_eq!(sim.rails.graph_rebuilds, rebuilds, "an idle tick rebuilds");

    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    place_at(&mut sim, chest, x + 4, y, Direction::North);
    sim.tick();
    assert_eq!(
        sim.rails.graph_rebuilds, rebuilds,
        "placing a chest should not touch the rail graph"
    );

    place_rail(&mut sim, "rail_straight", x, y + 2, Direction::North);
    sim.tick();
    assert_eq!(sim.rails.graph_rebuilds, rebuilds + 1);
}

/// Rotating a straight piece has to move its geometry with it, or a rotated rail
/// would connect where it is not drawn.
#[test]
fn rotating_a_piece_moves_its_travel_geometry() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = rail_test_area(&sim);
    let rail = place_rail(&mut sim, "rail_straight", x, y, Direction::North);

    let vertical = sim
        .rail_piece_geometry(rail)
        .expect("a placed rail has geometry");
    assert_eq!(vertical.curve, RailCurveGeometry::Straight);
    assert_eq!(
        vertical.endpoints[0].position,
        RailPoint::new(x * 1024 + 512, y * 1024)
    );

    crate::entity_mutation::rotate(&mut sim, rail, Direction::East)
        .expect("a placed rail can be rotated");

    let horizontal = sim
        .rail_piece_geometry(rail)
        .expect("a rotated rail has geometry");
    assert_eq!(
        horizontal.endpoints[0].position,
        RailPoint::new((x + 2) * 1024, y * 1024 + 512)
    );
    assert_eq!(horizontal.endpoints[0].direction, Direction::East);
}

/// Track goes through the ordinary ghost path like any other entity, and a
/// ghost that lost its direction would build a rail running the wrong way.
#[test]
fn a_rail_ghost_builds_track_facing_the_planned_direction() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = rail_test_area(&sim);
    let curved = entity_id_by_name(&sim.world.prototypes, "rail_curved");
    let rail_item = item_id_by_name(&sim.world.prototypes, "rail");

    complete_research_with_prerequisites(&mut sim, "railway");

    let ghost_id = construction_ops::place_ghost(
        &mut sim,
        crate::simulation::construction_ops::GhostPlacementRequest {
            prototype_id: curved,
            x,
            y,
            direction: Direction::East,
            recipe: None,
        },
    )
    .expect("a rail ghost should be placeable");
    assert_eq!(
        sim.construction().queue().collect::<Vec<_>>(),
        vec![crate::construction::ConstructionJob::BuildGhost(ghost_id)]
    );

    sim.player_inventory
        .insert(&sim.world.prototypes.clone(), rail_item, 1)
        .expect("the player inventory should accept a rail");
    let entity_id = construction_ops::build_ghost_from_player_inventory(&mut sim, ghost_id)
        .expect("a rail ghost should build from the rail item");
    sim.tick();

    let placed = sim
        .entities
        .placed_entity(entity_id)
        .expect("the built rail is placed");
    assert_eq!(placed.direction, Direction::East);
    assert_eq!(
        sim.rail_piece_geometry(entity_id),
        crate::simulation::rail_ops::rail_geometry_in_footprint(
            sim.world
                .prototypes
                .entity(curved)
                .expect("the curved rail prototype exists"),
            Direction::East,
        )
        .map(|local| shift_geometry(local, x * 1024, y * 1024))
    );
    sim.validate_state()
        .expect("a world with built track should validate");
}

/// Blueprints capture and replay a curve's direction, which is the only thing
/// that distinguishes one quarter turn from the other three.
#[test]
fn blueprints_round_trip_a_curve_with_its_direction() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = rail_test_area(&sim);
    complete_research_with_prerequisites(&mut sim, "railway");
    place_rail(&mut sim, "rail_curved", x, y, Direction::West);

    let blueprint = sim
        .capture_blueprint("track", x, y, x + 1, y + 1)
        .expect("an area with track should capture");

    assert_eq!(blueprint.entities.len(), 1);
    assert_eq!(blueprint.entities[0].direction, Direction::West);

    let (placed, skipped) =
        construction_ops::paste_blueprint_ghosts(&mut sim, &blueprint.entities, x + 2, y);
    assert_eq!((placed, skipped), (1, 0));
    let ghost = sim
        .construction()
        .ghost_at(x + 2, y)
        .expect("the pasted curve leaves a ghost");
    assert_eq!(ghost.direction, Direction::West);
}

/// Shifts footprint-local geometry to a world origin, so a placed piece can be
/// compared against the geometry its prototype declares.
fn shift_geometry(
    mut geometry: crate::rails::RailPieceGeometry,
    origin_x: i64,
    origin_y: i64,
) -> crate::rails::RailPieceGeometry {
    let shift = |point: &mut RailPoint| {
        point.x += origin_x;
        point.y += origin_y;
    };
    for endpoint in &mut geometry.endpoints {
        shift(&mut endpoint.position);
    }
    if let RailCurveGeometry::Arc { center, .. } = &mut geometry.curve {
        shift(center);
    }
    geometry
}
