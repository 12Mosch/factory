//! Sending trains places: the route the search produces, the driving that
//! follows it, and what happens to a plan when the track under it changes.
//!
//! The searches themselves are tested against hand-built graphs beside the
//! search; what these tests are about is a train in a world actually getting
//! there.

use super::super::*;
use super::rolling_stock::{fuel_train, place_stock, world_with_a_driveable_locomotive};
use crate::rolling_stock::{RollingStockId, TrainId, TrainThrottle};

/// Ticks until the train has arrived, or gives up. Long enough for a
/// locomotive to run the length of these fixtures twice over, which is the
/// point at which a train that has not arrived is not going to.
const ARRIVAL_TICKS: usize = 4_000;

/// A twenty-four piece run with one fuelled locomotive standing a third of the
/// way along it, facing up the run.
fn world_with_a_routed_locomotive() -> (Simulation, Vec<EntityId>, RollingStockId, TrainId) {
    let (mut sim, rails) = super::rolling_stock::world_with_rail_run(24);
    let stock_id =
        place_stock(&mut sim, &rails, 8, "locomotive").expect("a locomotive fits on a long run");
    let train_id = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive was just placed")
        .train;
    fuel_train(&mut sim, train_id, 50);
    (sim, rails, stock_id, train_id)
}

/// Ticks until `ready` holds, failing the test if it never does.
///
/// Every test that wants to catch a train *part way* through a journey waits
/// for the state it wants rather than counting ticks at it: how far a
/// locomotive gets in a second is a property of the catalog, and a tick count
/// that happens to land mid-run today is one balance change from landing after
/// the train has already arrived.
fn run_until(sim: &mut Simulation, ready: impl Fn(&Simulation) -> bool) {
    for _ in 0..ARRIVAL_TICKS {
        sim.tick();
        if ready(sim) {
            return;
        }
    }
    panic!("the train never reached the state this test was waiting for");
}

fn run_until_arrived(sim: &mut Simulation, train_id: TrainId) {
    for _ in 0..ARRIVAL_TICKS {
        sim.tick();
        if sim
            .train(train_id)
            .is_none_or(|train| train.destination.is_none() && train.is_stationary())
        {
            return;
        }
    }
}

/// Where the train's leading piece stands, which is what a route is measured
/// from and therefore what "arrived" is about.
fn position(sim: &Simulation, train_id: TrainId) -> RailPosition {
    let stock_id = *sim
        .train(train_id)
        .expect("the train exists")
        .stock
        .first()
        .expect("a train has stock");
    sim.rolling_stock_piece(stock_id)
        .expect("the train's stock is placed")
        .position
}

fn rail_middle(sim: &Simulation, rail: EntityId) -> i64 {
    sim.rail_piece_geometry(rail)
        .expect("the rail is placed")
        .length_fixed
        / 2
}

#[test]
fn a_train_sent_to_a_rail_drives_there_and_stops_on_it() {
    let (mut sim, rails, _, train_id) = world_with_a_routed_locomotive();

    sim.set_train_destination(train_id, rails[20])
        .expect("the train takes a destination");
    run_until_arrived(&mut sim, train_id);

    // Exactly on the mark, not near it: the route clips the last step the same
    // way the end of the line does, so the train stops where it was sent rather
    // than wherever braking happened to leave it.
    assert_eq!(
        position(&sim, train_id),
        RailPosition::new(rails[20], rail_middle(&sim, rails[20]), true)
    );
    let train = sim.train(train_id).expect("the train exists");
    assert!(train.is_stationary());
    assert_eq!(train.route, None, "an arrived train has no plan left");
    assert_eq!(train.destination, None);
    assert_eq!(train.throttle, TrainThrottle::Coast);
    sim.validate().expect("an arrived train is a valid world");
}

/// A destination behind the train is driven to in reverse rather than by
/// turning the train around, which is what a locomotive with a cab at both ends
/// actually does.
#[test]
fn a_train_sent_behind_itself_backs_down_the_run() {
    let (mut sim, rails, _, train_id) = world_with_a_routed_locomotive();
    let facing = position(&sim, train_id).forward;

    sim.set_train_destination(train_id, rails[2])
        .expect("the train takes a destination");
    sim.tick();
    assert_eq!(
        sim.train(train_id)
            .expect("the train exists")
            .route
            .as_ref()
            .and_then(|route| route.current_leg())
            .map(|leg| leg.forward),
        Some(false),
        "the only leg of this route is driven in reverse"
    );

    run_until_arrived(&mut sim, train_id);

    let arrived = position(&sim, train_id);
    assert_eq!(
        arrived,
        RailPosition::new(rails[2], rail_middle(&sim, rails[2]), facing),
        "a train that reversed there is still facing the way it started"
    );
}

