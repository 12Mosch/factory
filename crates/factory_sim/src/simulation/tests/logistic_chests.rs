use super::super::*;
use super::support::*;

/// Places one roboport, then a chest on a buildable tile inside its logistic
/// square.
///
/// The world is generated, so neither position can be a fixed coordinate; the
/// chest is searched for among the tiles the roboport actually covers, which
/// is also what makes the "covered" precondition of these tests explicit.
fn roboport_with_chest(sim: &mut Simulation, chest_name: &str) -> (EntityId, EntityId) {
    let (roboport, origin) = place_roboport(sim);
    let chest = place_covered_chest(sim, chest_name, origin);
    sim.tick();
    (roboport, chest)
}

/// Places a chest on the first buildable tile no roboport reaches.
fn place_uncovered_chest(sim: &mut Simulation, chest_name: &str) -> EntityId {
    let prototype_id = entity_id_by_name(&sim.world.prototypes, chest_name);
    let (x, y) = all_tile_coords(&sim.world)
        .into_iter()
        .find(|(x, y)| {
            sim.logistic_network_covering_tile(*x, *y).is_none()
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
        .expect("the generated world is larger than one roboport's reach");
    place_at(sim, prototype_id, x, y, Direction::North)
}

fn totals(sim: &Simulation, chest: EntityId, item_id: ItemId) -> crate::robots::LogisticItemTotals {
    let network_id = sim
        .logistic_network_id_for_chest(chest)
        .expect("the chest should be indexed into a network");
    sim.logistic_network_item_totals(network_id, item_id)
}

#[test]
fn a_provider_chests_contents_become_network_supply() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, chest) = roboport_with_chest(&mut sim, "passive_provider_chest");

    insert_into_chest(&mut sim, chest, iron, 40);
    sim.tick();

    let totals = totals(&sim, chest, iron);
    assert_eq!(totals.available, 40);
    assert_eq!(totals.stored, 40);
    assert_eq!(totals.requested, 0);
}

/// A requester's stock is not supply: nothing may take back what was delivered
/// to it, which is the whole difference between it and a buffer chest.
#[test]
fn a_requester_chests_contents_are_not_network_supply() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, chest) = roboport_with_chest(&mut sim, "requester_chest");

    insert_into_chest(&mut sim, chest, iron, 25);
    sim.tick();

    let totals = totals(&sim, chest, iron);
    assert_eq!(totals.available, 0);
    assert_eq!(totals.stored, 25);
}

#[test]
fn a_request_reports_only_the_shortfall_it_still_needs() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, chest) = roboport_with_chest(&mut sim, "requester_chest");

    sim.set_logistic_request(
        chest,
        0,
        LogisticRequest {
            item: Some(iron),
            count: 100,
        },
    )
    .expect("a requester chest takes an amount");
    sim.tick();
    assert_eq!(totals(&sim, chest, iron).requested, 100);

    insert_into_chest(&mut sim, chest, iron, 30);
    sim.tick();

    // The delivered 30 is no longer work for the network.
    assert_eq!(totals(&sim, chest, iron).requested, 70);
}

/// Nothing stops a player from naming the same item on two rows, and the two
/// together are one request for their total. Netting the held stock off each
/// row separately would subtract it twice and understate the shortfall.
#[test]
fn two_rows_naming_one_item_share_a_single_shortfall() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, chest) = roboport_with_chest(&mut sim, "requester_chest");

    for (slot_index, count) in [(0, 100), (1, 50)] {
        sim.set_logistic_request(
            chest,
            slot_index,
            LogisticRequest {
                item: Some(iron),
                count,
            },
        )
        .expect("a requester chest takes an amount");
    }
    insert_into_chest(&mut sim, chest, iron, 30);
    sim.tick();

    // 150 asked for, 30 already here.
    assert_eq!(totals(&sim, chest, iron).requested, 120);
}

/// A buffer both asks and supplies, which is what distinguishes it from the
/// other four modes and the one combination the index has to get right.
#[test]
fn a_buffer_chest_both_supplies_and_requests() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, chest) = roboport_with_chest(&mut sim, "buffer_chest");

    sim.set_logistic_request(
        chest,
        0,
        LogisticRequest {
            item: Some(iron),
            count: 50,
        },
    )
    .expect("a buffer chest takes an amount");
    insert_into_chest(&mut sim, chest, iron, 20);
    sim.tick();

    let totals = totals(&sim, chest, iron);
    assert_eq!(totals.available, 20);
    assert_eq!(totals.requested, 30);
}

