use super::super::*;
use super::support::*;

/// Roboports are 4x4 with a 25-tile logistic radius, so two placed 50 tiles
/// apart still share a logistic edge and a 51-tile gap splits them. Tests below
/// use these two spacings rather than magic numbers.
const CONNECTING_GAP: WorldTileCoord = 50;
const SPLITTING_GAP: WorldTileCoord = 51;

fn place_roboport(sim: &mut Simulation, x: WorldTileCoord, y: WorldTileCoord) -> EntityId {
    let roboport = entity_id_by_name(&sim.world.prototypes, "roboport");
    place_at(sim, roboport, x, y, Direction::North)
}

/// Places roboports at the given x offsets from one clear origin.
///
/// The world is generated, so an arbitrary strip may contain water or ore;
/// placing directly with [`place_at`] would panic there. This searches for an
/// origin where every requested spot is buildable instead.
fn place_roboport_row(sim: &mut Simulation, offsets: &[WorldTileCoord]) -> Vec<EntityId> {
    let roboport = entity_id_by_name(&sim.world.prototypes, "roboport");
    for (x, y) in all_tile_coords(&sim.world) {
        let row_is_placeable = offsets.iter().all(|offset| {
            crate::placement::validate(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: roboport,
                    x: x + offset,
                    y,
                    direction: Direction::North,
                },
            )
            .is_ok()
        });
        if !row_is_placeable {
            continue;
        }
        return offsets
            .iter()
            .map(|offset| place_roboport(sim, x + offset, y))
            .collect();
    }

    panic!("expected a placeable roboport row");
}

fn network_ids(sim: &Simulation, roboports: &[EntityId]) -> Vec<Option<u32>> {
    roboports
        .iter()
        .map(|entity_id| sim.robot_network_id_for_entity(*entity_id))
        .collect()
}

#[test]
fn overlapping_logistic_areas_join_one_network() {
    let mut sim = Simulation::new_test_world(123);
    let roboports = place_roboport_row(&mut sim, &[0, CONNECTING_GAP]);

    sim.tick();

    assert_eq!(sim.robot_networks().len(), 1);
    assert_eq!(sim.robot_networks()[0].roboports.len(), 2);
    assert_eq!(network_ids(&sim, &roboports), vec![Some(0), Some(0)]);
}

#[test]
fn separated_logistic_areas_stay_separate_networks() {
    let mut sim = Simulation::new_test_world(123);
    let roboports = place_roboport_row(&mut sim, &[0, SPLITTING_GAP]);

    sim.tick();

    assert_eq!(sim.robot_networks().len(), 2);
    assert_eq!(network_ids(&sim, &roboports), vec![Some(0), Some(1)]);
}

/// The outer two roboports are far apart, but the middle one reaches both, so
/// all three share a network. Without transitive merging this would be three.
#[test]
fn a_middle_roboport_merges_two_networks() {
    let mut sim = Simulation::new_test_world(123);
    let roboports = place_roboport_row(&mut sim, &[0, CONNECTING_GAP, 2 * CONNECTING_GAP]);

    sim.tick();

    assert_eq!(sim.robot_networks().len(), 1);
    assert_eq!(
        network_ids(&sim, &roboports),
        vec![Some(0), Some(0), Some(0)]
    );
}

/// Removing the bridge splits the network again, which is the invalidation path
/// that matters: destroying a roboport has to rebuild the topology.
#[test]
fn destroying_the_bridge_splits_the_network() {
    let mut sim = Simulation::new_test_world(123);
    let roboports = place_roboport_row(&mut sim, &[0, CONNECTING_GAP, 2 * CONNECTING_GAP]);
    sim.tick();
    assert_eq!(sim.robot_networks().len(), 1);

    crate::entity_mutation::destroy_to_player_inventory(&mut sim, roboports[1])
        .expect("a placed roboport should be removable");
    sim.tick();

    assert_eq!(sim.robot_networks().len(), 2);
    assert_eq!(
        network_ids(&sim, &[roboports[0], roboports[2]]),
        vec![Some(0), Some(1)]
    );
    sim.validate()
        .expect("removing a roboport should leave valid state");
}

