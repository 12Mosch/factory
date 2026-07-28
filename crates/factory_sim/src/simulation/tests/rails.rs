use super::super::*;
use super::support::*;
use crate::rail::{RailCurve, RailPoint};

/// One piece of a test layout: a prototype, its offset from the layout origin,
/// and the direction to place it in.
type LayoutPiece = (EntityPrototypeId, WorldTileCoord, WorldTileCoord, Direction);

fn straight_id(sim: &Simulation) -> EntityPrototypeId {
    entity_id_by_name(&sim.world.prototypes, "rail_straight")
}

fn curved_id(sim: &Simulation) -> EntityPrototypeId {
    entity_id_by_name(&sim.world.prototypes, "rail_curved")
}

/// The first origin tile where every piece of `layout` is placeable.
///
/// Searching by placement validity rather than by terrain alone is what lets a
/// test describe the shape it wants — a corner, a run with a gap — without
/// hand-picking tiles that happen to be clear of water, resources, and the
/// player.
fn layout_origin(sim: &Simulation, layout: &[LayoutPiece]) -> (WorldTileCoord, WorldTileCoord) {
    for (x, y) in all_tile_coords(&sim.world) {
        let placeable = layout.iter().all(|(prototype_id, dx, dy, direction)| {
            crate::placement::validate(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: *prototype_id,
                    x: x + dx,
                    y: y + dy,
                    direction: *direction,
                },
            )
            .is_ok()
        });
        if placeable {
            return (x, y);
        }
    }

    panic!("expected a placeable rail layout");
}

/// Places `layout` at the first origin that takes all of it, returning the
/// origin and one entity id per piece in layout order.
fn place_layout(
    sim: &mut Simulation,
    layout: &[LayoutPiece],
) -> ((WorldTileCoord, WorldTileCoord), Vec<EntityId>) {
    let (x, y) = layout_origin(sim, layout);
    let ids = layout
        .iter()
        .map(|(prototype_id, dx, dy, direction)| {
            place_at(sim, *prototype_id, x + dx, y + dy, *direction)
        })
        .collect();
    ((x, y), ids)
}

#[test]
fn a_straight_rail_reports_its_own_geometry_in_world_space() {
    let mut sim = Simulation::new_test_world(123);
    let straight = straight_id(&sim);
    let ((x, y), rails) = place_layout(&mut sim, &[(straight, 0, 0, Direction::North)]);
    sim.tick();

    let geometry = sim
        .rail_piece_geometry(rails[0])
        .expect("a placed rail reports geometry");

    assert_eq!(
        geometry.start.position,
        RailPoint::new(x * POSITION_SCALE + 512, y * POSITION_SCALE)
    );
    assert_eq!(geometry.start.heading, Direction::South);
    assert_eq!(
        geometry.end.position,
        RailPoint::new(x * POSITION_SCALE + 512, (y + 2) * POSITION_SCALE)
    );
    assert_eq!(geometry.end.heading, Direction::North);
    assert_eq!(geometry.curve, RailCurve::Straight);
    assert_eq!(geometry.length_fixed, 2 * POSITION_SCALE);
}

/// Rotating a rail rotates its travel path with it, which is what stops the
/// renderer and the graph from disagreeing about where an east-west rail runs.
#[test]
fn rotating_a_rail_rotates_its_path() {
    let mut sim = Simulation::new_test_world(123);
    let straight = straight_id(&sim);
    let ((x, y), rails) = place_layout(&mut sim, &[(straight, 0, 0, Direction::East)]);
    sim.tick();

    let geometry = sim
        .rail_piece_geometry(rails[0])
        .expect("a placed rail reports geometry");

    // A north-facing straight runs up the middle of its column; rotated east it
    // runs along the middle of its row, and its headings turn with it.
    assert_eq!(
        geometry.start.position,
        RailPoint::new(x * POSITION_SCALE, y * POSITION_SCALE + 512)
    );
    assert_eq!(geometry.start.heading, Direction::West);
    assert_eq!(
        geometry.end.position,
        RailPoint::new((x + 2) * POSITION_SCALE, y * POSITION_SCALE + 512)
    );
    assert_eq!(geometry.end.heading, Direction::East);
    assert_eq!(geometry.length_fixed, 2 * POSITION_SCALE);
}

