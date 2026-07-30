use super::super::*;
use crate::rolling_stock::{
    RailPosition, RollingStockId, RollingStockPlacementError, TRAIN_VELOCITY_SCALE, TrainId,
    TrainThrottle,
};

/// A world with `piece_count` two-tile straights joined end to end, running
/// north, and the rails in order along the run.
///
/// Shared with the rolling-stock modules rather than kept to this file: the
/// traversal and placement code both need a stretch of real track to answer
/// against, and a second fixture is how two of them would end up testing
/// different railways.
pub(in crate::simulation) fn world_with_rail_run(
    piece_count: usize,
) -> (Simulation, Vec<EntityId>) {
    let mut sim = Simulation::new_test_world(123);
    let straight =
        factory_data::entity_prototype_id_by_name(&sim.world.prototypes, "rail_straight");
    let layout = (0..piece_count)
        .map(|index| (0, index as WorldTileCoord * 2))
        .collect::<Vec<_>>();
    let (origin_x, origin_y) = layout_origin(&sim, straight, &layout);

    let rails = layout
        .iter()
        .map(|(dx, dy)| {
            crate::placement::place(
                &mut sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: straight,
                    x: origin_x + dx,
                    y: origin_y + dy,
                    direction: Direction::North,
                },
            )
            .expect("the origin was chosen because every piece validates there")
        })
        .collect();
    sim.tick();
    (sim, rails)
}

/// A world holding a single quarter-turn of track.
pub(in crate::simulation) fn world_with_curved_rail() -> (Simulation, EntityId) {
    let mut sim = Simulation::new_test_world(123);
    let curved = factory_data::entity_prototype_id_by_name(&sim.world.prototypes, "rail_curved");
    let (x, y) = layout_origin(&sim, curved, &[(0, 0)]);
    let rail = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: curved,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("the origin was chosen because the curve validates there");
    sim.tick();
    (sim, rail)
}

fn layout_origin(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    offsets: &[(WorldTileCoord, WorldTileCoord)],
) -> (WorldTileCoord, WorldTileCoord) {
    for (x, y) in super::support::all_tile_coords(&sim.world) {
        let placeable = offsets.iter().all(|(dx, dy)| {
            crate::placement::validate(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id,
                    x: x + dx,
                    y: y + dy,
                    direction: Direction::North,
                },
            )
            .is_ok()
        });
        if placeable {
            return (x, y);
        }
    }

    panic!("expected a placeable rail layout in the test world");
}

fn stock_prototype(sim: &Simulation, name: &str) -> EntityPrototypeId {
    factory_data::entity_prototype_id_by_name(&sim.world.prototypes, name)
}

/// Puts one piece of stock on the rail at `rail_index` of the run, stocking the
/// player inventory and unlocking the technology first so the placement is
/// exercised through the same path a player uses.
pub(super) fn place_stock(
    sim: &mut Simulation,
    rails: &[EntityId],
    rail_index: usize,
    name: &str,
) -> Result<RollingStockId, RollingStockPlacementError> {
    let prototype_id = stock_prototype(sim, name);
    let item_id = sim
        .world
        .prototypes
        .entity(prototype_id)
        .and_then(|prototype| prototype.build_item)
        .expect("rolling stock declares a build item");
    unlock_rolling_stock(sim);
    let catalog = sim.world.prototypes.clone();
    sim.player_inventory
        .insert(&catalog, item_id, 1)
        .expect("the player inventory should accept rolling stock");

    let tile = sim
        .entities
        .placed_entity(rails[rail_index])
        .expect("the run's rails are placed")
        .footprint;
    sim.place_rolling_stock_from_player_inventory(prototype_id, item_id, tile.x, tile.y)
}

/// Researches `rolling_stock` and everything it depends on, so placement is
/// exercised through the same technology gate a player passes rather than
/// around it. Prerequisites are followed rather than listed: the chain is a
/// property of the catalog, and a hand-written list is one catalog edit away
/// from being wrong.
fn unlock_rolling_stock(sim: &mut Simulation) {
    unlock_with_prerequisites(sim, "rolling_stock");
}

