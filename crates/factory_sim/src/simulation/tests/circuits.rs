use super::super::*;
use super::support::*;

/// Enough clear ground for a handful of wired entities plus the pole that
/// powers them.
const AREA_WIDTH: i32 = 16;
const AREA_HEIGHT: i32 = 10;

fn clear_area(sim: &Simulation) -> (WorldTileCoord, WorldTileCoord) {
    first_buildable_rect_without_resource(&sim.world, AREA_WIDTH, AREA_HEIGHT)
}

fn place_named(sim: &mut Simulation, name: &str, x: WorldTileCoord, y: WorldTileCoord) -> EntityId {
    place_named_facing(sim, name, x, y, Direction::North)
}

fn place_named_facing(
    sim: &mut Simulation,
    name: &str,
    x: WorldTileCoord,
    y: WorldTileCoord,
    direction: Direction,
) -> EntityId {
    let prototype_id = entity_id_by_name(&sim.world.prototypes, name);
    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id,
            x,
            y,
            direction,
        },
    )
    .unwrap_or_else(|error| panic!("{name} should be placeable: {error:?}"))
}

/// Gives the player enough wire that connection commands are never rejected
/// for want of an item.
fn stock_wires(sim: &mut Simulation) {
    let catalog = sim.world.prototypes.clone();
    for name in ["red_wire", "green_wire"] {
        let item = item_id(&catalog, name);
        sim.player_inventory
            .insert(&catalog, item, 100)
            .expect("player inventory should accept wires");
    }
}

fn single(entity_id: EntityId) -> CircuitNode {
    CircuitNode::new(entity_id, ConnectorPort::Single)
}

fn connect(sim: &mut Simulation, first: EntityId, second: EntityId, color: WireColor) {
    sim.connect_circuit_wire(single(first), single(second), color)
        .expect("entities within reach should connect");
}

/// Loads a burner inserter's fuel slot directly; the transfer helpers all go
/// through the player inventory, which these tests already use for wires.
fn fuel_burner_inserter(sim: &mut Simulation, inserter: EntityId, fuel: ItemId, count: u16) {
    let stack = test_stack(fuel, count);
    let Some(MachineEnergy::Burner(burner)) = sim.entities.inserter_energy.get_mut(&inserter)
    else {
        panic!("burner inserter should have a burner energy source");
    };
    burner.fuel_slot = test_slot(stack);
}

fn belt_item_positions(sim: &Simulation, belt: EntityId, lane_index: usize) -> Vec<u16> {
    sim.entities
        .transport_belts
        .get(&belt)
        .expect("belt state should exist")
        .lanes[lane_index]
        .items
        .iter()
        .map(|item| item.position_subtile)
        .collect()
}

fn virtual_signal(sim: &Simulation, name: &str) -> SignalId {
    let id = sim
        .world
        .prototypes
        .virtual_signals
        .iter()
        .find(|signal| signal.name == name)
        .unwrap_or_else(|| panic!("catalog should define virtual signal {name}"))
        .id;
    SignalId::Virtual(id)
}

fn signal_value(sim: &Simulation, entity_id: EntityId, signal: SignalId) -> i32 {
    sim.circuit_signals_at_entity(entity_id).value(signal)
}

/// A constant combinator emitting `value` on `signal`, wired to `target`.
fn wire_constant_source(
    sim: &mut Simulation,
    target: CircuitNode,
    x: WorldTileCoord,
    y: WorldTileCoord,
    signal: SignalId,
    value: i32,
) -> EntityId {
    let combinator = place_named(sim, "constant_combinator", x, y);
    sim.set_constant_combinator_slot(
        combinator,
        0,
        ConstantSignalSlot {
            signal: Some(signal),
            value,
        },
    )
    .expect("constant combinator should accept a slot");
    sim.connect_circuit_wire(
        CircuitNode::new(combinator, ConnectorPort::Output),
        target,
        WireColor::Red,
    )
    .expect("constant combinator should reach the target");
    combinator
}

fn insert_into_chest(sim: &mut Simulation, chest: EntityId, item: ItemId, count: u16) {
    let catalog = sim.world.prototypes.clone();
    crate::entity_access::inventory_mut(sim, chest)
        .expect("chest has an inventory")
        .insert(&catalog, item, count)
        .expect("chest should accept the item");
}