/// The index is maintained by delta, so a withdrawal has to subtract exactly
/// what the chest previously contributed — a stale entry would leave the
/// network claiming supply that is no longer there.
#[test]
fn the_index_follows_inventory_changes_in_both_directions() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, chest) = roboport_with_chest(&mut sim, "passive_provider_chest");

    insert_into_chest(&mut sim, chest, iron, 40);
    sim.tick();
    assert_eq!(totals(&sim, chest, iron).available, 40);

    crate::entity_access::inventory_mut(&mut sim, chest)
        .expect("a chest has an inventory")
        .remove(iron, 15)
        .expect("the chest holds enough to withdraw");
    sim.tick();
    assert_eq!(totals(&sim, chest, iron).available, 25);

    crate::entity_access::inventory_mut(&mut sim, chest)
        .expect("a chest has an inventory")
        .remove(iron, 25)
        .expect("the chest holds enough to empty");
    sim.tick();
    // An item nothing holds leaves the index entirely rather than lingering
    // at zero.
    let network_id = sim
        .logistic_network_id_for_chest(chest)
        .expect("the chest is still indexed");
    assert!(
        !sim.logistic_network_contents(network_id)
            .expect("the network has an index entry")
            .contains_key(&iron)
    );
}

#[test]
fn destroying_a_chest_removes_its_contribution() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (roboport, chest) = roboport_with_chest(&mut sim, "passive_provider_chest");

    insert_into_chest(&mut sim, chest, iron, 40);
    sim.tick();
    let network_id = sim
        .logistic_network_id_for_chest(chest)
        .expect("the chest should be indexed");

    entity_mutation::destroy_to_player_inventory(&mut sim, chest)
        .expect("the chest is destructible");
    sim.tick();

    assert_eq!(sim.logistic_network_id_for_chest(chest), None);
    assert_eq!(
        sim.logistic_network_item_totals(network_id, iron),
        crate::robots::LogisticItemTotals::default()
    );
    // The roboport is untouched, so the network itself still exists.
    assert_eq!(sim.robot_network_id_for_entity(roboport), Some(network_id));
}

#[test]
fn a_chest_no_roboport_reaches_joins_no_network() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, covered) = roboport_with_chest(&mut sim, "passive_provider_chest");
    let stranded = place_uncovered_chest(&mut sim, "passive_provider_chest");

    insert_into_chest(&mut sim, covered, iron, 10);
    insert_into_chest(&mut sim, stranded, iron, 10);
    sim.tick();

    assert_eq!(sim.logistic_network_id_for_chest(stranded), None);
    // Only the covered chest's items reach the network.
    assert_eq!(totals(&sim, covered, iron).available, 10);
}

/// An ordinary chest is storage, not logistics: it must stay invisible to the
/// network so the index never claims items robots have no way to fetch.
#[test]
fn a_plain_chest_contributes_nothing() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (roboport, chest) = roboport_with_chest(&mut sim, "steel_chest");

    insert_into_chest(&mut sim, chest, iron, 40);
    sim.tick();

    assert_eq!(sim.logistic_chest_state(chest), None);
    assert_eq!(sim.logistic_network_id_for_chest(chest), None);
    let network_id = sim
        .robot_network_id_for_entity(roboport)
        .expect("the roboport anchors a network");
    assert_eq!(
        sim.logistic_network_item_totals(network_id, iron),
        crate::robots::LogisticItemTotals::default()
    );
}

#[test]
fn a_storage_chests_row_is_a_filter_and_takes_no_amount() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, chest) = roboport_with_chest(&mut sim, "storage_chest");

    assert_eq!(
        sim.set_logistic_request(
            chest,
            0,
            LogisticRequest {
                item: Some(iron),
                count: 10,
            },
        ),
        Err(LogisticChestError::ModeTakesNoAmount)
    );
    sim.set_logistic_request(
        chest,
        0,
        LogisticRequest {
            item: Some(iron),
            count: 0,
        },
    )
    .expect("a filter without an amount is what a storage row is");

    assert_eq!(
        sim.logistic_chest_state(chest)
            .and_then(LogisticChestState::storage_filter),
        Some(iron)
    );
}

