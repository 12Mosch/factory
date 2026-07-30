//! Loading and unloading stopped rolling stock: inserters against wagons and
//! locomotives, pumps against fluid wagons, and the player's own window.

use super::super::*;
use super::rolling_stock::{fuel_train, place_stock, unlock_with_prerequisites};
use super::support::*;
use crate::rolling_stock::{RollingStockId, TrainThrottle};

/// How many one-tile columns of clear ground the fixture keeps west of the
/// track. Three is what the longest thing placed against a wagon needs: a pump
/// lying across two of them with its outlet against the train, and a chest
/// behind that.
const CLEAR_COLUMNS_WEST: i64 = 3;

/// How many two-tile straights the fixture lays.
const RAIL_PIECES: usize = 12;

/// A straight run with clear ground beside it, one piece of stock parked on it,
/// and the tile that piece's centre lies over.
///
/// Every test here needs the same three things and they are not independent:
/// the inserter, the pump, and the index all have to be talking about one
/// square, so the fixture picks the track and hands back the square rather than
/// letting each test re-derive it. The run is placed where there is room beside
/// it, which the shared rail fixture does not promise — it only asks that the
/// rails themselves fit, and a run hugging the edge of the generated world has
/// nowhere to put a chest.
fn world_with_parked_wagon(name: &str) -> (Simulation, Vec<EntityId>, RollingStockId, (i64, i64)) {
    let mut sim = Simulation::new_test_world(123);
    let straight =
        factory_data::entity_prototype_id_by_name(&sim.world.prototypes, "rail_straight");
    let (origin_x, origin_y) = track_origin_with_room(&sim, straight);

    let rails = (0..RAIL_PIECES)
        .map(|index| {
            crate::placement::place(
                &mut sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: straight,
                    x: origin_x,
                    y: origin_y + index as WorldTileCoord * 2,
                    direction: Direction::North,
                },
            )
            .expect("the origin was chosen because every piece validates there")
        })
        .collect::<Vec<_>>();
    sim.tick();

    // Placement goes through the ordinary technology gate, and a fluid wagon
    // sits behind a technology of its own that locomotives and cargo wagons do
    // not need.
    if name == "fluid_wagon" {
        unlock_with_prerequisites(&mut sim, "fluid_wagon");
    }
    let stock_id = place_stock(&mut sim, &rails, 5, name).expect("the stock fits on the run");
    sim.tick();
    let tile = sim
        .rolling_stock_tile(stock_id)
        .expect("placed stock stands somewhere");
    (sim, rails, stock_id, tile)
}

/// The first origin where the whole run fits and the ground west of it is
/// clear the full length of the train.
fn track_origin_with_room(
    sim: &Simulation,
    straight: EntityPrototypeId,
) -> (WorldTileCoord, WorldTileCoord) {
    let chest = factory_data::entity_prototype_id_by_name(&sim.world.prototypes, "chest");
    let buildable = |prototype_id, x, y| {
        crate::placement::validate(
            sim,
            crate::placement::EntityPlacementRequest {
                prototype_id,
                x,
                y,
                direction: Direction::North,
            },
        )
        .is_ok()
    };

    for (x, y) in all_tile_coords(&sim.world) {
        if !(0..RAIL_PIECES as WorldTileCoord).all(|index| buildable(straight, x, y + index * 2)) {
            continue;
        }
        let clear_beside = (0..RAIL_PIECES as WorldTileCoord * 2)
            .all(|dy| (1..=CLEAR_COLUMNS_WEST).all(|dx| buildable(chest, x - dx, y + dy)));
        if clear_beside {
            return (x, y);
        }
    }
    panic!("the test world should hold a rail run with clear ground beside it");
}

/// Puts a burner inserter beside the track and fuels it.
///
/// A burner rather than an electric one because what is under test is the
/// wagon, not the power network: fuelling one machine by hand is a great deal
/// less fixture than wiring a pole, a boiler, and a steam engine along a
/// railway.
fn place_inserter_facing(
    sim: &mut Simulation,
    inserter_tile: (i64, i64),
    direction: Direction,
) -> EntityId {
    let inserter =
        factory_data::entity_prototype_id_by_name(&sim.world.prototypes, "burner_inserter");
    let entity_id = crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: inserter,
            x: inserter_tile.0,
            y: inserter_tile.1,
            direction,
        },
    )
    .expect("the inserter should be placeable beside the track");

    let catalog = sim.world.prototypes.clone();
    let coal = factory_data::item_id_by_name(&catalog, "coal");
    sim.entities
        .inserter_energy_mut(entity_id)
        .expect("a burner inserter keeps energy state")
        .fuel_slot_mut()
        .expect("a burner inserter keeps a fuel slot")
        .insert_stack(
            &catalog,
            ItemStack::new(&catalog, coal, 50).expect("coal stacks"),
        )
        .expect("an empty fuel slot takes a stack of coal");
    entity_id
}

