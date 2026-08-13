use super::super::*;
use super::support::*;

/// Ticks a delivery scenario long enough for a robot to be matched, dispatched,
/// fly a leg of the network, and hand over.
///
/// Matching runs on one tick in eight per network and a chest can sit twenty
/// tiles from the roboport, so a delivery is tens of ticks of flight, not one.
const DELIVERY_TICKS: usize = 400;

/// A roboport stocked with logistic robots, and the tile it was placed on.
fn logistic_roboport(
    sim: &mut Simulation,
    robot_count: u16,
) -> (EntityId, (WorldTileCoord, WorldTileCoord)) {
    let (roboport, origin) = place_roboport(sim);
    station_robots(sim, roboport, "logistic_robot", robot_count);
    (roboport, origin)
}

/// Ticks until `predicate` holds, or panics after `limit` ticks.
fn tick_until(sim: &mut Simulation, limit: usize, predicate: impl Fn(&Simulation) -> bool) {
    for _ in 0..limit {
        if predicate(sim) {
            return;
        }
        sim.tick();
    }
    panic!("condition did not hold within {limit} ticks");
}

/// Ticks `count` times, checking the whole simulation stays valid throughout.
///
/// Deliveries move items between two inventories across many ticks, so a
/// mistake shows up as items appearing or vanishing rather than as a panic;
/// validating every tick is what catches that where it happens.
fn tick_validated(sim: &mut Simulation, count: usize) {
    for _ in 0..count {
        sim.tick();
        sim.validate().expect("a delivery keeps the world valid");
    }
}

/// Places the first machine delivery endpoint inside logistic coverage and
/// makes its rocket ready without spending hundreds of fixture ticks crafting.
fn place_ready_covered_rocket_silo(
    sim: &mut Simulation,
    near: (WorldTileCoord, WorldTileCoord),
) -> EntityId {
    unlock_with_prerequisites(sim, "rocket_silo");
    let prototype_id = entity_id_by_name(&sim.world.prototypes, "rocket_silo");
    let (x, y) = all_tile_coords(&sim.world)
        .into_iter()
        .filter(|(x, y)| {
            sim.logistic_network_covering_tile(*x + 4, *y + 4).is_some()
                && crate::placement::validate(
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
        .min_by_key(|(x, y)| (x + 4 - near.0).pow(2) + (y + 4 - near.1).pow(2))
        .expect("a roboport's logistic square should fit a rocket silo");
    let entity_id = place_at(sim, prototype_id, x, y, Direction::North);
    let silo = sim.entities.rocket_silos.get_mut(&entity_id).unwrap();
    silo.parts_completed = silo.parts_per_rocket;
    sim.entities.note_logistic_endpoint_changed(entity_id);
    entity_id
}

fn machine_delivery_fixture(
    robot_count: u16,
) -> (Simulation, EntityId, EntityId, EntityId, ItemId) {
    let mut sim = Simulation::new_test_world(123);
    let satellite = item_id(&sim.world.prototypes, "satellite");
    let (_, origin) = logistic_roboport(&mut sim, robot_count);
    let silo = place_ready_covered_rocket_silo(&mut sim, origin);
    let provider = place_covered_chest(&mut sim, "passive_provider_chest", origin);
    let storage = place_covered_chest(&mut sim, "storage_chest", origin);
    insert_into_chest(&mut sim, provider, satellite, 1);
    (sim, silo, provider, storage, satellite)
}

#[test]
fn a_logistic_robot_delivers_one_satellite_to_a_ready_silo() {
    let (mut sim, silo, provider, _, satellite) = machine_delivery_fixture(4);

    assert_eq!(
        sim.logistic_network_id_for_chest(silo),
        None,
        "a machine endpoint must not change the chest-only public accessor"
    );

    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        sim.entities.rocket_silos[&silo]
            .cargo_inventory
            .count(satellite)
            == 1
    });

    assert_eq!(chest_count(&sim, provider, satellite), 0);
    assert_eq!(
        sim.entities.rocket_silos[&silo]
            .cargo_inventory
            .count(satellite),
        1
    );
    sim.validate()
        .expect("a machine delivery leaves valid state");
}

#[test]
fn a_machine_slot_has_only_one_in_flight_delivery() {
    let (mut sim, silo, provider, _, satellite) = machine_delivery_fixture(8);
    // Two source items make the inbound reservation the only thing preventing
    // a second robot from claiming the one-slot destination.
    insert_into_chest(&mut sim, provider, satellite, 1);

    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        sim.robots().any(|robot| {
            robot
                .delivery
                .is_some_and(|delivery| delivery.destination == silo)
        })
    });
    tick_validated(&mut sim, 8);

    assert_eq!(
        sim.robots()
            .filter(|robot| {
                robot
                    .delivery
                    .is_some_and(|delivery| delivery.destination == silo)
            })
            .count(),
        1,
        "one cargo slot must reserve only one delivery"
    );
}