#[test]
fn wiring_two_chests_merges_their_contents_onto_one_network() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let iron = item_id(&sim.world.prototypes, "iron_plate");

    let first = place_named(&mut sim, "chest", ox, oy);
    let second = place_named(&mut sim, "chest", ox + 2, oy);
    for chest in [first, second] {
        sim.set_circuit_read_contents(chest, true)
            .expect("chests publish their contents");
        insert_into_chest(&mut sim, chest, iron, 5);
    }
    connect(&mut sim, first, second, WireColor::Red);

    sim.tick();

    // Both chests sit on the same network, so each reads the merged total.
    assert_eq!(signal_value(&sim, first, SignalId::Item(iron)), 10);
    assert_eq!(signal_value(&sim, second, SignalId::Item(iron)), 10);
}

#[test]
fn red_and_green_wires_form_independent_networks() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let iron = item_id(&sim.world.prototypes, "iron_plate");

    let red_chest = place_named(&mut sim, "chest", ox, oy);
    let green_chest = place_named(&mut sim, "chest", ox + 2, oy);
    let reader = place_named(&mut sim, "chest", ox + 4, oy);
    for chest in [red_chest, green_chest] {
        sim.set_circuit_read_contents(chest, true)
            .expect("chests publish their contents");
    }
    insert_into_chest(&mut sim, red_chest, iron, 3);
    insert_into_chest(&mut sim, green_chest, iron, 4);

    connect(&mut sim, red_chest, reader, WireColor::Red);
    connect(&mut sim, green_chest, reader, WireColor::Green);
    sim.tick();

    // The reader sees both wires merged, but neither source sees the other:
    // red and green are separate networks that only meet at shared connectors.
    assert_eq!(signal_value(&sim, reader, SignalId::Item(iron)), 7);
    assert_eq!(signal_value(&sim, red_chest, SignalId::Item(iron)), 3);
    assert_eq!(signal_value(&sim, green_chest, SignalId::Item(iron)), 4);
}

#[test]
fn constant_combinator_publishes_its_configured_signals() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let signal = virtual_signal(&sim, "signal_a");

    let chest = place_named(&mut sim, "chest", ox, oy);
    let combinator = wire_constant_source(&mut sim, single(chest), ox + 2, oy, signal, 42);
    sim.tick();
    assert_eq!(signal_value(&sim, chest, signal), 42);

    sim.set_constant_combinator_enabled(combinator, false)
        .expect("constant combinator should toggle");
    sim.tick();
    assert_eq!(signal_value(&sim, chest, signal), 0);
}

#[test]
fn arithmetic_combinator_output_appears_one_tick_later() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let input_signal = virtual_signal(&sim, "signal_a");
    let output_signal = virtual_signal(&sim, "signal_b");

    let arithmetic = place_named(&mut sim, "arithmetic_combinator", ox + 4, oy);
    let reader = place_named(&mut sim, "chest", ox + 8, oy);
    // Source -> combinator input, combinator output -> reader.
    wire_constant_source(
        &mut sim,
        CircuitNode::new(arithmetic, ConnectorPort::Input),
        ox,
        oy,
        input_signal,
        7,
    );
    sim.connect_circuit_wire(
        CircuitNode::new(arithmetic, ConnectorPort::Output),
        single(reader),
        WireColor::Red,
    )
    .expect("combinator output should reach the reader");
    sim.configure_arithmetic_combinator(
        arithmetic,
        SignalOperand::Signal(input_signal),
        ArithmeticOperation::Multiply,
        SignalOperand::Constant(3),
        Some(output_signal),
    )
    .expect("arithmetic combinator should accept configuration");

    // Tick one: the combinator reads its input and stores the result, but the
    // networks were already filled before it ran.
    sim.tick();
    assert_eq!(signal_value(&sim, reader, output_signal), 0);

    // Tick two: the stored result is published.
    sim.tick();
    assert_eq!(signal_value(&sim, reader, output_signal), 21);
}