fn place_chest_at(sim: &mut Simulation, x: i64, y: i64) -> EntityId {
    let chest = factory_data::entity_prototype_id_by_name(&sim.world.prototypes, "chest");
    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chest,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("the chest should be placeable beside the track")
}

fn fill_chest(sim: &mut Simulation, chest: EntityId, item: &str, count: u16) -> ItemId {
    let catalog = sim.world.prototypes.clone();
    let item_id = factory_data::item_id_by_name(&catalog, item);
    sim.entities
        .chest_inventory_mut(chest)
        .expect("the chest holds an inventory")
        .insert(&catalog, item_id, count)
        .expect("a fresh chest has room");
    item_id
}

fn wagon_count(sim: &Simulation, stock_id: RollingStockId, item_id: ItemId) -> u32 {
    sim.rolling_stock_piece(stock_id)
        .and_then(|stock| stock.inventory.as_ref())
        .map_or(0, |inventory| inventory.count(item_id))
}

/// An inserter standing between a chest and a stopped cargo wagon loads the
/// wagon, which is the whole point of the feature: the wagon is reached through
/// the same tile lookup a chest would be.
#[test]
fn an_inserter_loads_a_stopped_cargo_wagon_from_a_chest() {
    let (mut sim, _rails, stock_id, (wagon_x, wagon_y)) = world_with_parked_wagon("cargo_wagon");
    // The run goes north up column `wagon_x`, so a west-facing line of chest,
    // inserter, wagon sits across it.
    let inserter = place_inserter_facing(&mut sim, (wagon_x - 1, wagon_y), Direction::East);
    let chest = place_chest_at(&mut sim, wagon_x - 2, wagon_y);
    let iron_plate = fill_chest(&mut sim, chest, "iron_plate", 20);

    for _ in 0..240 {
        sim.tick();
    }

    assert!(
        wagon_count(&sim, stock_id, iron_plate) > 0,
        "an inserter beside a stopped wagon should have loaded it"
    );
    assert!(
        sim.entities
            .chest_inventory_mut(inserter)
            .is_none_or(|inventory| inventory.count(iron_plate) == 0),
        "the inserter itself holds no inventory"
    );
}

/// The mirror image: a wagon with cargo in it is emptied into a chest.
#[test]
fn an_inserter_unloads_a_stopped_cargo_wagon_into_a_chest() {
    let (mut sim, _rails, stock_id, (wagon_x, wagon_y)) = world_with_parked_wagon("cargo_wagon");
    let catalog = sim.world.prototypes.clone();
    let iron_plate = factory_data::item_id_by_name(&catalog, "iron_plate");
    sim.rolling_stock
        .get_mut(stock_id)
        .and_then(|stock| stock.inventory.as_mut())
        .expect("a cargo wagon carries an inventory")
        .insert(&catalog, iron_plate, 20)
        .expect("an empty wagon has room");

    // Facing west this time, so the wagon is the pickup side and the chest the
    // drop side.
    place_inserter_facing(&mut sim, (wagon_x - 1, wagon_y), Direction::West);
    let chest = place_chest_at(&mut sim, wagon_x - 2, wagon_y);

    for _ in 0..240 {
        sim.tick();
    }

    assert!(
        sim.entities
            .chest_inventory_mut(chest)
            .expect("the chest holds an inventory")
            .count(iron_plate)
            > 0,
        "an inserter should have moved cargo out of the stopped wagon"
    );
}