#[test]
fn cargo_for_a_machine_that_stops_accepting_is_diverted_without_loss() {
    let (mut sim, silo, _, storage, satellite) = machine_delivery_fixture(1);
    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        sim.robots().any(|robot| !robot.cargo.is_empty())
    });

    sim.entities
        .rocket_silos
        .get_mut(&silo)
        .unwrap()
        .parts_completed = 0;
    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        chest_count(sim, storage, satellite) == 1
    });

    assert_eq!(chest_count(&sim, storage, satellite), 1);
    assert_eq!(
        sim.entities.rocket_silos[&silo]
            .cargo_inventory
            .count(satellite),
        0
    );
    sim.validate()
        .expect("a diverted machine delivery leaves valid state");
}

#[test]
fn an_in_flight_machine_delivery_round_trips_deterministically() {
    let (mut sim, silo, _, _, satellite) = machine_delivery_fixture(1);
    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        sim.robots().any(|robot| !robot.cargo.is_empty())
    });

    let snapshot = save_to_bytes(&sim).expect("an in-flight machine delivery should save");
    let mut loaded = load_from_bytes(&snapshot).expect("the machine delivery should load");
    assert_eq!(loaded.state_hash(), sim.state_hash());

    for _ in 0..DELIVERY_TICKS {
        sim.tick();
        loaded.tick();
        assert_eq!(loaded.state_hash(), sim.state_hash());
        if sim.entities.rocket_silos[&silo]
            .cargo_inventory
            .count(satellite)
            == 1
        {
            return;
        }
    }
    panic!("the reloaded robot did not deliver its machine cargo");
}

#[test]
fn a_requester_is_filled_from_a_provider_chest() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, origin) = logistic_roboport(&mut sim, 4);
    let provider = place_covered_chest(&mut sim, "passive_provider_chest", origin);
    let requester = place_covered_chest(&mut sim, "requester_chest", origin);
    insert_into_chest(&mut sim, provider, iron, 200);
    request_items(&mut sim, requester, iron, 100);

    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        chest_count(sim, requester, iron) >= 100
    });

    assert_eq!(chest_count(&sim, requester, iron), 100);
    assert_eq!(chest_count(&sim, provider, iron), 100);
    sim.validate().expect("a filled request leaves valid state");
}

/// A requester supplies nothing, so two of them cannot feed each other however
/// much stock one is holding.
#[test]
fn a_requester_never_supplies_another_requester() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, origin) = logistic_roboport(&mut sim, 4);
    let stocked = place_covered_chest(&mut sim, "requester_chest", origin);
    let empty = place_covered_chest(&mut sim, "requester_chest", origin);
    insert_into_chest(&mut sim, stocked, iron, 100);
    request_items(&mut sim, empty, iron, 100);

    tick_validated(&mut sim, 200);

    assert_eq!(chest_count(&sim, empty, iron), 0);
    assert_eq!(chest_count(&sim, stocked, iron), 100);
}

