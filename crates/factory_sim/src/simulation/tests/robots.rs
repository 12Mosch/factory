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

/// Places one roboport, stocks it with robots, and fills its charging buffer.
///
/// Dispatch pays for a robot's full charge out of the buffer, so a fixture that
/// only placed a roboport could never send anything; filling the buffer
/// directly stands in for the power fixture the flight rules do not need.
fn stocked_roboport(sim: &mut Simulation, robot_count: u16) -> EntityId {
    let roboport = place_roboport_row(sim, &[0])[0];
    sim.tick();
    let catalog = sim.world.prototypes.clone();
    let robot_item = item_id(&catalog, "construction_robot");
    let capacity = sim
        .entity_roboport_status(roboport)
        .expect("a placed roboport reports status")
        .charge_capacity_joules;

    let state = sim
        .entities
        .roboport_state_mut(roboport)
        .expect("roboport state exists");
    state
        .robots
        .insert(&catalog, robot_item, robot_count)
        .expect("the robot slots should hold the fixture's robots");
    state.charge_energy_joules = capacity;
    sim.robots.mark_roboport_dirty(roboport);
    sim.tick();
    roboport
}

fn roboport_tile(sim: &Simulation, roboport: EntityId) -> (WorldTileCoord, WorldTileCoord) {
    let placed = sim
        .entities
        .placed_entity(roboport)
        .expect("a placed roboport has a footprint");
    (placed.footprint.x, placed.footprint.y)
}

fn robot_energy(sim: &Simulation, robot_id: RobotId) -> u64 {
    sim.robot(robot_id)
        .expect("the robot should still be in flight")
        .energy_joules
}

/// Ticks until `predicate` holds, or panics after `limit` ticks.
fn tick_until(
    sim: &mut Simulation,
    limit: usize,
    predicate: impl Fn(&Simulation) -> bool,
) -> usize {
    for ticks in 0..limit {
        if predicate(sim) {
            return ticks;
        }
        sim.tick();
    }
    panic!("condition did not hold within {limit} ticks");
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

/// The two roboport inventories accept disjoint item sets: repair packs land in
/// the material slots and robots in the robot slots, whichever half a single
/// click tries first.
#[test]
fn stocking_a_roboport_routes_robots_and_repair_packs_to_their_own_slots() {
    let mut sim = Simulation::new_test_world(123);
    let roboports = place_roboport_row(&mut sim, &[0]);
    let repair_pack = item_id(&sim.world.prototypes, "repair_pack");
    let robot = item_id(&sim.world.prototypes, "construction_robot");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, repair_pack, 4);
    set_inventory_slot(&mut sim.player_inventory, 1, robot, 10);

    crate::entity_transfer::player_slot_to_roboport(&mut sim, roboports[0], 0)
        .expect("a roboport should accept repair packs");
    crate::entity_transfer::player_slot_to_roboport(&mut sim, roboports[0], 1)
        .expect("a roboport should accept robots");

    let state = sim
        .entities
        .roboport_state(roboports[0])
        .expect("roboport state exists");
    assert_eq!(state.materials.count(repair_pack), 4);
    assert_eq!(state.robots.count(robot), 10);
    assert_eq!(state.robots.count(repair_pack), 0);
    assert_eq!(state.materials.count(robot), 0);
    sim.validate()
        .expect("a stocked roboport should stay valid");
}