/// The plan is measured from the train, so a train told to drive to the rail it
/// is already standing on simply rolls to the mark on it.
#[test]
fn a_train_sent_to_the_rail_under_it_creeps_onto_the_mark() {
    let (mut sim, _, _, train_id) = world_with_a_routed_locomotive();
    let standing_on = position(&sim, train_id).edge;

    sim.set_train_destination(train_id, standing_on)
        .expect("the train takes a destination");
    run_until_arrived(&mut sim, train_id);

    assert_eq!(
        position(&sim, train_id),
        RailPosition::new(standing_on, rail_middle(&sim, standing_on), true)
    );
    assert_eq!(
        sim.train(train_id).expect("the train exists").destination,
        None
    );
}

/// Driving by hand wins over a plan. The routing pass writes the throttle of
/// every train it steers, so a drive command that left the destination behind
/// would be overwritten inside the tick and the train would look like it had
/// ignored the player.
#[test]
fn driving_a_train_by_hand_cancels_where_it_was_going() {
    let (mut sim, rails, _, train_id) = world_with_a_routed_locomotive();
    sim.set_train_destination(train_id, rails[20])
        .expect("the train takes a destination");
    sim.tick();
    assert!(
        sim.train(train_id)
            .expect("the train exists")
            .route
            .is_some()
    );

    sim.set_train_throttle(train_id, TrainThrottle::Reverse)
        .expect("the train takes a throttle command");

    let train = sim.train(train_id).expect("the train exists");
    assert_eq!(train.destination, None);
    assert_eq!(train.route, None);
    assert_eq!(train.throttle, TrainThrottle::Reverse);
}

/// Track pulled up under a plan invalidates that plan and nothing else: the
/// train still has somewhere to be, so it plans again from where it now stands
/// and carries on.
#[test]
fn pulling_up_track_a_route_used_replans_it() {
    let (mut sim, rails, _, train_id) = world_with_a_routed_locomotive();
    sim.set_train_destination(train_id, rails[20])
        .expect("the train takes a destination");
    let left_behind = rails[8];
    run_until(&mut sim, |sim| position(sim, train_id).edge != left_behind);
    let route = sim
        .train(train_id)
        .expect("the train exists")
        .route
        .clone()
        .expect("a driving train has a plan");
    assert!(
        route.uses_edge(left_behind),
        "the plan started on the rail the train was standing on"
    );
    assert_ne!(
        position(&sim, train_id).edge,
        left_behind,
        "the train has already run off the rail this test pulls up"
    );

    crate::entity_mutation::remove(&mut sim, left_behind);

    let train = sim.train(train_id).expect("the train exists");
    assert_eq!(train.route, None, "the plan went with the track");
    assert!(
        train.destination.is_some(),
        "where the train was going outlives the plan for getting there"
    );

    sim.tick();
    assert!(
        sim.train(train_id)
            .expect("the train exists")
            .route
            .is_some(),
        "the next tick plans the journey again"
    );
    run_until_arrived(&mut sim, train_id);
    assert_eq!(
        position(&sim, train_id),
        RailPosition::new(rails[20], rail_middle(&sim, rails[20]), true)
    );
}

/// Taking up the rail the train was sent to leaves it with nowhere to be, so it
/// stops asking and brakes rather than driving on toward a destination that is
/// no longer there.
#[test]
fn taking_up_the_destination_rail_cancels_the_journey() {
    let (mut sim, rails, _, train_id) = world_with_a_routed_locomotive();
    sim.set_train_destination(train_id, rails[20])
        .expect("the train takes a destination");
    run_until(&mut sim, |sim| position(sim, train_id).edge == rails[12]);

    crate::entity_mutation::remove(&mut sim, rails[20]);

    let train = sim.train(train_id).expect("the train exists");
    assert_eq!(train.destination, None);
    assert_eq!(train.route, None);
    assert_eq!(train.throttle, TrainThrottle::Brake);

    for _ in 0..600 {
        sim.tick();
    }
    assert!(
        sim.train(train_id)
            .expect("the train exists")
            .is_stationary(),
        "a train with nowhere to go comes to a stop"
    );
    sim.validate()
        .expect("a train whose destination was mined is a valid world");
}

/// Track cut between the train and where it is going is not a plan to redo but
/// a journey that cannot be made, and the train has to be told so rather than
/// re-searching for it every tick forever.
#[test]
fn cutting_the_track_ahead_gives_up_on_the_journey() {
    let (mut sim, rails, _, train_id) = world_with_a_routed_locomotive();
    sim.set_train_destination(train_id, rails[20])
        .expect("the train takes a destination");
    sim.tick();

    crate::entity_mutation::remove(&mut sim, rails[14]);
    sim.tick();

    let train = sim.train(train_id).expect("the train exists");
    assert_eq!(
        train.destination, None,
        "a destination on track that is no longer joined is given up on"
    );
    assert_eq!(train.route, None);
}