/// Two buffers both asking for an item would otherwise trade the same stack
/// back and forth forever, since a buffer both requests and supplies.
#[test]
fn one_buffer_never_stocks_another() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, origin) = logistic_roboport(&mut sim, 4);
    let stocked = place_covered_chest(&mut sim, "buffer_chest", origin);
    let empty = place_covered_chest(&mut sim, "buffer_chest", origin);
    insert_into_chest(&mut sim, stocked, iron, 100);
    request_items(&mut sim, stocked, iron, 100);
    request_items(&mut sim, empty, iron, 100);

    tick_validated(&mut sim, 200);

    assert_eq!(chest_count(&sim, empty, iron), 0);
    assert_eq!(chest_count(&sim, stocked, iron), 100);
}

/// Requester demand outranks buffer demand, so the one stack in the network
/// goes to the requester even though the buffer asked for it too.
#[test]
fn requester_demand_is_served_before_buffer_demand() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, origin) = logistic_roboport(&mut sim, 1);
    let provider = place_covered_chest(&mut sim, "passive_provider_chest", origin);
    let buffer = place_covered_chest(&mut sim, "buffer_chest", origin);
    let requester = place_covered_chest(&mut sim, "requester_chest", origin);
    insert_into_chest(&mut sim, provider, iron, 50);
    request_items(&mut sim, buffer, iron, 50);
    request_items(&mut sim, requester, iron, 50);

    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        chest_count(sim, provider, iron) == 0
    });
    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        chest_count(sim, requester, iron) == 50
    });

    assert_eq!(chest_count(&sim, buffer, iron), 0);
}

/// Both chests could serve the request; the active provider is the one the
/// network wants emptied, so it is drawn from first.
#[test]
fn supply_is_drawn_from_an_active_provider_before_a_passive_one() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, origin) = logistic_roboport(&mut sim, 1);
    let passive = place_covered_chest(&mut sim, "passive_provider_chest", origin);
    let active = place_covered_chest(&mut sim, "active_provider_chest", origin);
    let requester = place_covered_chest(&mut sim, "requester_chest", origin);
    insert_into_chest(&mut sim, passive, iron, 50);
    insert_into_chest(&mut sim, active, iron, 50);
    request_items(&mut sim, requester, iron, 50);

    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        chest_count(sim, requester, iron) == 50
    });

    assert_eq!(chest_count(&sim, active, iron), 0);
    assert_eq!(chest_count(&sim, passive, iron), 50);
}

/// An active provider is emptied into storage whether or not anything asked for
/// its contents. Nothing else in the network requests the item here, so the
/// only thing that can move it is the push.
#[test]
fn an_active_provider_pushes_its_contents_into_storage() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, origin) = logistic_roboport(&mut sim, 4);
    let active = place_covered_chest(&mut sim, "active_provider_chest", origin);
    let storage = place_covered_chest(&mut sim, "storage_chest", origin);
    insert_into_chest(&mut sim, active, iron, 100);

    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        chest_count(sim, storage, iron) == 100
    });

    assert_eq!(chest_count(&sim, active, iron), 0);
    sim.validate().expect("a surplus push leaves valid state");
}

/// A passive provider is exactly the mode that waits to be asked, so the same
/// setup must move nothing.
#[test]
fn a_passive_provider_is_not_pushed_into_storage() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, origin) = logistic_roboport(&mut sim, 4);
    let passive = place_covered_chest(&mut sim, "passive_provider_chest", origin);
    let storage = place_covered_chest(&mut sim, "storage_chest", origin);
    insert_into_chest(&mut sim, passive, iron, 100);

    tick_validated(&mut sim, 200);

    assert_eq!(chest_count(&sim, passive, iron), 100);
    assert_eq!(chest_count(&sim, storage, iron), 0);
}