/// The whole errand in one test: leave the roboport, reach the target, come
/// back, and become an item in the robot slots again.
#[test]
fn a_dispatched_robot_flies_to_its_target_and_docks_again() {
    let mut sim = Simulation::new_test_world(123);
    let roboport = stocked_roboport(&mut sim, 1);
    let robot_item = item_id(&sim.world.prototypes, "construction_robot");
    let (x, y) = roboport_tile(&sim, roboport);
    let target = (x + 5, y + 3);

    let robot_id = sim
        .dispatch_robot(roboport, target.0, target.1)
        .expect("a stocked, charged roboport should dispatch");
    assert_eq!(sim.robot_count(), 1);
    assert_eq!(
        sim.entities
            .roboport_state(roboport)
            .expect("roboport state exists")
            .robots
            .count(robot_item),
        0,
        "a dispatched robot leaves the robot slots"
    );

    tick_until(&mut sim, 600, |sim| {
        sim.robot(robot_id)
            .is_none_or(|robot| robot.errand.is_none())
    });
    let robot = sim
        .robot(robot_id)
        .expect("the robot should still be flying home");
    assert_eq!(
        (robot.x, robot.y),
        (
            crate::simulation::POSITION_SCALE * target.0 + crate::simulation::POSITION_SCALE / 2,
            crate::simulation::POSITION_SCALE * target.1 + crate::simulation::POSITION_SCALE / 2
        ),
        "the robot should stand exactly on its errand target before turning around"
    );

    tick_until(&mut sim, 600, |sim| sim.robot_count() == 0);
    assert_eq!(
        sim.entities
            .roboport_state(roboport)
            .expect("roboport state exists")
            .robots
            .count(robot_item),
        1,
        "a docked robot is an item in the robot slots again"
    );
    sim.validate()
        .expect("a completed errand should leave valid state");
}

#[test]
fn dispatch_needs_a_stationed_robot() {
    let mut sim = Simulation::new_test_world(123);
    let roboport = stocked_roboport(&mut sim, 0);
    let (x, y) = roboport_tile(&sim, roboport);

    assert_eq!(
        sim.dispatch_robot(roboport, x + 2, y),
        Err(RobotDispatchError::NoRobotAvailable)
    );
    assert_eq!(sim.robot_count(), 0);
}

/// Robots leave fully charged, so an unpowered roboport that never filled its
/// buffer refuses to send one rather than stranding it a few tiles out.
#[test]
fn dispatch_needs_a_full_charge_in_the_buffer() {
    let mut sim = Simulation::new_test_world(123);
    let roboport = stocked_roboport(&mut sim, 2);
    let robot_item = item_id(&sim.world.prototypes, "construction_robot");
    let (x, y) = roboport_tile(&sim, roboport);
    sim.entities
        .roboport_state_mut(roboport)
        .expect("roboport state exists")
        .charge_energy_joules = 0;
    sim.robots.mark_roboport_dirty(roboport);

    let error = sim
        .dispatch_robot(roboport, x + 2, y)
        .expect_err("an empty buffer cannot pay for a robot's charge");

    assert!(matches!(
        error,
        RobotDispatchError::InsufficientCharge {
            available_joules: 0,
            ..
        }
    ));
    assert_eq!(sim.robot_count(), 0);
    assert_eq!(
        sim.entities
            .roboport_state(roboport)
            .expect("roboport state exists")
            .robots
            .count(robot_item),
        2,
        "a refused dispatch consumes nothing"
    );
}

/// Flying costs energy; hovering in a charging queue does not. Otherwise a
/// robot waiting behind a full set of pads could drain itself into a state it
/// can never leave.
#[test]
fn flying_spends_energy_and_queuing_does_not() {
    let mut sim = Simulation::new_test_world(123);
    let roboport = stocked_roboport(&mut sim, 1);
    let (x, y) = roboport_tile(&sim, roboport);
    let robot_id = sim
        .dispatch_robot(roboport, x + 40, y)
        .expect("a stocked, charged roboport should dispatch");
    let full = robot_energy(&sim, robot_id);

    sim.tick();
    let after_one_tick = robot_energy(&sim, robot_id);
    sim.tick();
    let after_two_ticks = robot_energy(&sim, robot_id);

    assert!(after_one_tick < full);
    assert_eq!(
        full - after_one_tick,
        after_one_tick - after_two_ticks,
        "flight drain is a flat per-tick cost"
    );

    sim.robot_flights
        .robots
        .get_mut(&robot_id)
        .expect("the robot is in flight")
        .activity = RobotActivity::Queued(roboport);
    sim.robot_flights
        .charging
        .entry(roboport)
        .or_default()
        .queue
        .push_back(robot_id);
    let queued_energy = robot_energy(&sim, robot_id);
    sim.tick();

    assert!(robot_energy(&sim, robot_id) >= queued_energy);
}