#[test]
fn rails_laid_end_to_end_form_one_network_and_a_gap_splits_them() {
    let mut sim = Simulation::new_test_world(123);
    let straight = straight_id(&sim);
    // The third piece leaves the tiles at +4 empty, so it cannot join the pair.
    place_layout(
        &mut sim,
        &[
            (straight, 0, 0, Direction::North),
            (straight, 0, 2, Direction::North),
            (straight, 0, 6, Direction::North),
        ],
    );
    sim.tick();

    let piece_counts = sim
        .rail_networks()
        .iter()
        .map(|network| network.piece_count)
        .collect::<Vec<_>>();
    assert_eq!(piece_counts, vec![2, 1]);
    // Four tiles of track in the joined pair, and three ends between them.
    assert_eq!(
        sim.rail_networks()[0].total_length_fixed,
        4 * POSITION_SCALE
    );
    assert_eq!(sim.rail_networks()[0].node_count, 3);
}

/// Rails only join where an end and a heading match. Two straights side by side
/// touch along their whole length and still form two networks.
#[test]
fn parallel_rails_do_not_connect() {
    let mut sim = Simulation::new_test_world(123);
    let straight = straight_id(&sim);
    place_layout(
        &mut sim,
        &[
            (straight, 0, 0, Direction::North),
            (straight, 1, 0, Direction::North),
        ],
    );
    sim.tick();

    assert_eq!(sim.rail_networks().len(), 2);
}

/// The corner the piece set exists for: a straight into a curve into a straight
/// running the other way. If the curve's ends did not line up with the
/// straights', no corner could ever be built.
#[test]
fn a_curve_joins_two_perpendicular_straights() {
    let mut sim = Simulation::new_test_world(123);
    let straight = straight_id(&sim);
    let curved = curved_id(&sim);
    // The curve occupies a 2x2 block: it enters at the bottom of its west
    // column and leaves at the east edge of its north row.
    let (_, rails) = place_layout(
        &mut sim,
        &[
            (curved, 0, 2, Direction::North),
            (straight, 0, 0, Direction::North),
            (straight, 2, 3, Direction::East),
        ],
    );
    sim.tick();
    let (curve, below, right) = (rails[0], rails[1], rails[2]);

    assert_eq!(sim.rail_networks().len(), 1);
    assert_eq!(sim.rail_networks()[0].piece_count, 3);
    assert_eq!(
        sim.rail_piece_connections(curve),
        [Some(below), Some(right)]
    );
    assert_eq!(
        sim.rail_network_id_for_entity(below),
        sim.rail_network_id_for_entity(right)
    );
}

/// Every rotation of the curve has to join the track around it, or three of the
/// four corners would be unbuildable. The arc is symmetric about its diagonal,
/// so these four rotations are the whole corner set.
#[test]
fn every_curve_rotation_joins_the_track_around_it() {
    for direction in Direction::ALL {
        let mut sim = Simulation::new_test_world(123);
        let straight = straight_id(&sim);
        let curved = curved_id(&sim);
        // The continuing straights are derived from the curve's own rotated
        // geometry, so the layout is exactly "this curve plus the track that
        // should meet it" for whichever pair of ends this rotation exposes.
        let curve_prototype = sim
            .world
            .prototypes
            .entity(curved)
            .expect("the base catalog defines a curved rail");
        let curve_geometry = rail_ops::piece_geometry(curve_prototype, direction)
            .expect("a curved rail declares geometry");
        let mut layout = vec![(curved, 0, 0, direction)];
        layout.extend(curve_geometry.ends().map(|end| {
            let (dx, dy, straight_direction) =
                continuing_straight_placement(end.position, end.heading);
            (straight, dx, dy, straight_direction)
        }));

        let (_, rails) = place_layout(&mut sim, &layout);
        sim.tick();

        assert_eq!(
            sim.rail_networks().len(),
            1,
            "a {direction:?} curve should join the straights at both of its ends"
        );
        assert_eq!(sim.rail_networks()[0].piece_count, 3);
        assert_eq!(
            sim.rail_piece_connections(rails[0]),
            [Some(rails[1]), Some(rails[2])]
        );
    }
}

/// Origin tile and direction of the two-tile straight that continues the track
/// through an end at `position` heading `heading`.
///
/// Works in whatever frame `position` is given in: the maths is a division by
/// the tile size, so a prototype-local end yields an offset from the piece's own
/// origin and a world end yields a world tile.
fn continuing_straight_placement(
    position: RailPoint,
    heading: Direction,
) -> (WorldTileCoord, WorldTileCoord, Direction) {
    let (tile_x, tile_y) = position.tile();

    // A straight is placed from its minimum corner and covers two tiles, so one
    // running away to the south or the west starts two tiles back.
    match heading {
        Direction::North => (tile_x, tile_y, Direction::North),
        Direction::South => (tile_x, tile_y - 2, Direction::North),
        Direction::East => (tile_x, tile_y, Direction::East),
        Direction::West => (tile_x - 2, tile_y, Direction::East),
    }
}