/// The rule the whole index exists to enforce. A train that pulls away is out
/// of reach in the same tick it starts to move, mid-swing or not.
#[test]
fn a_departing_train_is_no_longer_a_valid_inserter_endpoint() {
    let (mut sim, _rails, stock_id, (wagon_x, wagon_y)) = world_with_parked_wagon("cargo_wagon");
    let locomotive_id = {
        // A locomotive coupled behind the wagon, so the pair form one train the
        // throttle can actually drive.
        let rails = sim
            .entities
            .placed_entities
            .values()
            .filter(|placed| sim.rail_piece_geometry(placed.id).is_some())
            .map(|placed| placed.id)
            .collect::<Vec<_>>();
        place_stock(&mut sim, &rails, 2, "locomotive").expect("a locomotive fits behind the wagon")
    };
    let train_id = sim
        .rolling_stock_piece(locomotive_id)
        .expect("the locomotive was just placed")
        .train;
    fuel_train(&mut sim, train_id, 50);

    place_inserter_facing(&mut sim, (wagon_x - 1, wagon_y), Direction::East);
    let chest = place_chest_at(&mut sim, wagon_x - 2, wagon_y);
    let iron_plate = fill_chest(&mut sim, chest, "iron_plate", 20);
    sim.tick();
    assert!(
        sim.rolling_stock_is_stopped(stock_id),
        "the fixture parks the train before anything drives it"
    );

    sim.set_train_throttle(train_id, TrainThrottle::Forward)
        .expect("the train exists");
    sim.tick();
    assert!(
        !sim.rolling_stock_is_stopped(stock_id),
        "a train under power is not a valid transfer endpoint"
    );

    let carried = wagon_count(&sim, stock_id, iron_plate);
    for _ in 0..120 {
        sim.tick();
    }
    assert_eq!(
        wagon_count(&sim, stock_id, iron_plate),
        carried,
        "nothing should reach a moving wagon"
    );
}

/// A locomotive is a burner, and an inserter fuels it the way it fuels any
/// other burner machine. Its fuel slot is an input only: an unloading inserter
/// beside a locomotive must not strip the coal back out again.
#[test]
fn an_inserter_fuels_a_stopped_locomotive_but_cannot_empty_it() {
    let (mut sim, _rails, stock_id, (wagon_x, wagon_y)) = world_with_parked_wagon("locomotive");
    place_inserter_facing(&mut sim, (wagon_x - 1, wagon_y), Direction::East);
    let chest = place_chest_at(&mut sim, wagon_x - 2, wagon_y);
    let coal = fill_chest(&mut sim, chest, "coal", 20);

    for _ in 0..240 {
        sim.tick();
    }

    let fuelled = sim
        .rolling_stock_piece(stock_id)
        .and_then(|stock| stock.energy.as_ref())
        .and_then(|energy| energy.fuel_slot.stack())
        .map(|stack| stack.count())
        .unwrap_or(0);
    assert!(fuelled > 0, "an inserter should fuel a stopped locomotive");

    // The locomotive is not a source: peeking at it as a pickup finds nothing.
    assert_eq!(
        machine_ops::peek_inserter_source_item(
            &sim.entities,
            sim.stopped_stock(),
            (wagon_x, wagon_y)
        ),
        None,
        "a locomotive's fuel slot is an input, never a pickup"
    );
    let _ = coal;
}

/// A filtered slot only takes what it was filtered to, and an unfiltered wagon
/// behaves exactly as it did before filters existed.
#[test]
fn a_slot_filter_decides_what_a_wagon_will_take() {
    let (mut sim, _rails, stock_id, _tile) = world_with_parked_wagon("cargo_wagon");
    let catalog = sim.world.prototypes.clone();
    let iron_plate = factory_data::item_id_by_name(&catalog, "iron_plate");
    let copper_plate = factory_data::item_id_by_name(&catalog, "copper_plate");

    let inventory = sim
        .rolling_stock
        .get_mut(stock_id)
        .and_then(|stock| stock.inventory.as_mut())
        .expect("a cargo wagon carries an inventory");
    let slot_count = inventory.slots().len();
    for slot_index in 0..slot_count {
        inventory
            .set_filter(slot_index, Some(iron_plate))
            .expect("an empty slot takes any filter");
    }

    let inventory = sim
        .rolling_stock_piece(stock_id)
        .and_then(|stock| stock.inventory.as_ref())
        .expect("a cargo wagon carries an inventory");
    assert!(
        !inventory.can_insert(&catalog, copper_plate, 1),
        "a wholly iron-filtered wagon takes no copper"
    );
    assert!(
        inventory.can_insert(&catalog, iron_plate, 1),
        "a wholly iron-filtered wagon still takes iron"
    );

    // Clearing one slot reopens exactly that slot, and no more.
    sim.rolling_stock
        .get_mut(stock_id)
        .and_then(|stock| stock.inventory.as_mut())
        .expect("a cargo wagon carries an inventory")
        .set_filter(0, None)
        .expect("clearing a filter on an empty slot always works");
    let inventory = sim
        .rolling_stock_piece(stock_id)
        .and_then(|stock| stock.inventory.as_ref())
        .expect("a cargo wagon carries an inventory");
    let stack_size = catalog
        .item(copper_plate)
        .expect("copper plate is in the catalog")
        .stack_size;
    assert!(inventory.can_insert(&catalog, copper_plate, stack_size));
    assert!(!inventory.can_insert(&catalog, copper_plate, stack_size + 1));
}