/// Running dry is survivable: the robot crawls to the nearest roboport in its
/// own network, charges, and carries on with the errand it was on.
#[test]
fn a_robot_that_runs_dry_diverts_to_a_roboport_and_resumes() {
    let mut sim = Simulation::new_test_world(123);
    let roboport = stocked_roboport(&mut sim, 1);
    let (x, y) = roboport_tile(&sim, roboport);
    let robot_id = sim
        .dispatch_robot(roboport, x + 10, y)
        .expect("a stocked, charged roboport should dispatch");
    for _ in 0..20 {
        sim.tick();
    }
    let errand = sim.robot(robot_id).expect("the robot is in flight").errand;
    sim.robot_flights
        .robots
        .get_mut(&robot_id)
        .expect("the robot is in flight")
        .energy_joules = 0;

    sim.tick();
    assert!(
        matches!(
            sim.robot(robot_id).expect("the robot is in flight").activity,
            RobotActivity::SeekingCharge(diverted) if diverted == roboport
        ),
        "an empty robot heads for a roboport instead of stalling"
    );

    tick_until(&mut sim, 600, |sim| {
        matches!(
            sim.robot(robot_id).map(|robot| robot.activity),
            Some(RobotActivity::Charging(_))
        )
    });
    tick_until(&mut sim, 600, |sim| {
        matches!(
            sim.robot(robot_id).map(|robot| robot.activity),
            Some(RobotActivity::Flying)
        )
    });

    let robot = sim.robot(robot_id).expect("the robot is in flight");
    assert_eq!(robot.errand, errand, "a charging stop keeps the errand");
    assert!(robot.energy_joules > 0);
    sim.validate().expect("a charging detour should stay valid");
}

/// Charging is a throughput limit, not a free service: a roboport charges as
/// many robots as it has pads and the rest wait in arrival order.
#[test]
fn arrivals_beyond_the_pad_count_queue_in_arrival_order() {
    let mut sim = Simulation::new_test_world(123);
    let roboport = stocked_roboport(&mut sim, 6);
    let (x, y) = roboport_tile(&sim, roboport);
    let pad_count = usize::from(
        sim.world
            .prototypes
            .entity(
                sim.entities
                    .placed_entity(roboport)
                    .expect("a placed roboport")
                    .prototype_id,
            )
            .and_then(|prototype| prototype.roboport)
            .expect("roboport metadata")
            .charging_pad_count,
    );

    let robot_ids = (0..6)
        .map(|index| {
            let robot_id = sim
                .dispatch_robot(roboport, x + 1 + index, y)
                .expect("a stocked, charged roboport should dispatch");
            sim.robot_flights
                .robots
                .get_mut(&robot_id)
                .expect("the robot is in flight")
                .energy_joules = 0;
            robot_id
        })
        .collect::<Vec<_>>();

    tick_until(&mut sim, 600, |sim| {
        sim.roboport_charging_state(roboport)
            .is_some_and(|state| state.charging.len() + state.queue.len() == robot_ids.len())
    });

    let state = sim
        .roboport_charging_state(roboport)
        .expect("robots should be charging here");
    assert_eq!(state.charging.len(), pad_count);
    assert_eq!(state.queue.len(), robot_ids.len() - pad_count);
    assert_eq!(
        state.charging.iter().copied().collect::<Vec<_>>(),
        robot_ids[..pad_count],
        "the first arrivals take the pads"
    );
    assert_eq!(
        state.queue.iter().copied().collect::<Vec<_>>(),
        robot_ids[pad_count..],
        "the rest wait in arrival order"
    );
    sim.validate()
        .expect("a full charging queue should be valid");

    // The queue drains: every robot eventually charges and docks.
    tick_until(&mut sim, 4_000, |sim| sim.robot_count() == 0);
}