#[test]
fn a_request_is_clamped_to_what_the_chest_could_hold() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, chest) = roboport_with_chest(&mut sim, "requester_chest");
    let capacity = 48
        * u32::from(
            sim.world
                .prototypes
                .item(iron)
                .expect("iron plate is in the catalog")
                .stack_size,
        );

    sim.set_logistic_request(
        chest,
        0,
        LogisticRequest {
            item: Some(iron),
            count: u32::MAX,
        },
    )
    .expect("an oversized request is clamped rather than refused");

    assert_eq!(
        sim.logistic_chest_state(chest)
            .and_then(|state| state.requests.first().copied()),
        Some(LogisticRequest {
            item: Some(iron),
            count: capacity,
        })
    );
}

/// Clearing the item clears the amount with it, so no orphaned number survives
/// for the index to attach to a later, unrelated item.
#[test]
fn clearing_a_rows_item_clears_its_amount() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, chest) = roboport_with_chest(&mut sim, "requester_chest");

    sim.set_logistic_request(
        chest,
        0,
        LogisticRequest {
            item: Some(iron),
            count: 100,
        },
    )
    .expect("a requester chest takes an amount");
    sim.set_logistic_request(
        chest,
        0,
        LogisticRequest {
            item: None,
            count: 100,
        },
    )
    .expect("clearing a row always succeeds");

    assert_eq!(
        sim.logistic_chest_state(chest)
            .and_then(|state| state.requests.first().copied()),
        Some(LogisticRequest::default())
    );
    sim.tick();
    assert_eq!(totals(&sim, chest, iron).requested, 0);
}

#[test]
fn configuring_an_ordinary_chest_is_rejected() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, chest) = roboport_with_chest(&mut sim, "steel_chest");

    assert_eq!(
        sim.set_logistic_request(
            chest,
            0,
            LogisticRequest {
                item: Some(iron),
                count: 10,
            },
        ),
        Err(LogisticChestError::NotLogisticChest(chest))
    );
}

/// The roboport is the one entity that knows which network it anchors, so it
/// is where a circuit network reads the logistic contents from.
#[test]
fn a_roboport_publishes_its_networks_contents_onto_a_wire() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (roboport, chest) = roboport_with_chest(&mut sim, "passive_provider_chest");

    let red_wire = item_id(&sim.world.prototypes, "red_wire");
    let catalog = sim.world.prototypes.clone();
    sim.player_inventory
        .insert(&catalog, red_wire, 10)
        .expect("the player inventory should accept wires");
    sim.set_circuit_read_contents(roboport, true)
        .expect("a roboport publishes its network contents");
    // Contents only reach a network the connector is actually wired to, so the
    // chest doubles as the far end of the wire.
    sim.connect_circuit_wire(
        CircuitNode::new(roboport, ConnectorPort::Single),
        CircuitNode::new(chest, ConnectorPort::Single),
        WireColor::Red,
    )
    .expect("a chest beside the roboport is within wire reach");
    insert_into_chest(&mut sim, chest, iron, 40);
    // One tick indexes the chest; the next publishes the settled index, which
    // is the same one-tick delay every combinator has.
    sim.tick();
    sim.tick();

    assert_eq!(
        sim.circuit_signals_at_entity(roboport)
            .value(SignalId::Item(iron)),
        40
    );
}

/// Logistic configuration is durable state, so it has to survive a save and
/// leave the reloaded world validating.
#[test]
fn logistic_configuration_survives_a_save_round_trip() {
    let mut sim = Simulation::new_test_world(123);
    let iron = item_id(&sim.world.prototypes, "iron_plate");
    let (_, chest) = roboport_with_chest(&mut sim, "requester_chest");

    sim.set_logistic_request(
        chest,
        3,
        LogisticRequest {
            item: Some(iron),
            count: 250,
        },
    )
    .expect("a requester chest takes an amount");
    insert_into_chest(&mut sim, chest, iron, 60);
    sim.tick();

    let bytes = save_to_bytes(&sim).expect("the world should save");
    let mut restored = load_from_bytes(&bytes).expect("the saved world should load");

    assert_eq!(
        restored.logistic_chest_state(chest),
        sim.logistic_chest_state(chest)
    );
    // The index is derived, so it takes no part in simulation identity and is
    // rebuilt by the first tick after loading.
    assert_eq!(restored.state_hash(), sim.state_hash());
    restored.tick();
    assert_eq!(totals(&restored, chest, iron).requested, 190);
}