/// A storage chest filtered to another item is not somewhere the network may
/// put this one, however much room it has.
#[test]
fn a_filtered_storage_chest_only_accepts_its_item() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let copper = item_id(&sim.world.prototypes, "copper_plate");
    let (_, origin) = logistic_roboport(&mut sim, 4);
    let active = place_covered_chest(&mut sim, "active_provider_chest", origin);
    let storage = place_covered_chest(&mut sim, "storage_chest", origin);
    sim.set_logistic_request(
        storage,
        0,
        LogisticRequest {
            item: Some(copper),
            count: 0,
        },
    )
    .expect("a storage chest takes a filter");
    insert_into_chest(&mut sim, active, iron, 100);

    tick_validated(&mut sim, 200);

    assert_eq!(chest_count(&sim, storage, iron), 0);
    assert_eq!(chest_count(&sim, active, iron), 100);
}

/// A bounded storage search must rotate rather than permanently treating the
/// first budget-sized prefix as the whole network.
#[test]
fn storage_search_reaches_a_usable_chest_after_an_incompatible_prefix() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let copper = item_id(&sim.world.prototypes, "copper_plate");
    let (_, origin) = logistic_roboport(&mut sim, 1);

    for _ in 0..16 {
        let storage = place_covered_chest(&mut sim, "storage_chest", origin);
        sim.set_logistic_request(
            storage,
            0,
            LogisticRequest {
                item: Some(copper),
                count: 0,
            },
        )
        .expect("a storage chest takes a filter");
    }
    let usable_storage = place_covered_chest(&mut sim, "storage_chest", origin);
    let active = place_covered_chest(&mut sim, "active_provider_chest", origin);
    insert_into_chest(&mut sim, active, iron, 100);

    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        chest_count(sim, usable_storage, iron) == 100
    });

    assert_eq!(chest_count(&sim, active, iron), 0);
    sim.validate()
        .expect("rotating the bounded storage search leaves valid state");
}

/// One trip carries one stack, so a request larger than a stack takes several.
#[test]
fn a_delivery_carries_at_most_one_stack() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let stack_size = u32::from(
        sim.world
            .prototypes
            .item(iron)
            .expect("iron plate is in the catalog")
            .stack_size,
    );
    let (_, origin) = logistic_roboport(&mut sim, 1);
    let provider = place_covered_chest(&mut sim, "passive_provider_chest", origin);
    let requester = place_covered_chest(&mut sim, "requester_chest", origin);
    insert_into_chest(&mut sim, provider, iron, 200);
    request_items(&mut sim, requester, iron, 200);

    // One robot, so the first hand-over cannot be two trips' worth.
    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        chest_count(sim, requester, iron) > 0
    });
    assert_eq!(chest_count(&sim, requester, iron), stack_size);

    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        chest_count(sim, requester, iron) == 200
    });
    assert_eq!(chest_count(&sim, provider, iron), 0);
}

/// Two robots must not both be promised the one stack a provider holds: the
/// second would arrive at an empty chest and the request would stay unfilled.
#[test]
fn two_robots_are_never_promised_the_same_items() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, origin) = logistic_roboport(&mut sim, 8);
    let provider = place_covered_chest(&mut sim, "passive_provider_chest", origin);
    let requester = place_covered_chest(&mut sim, "requester_chest", origin);
    insert_into_chest(&mut sim, provider, iron, 50);
    request_items(&mut sim, requester, iron, 500);

    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        chest_count(sim, provider, iron) == 0
    });
    let carrying = sim
        .robots()
        .filter(|robot| robot.delivery.is_some())
        .count();

    assert!(
        carrying <= 1,
        "50 plates are one delivery, but {carrying} robots claimed them"
    );
    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        chest_count(sim, requester, iron) == 50
    });
}