/// A filter cannot be set on a slot that would contradict it, so "a filtered
/// slot holds only what it filters for" stays a fact everything else can rely
/// on.
#[test]
fn a_filter_is_refused_on_a_slot_holding_something_else() {
    let catalog = PrototypeCatalog::load_base().expect("base prototypes should load");
    let iron_plate = factory_data::item_id_by_name(&catalog, "iron_plate");
    let copper_plate = factory_data::item_id_by_name(&catalog, "copper_plate");
    let mut inventory = Inventory::with_slot_count(2);
    inventory
        .insert(&catalog, iron_plate, 1)
        .expect("an empty inventory has room");

    assert_eq!(
        inventory.set_filter(0, Some(copper_plate)),
        Err(InventoryError::FilterMismatch { slot_index: 0 })
    );
    assert_eq!(inventory.set_filter(0, Some(iron_plate)), Ok(()));
    assert_eq!(inventory.filter(0), Some(iron_plate));
}

/// An inventory nobody filtered stores no filters at all, so two worlds that
/// filtered nothing cannot hash differently.
#[test]
fn clearing_the_last_filter_leaves_no_filter_row() {
    let catalog = PrototypeCatalog::load_base().expect("base prototypes should load");
    let iron_plate = factory_data::item_id_by_name(&catalog, "iron_plate");
    let mut inventory = Inventory::with_slot_count(3);
    assert!(inventory.filters().is_empty());

    inventory
        .set_filter(1, Some(iron_plate))
        .expect("an empty slot takes any filter");
    assert_eq!(inventory.filters().len(), 3);

    inventory
        .set_filter(1, None)
        .expect("clearing a filter always works");
    assert!(
        inventory.filters().is_empty(),
        "the last filter cleared leaves the canonical empty row"
    );
}

/// Filtered empty slots are filled before unfiltered ones, so a wagon accepts
/// as much as its filters promised rather than spending its free slots first.
#[test]
fn insertion_fills_filtered_slots_before_free_ones() {
    let catalog = PrototypeCatalog::load_base().expect("base prototypes should load");
    let iron_plate = factory_data::item_id_by_name(&catalog, "iron_plate");
    let mut inventory = Inventory::with_slot_count(2);
    inventory
        .set_filter(1, Some(iron_plate))
        .expect("an empty slot takes any filter");

    inventory
        .insert(&catalog, iron_plate, 1)
        .expect("the filtered slot accepts iron");
    assert_eq!(inventory.slot(0), None);
    assert_eq!(inventory.slot(1).map(|stack| stack.count()), Some(1));
}

/// The player's own window reaches a wagon by id rather than by tile, and moves
/// items both ways through the shared transfer planner.
#[test]
fn the_player_can_move_items_into_and_out_of_a_wagon() {
    let (mut sim, _rails, stock_id, _tile) = world_with_parked_wagon("cargo_wagon");
    let catalog = sim.world.prototypes.clone();
    let iron_plate = factory_data::item_id_by_name(&catalog, "iron_plate");
    let carried_before = sim.player_inventory.count(iron_plate);
    sim.player_inventory
        .insert(&catalog, iron_plate, 10)
        .expect("the player inventory has room");
    // A fresh world hands the player a starting inventory, so the plates are
    // wherever they landed rather than in slot zero.
    let player_slot = sim
        .player_inventory
        .slots()
        .iter()
        .position(|slot| {
            slot.stack()
                .is_some_and(|stack| stack.item_id() == iron_plate)
        })
        .expect("the plates just went in somewhere");
    let moved = sim
        .player_inventory
        .slot(player_slot)
        .expect("the slot was just found")
        .count();

    let outcome = entity_transfer::transfer_rolling_stock_slot(
        &mut sim,
        stock_id,
        InventoryPanel::Player,
        player_slot,
    )
    .expect("a cargo wagon takes iron plate from the player");
    assert_eq!(outcome.moved_quantity, moved);
    assert_eq!(wagon_count(&sim, stock_id, iron_plate), u32::from(moved));
    assert_eq!(
        sim.player_inventory.count(iron_plate),
        carried_before + 10 - u32::from(moved)
    );

    entity_transfer::transfer_rolling_stock_slot(
        &mut sim,
        stock_id,
        InventoryPanel::RollingStockCargo,
        0,
    )
    .expect("the wagon gives the cargo back");
    assert_eq!(wagon_count(&sim, stock_id, iron_plate), 0);
    assert_eq!(sim.player_inventory.count(iron_plate), carried_before + 10);
}