#[test]
fn decider_combinator_emits_only_while_its_condition_holds() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let input_signal = virtual_signal(&sim, "signal_a");
    let output_signal = virtual_signal(&sim, "signal_b");

    let decider = place_named(&mut sim, "decider_combinator", ox + 4, oy);
    let reader = place_named(&mut sim, "chest", ox + 8, oy);
    let source = wire_constant_source(
        &mut sim,
        CircuitNode::new(decider, ConnectorPort::Input),
        ox,
        oy,
        input_signal,
        10,
    );
    sim.connect_circuit_wire(
        CircuitNode::new(decider, ConnectorPort::Output),
        single(reader),
        WireColor::Red,
    )
    .expect("combinator output should reach the reader");
    sim.configure_decider_combinator(
        decider,
        Some(input_signal),
        Comparator::Greater,
        SignalOperand::Constant(5),
        Some(output_signal),
        DeciderOutputValue::One,
    )
    .expect("decider should accept configuration");

    // Two ticks: one to evaluate, one to publish.
    sim.tick();
    sim.tick();
    assert_eq!(signal_value(&sim, reader, output_signal), 1);

    sim.set_constant_combinator_slot(
        source,
        0,
        ConstantSignalSlot {
            signal: Some(input_signal),
            value: 1,
        },
    )
    .expect("constant combinator should accept a slot");
    sim.tick();
    sim.tick();
    assert_eq!(signal_value(&sim, reader, output_signal), 0);
}

#[test]
fn arithmetic_each_runs_the_operation_per_input_signal() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let each = virtual_signal(&sim, "signal_each");
    let first = virtual_signal(&sim, "signal_a");
    let second = virtual_signal(&sim, "signal_b");

    let arithmetic = place_named(&mut sim, "arithmetic_combinator", ox + 4, oy);
    let reader = place_named(&mut sim, "chest", ox + 8, oy);
    let source = place_named(&mut sim, "constant_combinator", ox, oy);
    for (index, (signal, value)) in [(first, 2), (second, 5)].into_iter().enumerate() {
        sim.set_constant_combinator_slot(
            source,
            index,
            ConstantSignalSlot {
                signal: Some(signal),
                value,
            },
        )
        .expect("constant combinator should accept a slot");
    }
    sim.connect_circuit_wire(
        CircuitNode::new(source, ConnectorPort::Output),
        CircuitNode::new(arithmetic, ConnectorPort::Input),
        WireColor::Red,
    )
    .expect("source should reach the combinator input");
    sim.connect_circuit_wire(
        CircuitNode::new(arithmetic, ConnectorPort::Output),
        single(reader),
        WireColor::Red,
    )
    .expect("combinator output should reach the reader");
    sim.configure_arithmetic_combinator(
        arithmetic,
        SignalOperand::Signal(each),
        ArithmeticOperation::Multiply,
        SignalOperand::Constant(10),
        Some(each),
    )
    .expect("arithmetic combinator should accept Each configuration");

    sim.tick();
    sim.tick();

    // Each input keeps its own channel, scaled independently.
    assert_eq!(signal_value(&sim, reader, first), 20);
    assert_eq!(signal_value(&sim, reader, second), 50);
}

#[test]
fn inserter_stops_while_its_condition_fails() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let signal = virtual_signal(&sim, "signal_a");
    let iron = item_id(&sim.world.prototypes, "iron_plate");

    // Source chest -> inserter -> destination chest, all in a row.
    let source = place_named(&mut sim, "chest", ox, oy);
    let inserter = place_named_facing(&mut sim, "burner_inserter", ox + 1, oy, Direction::East);
    let destination = place_named(&mut sim, "chest", ox + 2, oy);
    let coal = item_id(&sim.world.prototypes, "coal");
    fill_inventory_with(&mut sim, source, iron);
    fuel_burner_inserter(&mut sim, inserter, coal, 20);

    let combinator = wire_constant_source(&mut sim, single(inserter), ox + 4, oy, signal, 0);
    sim.set_circuit_condition(
        inserter,
        Some(CircuitCondition {
            left: signal,
            comparator: Comparator::Greater,
            right: SignalOperand::Constant(0),
        }),
    )
    .expect("inserters accept an enable condition");

    for _ in 0..180 {
        sim.tick();
    }
    let moved_while_disabled = crate::entity_access::inventory(&sim, destination)
        .expect("chest has an inventory")
        .count(iron);
    assert_eq!(moved_while_disabled, 0, "a disabled inserter must not work");

    sim.set_constant_combinator_slot(
        combinator,
        0,
        ConstantSignalSlot {
            signal: Some(signal),
            value: 1,
        },
    )
    .expect("constant combinator should accept a slot");
    for _ in 0..180 {
        sim.tick();
    }
    let moved_while_enabled = crate::entity_access::inventory(&sim, destination)
        .expect("chest has an inventory")
        .count(iron);
    assert!(
        moved_while_enabled > 0,
        "an enabled inserter must resume working"
    );
}