/// Construction coverage is the union of the per-roboport squares, so the far
/// corner of an L-shaped network's bounding box is *not* covered even though
/// the bounding box contains it.
#[test]
fn construction_coverage_is_the_union_not_the_bounding_box() {
    let mut sim = Simulation::new_test_world(123);
    let roboports = place_roboport_row(&mut sim, &[0, CONNECTING_GAP]);
    sim.tick();

    let first = sim
        .entity_roboport_status(roboports[0])
        .expect("a placed roboport reports status");
    let second = sim
        .entity_roboport_status(roboports[1])
        .expect("a placed roboport reports status");
    let network = &sim.robot_networks()[0];

    // A tile just outside both construction squares but inside the network's
    // bounding box: same x as the first roboport, same y-range as neither.
    let outside_x = first.construction_bounds.min_x;
    let outside_y = second.construction_bounds.max_y + 1;
    let bounding_box_corner_x = second.construction_bounds.max_x;

    assert!(
        network
            .construction_bounds
            .contains(outside_x, outside_y - 1)
    );
    assert!(
        network
            .construction_bounds
            .contains(bounding_box_corner_x, first.construction_bounds.min_y)
    );
    assert_eq!(
        sim.construction_network_covering_tile(
            first.construction_bounds.min_x,
            first.construction_bounds.min_y
        ),
        Some(0)
    );
    assert_eq!(
        sim.construction_network_covering_tile(outside_x, outside_y),
        None
    );
}

#[test]
fn tiles_outside_every_construction_square_are_uncovered() {
    let mut sim = Simulation::new_test_world(123);
    let roboports = place_roboport_row(&mut sim, &[0]);
    sim.tick();

    let bounds = sim
        .entity_roboport_status(roboports[0])
        .expect("a placed roboport reports status")
        .construction_bounds;
    let inside_y = bounds.min_y;

    assert_eq!(
        sim.construction_network_covering_tile(bounds.max_x, inside_y),
        Some(0)
    );
    assert_eq!(
        sim.construction_network_covering_tile(bounds.max_x + 1, inside_y),
        None
    );
    assert_eq!(
        sim.construction_network_covering_tile(bounds.max_x, bounds.min_y - 1),
        None
    );
}

/// Placing an unrelated entity must not force a robot-network rebuild: only
/// roboports define the graph, and rebuilding walks every one of them.
#[test]
fn unrelated_placement_does_not_rebuild_the_robot_topology() {
    let mut sim = Simulation::new_test_world(123);
    place_roboport_row(&mut sim, &[0]);
    sim.tick();
    let rebuilds = sim.robot_topology_rebuild_count();

    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let (x, y) = first_placeable_entity_tile(&sim, chest, Direction::North);
    place_at(&mut sim, chest, x, y, Direction::North);
    sim.tick();

    assert_eq!(sim.robot_topology_rebuild_count(), rebuilds);
}

#[test]
fn placing_a_roboport_rebuilds_the_robot_topology() {
    let mut sim = Simulation::new_test_world(123);
    place_roboport_row(&mut sim, &[0]);
    sim.tick();
    let rebuilds = sim.robot_topology_rebuild_count();

    place_roboport_row(&mut sim, &[0]);
    sim.tick();

    assert_eq!(sim.robot_topology_rebuild_count(), rebuilds + 1);
}

/// An unpowered roboport draws its idle drain and never fills its buffer, so
/// the whole "powered roboport" story is visible in the charge total.
#[test]
fn an_unpowered_roboport_never_charges() {
    let mut sim = Simulation::new_test_world(123);
    let roboports = place_roboport_row(&mut sim, &[0]);

    for _ in 0..10 {
        sim.tick();
    }

    let status = sim
        .entity_roboport_status(roboports[0])
        .expect("a placed roboport reports status");
    assert_eq!(status.charge_energy_joules, 0);
    assert_eq!(sim.robot_networks()[0].charge_energy_joules, 0);
    assert_eq!(
        sim.machine_status_for_entity(roboports[0]),
        Some(MachineStatus::NoPower)
    );
}

#[test]
fn a_powered_roboport_fills_its_charging_buffer() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy, _) = place_powered_fixture_origin_with_boiler(&mut sim, 4, 4, (0, 5));
    let roboport = entity_id_by_name(&sim.world.prototypes, "roboport");
    let roboport_id = place_at(&mut sim, roboport, ox, oy, Direction::North);

    for _ in 0..30 {
        sim.tick();
    }

    let status = sim
        .entity_roboport_status(roboport_id)
        .expect("a placed roboport reports status");
    assert!(
        status.charge_energy_joules > 0,
        "a powered roboport should have started filling its buffer"
    );
    assert!(status.charge_energy_joules < status.charge_capacity_joules);
    assert_eq!(
        sim.robot_networks()[0].charge_energy_joules,
        status.charge_energy_joules
    );
    assert_eq!(
        sim.machine_status_for_entity(roboport_id),
        Some(MachineStatus::Working)
    );
    sim.validate().expect("a charging roboport should be valid");
}