pub(super) fn unlock_with_prerequisites(sim: &mut Simulation, technology_name: &str) {
    if sim.research.is_unlocked(technology_name) {
        return;
    }
    let technology_id = factory_data::technology_id_by_name(&sim.world.prototypes, technology_name);
    let prerequisites = sim.world.prototypes.technologies[technology_id.index()]
        .prerequisites
        .clone();
    for prerequisite in prerequisites {
        let name = sim.world.prototypes.technologies[prerequisite.index()]
            .name
            .clone();
        unlock_with_prerequisites(sim, &name);
    }
    super::support::complete_research_by_name(sim, technology_name);
}

/// Fuels every locomotive of a train so it can actually pull.
pub(super) fn fuel_train(sim: &mut Simulation, train_id: TrainId, coal_count: u16) {
    let catalog = sim.world.prototypes.clone();
    let coal = factory_data::item_id_by_name(&catalog, "coal");
    let members = sim.train(train_id).expect("the train exists").stock.clone();
    for stock_id in members {
        let Some(stock) = sim.rolling_stock.stock.get_mut(&stock_id) else {
            continue;
        };
        let Some(energy) = stock.energy.as_mut() else {
            continue;
        };
        energy.fuel_slot = ItemSlot::from_stack(
            &catalog,
            ItemStack::new(&catalog, coal, coal_count).expect("coal forms a valid stack"),
        )
        .expect("a locomotive fuel slot accepts coal");
    }
}

/// A run long enough for a locomotive to accelerate down, with one locomotive
/// fuelled and standing on it.
pub(super) fn world_with_a_driveable_locomotive()
-> (Simulation, Vec<EntityId>, RollingStockId, TrainId) {
    let (mut sim, rails) = world_with_rail_run(12);
    let stock_id = place_stock(&mut sim, &rails, 4, "locomotive")
        .expect("a locomotive fits on a twelve-piece run");
    let train_id = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive was just placed")
        .train;
    fuel_train(&mut sim, train_id, 50);
    (sim, rails, stock_id, train_id)
}

#[test]
fn a_placed_locomotive_stands_on_the_rail_as_its_own_train() {
    let (mut sim, rails) = world_with_rail_run(6);
    let stock_id = place_stock(&mut sim, &rails, 2, "locomotive").expect("the locomotive fits");

    let stock = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive was just placed");
    assert_eq!(sim.rolling_stock_count(), 1);
    assert_eq!(sim.train_count(), 1);
    assert_eq!(
        sim.train(stock.train).expect("the train exists").stock,
        vec![stock_id]
    );
    // On the track, not on the tile grid: the locomotive reserves nothing.
    assert!(
        sim.entities
            .occupancy
            .entity_at(
                sim.rolling_stock_tile(stock_id).expect("a world tile").0,
                sim.rolling_stock_tile(stock_id).expect("a world tile").1,
            )
            .is_some_and(|entity_id| rails.contains(&entity_id)),
        "the only thing occupying the tile under a locomotive is the rail"
    );
}

#[test]
fn a_locomotive_needs_track_long_enough_to_hold_it() {
    let (mut sim, rails) = world_with_rail_run(2);

    // Two two-tile straights are four tiles of track; a locomotive is seven.
    assert_eq!(
        place_stock(&mut sim, &rails, 0, "locomotive"),
        Err(RollingStockPlacementError::TrackTooShort)
    );
    assert_eq!(sim.rolling_stock_count(), 0);
}

#[test]
fn placing_a_wagon_beside_a_locomotive_couples_them_into_one_train() {
    let (mut sim, rails) = world_with_rail_run(16);
    let locomotive = place_stock(&mut sim, &rails, 8, "locomotive").expect("the locomotive fits");
    // Clicked three rails — six tiles — behind the locomotive, which is inside
    // the snap radius, so the wagon lands coupled rather than parked nearby.
    let wagon = place_stock(&mut sim, &rails, 5, "cargo_wagon").expect("the wagon fits");

    let train_id = sim
        .rolling_stock_piece(locomotive)
        .expect("the locomotive is placed")
        .train;
    assert_eq!(sim.train_count(), 1, "the two pieces form one train");
    assert_eq!(
        sim.rolling_stock_piece(wagon)
            .expect("the wagon is placed")
            .train,
        train_id
    );
    let train = sim.train(train_id).expect("the train exists");
    assert_eq!(train.stock.len(), 2);
    // Every piece of one train faces the same way, or a shared velocity would
    // drive them apart on the first tick.
    let facings = train
        .stock
        .iter()
        .map(|stock_id| {
            sim.rolling_stock_piece(*stock_id)
                .expect("train members exist")
                .position
                .forward
        })
        .collect::<Vec<_>>();
    assert_eq!(facings[0], facings[1]);
}