/// A train held up by something parked in its way is still on its journey. Only
/// the route and the end of the line retire a leg; stock in the way is a wait.
#[test]
fn a_train_blocked_by_stock_keeps_its_plan() {
    let (mut sim, rails) = super::rolling_stock::world_with_rail_run(24);
    place_stock(&mut sim, &rails, 16, "cargo_wagon").expect("the wagon fits");
    let driver = place_stock(&mut sim, &rails, 4, "locomotive").expect("the locomotive fits");
    let train_id = sim
        .rolling_stock_piece(driver)
        .expect("the locomotive is placed")
        .train;
    fuel_train(&mut sim, train_id, 50);

    sim.set_train_destination(train_id, rails[20])
        .expect("the train takes a destination");
    for _ in 0..1_200 {
        sim.tick();
    }

    let train = sim.train(train_id).expect("the train exists");
    assert!(
        train.is_stationary(),
        "the locomotive should have stopped behind the wagon"
    );
    assert!(
        train.route.is_some() && train.destination.is_some(),
        "a train waiting for the road ahead has not arrived"
    );
    assert_eq!(sim.train_count(), 2, "waiting is not coupling");
    sim.validate().expect("a train held up by another is valid");
}

/// A plan is durable state, so a train part way through one is part way through
/// it after a save — and goes on to arrive at the same place, tick for tick.
#[test]
fn a_train_mid_journey_survives_a_save_and_load() {
    let (mut sim, rails, _, train_id) = world_with_a_routed_locomotive();
    sim.set_train_destination(train_id, rails[20])
        .expect("the train takes a destination");
    run_until(&mut sim, |sim| position(sim, train_id).edge == rails[12]);
    assert!(
        sim.train(train_id)
            .expect("the train exists")
            .route
            .is_some()
    );

    let bytes = crate::save_to_bytes(&sim).expect("a world with a routed train saves");
    let mut loaded = crate::load_from_bytes(&bytes).expect("a world with a routed train loads");
    assert_eq!(sim.state_hash(), loaded.state_hash());

    run_until_arrived(&mut sim, train_id);
    run_until_arrived(&mut loaded, train_id);
    assert_eq!(sim.state_hash(), loaded.state_hash());
    assert_eq!(
        position(&sim, train_id),
        RailPosition::new(rails[20], rail_middle(&sim, rails[20]), true)
    );
}

/// Coupling changes what the train is, and a plan priced for the train that
/// found it is not a plan for the one that comes out. The destination stays, so
/// the longer train plans the same journey again for itself.
#[test]
fn coupling_a_wagon_on_replans_the_journey() {
    let (mut sim, rails, _, train_id) = world_with_a_routed_locomotive();
    sim.set_train_destination(train_id, rails[20])
        .expect("the train takes a destination");
    sim.tick();
    assert!(
        sim.train(train_id)
            .expect("the train exists")
            .route
            .is_some()
    );

    place_stock(&mut sim, &rails, 5, "cargo_wagon").expect("the wagon fits behind the locomotive");

    let train = sim.train(train_id).expect("the train exists");
    assert_eq!(train.stock.len(), 2, "the wagon coupled on");
    assert_eq!(train.route, None, "the plan belonged to the shorter train");
    assert!(train.destination.is_some());
}

#[test]
fn a_destination_that_is_not_a_rail_is_refused() {
    let (mut sim, _, stock_id, train_id) = world_with_a_driveable_locomotive();
    let not_a_rail = EntityId::new(u64::MAX);

    assert_eq!(
        sim.set_train_destination(train_id, not_a_rail),
        Err(TrainControlError::NotRail(not_a_rail))
    );
    assert_eq!(
        sim.set_train_destination(TrainId::new(u64::MAX), rails_under(&sim, stock_id)),
        Err(TrainControlError::MissingTrain(TrainId::new(u64::MAX)))
    );
    assert_eq!(
        sim.train(train_id).expect("the train exists").destination,
        None
    );
}

fn rails_under(sim: &Simulation, stock_id: RollingStockId) -> EntityId {
    sim.rolling_stock_piece(stock_id)
        .expect("the stock is placed")
        .position
        .edge
}

/// Cancelling brings a train to a stop rather than leaving it rolling toward
/// somewhere nobody is steering it to.
#[test]
fn cancelling_a_journey_brakes_the_train() {
    let (mut sim, rails, _, train_id) = world_with_a_routed_locomotive();
    sim.set_train_destination(train_id, rails[20])
        .expect("the train takes a destination");
    run_until(&mut sim, |sim| position(sim, train_id).edge == rails[12]);
    assert!(sim.train(train_id).expect("the train exists").velocity > 0);

    sim.clear_train_destination(train_id)
        .expect("the train takes a cancellation");

    let train = sim.train(train_id).expect("the train exists");
    assert_eq!(train.destination, None);
    assert_eq!(train.route, None);
    assert_eq!(train.throttle, TrainThrottle::Brake);

    for _ in 0..600 {
        sim.tick();
    }
    assert!(
        sim.train(train_id)
            .expect("the train exists")
            .is_stationary()
    );
}