/// Once the buffer is full the roboport stops asking for charging power, which
/// is what returns it to its idle drain instead of a permanent claim.
#[test]
fn a_full_roboport_drops_to_its_idle_drain() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy, _) = place_powered_fixture_origin_with_boiler(&mut sim, 4, 4, (0, 5));
    let roboport = entity_id_by_name(&sim.world.prototypes, "roboport");
    let roboport_id = place_at(&mut sim, roboport, ox, oy, Direction::North);
    sim.tick();
    let capacity = sim
        .entity_roboport_status(roboport_id)
        .expect("a placed roboport reports status")
        .charge_capacity_joules;
    sim.entities
        .roboports
        .get_mut(&roboport_id)
        .expect("roboport state exists")
        .charge_energy_joules = capacity;
    // Mirrors what the charging pass does after touching a buffer, so the
    // network snapshot still describes its members.
    sim.robots.mark_roboport_dirty(roboport_id);

    sim.tick();

    let power = sim
        .entity_power_status(roboport_id)
        .expect("a roboport is an electric consumer");
    assert_eq!(power.active_usage_watts, 0);
    assert!(power.drain_watts > 0);
    assert_eq!(
        sim.machine_status_for_entity(roboport_id),
        Some(MachineStatus::Idle)
    );
}

/// Repair packs belong in the material slots; the robot slots hold robots,
/// which do not exist yet, so nothing may be inserted there.
#[test]
fn repair_packs_go_to_the_material_slots_and_robot_slots_reject_them() {
    let mut sim = Simulation::new_test_world(123);
    let roboports = place_roboport_row(&mut sim, &[0]);
    let repair_pack = item_id(&sim.world.prototypes, "repair_pack");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, repair_pack, 4);

    crate::entity_transfer::player_slot_to_roboport(&mut sim, roboports[0], 0)
        .expect("a roboport should accept repair packs");

    let state = sim
        .entities
        .roboport_state(roboports[0])
        .expect("roboport state exists");
    assert_eq!(state.materials.count(repair_pack), 4);
    assert_eq!(state.robots.count(repair_pack), 0);
    sim.validate()
        .expect("stocked repair material should stay valid");
}

/// Roboports join the aggregated diagnostics alongside every other powered
/// machine, so a network stalled for want of power is visible in the status
/// panel rather than only on the roboport itself.
#[test]
fn roboports_appear_in_the_aggregated_machine_statuses() {
    let mut sim = Simulation::new_test_world(123);
    place_roboport_row(&mut sim, &[0]);
    sim.tick();

    let snapshot = sim.machine_statuses();
    let group = snapshot
        .groups
        .iter()
        .find(|group| group.kind == EntityKind::Roboport)
        .expect("an unpowered roboport should report a status group");

    assert_eq!(
        group
            .counts
            .iter()
            .find(|count| count.status == MachineStatus::NoPower)
            .map(|count| count.count),
        Some(1)
    );
}

/// Validation re-checks the slot policies, so contents no insertion path could
/// have produced — a catalog-valid stack in the wrong half — are still caught.
#[test]
fn roboport_slots_holding_the_wrong_item_fail_validation() {
    let mut sim = Simulation::new_test_world(123);
    let roboports = place_roboport_row(&mut sim, &[0]);
    let repair_pack = item_id(&sim.world.prototypes, "repair_pack");
    let catalog = sim.world.prototypes.clone();
    sim.tick();
    sim.validate().expect("an empty roboport should be valid");

    sim.entities
        .roboport_state_mut(roboports[0])
        .expect("roboport state exists")
        .robots
        .insert(&catalog, repair_pack, 1)
        .expect("the robot slots have room for a stack");

    assert_eq!(
        sim.validate(),
        Err(SimValidationError::InvalidRoboportState {
            entity_id: roboports[0]
        })
    );
}

#[test]
fn destroying_a_roboport_recovers_its_stocked_material() {
    let mut sim = Simulation::new_test_world(123);
    let roboports = place_roboport_row(&mut sim, &[0]);
    let repair_pack = item_id(&sim.world.prototypes, "repair_pack");
    sim.entities
        .roboport_state_mut(roboports[0])
        .expect("roboport state exists")
        .materials
        .insert(&sim.world.prototypes.clone(), repair_pack, 3)
        .expect("material slots should accept repair packs");
    let before = sim.player_inventory.count(repair_pack);

    crate::entity_mutation::destroy_to_player_inventory(&mut sim, roboports[0])
        .expect("a placed roboport should be removable");

    assert_eq!(sim.player_inventory.count(repair_pack), before + 3);
    assert!(!sim.entities.roboports.contains_key(&roboports[0]));
}

#[test]
fn robot_networks_survive_a_save_load_round_trip() {
    let mut sim = Simulation::new_test_world(123);
    place_roboport_row(&mut sim, &[0, CONNECTING_GAP]);
    sim.tick();
    let before = sim.state_hash();

    let bytes = crate::save_to_bytes(&sim).expect("a world with roboports should save");
    let loaded = crate::load_from_bytes(&bytes).expect("a saved roboport world should load");

    assert_eq!(loaded.state_hash(), before);
    assert_eq!(loaded.robot_networks(), sim.robot_networks());
}