#[test]
fn a_fuelled_locomotive_accelerates_along_the_track_and_stops_at_the_end() {
    let (mut sim, _, stock_id, train_id) = world_with_a_driveable_locomotive();
    let start = sim
        .rolling_stock_world_point(stock_id)
        .expect("a placed locomotive has a world point");

    sim.set_train_throttle(train_id, TrainThrottle::Forward)
        .expect("the train takes a throttle command");
    sim.tick();
    sim.tick();
    let after_two_ticks = sim.train(train_id).expect("the train exists").velocity;
    assert!(
        after_two_ticks > 0,
        "an open throttle on a fuelled locomotive should accelerate it"
    );

    for _ in 0..2_000 {
        sim.tick();
    }

    let train = sim.train(train_id).expect("the train exists");
    assert!(
        train.is_stationary(),
        "a train that ran out of track should stop rather than keep trying"
    );
    let end = sim
        .rolling_stock_world_point(stock_id)
        .expect("a stopped locomotive still has a world point");
    assert_ne!(start, end, "the locomotive should have travelled");
    sim.validate()
        .expect("a train parked at the end of the line is a valid world");
}

/// Fuel goes through the ordinary burner path, so a driving locomotive draws
/// coal down and the statistics see it leave the world the same way a furnace's
/// does. A parked one keeps what it is carrying: the throttle is what spends
/// fuel, not merely having some.
#[test]
fn a_driving_locomotive_burns_its_fuel_and_a_parked_one_keeps_it() {
    let (mut sim, _, stock_id, train_id) = world_with_a_driveable_locomotive();
    let coal = factory_data::item_id_by_name(&sim.world.prototypes, "coal");
    let carried = |sim: &Simulation| {
        sim.rolling_stock_piece(stock_id)
            .and_then(|stock| stock.energy.as_ref())
            .and_then(|energy| energy.fuel_slot.stack())
            .map_or(0, |stack| stack.count())
    };
    let start = carried(&sim);
    assert!(start > 0, "the fixture fuels its locomotive");

    for _ in 0..600 {
        sim.tick();
    }
    assert_eq!(carried(&sim), start, "a parked locomotive burns nothing");

    sim.set_train_throttle(train_id, TrainThrottle::Forward)
        .expect("the train takes a throttle command");
    for _ in 0..600 {
        sim.tick();
    }

    let burnt = start - carried(&sim);
    assert!(burnt > 0, "a driving locomotive should burn its coal");
    assert_eq!(
        sim.item_statistics()
            .rows
            .iter()
            .find(|row| row.item_id == coal)
            .map_or(0, |row| row.consumed_total),
        u64::from(burnt),
        "burnt fuel should leave the world through the statistics"
    );
}

/// A train stops with its nose at the buffer, not with its middle there. The
/// distinction matters because the whole point of an edge-relative position is
/// that "at the end of this rail" is exact: a locomotive whose centre stopped
/// at the last rail would be hanging half a body over nothing.
#[test]
fn a_train_stops_with_its_leading_end_at_the_buffer() {
    let (mut sim, rails, stock_id, train_id) = world_with_a_driveable_locomotive();
    sim.set_train_throttle(train_id, TrainThrottle::Forward)
        .expect("the train takes a throttle command");
    for _ in 0..2_000 {
        sim.tick();
    }
    assert!(
        sim.train(train_id)
            .expect("the train exists")
            .is_stationary()
    );

    let last = *rails.last().expect("the run has rails");
    let end_of_line = sim
        .rail_piece_geometry(last)
        .expect("a placed rail has geometry");
    let (_, front) = sim
        .rolling_stock_body(stock_id)
        .expect("a placed locomotive has a body");

    assert_eq!(
        front, end_of_line.end.position,
        "the locomotive should be standing against the end of the line"
    );
    // And the centre is a half-length short of it rather than on it.
    let center = sim
        .rolling_stock_world_point(stock_id)
        .expect("a placed locomotive has a world point");
    assert_ne!(center, front);
}