/// A robot outlives the roboport it came from, so destroying one must not leave
/// robots pointing at it — they adopt another roboport instead.
#[test]
fn destroying_a_roboport_rehomes_the_robots_it_was_charging() {
    let mut sim = Simulation::new_test_world(123);
    let roboports = place_roboport_row(&mut sim, &[0, CONNECTING_GAP]);
    sim.tick();
    let catalog = sim.world.prototypes.clone();
    let robot_item = item_id(&catalog, "construction_robot");
    let capacity = sim
        .entity_roboport_status(roboports[0])
        .expect("a placed roboport reports status")
        .charge_capacity_joules;
    let state = sim
        .entities
        .roboport_state_mut(roboports[0])
        .expect("roboport state exists");
    state
        .robots
        .insert(&catalog, robot_item, 1)
        .expect("the robot slots should hold one robot");
    state.charge_energy_joules = capacity;
    sim.robots.mark_roboport_dirty(roboports[0]);
    sim.tick();

    let (x, y) = roboport_tile(&sim, roboports[0]);
    let robot_id = sim
        .dispatch_robot(roboports[0], x + 3, y)
        .expect("a stocked, charged roboport should dispatch");
    sim.robot_flights
        .robots
        .get_mut(&robot_id)
        .expect("the robot is in flight")
        .energy_joules = 0;
    tick_until(&mut sim, 600, |sim| {
        matches!(
            sim.robot(robot_id).map(|robot| robot.activity),
            Some(RobotActivity::Charging(_))
        )
    });

    crate::entity_mutation::destroy_to_player_inventory(&mut sim, roboports[0])
        .expect("a placed roboport should be removable");

    let robot = sim
        .robot(robot_id)
        .expect("the robot survives its roboport");
    assert_eq!(robot.activity, RobotActivity::Flying);
    assert_eq!(robot.home_roboport, None);
    assert!(sim.roboport_charging_state(roboports[0]).is_none());
    sim.validate()
        .expect("destroying a roboport should leave valid state");

    sim.tick();
    assert_eq!(
        sim.robot(robot_id)
            .expect("the robot is in flight")
            .home_roboport,
        Some(roboports[1]),
        "the surviving roboport adopts the orphaned robot"
    );
}

#[test]
fn robots_in_flight_survive_a_save_load_round_trip() {
    let mut sim = Simulation::new_test_world(123);
    let roboport = stocked_roboport(&mut sim, 3);
    let (x, y) = roboport_tile(&sim, roboport);
    for index in 0..3 {
        sim.dispatch_robot(roboport, x + 8 + index, y + index)
            .expect("a stocked, charged roboport should dispatch");
    }
    for _ in 0..40 {
        sim.tick();
    }
    let before = sim.state_hash();
    let positions = sim
        .robots()
        .map(|robot| (robot.id, robot.x, robot.y, robot.energy_joules))
        .collect::<Vec<_>>();

    let bytes = crate::save_to_bytes(&sim).expect("a world with robots in flight should save");
    let mut loaded = crate::load_from_bytes(&bytes).expect("a saved robot world should load");

    assert_eq!(loaded.state_hash(), before);
    assert_eq!(
        loaded
            .robots()
            .map(|robot| (robot.id, robot.x, robot.y, robot.energy_joules))
            .collect::<Vec<_>>(),
        positions
    );

    // Loading must also reproduce what happens next, not just what was stored.
    for _ in 0..40 {
        sim.tick();
        loaded.tick();
    }
    assert_eq!(loaded.state_hash(), sim.state_hash());
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