/// A fluid wagon has no item inventory, so the transfer path says so rather
/// than silently doing nothing.
#[test]
fn a_fluid_wagon_takes_no_items_from_the_player() {
    let (mut sim, _rails, stock_id, _tile) = world_with_parked_wagon("fluid_wagon");
    let catalog = sim.world.prototypes.clone();
    let iron_plate = factory_data::item_id_by_name(&catalog, "iron_plate");
    sim.player_inventory
        .insert(&catalog, iron_plate, 1)
        .expect("the player inventory has room");
    let player_slot = sim
        .player_inventory
        .slots()
        .iter()
        .position(|slot| {
            slot.stack()
                .is_some_and(|stack| stack.item_id() == iron_plate)
        })
        .expect("the plate just went in somewhere");

    assert!(matches!(
        entity_transfer::transfer_rolling_stock_slot(
            &mut sim,
            stock_id,
            InventoryPanel::Player,
            player_slot
        ),
        Err(crate::rolling_stock::RollingStockTransferError::NoFuelSlot(
            _
        ))
    ));
}

/// Mining a wagon takes it out of the index with it, so nothing keeps swinging
/// into a wagon that is in the player's pocket.
#[test]
fn mining_a_stopped_wagon_takes_it_out_of_reach() {
    let (mut sim, _rails, stock_id, tile) = world_with_parked_wagon("cargo_wagon");
    assert!(sim.stopped_stock().at(tile.0, tile.1).is_some());

    sim.mine_rolling_stock(stock_id)
        .expect("an empty wagon fits in the player inventory");
    assert!(
        sim.stopped_stock().at(tile.0, tile.1).is_none(),
        "a mined wagon leaves no entry behind"
    );
    sim.validate_state().expect("mining leaves a valid world");
}

/// Pulling up the track under a parked train invalidates every tile derived
/// from its geometry.
#[test]
fn changing_the_track_clears_the_stopped_stock_index() {
    let (mut sim, rails, _stock_id, tile) = world_with_parked_wagon("cargo_wagon");
    assert!(sim.stopped_stock().at(tile.0, tile.1).is_some());

    // A rail at the far end of the run, nowhere near the wagon: the index is
    // cleared wholesale because every entry in it is derived from the graph
    // that just changed.
    entity_mutation::remove(&mut sim, rails[11]).expect("the rail is placed");
    assert!(sim.stopped_stock().at(tile.0, tile.1).is_none());

    sim.tick();
    assert!(
        sim.stopped_stock().at(tile.0, tile.1).is_some(),
        "the next tick re-indexes a train that never moved"
    );
}