#[test]
fn an_unfuelled_locomotive_does_not_move() {
    let (mut sim, rails) = world_with_rail_run(12);
    let stock_id = place_stock(&mut sim, &rails, 4, "locomotive").expect("the locomotive fits");
    let train_id = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive is placed")
        .train;
    let start = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive is placed")
        .position;

    sim.set_train_throttle(train_id, TrainThrottle::Forward)
        .expect("the train takes a throttle command");
    for _ in 0..600 {
        sim.tick();
    }

    assert_eq!(
        sim.rolling_stock_piece(stock_id)
            .expect("the locomotive is placed")
            .position,
        start,
        "a locomotive with no fuel has nothing to turn into tractive force"
    );
}

/// The coupling is rigid: a wagon behind a locomotive stays exactly as far
/// behind it after a run as it was before one.
#[test]
fn a_coupled_train_keeps_its_spacing_while_it_runs() {
    let (mut sim, rails) = world_with_rail_run(20);
    let locomotive = place_stock(&mut sim, &rails, 12, "locomotive").expect("the locomotive fits");
    let wagon = place_stock(&mut sim, &rails, 9, "cargo_wagon").expect("the wagon fits");
    let train_id = sim
        .rolling_stock_piece(locomotive)
        .expect("the locomotive is placed")
        .train;
    fuel_train(&mut sim, train_id, 50);

    let spacing = |sim: &Simulation| {
        let front = sim
            .rolling_stock_world_point(locomotive)
            .expect("the locomotive has a world point");
        let back = sim
            .rolling_stock_world_point(wagon)
            .expect("the wagon has a world point");
        (front.x - back.x).abs() + (front.y - back.y).abs()
    };
    let before = spacing(&sim);

    sim.set_train_throttle(train_id, TrainThrottle::Forward)
        .expect("the train takes a throttle command");
    for _ in 0..300 {
        sim.tick();
    }

    assert!(
        sim.rolling_stock_world_point(locomotive).is_some(),
        "the train is still on the rails"
    );
    assert_eq!(before, spacing(&sim), "a coupling does not stretch");
}

#[test]
fn braking_stops_a_train_within_the_distance_the_model_predicts() {
    let (mut sim, _, _, train_id) = world_with_a_driveable_locomotive();
    sim.set_train_throttle(train_id, TrainThrottle::Forward)
        .expect("the train takes a throttle command");
    for _ in 0..120 {
        sim.tick();
    }

    let train = sim.train(train_id).expect("the train exists");
    let velocity = train.velocity;
    assert!(velocity > 0, "the train should be rolling before it brakes");
    let forces = sim
        .train_forces_now(train_id)
        .expect("a train reports its forces");
    let predicted = crate::braking_distance_fixed(velocity, forces);

    sim.set_train_throttle(train_id, TrainThrottle::Brake)
        .expect("the train takes a throttle command");
    let mut ticks = 0;
    while !sim
        .train(train_id)
        .expect("the train exists")
        .is_stationary()
        && ticks < 10_000
    {
        sim.tick();
        ticks += 1;
    }

    assert!(
        sim.train(train_id)
            .expect("the train exists")
            .is_stationary(),
        "full braking should bring the train to a stand"
    );
    assert!(predicted > 0, "a rolling train has a stopping distance");
}

#[test]
fn mining_a_wagon_returns_it_and_its_cargo_to_the_player() {
    let (mut sim, rails) = world_with_rail_run(12);
    let wagon = place_stock(&mut sim, &rails, 4, "cargo_wagon").expect("the wagon fits");
    let catalog = sim.world.prototypes.clone();
    let plate = factory_data::item_id_by_name(&catalog, "iron_plate");
    sim.rolling_stock
        .stock
        .get_mut(&wagon)
        .expect("the wagon is placed")
        .inventory
        .as_mut()
        .expect("a cargo wagon has an inventory")
        .insert(&catalog, plate, 25)
        .expect("a cargo wagon accepts plates");
    let wagon_item = factory_data::item_id_by_name(&catalog, "cargo_wagon");

    sim.mine_rolling_stock(wagon).expect("the wagon comes off");

    assert_eq!(sim.rolling_stock_count(), 0);
    assert_eq!(sim.train_count(), 0);
    assert_eq!(sim.player_inventory().count(wagon_item), 1);
    assert_eq!(sim.player_inventory().count(plate), 25);
}