#[test]
fn a_condition_on_an_unwired_entity_keeps_it_disabled() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    let signal = virtual_signal(&sim, "signal_a");

    let inserter = place_named(&mut sim, "burner_inserter", ox, oy);
    sim.set_circuit_condition(
        inserter,
        Some(CircuitCondition {
            left: signal,
            comparator: Comparator::Equal,
            right: SignalOperand::Constant(0),
        }),
    )
    .expect("inserters accept an enable condition");

    sim.tick();

    // The comparison `0 == 0` would pass on an empty network, but an unwired
    // entity is never enabled by a condition it cannot receive.
    assert!(!sim.circuit_work_allowed(inserter));
}

#[test]
fn removing_an_entity_unlinks_and_refunds_its_wires() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let red_wire = item_id(&sim.world.prototypes, "red_wire");

    let first = place_named(&mut sim, "chest", ox, oy);
    let second = place_named(&mut sim, "chest", ox + 2, oy);
    connect(&mut sim, first, second, WireColor::Red);
    let after_connect = sim.player_inventory.count(red_wire);

    crate::simulation::entity_recovery_ops::destroy_to_player_inventory(&mut sim, first)
        .expect("chest should be recoverable");

    assert_eq!(sim.player_inventory.count(red_wire), after_connect + 1);
    assert!(
        sim.circuit_entity_state(second).is_none(),
        "the surviving neighbor must not keep a dangling link"
    );
    sim.validate_state()
        .expect("state must stay valid after the wired entity is removed");
}

#[test]
fn cutting_a_wire_returns_the_item_and_splits_the_network() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let red_wire = item_id(&sim.world.prototypes, "red_wire");
    let iron = item_id(&sim.world.prototypes, "iron_plate");

    let first = place_named(&mut sim, "chest", ox, oy);
    let second = place_named(&mut sim, "chest", ox + 2, oy);
    sim.set_circuit_read_contents(first, true)
        .expect("chests publish their contents");
    insert_into_chest(&mut sim, first, iron, 6);
    connect(&mut sim, first, second, WireColor::Red);
    sim.tick();
    assert_eq!(signal_value(&sim, second, SignalId::Item(iron)), 6);

    let before_cut = sim.player_inventory.count(red_wire);
    sim.disconnect_circuit_wire(single(first), single(second), WireColor::Red)
        .expect("a connected wire should be cuttable");
    sim.tick();

    assert_eq!(sim.player_inventory.count(red_wire), before_cut + 1);
    assert_eq!(signal_value(&sim, second, SignalId::Item(iron)), 0);
}

#[test]
fn cutting_a_wire_rejects_a_missing_back_link_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let red_wire = item_id(&sim.world.prototypes, "red_wire");

    let first = place_named(&mut sim, "chest", ox, oy);
    let second = place_named(&mut sim, "chest", ox + 2, oy);
    connect(&mut sim, first, second, WireColor::Red);
    let wire_count = sim.player_inventory.count(red_wire);
    sim.entities.circuit_entities.remove(&second);

    let result = sim.disconnect_circuit_wire(single(first), single(second), WireColor::Red);

    assert_eq!(result, Err(CircuitError::NotConnected));
    assert_eq!(sim.player_inventory.count(red_wire), wire_count);
    assert!(sim.circuit_entity_state(first).is_some_and(|state| {
        state
            .connections
            .neighbors(ConnectorPort::Single, WireColor::Red)
            == [single(second)]
    }));
}

#[test]
fn wires_beyond_reach_are_rejected() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let red_wire = item_id(&sim.world.prototypes, "red_wire");
    let before = sim.player_inventory.count(red_wire);

    let first = place_named(&mut sim, "chest", ox, oy);
    // Circuit wire reaches nine tiles; fifteen is comfortably outside it.
    let far = place_named(&mut sim, "chest", ox + 15, oy);

    let result = sim.connect_circuit_wire(single(first), single(far), WireColor::Red);

    assert!(matches!(result, Err(CircuitError::OutOfReach { .. })));
    // A rejected connection must not consume the wire.
    assert_eq!(sim.player_inventory.count(red_wire), before);
}