/// A stopped fluid wagon at a pump joins the network the pump feeds, and leaves
/// it again when the train departs.
#[test]
fn a_pump_fills_a_stopped_fluid_wagon() {
    let (mut sim, _rails, stock_id, (wagon_x, wagon_y)) = world_with_parked_wagon("fluid_wagon");

    // A pump is one tile by two, input at one end and output at the other.
    // Turned west it lies across the track's side, occupying the two tiles west
    // of the wagon, with its outlet opening east onto the wagon's own tile —
    // which is the adjacency the wagon joins the network on.
    let pump = factory_data::entity_prototype_id_by_name(&sim.world.prototypes, "pump");
    let pump_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pump,
            x: wagon_x - 2,
            y: wagon_y,
            direction: Direction::West,
        },
    )
    .expect("a pump should fit in the clear ground beside the track");
    sim.tick();
    assert!(
        sim.fluid_networks()
            .iter()
            .flat_map(|network| &network.boxes)
            .any(|box_snapshot| box_snapshot.owner == FluidBoxOwner::RollingStock(stock_id)),
        "a pump opening onto a stopped fluid wagon should take its tank onto the network"
    );

    // With the wagon on the network, fluid put into that network reaches it.
    let network_id = sim
        .fluid_networks()
        .iter()
        .find(|network| {
            network
                .boxes
                .iter()
                .any(|box_snapshot| box_snapshot.owner == FluidBoxOwner::RollingStock(stock_id))
        })
        .expect("the wagon is on a network")
        .network_id;
    let water = factory_data::BasePrototypeIds::from_catalog(&sim.world.prototypes)
        .fluids
        .water;
    let added = sim.add_fluid_to_network(network_id, water, 500_000);
    assert!(added > 0, "the wagon's tank is capacity on that network");
    sim.tick();
    assert!(
        sim.rolling_stock_piece(stock_id)
            .and_then(|stock| stock.fluid_boxes.first())
            .is_some_and(|state| state.amount_milliunits > 0),
        "fluid added to the pump's network should settle into the wagon"
    );
    sim.validate_state()
        .expect("a wagon on a fluid network is a valid world");

    // And the pump is what holds the wagon on: take it away and the wagon's
    // tank leaves the topology.
    entity_mutation::remove(&mut sim, pump_id).expect("the pump is placed");
    sim.tick();
    assert!(
        !sim.fluid_networks()
            .iter()
            .flat_map(|network| &network.boxes)
            .any(|box_snapshot| box_snapshot.owner == FluidBoxOwner::RollingStock(stock_id)),
        "no pump reaches the wagon any more"
    );
    sim.validate_state()
        .expect("a wagon off every network is a valid world");
}

/// A world with a loaded wagon survives a save round trip, contents, filters,
/// and all.
#[test]
fn wagon_contents_and_filters_survive_a_save_round_trip() {
    let (mut sim, _rails, stock_id, _tile) = world_with_parked_wagon("cargo_wagon");
    let catalog = sim.world.prototypes.clone();
    let iron_plate = factory_data::item_id_by_name(&catalog, "iron_plate");
    let inventory = sim
        .rolling_stock
        .get_mut(stock_id)
        .and_then(|stock| stock.inventory.as_mut())
        .expect("a cargo wagon carries an inventory");
    inventory
        .insert(&catalog, iron_plate, 7)
        .expect("an empty wagon has room");
    inventory
        .set_filter(3, Some(iron_plate))
        .expect("an empty slot takes any filter");
    sim.tick();

    let bytes = save_to_bytes(&sim).expect("the world should serialize");
    let loaded = load_from_bytes(&bytes).expect("the world should load");

    assert_eq!(wagon_count(&loaded, stock_id, iron_plate), 7);
    assert_eq!(
        loaded
            .rolling_stock_piece(stock_id)
            .and_then(|stock| stock.inventory.as_ref())
            .and_then(|inventory| inventory.filter(3)),
        Some(iron_plate)
    );
    assert_eq!(loaded.state_hash(), sim.state_hash());
}

/// Nothing joins a fluid network unless a pump is reaching for it, and a
/// railway with no tanker parked anywhere does not even look for the pumps.
///
/// Both halves are asserted on the node set rather than on the guard alone, so
/// the test still means what its name says if the guard is ever moved: what
/// matters is which fluid boxes the topology ends up with, not which shortcut
/// got it there.
#[test]
fn a_railway_without_a_pump_at_a_tanker_adds_no_fluid_nodes() {
    let (sim, _rails, _stock_id, _tile) = world_with_parked_wagon("cargo_wagon");
    assert!(
        !sim.any_stopped_stock_carries_fluid(),
        "a cargo wagon has no tank, so nothing asks the pumps about it"
    );
    assert_eq!(sim.networked_rolling_stock_fluid_boxes().count(), 0);

    // A parked tanker does put the pump search back on — and still adds
    // nothing, because no pump on this railway opens onto it. A wagon joins a
    // network only where something reaches for it, which is what
    // `a_pump_fills_a_stopped_fluid_wagon` covers from the other side.
    let (sim, _rails, _stock_id, _tile) = world_with_parked_wagon("fluid_wagon");
    assert!(sim.any_stopped_stock_carries_fluid());
    assert_eq!(sim.networked_rolling_stock_fluid_boxes().count(), 0);
}