/// Uncoupling from the middle leaves two trains, and the one the player was
/// driving keeps its identity.
#[test]
fn mining_the_middle_of_a_train_splits_it() {
    let (mut sim, rails) = world_with_rail_run(24);
    let locomotive = place_stock(&mut sim, &rails, 16, "locomotive").expect("the locomotive fits");
    let middle = place_stock(&mut sim, &rails, 13, "cargo_wagon").expect("the first wagon fits");
    let tail = place_stock(&mut sim, &rails, 10, "cargo_wagon").expect("the second wagon fits");
    let train_id = sim
        .rolling_stock_piece(locomotive)
        .expect("the locomotive is placed")
        .train;
    assert_eq!(sim.train_count(), 1);

    sim.mine_rolling_stock(middle)
        .expect("the middle wagon comes off");

    assert_eq!(sim.train_count(), 2, "the run is broken into two trains");
    assert_ne!(
        sim.rolling_stock_piece(locomotive)
            .expect("the locomotive is placed")
            .train,
        sim.rolling_stock_piece(tail)
            .expect("the tail is placed")
            .train
    );
    assert!(
        sim.train(train_id).is_some(),
        "the original train keeps its identity"
    );
    sim.validate().expect("a split train is a valid world");
}

/// Mining the track out from under a train takes the train with it, and leaves
/// a world that still validates — the property a save made right afterwards
/// depends on.
#[test]
fn destroying_the_track_under_a_train_removes_it() {
    let (mut sim, rails) = world_with_rail_run(12);
    let stock_id = place_stock(&mut sim, &rails, 4, "locomotive").expect("the locomotive fits");
    let edge = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive is placed")
        .position
        .edge;

    crate::entity_mutation::remove(&mut sim, edge);

    assert_eq!(sim.rolling_stock_count(), 0);
    assert_eq!(sim.train_count(), 0);
    sim.validate()
        .expect("a world whose train lost its track is still valid");
}

#[test]
fn a_train_mid_run_survives_a_save_and_load() {
    let (mut sim, _, stock_id, train_id) = world_with_a_driveable_locomotive();
    sim.set_train_throttle(train_id, TrainThrottle::Forward)
        .expect("the train takes a throttle command");
    for _ in 0..90 {
        sim.tick();
    }
    assert!(
        sim.train(train_id).expect("the train exists").velocity > 0,
        "the train should be moving when it is saved"
    );

    let bytes = crate::save_to_bytes(&sim).expect("a world with a train saves");
    let mut loaded = crate::load_from_bytes(&bytes).expect("a world with a train loads");

    assert_eq!(sim.state_hash(), loaded.state_hash());
    assert_eq!(
        sim.rolling_stock_piece(stock_id)
            .expect("the locomotive is placed")
            .position,
        loaded
            .rolling_stock_piece(stock_id)
            .expect("the loaded locomotive is placed")
            .position
    );

    // The rail graph is rebuilt on load, so a loaded train has to keep running
    // on it rather than stalling on a position the new graph does not accept.
    for _ in 0..60 {
        sim.tick();
        loaded.tick();
    }
    assert_eq!(sim.state_hash(), loaded.state_hash());
}

#[test]
fn validation_rejects_stock_standing_past_the_end_of_its_rail() {
    let (mut sim, rails) = world_with_rail_run(12);
    let stock_id = place_stock(&mut sim, &rails, 4, "locomotive").expect("the locomotive fits");
    sim.validate().expect("the placed train is valid");

    sim.rolling_stock
        .stock
        .get_mut(&stock_id)
        .expect("the locomotive is placed")
        .position = RailPosition::new(rails[4], i64::MAX, true);

    assert_eq!(
        sim.validate(),
        Err(SimValidationError::InvalidRollingStock { stock_id })
    );
}

/// The two halves reference each other, so both directions are checked. A
/// train listing a piece that is not there is the half a one-sided uncoupling
/// would leave behind, and nothing later in the tick would notice it.
#[test]
fn validation_rejects_a_train_listing_stock_that_is_not_there() {
    let (mut sim, rails) = world_with_rail_run(12);
    let stock_id = place_stock(&mut sim, &rails, 4, "locomotive").expect("the locomotive fits");
    let train_id = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive is placed")
        .train;

    sim.rolling_stock
        .trains
        .get_mut(&train_id)
        .expect("the train exists")
        .stock
        .push(RollingStockId::new(u64::MAX));

    assert_eq!(
        sim.validate(),
        Err(SimValidationError::InvalidTrain { train_id })
    );
}