#[test]
fn connecting_without_a_wire_item_fails() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);

    let first = place_named(&mut sim, "chest", ox, oy);
    let second = place_named(&mut sim, "chest", ox + 2, oy);

    let result = sim.connect_circuit_wire(single(first), single(second), WireColor::Red);

    assert!(matches!(result, Err(CircuitError::MissingWireItem(_))));
}

#[test]
fn network_topology_is_rebuilt_only_when_wiring_changes() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);

    let first = place_named(&mut sim, "chest", ox, oy);
    let second = place_named(&mut sim, "chest", ox + 2, oy);
    connect(&mut sim, first, second, WireColor::Red);
    sim.tick();
    let after_first_tick = sim.circuit_topology_rebuild_count();

    for _ in 0..10 {
        sim.tick();
    }

    assert_eq!(
        sim.circuit_topology_rebuild_count(),
        after_first_tick,
        "steady-state ticks must not rebuild the circuit topology"
    );
}

#[test]
fn network_ids_do_not_depend_on_the_order_wires_were_added() {
    let build = |reverse: bool| {
        let mut sim = Simulation::new_test_world(123);
        let (ox, oy) = clear_area(&sim);
        stock_wires(&mut sim);
        let iron = item_id(&sim.world.prototypes, "iron_plate");

        let chests = (0..4)
            .map(|index| place_named(&mut sim, "chest", ox + index * 2, oy))
            .collect::<Vec<_>>();
        for chest in &chests {
            sim.set_circuit_read_contents(*chest, true)
                .expect("chests publish their contents");
        }
        insert_into_chest(&mut sim, chests[0], iron, 9);

        let mut links = vec![(chests[0], chests[1]), (chests[1], chests[2])];
        if reverse {
            links.reverse();
        }
        for (a, b) in links {
            connect(&mut sim, a, b, WireColor::Red);
        }
        sim.tick();
        (
            signal_value(&sim, chests[2], SignalId::Item(iron)),
            signal_value(&sim, chests[3], SignalId::Item(iron)),
        )
    };

    // Wiring the same graph in either order must produce the same reading:
    // network identity is derived from the connectors, not from insertion
    // order.
    assert_eq!(build(false), build(true));
    assert_eq!(build(false), (9, 0));
}

#[test]
fn circuit_state_survives_a_save_round_trip() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let signal = virtual_signal(&sim, "signal_a");
    let iron = item_id(&sim.world.prototypes, "iron_plate");

    let chest = place_named(&mut sim, "chest", ox, oy);
    let inserter = place_named(&mut sim, "burner_inserter", ox + 1, oy);
    sim.set_circuit_read_contents(chest, true)
        .expect("chests publish their contents");
    insert_into_chest(&mut sim, chest, iron, 4);
    wire_constant_source(&mut sim, single(chest), ox + 3, oy, signal, 11);
    // Red carries the combinator's signal to the inserter; green additionally
    // links the pair so the round trip covers both colors.
    connect(&mut sim, chest, inserter, WireColor::Red);
    connect(&mut sim, chest, inserter, WireColor::Green);
    sim.set_circuit_condition(
        inserter,
        Some(CircuitCondition {
            left: signal,
            comparator: Comparator::GreaterOrEqual,
            right: SignalOperand::Constant(10),
        }),
    )
    .expect("inserters accept an enable condition");
    sim.tick();

    let before = sim.state_hash();
    let bytes = save_to_bytes(&sim).expect("save should serialize");
    let mut loaded = load_from_bytes(&bytes).expect("save should load");

    assert_eq!(before, loaded.state_hash());
    // The networks are runtime state rebuilt from the durable wires, so the
    // loaded simulation must read the same signals without needing a tick.
    assert_eq!(signal_value(&loaded, chest, signal), 11);
    assert!(loaded.circuit_work_allowed(inserter));
    loaded.tick();
    sim.tick();
    assert_eq!(sim.state_hash(), loaded.state_hash());
}