/// The destination is destroyed while a loaded robot is on its way to it. The
/// items are already aboard, so the only place they can go is storage — losing
/// them would be the network eating a stack.
#[test]
fn cargo_goes_to_storage_when_the_destination_disappears() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, origin) = logistic_roboport(&mut sim, 1);
    let provider = place_covered_chest(&mut sim, "passive_provider_chest", origin);
    let storage = place_covered_chest(&mut sim, "storage_chest", origin);
    let requester = place_covered_chest(&mut sim, "requester_chest", origin);
    insert_into_chest(&mut sim, provider, iron, 100);
    request_items(&mut sim, requester, iron, 100);

    // Wait for the pickup to land, so the robot is carrying the stack rather
    // than still on its way to fetch it.
    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        sim.robots().any(|robot| !robot.cargo.is_empty())
    });
    crate::entity_mutation::remove(&mut sim, requester)
        .expect("a placed chest should be removable");

    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        chest_count(sim, storage, iron) == 100
    });
    sim.validate()
        .expect("a diverted delivery leaves valid state");
}

/// The source is emptied by something else while the robot is on its way to it.
/// Nothing was picked up, so the robot has to give up and go home rather than
/// wait at an empty chest forever.
#[test]
fn a_robot_gives_up_when_the_source_is_emptied_first() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (roboport, origin) = logistic_roboport(&mut sim, 1);
    let provider = place_covered_chest(&mut sim, "passive_provider_chest", origin);
    let requester = place_covered_chest(&mut sim, "requester_chest", origin);
    insert_into_chest(&mut sim, provider, iron, 100);
    request_items(&mut sim, requester, iron, 100);

    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        sim.robots().any(|robot| robot.delivery.is_some())
    });
    crate::entity_access::inventory_mut(&mut sim, provider)
        .expect("a chest has an inventory")
        .remove(iron, 100)
        .expect("the fixture stocked the chest");

    // The robot docks again, which is the only way its item returns to the
    // roboport's robot slots.
    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        sim.entity_roboport_status(roboport)
            .expect("a placed roboport reports status")
            .available_logistic_robots
            == 1
    });
    assert_eq!(chest_count(&sim, requester, iron), 0);
    sim.validate()
        .expect("an abandoned delivery leaves valid state");
}

/// A chest outside every roboport's logistic square is not part of a network,
/// so nothing may be flown to it.
#[test]
fn an_uncovered_requester_is_never_served() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, origin) = logistic_roboport(&mut sim, 4);
    let provider = place_covered_chest(&mut sim, "passive_provider_chest", origin);
    insert_into_chest(&mut sim, provider, iron, 100);

    let requester = place_uncovered_chest(&mut sim, "requester_chest");
    request_items(&mut sim, requester, iron, 100);

    tick_validated(&mut sim, 200);

    assert_eq!(chest_count(&sim, requester, iron), 0);
    assert_eq!(chest_count(&sim, provider, iron), 100);
}

/// Deliveries survive a save: the robot in the air keeps the leg it was flying,
/// and the reservations the matcher works from are read back off it rather than
/// restored from a table that was never saved.
#[test]
fn a_delivery_in_flight_survives_a_save_round_trip() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, origin) = logistic_roboport(&mut sim, 1);
    let provider = place_covered_chest(&mut sim, "passive_provider_chest", origin);
    let requester = place_covered_chest(&mut sim, "requester_chest", origin);
    insert_into_chest(&mut sim, provider, iron, 100);
    request_items(&mut sim, requester, iron, 100);

    tick_until(&mut sim, DELIVERY_TICKS, |sim| {
        sim.robots().any(|robot| !robot.cargo.is_empty())
    });

    let snapshot = save_to_bytes(&sim).expect("a world with robots should save");
    let mut restored = load_from_bytes(&snapshot).expect("the save reloads");
    assert_eq!(
        restored
            .robots()
            .map(|robot| robot.delivery)
            .collect::<Vec<_>>(),
        sim.robots().map(|robot| robot.delivery).collect::<Vec<_>>()
    );

    tick_until(&mut restored, DELIVERY_TICKS, |sim| {
        chest_count(sim, requester, iron) == 100
    });
    restored
        .validate()
        .expect("a reloaded delivery leaves valid state");
}