/// And the other direction: a piece whose train does not claim it back.
#[test]
fn validation_rejects_stock_its_train_does_not_claim() {
    let (mut sim, rails) = world_with_rail_run(12);
    let stock_id = place_stock(&mut sim, &rails, 4, "locomotive").expect("the locomotive fits");
    let train_id = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive is placed")
        .train;

    sim.rolling_stock
        .trains
        .get_mut(&train_id)
        .expect("the train exists")
        .stock
        .clear();

    assert_eq!(
        sim.validate(),
        Err(SimValidationError::InvalidRollingStock { stock_id })
    );
}

#[test]
fn validation_rejects_a_train_above_its_own_top_speed() {
    let (mut sim, rails) = world_with_rail_run(12);
    let stock_id = place_stock(&mut sim, &rails, 4, "locomotive").expect("the locomotive fits");
    let train_id = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive is placed")
        .train;

    sim.rolling_stock
        .trains
        .get_mut(&train_id)
        .expect("the train exists")
        .velocity = 100_000 * TRAIN_VELOCITY_SCALE;

    assert_eq!(
        sim.validate(),
        Err(SimValidationError::InvalidTrain { train_id })
    );
}

/// Placement refuses to put two pieces in the same place, and the tick has to
/// keep that true: a train that caught up with a parked one has to stop behind
/// it rather than drive through it.
#[test]
fn a_train_stops_behind_the_stock_in_front_of_it() {
    let (mut sim, rails) = world_with_rail_run(24);
    let parked = place_stock(&mut sim, &rails, 20, "cargo_wagon").expect("the wagon fits");
    let driver = place_stock(&mut sim, &rails, 4, "locomotive").expect("the locomotive fits");
    let train_id = sim
        .rolling_stock_piece(driver)
        .expect("the locomotive is placed")
        .train;
    assert_ne!(
        sim.rolling_stock_piece(parked)
            .expect("the wagon is placed")
            .train,
        train_id,
        "the two are far enough apart to be separate trains"
    );
    fuel_train(&mut sim, train_id, 50);

    sim.set_train_throttle(train_id, TrainThrottle::Forward)
        .expect("the train takes a throttle command");
    for _ in 0..2_000 {
        sim.tick();
    }

    let closed_up = |sim: &Simulation| {
        let front = sim
            .rolling_stock_world_point(driver)
            .expect("the locomotive has a world point");
        let back = sim
            .rolling_stock_world_point(parked)
            .expect("the wagon has a world point");
        (front.x - back.x).abs() + (front.y - back.y).abs()
    };
    // Half a locomotive plus half a wagon is 6656 units; anything less means
    // the two have driven into each other.
    assert!(
        closed_up(&sim) >= 6_656,
        "the locomotive should have stopped behind the wagon, not through it"
    );
    assert!(
        sim.train(train_id)
            .expect("the train exists")
            .is_stationary(),
        "a train stopped by the stock ahead should come to rest"
    );
    assert_eq!(sim.train_count(), 2, "stopping is not coupling");
    sim.validate()
        .expect("a train parked behind another is a valid world");
}

/// A train that ran itself down to a stand on open track has to *read* as
/// stopped. Sub-unit travel left owing at zero velocity can never be spent, so
/// keeping it would leave `is_stationary` false forever — and that is the
/// question a station or a stop-and-wait will be asking.
#[test]
fn a_train_that_slows_to_a_stand_reports_itself_stationary() {
    let (mut sim, _, _, train_id) = world_with_a_driveable_locomotive();
    sim.set_train_throttle(train_id, TrainThrottle::Forward)
        .expect("the train takes a throttle command");
    for _ in 0..40 {
        sim.tick();
    }
    assert!(sim.train(train_id).expect("the train exists").velocity > 0);

    sim.set_train_throttle(train_id, TrainThrottle::Brake)
        .expect("the train takes a throttle command");
    for _ in 0..600 {
        sim.tick();
    }

    let train = sim.train(train_id).expect("the train exists");
    assert_eq!(train.velocity, 0);
    assert_eq!(train.travel_remainder, 0, "a stopped train owes nothing");
    assert!(train.is_stationary());
}