#[test]
fn placing_and_removing_track_rebuilds_the_graph_once_each() {
    let mut sim = Simulation::new_test_world(123);
    let straight = straight_id(&sim);
    let ((x, y), rails) = place_layout(
        &mut sim,
        &[
            (straight, 0, 0, Direction::North),
            (straight, 0, 2, Direction::North),
        ],
    );
    crate::entity_mutation::remove(&mut sim, rails[1]).expect("a placed rail can be removed");
    sim.tick();
    let after_first = sim.rail_graph_rebuild_count();

    sim.tick();
    assert_eq!(
        sim.rail_graph_rebuild_count(),
        after_first,
        "an unchanged world must not rebuild the rail graph"
    );

    place_at(&mut sim, straight, x, y + 2, Direction::North);
    sim.tick();
    assert_eq!(sim.rail_graph_rebuild_count(), after_first + 1);
    assert_eq!(sim.rail_networks()[0].piece_count, 2);

    crate::entity_mutation::remove(&mut sim, rails[0]).expect("a placed rail can be removed");
    sim.tick();
    assert_eq!(sim.rail_graph_rebuild_count(), after_first + 2);
    assert_eq!(sim.rail_networks().len(), 1);
    assert_eq!(sim.rail_networks()[0].piece_count, 1);
}

/// Placing an unrelated entity must not cost a rail rebuild: connectivity only
/// changes when track does.
#[test]
fn unrelated_placement_leaves_the_rail_graph_alone() {
    let mut sim = Simulation::new_test_world(123);
    let straight = straight_id(&sim);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let ((x, y), _) = place_layout(&mut sim, &[(straight, 0, 0, Direction::North)]);
    sim.tick();
    let rebuilds = sim.rail_graph_rebuild_count();

    place_at(&mut sim, chest, x, y + 3, Direction::North);
    sim.tick();

    assert_eq!(sim.rail_graph_rebuild_count(), rebuilds);
}

/// The preview answers what a placement *would* join, before it is placed and
/// before the graph has seen it.
#[test]
fn the_placement_preview_reports_the_connection_it_would_form() {
    let mut sim = Simulation::new_test_world(123);
    let straight = straight_id(&sim);
    // The upper pieces only prove the room above the first is clear; they are
    // removed so the preview has somewhere to be previewed.
    let ((x, y), rails) = place_layout(
        &mut sim,
        &[
            (straight, 0, 0, Direction::North),
            (straight, 0, 2, Direction::North),
            (straight, 0, 4, Direction::North),
        ],
    );
    crate::entity_mutation::remove(&mut sim, rails[1]).expect("a placed rail can be removed");
    crate::entity_mutation::remove(&mut sim, rails[2]).expect("a placed rail can be removed");
    sim.tick();

    let joining = crate::placement::rail_connection_preview(
        &sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: straight,
            x,
            y: y + 2,
            direction: Direction::North,
        },
    );
    assert_eq!(joining.len(), 2);
    assert_eq!(joining[0].joins, Some(rails[0]));
    assert_eq!(joining[1].joins, None);

    // One tile further on, the ends no longer meet and nothing joins.
    let apart = crate::placement::rail_connection_preview(
        &sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: straight,
            x,
            y: y + 3,
            direction: Direction::North,
        },
    );
    assert!(apart.iter().all(|connection| connection.joins.is_none()));
}

/// Rails are ordinary placed entities: they round-trip through a save, and the
/// graph they form is rebuilt from them rather than stored.
#[test]
fn rails_round_trip_through_a_save_and_rebuild_their_graph() {
    let mut sim = Simulation::new_test_world(123);
    let straight = straight_id(&sim);
    let curved = curved_id(&sim);
    place_layout(
        &mut sim,
        &[
            (curved, 0, 2, Direction::North),
            (straight, 0, 0, Direction::North),
            (straight, 2, 3, Direction::East),
        ],
    );
    sim.tick();

    let bytes = crate::save_to_bytes(&sim).expect("a world with track should save");
    let loaded = crate::load_from_bytes(&bytes).expect("a world with track should load");

    assert_eq!(sim.state_hash(), loaded.state_hash());
    assert_eq!(loaded.rail_networks().len(), 1);
    assert_eq!(loaded.rail_networks()[0].piece_count, 3);
    assert_eq!(
        loaded.rail_networks()[0].total_length_fixed,
        sim.rail_networks()[0].total_length_fixed
    );
}