#[test]
fn lamps_light_only_while_powered_and_enabled() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let signal = virtual_signal(&sim, "signal_a");

    let lamp = place_named(&mut sim, "lamp", ox, oy);
    let combinator = wire_constant_source(&mut sim, single(lamp), ox + 2, oy, signal, 1);
    sim.set_circuit_condition(
        lamp,
        Some(CircuitCondition {
            left: signal,
            comparator: Comparator::Greater,
            right: SignalOperand::Constant(0),
        }),
    )
    .expect("lamps accept an enable condition");

    // No power network yet, so the condition alone cannot light it.
    sim.tick();
    assert_eq!(
        crate::entity_access::lamp_is_lit(&sim, lamp),
        Some(false),
        "an unpowered lamp stays dark"
    );

    place_named(&mut sim, "small_electric_pole", ox + 1, oy + 1);
    place_named(&mut sim, "solar_panel", ox + 3, oy + 2);
    sim.tick();
    assert_eq!(crate::entity_access::lamp_is_lit(&sim, lamp), Some(true));

    sim.set_constant_combinator_slot(
        combinator,
        0,
        ConstantSignalSlot {
            signal: Some(signal),
            value: 0,
        },
    )
    .expect("constant combinator should accept a slot");
    sim.tick();
    assert_eq!(crate::entity_access::lamp_is_lit(&sim, lamp), Some(false));
}

#[test]
fn accumulator_reports_its_charge_percentage() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let signal = virtual_signal(&sim, "signal_a");

    let accumulator = place_named(&mut sim, "accumulator", ox, oy);
    let reader = place_named(&mut sim, "chest", ox + 3, oy);
    sim.set_circuit_read_contents(accumulator, true)
        .expect("accumulators publish their charge");
    sim.set_accumulator_charge_signal(accumulator, Some(signal))
        .expect("accumulators accept a charge signal");
    connect(&mut sim, accumulator, reader, WireColor::Red);

    let capacity = sim
        .world
        .prototypes
        .entity(
            sim.entities
                .placed_entity(accumulator)
                .expect("accumulator is placed")
                .prototype_id,
        )
        .and_then(|prototype| prototype.accumulator)
        .expect("accumulator prototype declares a capacity")
        .capacity_joules;
    sim.entities
        .accumulators
        .get_mut(&accumulator)
        .expect("accumulator state should exist")
        .stored_energy_joules = capacity / 4;

    sim.tick();

    assert_eq!(signal_value(&sim, reader, signal), 25);
}

#[test]
fn charge_signal_rejects_non_accumulators_with_specific_error() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    let chest = place_named(&mut sim, "chest", ox, oy);

    assert_eq!(
        sim.set_accumulator_charge_signal(chest, None),
        Err(CircuitError::NotAnAccumulator(chest))
    );
}

#[test]
fn runtime_circuit_state_participates_in_state_hash() {
    let baseline = Simulation::new_test_world(123);
    let node = CircuitNode::new(EntityId::new(1), ConnectorPort::Single);
    let signal = virtual_signal(&baseline, "signal_a");

    let mut different_topology = baseline.clone();
    different_topology
        .circuits
        .topology
        .network_ids
        .insert((node, WireColor::Red), 0);
    different_topology.circuits.topology.network_count = 1;

    let mut different_signals = baseline.clone();
    let mut network = SignalSet::default();
    network.add(signal, 1);
    different_signals.circuits.networks.push(network);

    let mut different_disabled_entities = baseline.clone();
    different_disabled_entities
        .circuits
        .disabled_entities
        .push(EntityId::new(1));

    for different in [
        different_topology,
        different_signals,
        different_disabled_entities,
    ] {
        assert_ne!(baseline.state_hash(), different.state_hash());
    }
}

#[test]
fn a_disabled_belt_holds_its_items() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = clear_area(&sim);
    stock_wires(&mut sim);
    let signal = virtual_signal(&sim, "signal_a");
    let iron = item_id(&sim.world.prototypes, "iron_plate");

    let belt = place_named(&mut sim, "transport_belt", ox, oy);
    wire_constant_source(&mut sim, single(belt), ox + 2, oy + 2, signal, 0);
    sim.set_circuit_condition(
        belt,
        Some(CircuitCondition {
            left: signal,
            comparator: Comparator::Greater,
            right: SignalOperand::Constant(0),
        }),
    )
    .expect("belts accept an enable condition");
    sim.insert_item_onto_belt(belt, 0, iron)
        .expect("belt should accept an item");
    let start = belt_item_positions(&sim, belt, 0);

    for _ in 0..10 {
        sim.tick();
    }

    assert_eq!(
        belt_item_positions(&sim, belt, 0),
        start,
        "a disabled belt must not advance its items"
    );
}