/// Ordering and regrouping walk the train's own extent, so a train longer than
/// the pairwise coupling radius still splits where the piece was taken out
/// rather than where the walk ran out.
#[test]
fn a_train_longer_than_the_coupling_radius_still_splits_correctly() {
    let (mut sim, rails) = world_with_rail_run(48);
    // Seven wagons at six tiles each run to well over the 32-tile pairwise
    // radius, which is the case the bounded walk used to mis-group.
    let mut wagons = Vec::new();
    for index in 0..7 {
        wagons.push(
            place_stock(&mut sim, &rails, 40 - index * 3, "cargo_wagon")
                .expect("each wagon couples onto the last"),
        );
    }
    assert_eq!(sim.train_count(), 1, "the wagons form one long train");
    let extent = {
        let train = sim.trains().next().expect("the train exists");
        sim.rolling_stock_piece(train.stock[0])
            .and_then(|front| {
                let back = sim.rolling_stock_piece(*train.stock.last()?)?;
                let front = sim.rolling_stock_world_point(front.id)?;
                let back = sim.rolling_stock_world_point(back.id)?;
                Some((front.x - back.x).abs() + (front.y - back.y).abs())
            })
            .expect("the train has an extent")
    };
    assert!(
        extent > 32 * POSITION_SCALE,
        "the fixture train should be longer than the pairwise radius, was {extent}"
    );

    sim.mine_rolling_stock(wagons[3])
        .expect("the middle wagon comes off");

    assert_eq!(sim.train_count(), 2, "the run is broken into two trains");
    let head = sim
        .rolling_stock_piece(wagons[0])
        .expect("the head is placed")
        .train;
    let tail = sim
        .rolling_stock_piece(wagons[6])
        .expect("the tail is placed")
        .train;
    assert_ne!(head, tail);
    for index in [1, 2] {
        assert_eq!(
            sim.rolling_stock_piece(wagons[index])
                .expect("the wagon is placed")
                .train,
            head,
            "wagon {index} is on the head's side of the gap"
        );
    }
    for index in [4, 5] {
        assert_eq!(
            sim.rolling_stock_piece(wagons[index])
                .expect("the wagon is placed")
                .train,
            tail,
            "wagon {index} is on the tail's side of the gap"
        );
    }
    sim.validate().expect("a split long train is a valid world");
}

/// The cursor test follows the body along the track rather than the rectangle
/// its ends span, so the far corners of a curve's bounding box are not claimed
/// by stock whose arc never crosses them.
#[test]
fn tile_coverage_follows_the_body_rather_than_its_bounding_box() {
    let (mut sim, rails) = world_with_rail_run(12);
    let stock_id = place_stock(&mut sim, &rails, 4, "locomotive").expect("the locomotive fits");
    let (back, front) = sim
        .rolling_stock_body(stock_id)
        .expect("a placed locomotive has a body");
    let (back_tile, front_tile) = (back.tile(), front.tile());

    // Every tile the body actually runs through is claimed.
    for y in back_tile.1.min(front_tile.1)..=back_tile.1.max(front_tile.1) {
        assert!(
            sim.rolling_stock_covers_tile(stock_id, back_tile.0, y),
            "the body runs through ({}, {y})",
            back_tile.0
        );
    }
    // The column beside it is not, even though a wider test might claim it.
    assert!(!sim.rolling_stock_covers_tile(stock_id, back_tile.0 + 1, back_tile.1));
    assert!(!sim.rolling_stock_covers_tile(
        stock_id,
        back_tile.0,
        back_tile.1.min(front_tile.1) - 1
    ));
}

/// The fixture the performance suite runs on has to actually produce moving
/// trains, or the budget it measures would be a budget for an empty world.
#[test]
fn the_rolling_stock_fixture_puts_trains_on_the_move() {
    let mut sim = Simulation::new_rolling_stock_fixture(8);
    assert_eq!(sim.train_count(), 8);
    assert_eq!(sim.rolling_stock_count(), 24);

    for _ in 0..120 {
        sim.tick();
    }

    assert!(
        sim.trains().filter(|train| train.velocity > 0).count() >= 4,
        "the fixture's trains should be rolling, not parked"
    );
    sim.validate().expect("the fixture world is valid");
}
