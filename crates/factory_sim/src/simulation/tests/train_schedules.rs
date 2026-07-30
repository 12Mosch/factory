//! Trains running unattended: the stops a schedule names, the conditions that
//! let a train leave one, and what a schedule does when the station it names
//! stops existing.
//!
//! Every test here drives a train through a real world rather than poking the
//! schedule fields directly. What a schedule is *for* is a train that arrives,
//! waits for the right thing, and leaves again, and none of those three can be
//! checked without the route, the motion model, and the tick that ties them
//! together.

use super::super::*;
use super::rolling_stock::{fuel_train, place_stock, world_with_rail_run};
use crate::rolling_stock::{
    Train, TrainId, TrainSchedule, TrainScheduleEntry, TrainStopId, TrainThrottle,
    TrainWaitCondition, TrainWaitConditionGroup,
};

/// Ticks long enough for a locomotive to run the length of these fixtures twice
/// over, which is the point at which a train that has not arrived is not going
/// to.
const ARRIVAL_TICKS: usize = 4_000;

/// A wait no test is going to sit through: what an entry carries when the point
/// is that the train stays put until something else moves it on.
const FOREVER: TrainWaitCondition = TrainWaitCondition::TimePassed { ticks: u64::MAX };

fn entry(stop_name: &str, conditions: &[TrainWaitCondition]) -> TrainScheduleEntry {
    TrainScheduleEntry {
        stop_name: stop_name.into(),
        wait_conditions: if conditions.is_empty() {
            Vec::new()
        } else {
            vec![TrainWaitConditionGroup(conditions.to_vec())]
        },
    }
}

fn schedule(entries: Vec<TrainScheduleEntry>) -> TrainSchedule {
    TrainSchedule {
        entries,
        current: 0,
    }
}

fn rail_middle(sim: &Simulation, rail: EntityId) -> i64 {
    sim.rail_piece_geometry(rail)
        .expect("the rail is placed")
        .length_fixed
        / 2
}

fn stop_at(sim: &mut Simulation, name: &str, rail: EntityId, train_limit: u32) -> TrainStopId {
    let distance = rail_middle(sim, rail);
    sim.create_train_stop(name, rail, distance, train_limit)
        .expect("the middle of a placed rail takes a stop")
}

fn train(sim: &Simulation, train_id: TrainId) -> &Train {
    sim.train(train_id).expect("the train exists")
}

fn position(sim: &Simulation, train_id: TrainId) -> RailPosition {
    let stock_id = *train(sim, train_id)
        .stock
        .first()
        .expect("a train has stock");
    sim.rolling_stock_piece(stock_id)
        .expect("the train's stock is placed")
        .position
}

/// A twenty-four piece run with one fuelled locomotive standing a third of the
/// way up it.
fn world_with_a_schedulable_train() -> (Simulation, Vec<EntityId>, TrainId) {
    let (mut sim, rails) = world_with_rail_run(24);
    let stock_id =
        place_stock(&mut sim, &rails, 8, "locomotive").expect("a locomotive fits on a long run");
    let train_id = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive was just placed")
        .train;
    fuel_train(&mut sim, train_id, 50);
    (sim, rails, train_id)
}

/// Ticks until `ready` holds, failing the test if it never does.
fn run_until(sim: &mut Simulation, ready: impl Fn(&Simulation) -> bool) {
    for _ in 0..ARRIVAL_TICKS {
        sim.tick();
        if ready(sim) {
            return;
        }
    }
    panic!("the train never reached the state this test was waiting for");
}

fn run_until_waiting(sim: &mut Simulation, train_id: TrainId, stop_id: TrainStopId) {
    run_until(sim, |sim| {
        sim.train(train_id).is_some_and(|train| {
            train.is_waiting_at_scheduled_stop() && train.scheduled_stop == Some(stop_id)
        })
    });
}

#[test]
fn a_scheduled_train_drives_to_the_stop_its_entry_names_and_waits_there() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let north = stop_at(&mut sim, "North", rails[20], 1);
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");

    run_until_waiting(&mut sim, train_id, north);

    assert_eq!(
        position(&sim, train_id),
        RailPosition::new(rails[20], rail_middle(&sim, rails[20]), true),
        "it stopped on the mark the stop names rather than near it"
    );
    assert!(train(&sim, train_id).is_stationary());
    assert_eq!(
        train(&sim, train_id).schedule.current,
        0,
        "a train waiting at a stop is still serving that entry"
    );
    sim.validate().expect("a waiting train is a valid world");
}

/// The whole point of a schedule: a train alternating between two stations
/// without being told to go anywhere.
#[test]
fn a_two_stop_schedule_cycles_between_its_stops() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let north = stop_at(&mut sim, "North", rails[20], 1);
    let south = stop_at(&mut sim, "South", rails[3], 1);
    sim.set_train_schedule(
        train_id,
        schedule(vec![entry("North", &[]), entry("South", &[])]),
    )
    .expect("the train takes a schedule");

    // An entry with no conditions departs the tick after it arrives, so the
    // train is caught at each stop by where it stands rather than by waiting for
    // a claim it has already given back.
    let mark = |sim: &Simulation, rail: EntityId| {
        position(sim, train_id) == RailPosition::new(rail, rail_middle(sim, rail), true)
    };
    run_until(&mut sim, |sim| mark(sim, rails[20]));
    run_until(&mut sim, |sim| mark(sim, rails[3]));
    run_until(&mut sim, |sim| mark(sim, rails[20]));

    assert!(
        [Some(north), Some(south), None].contains(&train(&sim, train_id).scheduled_stop),
        "the train is always either serving one of its stops or between claims"
    );
}

/// Arrival is reaching the platform, not merely coming to a stand. A plan
/// withdrawn by hand leaves a train stationary with no route in the middle of
/// nowhere, and a schedule that counted that as an arrival would tick its cursor
/// past a station the train never visited.
#[test]
fn a_plan_cancelled_by_hand_is_not_an_arrival() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    stop_at(&mut sim, "North", rails[20], 1);
    stop_at(&mut sim, "South", rails[3], 1);
    sim.set_train_schedule(
        train_id,
        schedule(vec![entry("North", &[]), entry("South", &[])]),
    )
    .expect("the train takes a schedule");
    // One tick to claim the stop and be given a route, then out on the run.
    run_until(&mut sim, |sim| {
        sim.train(train_id).is_some_and(|train| train.velocity > 0)
    });

    sim.clear_train_destination(train_id)
        .expect("the train takes a cancellation");
    assert_eq!(
        train(&sim, train_id).scheduled_stop,
        None,
        "a train with no plan holds no place at a stop"
    );
    assert_eq!(train(&sim, train_id).schedule_arrival_tick, None);

    // Left to itself it brakes, is scheduled afresh, and serves the entry it
    // never finished — rather than departing an entry it never reached.
    run_until(&mut sim, |sim| {
        position(sim, train_id) == RailPosition::new(rails[20], rail_middle(sim, rails[20]), true)
    });
    sim.validate()
        .expect("a rescheduled train is a valid world");
}

/// A train being driven by hand is not on its way to the platform it booked, so
/// the place it was holding goes back rather than being kept against every train
/// that is actually coming.
#[test]
fn driving_a_scheduled_train_by_hand_gives_its_claim_back() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let north = stop_at(&mut sim, "North", rails[20], 1);
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    run_until_waiting(&mut sim, train_id, north);

    sim.set_train_throttle(train_id, TrainThrottle::Reverse)
        .expect("the train takes a throttle command");

    let driven = train(&sim, train_id);
    assert_eq!(driven.scheduled_stop, None);
    assert_eq!(driven.schedule_arrival_tick, None);
    assert_eq!(driven.destination, None);
}

/// Removing the last stop of a name would otherwise strand every train whose
/// current entry named it: the claim is gone, so the arrival check cannot fire,
/// and no stop answers to the name, so nothing can be claimed again.
#[test]
fn removing_the_last_stop_of_a_name_steps_the_schedule_past_it() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let north = stop_at(&mut sim, "North", rails[20], 1);
    stop_at(&mut sim, "South", rails[3], 1);
    sim.set_train_schedule(
        train_id,
        schedule(vec![entry("North", &[FOREVER]), entry("South", &[FOREVER])]),
    )
    .expect("the train takes a schedule");
    run_until_waiting(&mut sim, train_id, north);

    sim.remove_train_stop(north).expect("the stop exists");

    let stranded = train(&sim, train_id);
    assert_eq!(stranded.scheduled_stop, None);
    assert_eq!(
        stranded.schedule.current, 1,
        "the entry naming a station that no longer exists is stepped past"
    );
    run_until(&mut sim, |sim| {
        position(sim, train_id) == RailPosition::new(rails[3], rail_middle(sim, rails[3]), true)
    });
}

/// While another stop still answers to the name, removing one is not the end of
/// the entry: the train simply goes to the platform that is left.
#[test]
fn removing_one_of_two_stops_sharing_a_name_keeps_the_schedule_on_it() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let first = stop_at(&mut sim, "North", rails[20], 1);
    stop_at(&mut sim, "North", rails[16], 1);
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    run_until_waiting(&mut sim, train_id, first);

    sim.remove_train_stop(first).expect("the stop exists");

    assert_eq!(
        train(&sim, train_id).schedule.current,
        0,
        "the name is still served, so the entry still is"
    );
    run_until(&mut sim, |sim| {
        position(sim, train_id) == RailPosition::new(rails[16], rail_middle(sim, rails[16]), true)
    });
}

/// Renaming one platform of a two-platform station is a change to that platform
/// alone. Rewriting the schedule then would quietly stop the platform the player
/// did not touch from ever being served again.
#[test]
fn renaming_one_of_two_stops_sharing_a_name_leaves_schedules_alone() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let first = stop_at(&mut sim, "North", rails[20], 1);
    let second = stop_at(&mut sim, "North", rails[16], 1);
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");

    sim.rename_train_stop(first, "Depot")
        .expect("the stop takes a name");
    assert_eq!(
        train(&sim, train_id).schedule.entries[0].stop_name,
        "North",
        "the other platform still answers to the old name"
    );

    // The last stop of a name is a different matter: a schedule left pointing at
    // a station nobody answers to is a train with nowhere to go, so it follows.
    sim.rename_train_stop(second, "Depot")
        .expect("the stop takes a name");
    assert_eq!(
        train(&sim, train_id).schedule.entries[0].stop_name,
        "Depot",
        "the name left the world with the last stop bearing it, so schedules followed"
    );
}

/// A stop names a rail, and mining that rail leaves it naming nothing — a world
/// validation refuses, which would make an ordinary bit of track-pulling
/// unsaveable.
#[test]
fn a_stop_whose_rail_is_mined_is_forgotten() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let north = stop_at(&mut sim, "North", rails[20], 1);
    stop_at(&mut sim, "South", rails[3], 1);
    sim.set_train_schedule(
        train_id,
        schedule(vec![entry("North", &[FOREVER]), entry("South", &[FOREVER])]),
    )
    .expect("the train takes a schedule");
    // One tick to book the stop and be given a route to it. The rail is pulled up
    // while the train is still a third of the way down the run, so what is being
    // tested is the stop going rather than the train being stranded on the track
    // that went with it.
    sim.tick();
    assert_eq!(train(&sim, train_id).scheduled_stop, Some(north));

    crate::entity_mutation::remove(&mut sim, rails[20]);

    assert_eq!(sim.train_stops().count(), 1, "the stop went with its rail");
    let train_now = train(&sim, train_id);
    assert_eq!(train_now.scheduled_stop, None);
    assert_eq!(
        train_now.schedule.current, 1,
        "the entry naming the vanished station is stepped past"
    );
    sim.validate()
        .expect("a world whose track was pulled up is still a valid world");
    // And it goes on to serve the entry after it rather than idling for ever.
    run_until(&mut sim, |sim| {
        position(sim, train_id) == RailPosition::new(rails[3], rail_middle(sim, rails[3]), true)
    });
}

/// A stop's train limit is a limit on trains, so a second train wanting a full
/// stop books nothing rather than crowding onto it. Which train wins is settled
/// by id, so it is the same on every machine and every replay.
#[test]
fn a_stop_at_its_train_limit_takes_no_further_claims() {
    let (mut sim, rails) = world_with_rail_run(24);
    let first_stock = place_stock(&mut sim, &rails, 4, "locomotive").expect("the first fits");
    let second_stock = place_stock(&mut sim, &rails, 12, "locomotive").expect("the second fits");
    let first = sim.rolling_stock_piece(first_stock).expect("placed").train;
    let second = sim.rolling_stock_piece(second_stock).expect("placed").train;
    assert_ne!(first, second, "the two locomotives are two trains");
    fuel_train(&mut sim, first, 50);
    fuel_train(&mut sim, second, 50);
    let north = stop_at(&mut sim, "North", rails[20], 1);
    for train_id in [first, second] {
        sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
            .expect("the train takes a schedule");
    }

    sim.tick();

    assert_eq!(
        train(&sim, first).scheduled_stop,
        Some(north),
        "the lower train id claims the one place"
    );
    assert_eq!(
        train(&sim, second).scheduled_stop,
        None,
        "the stop is full, so the second train books nothing"
    );
    assert_eq!(train(&sim, second).destination, None);
}

/// Full is a statement about capacity, not about occupancy: a wagon holding one
/// plate in each of its forty slots is all but empty, and departing it as full
/// would send a train away with a fortieth of a load.
#[test]
fn cargo_full_needs_full_stacks_rather_than_occupied_slots() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let wagon = place_stock(&mut sim, &rails, 6, "cargo_wagon").expect("the wagon fits");
    assert_eq!(
        sim.rolling_stock_piece(wagon).expect("placed").train,
        train_id,
        "the wagon coupled onto the locomotive"
    );
    let north = stop_at(&mut sim, "North", rails[20], 1);
    sim.set_train_schedule(
        train_id,
        schedule(vec![
            entry("North", &[TrainWaitCondition::CargoFull]),
            entry("South", &[FOREVER]),
        ]),
    )
    .expect("the train takes a schedule");
    put_one_item_in_every_slot(&mut sim, wagon);

    run_until_waiting(&mut sim, train_id, north);
    for _ in 0..60 {
        sim.tick();
    }

    assert_eq!(
        train(&sim, train_id).scheduled_stop,
        Some(north),
        "a wagon with room in every stack is not full"
    );
    assert_eq!(train(&sim, train_id).schedule.current, 0);

    fill_every_slot(&mut sim, wagon);
    sim.tick();
    assert_eq!(
        train(&sim, train_id).schedule.current,
        1,
        "full stacks in every slot is what the condition was waiting for"
    );
}

/// Inactivity is time since the cargo last changed, not time since arrival. A
/// train still being loaded has to stay put, however long it has been standing
/// there.
#[test]
fn loading_a_waiting_train_resets_its_inactivity_clock() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let wagon = place_stock(&mut sim, &rails, 6, "cargo_wagon").expect("the wagon fits");
    let north = stop_at(&mut sim, "North", rails[20], 1);
    sim.set_train_schedule(
        train_id,
        schedule(vec![
            entry("North", &[TrainWaitCondition::Inactivity { ticks: 30 }]),
            entry("South", &[FOREVER]),
        ]),
    )
    .expect("the train takes a schedule");
    run_until_waiting(&mut sim, train_id, north);

    for _ in 0..20 {
        sim.tick();
    }
    let before = train(&sim, train_id).schedule_last_activity_tick;
    put_one_item_in_every_slot(&mut sim, wagon);
    sim.tick();
    assert!(
        train(&sim, train_id).schedule_last_activity_tick > before,
        "cargo changing is activity"
    );

    for _ in 0..20 {
        sim.tick();
    }
    assert_eq!(
        train(&sim, train_id).scheduled_stop,
        Some(north),
        "forty ticks after arriving but twenty after the last transfer, it stays"
    );

    for _ in 0..15 {
        sim.tick();
    }
    assert_eq!(
        train(&sim, train_id).schedule.current,
        1,
        "thirty ticks with nothing moving is what the condition was waiting for"
    );
}

/// An empty station name is refused where the stop APIs refuse it, rather than
/// accepted and reported much later as a broken world by validation.
#[test]
fn a_schedule_naming_no_station_is_refused() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    stop_at(&mut sim, "North", rails[20], 1);
    let good = schedule(vec![entry("North", &[])]);
    sim.set_train_schedule(train_id, good.clone())
        .expect("a named schedule is taken");

    assert_eq!(
        sim.set_train_schedule(
            train_id,
            schedule(vec![entry("North", &[]), entry("  ", &[])])
        ),
        Err(crate::rolling_stock::TrainControlError::EmptyStopName)
    );
    assert_eq!(
        train(&sim, train_id).schedule,
        good,
        "a refused schedule leaves the train the one it had"
    );
}

/// Puts a single item in every slot of a wagon: occupied everywhere, full
/// nowhere.
fn put_one_item_in_every_slot(
    sim: &mut Simulation,
    stock_id: crate::rolling_stock::RollingStockId,
) {
    set_every_slot(sim, stock_id, 1);
}

/// Fills every slot of a wagon to its stack size.
fn fill_every_slot(sim: &mut Simulation, stock_id: crate::rolling_stock::RollingStockId) {
    let catalog = sim.world.prototypes.clone();
    let coal = factory_data::item_id_by_name(&catalog, "coal");
    let stack_size = catalog.items[coal.index()].stack_size;
    set_every_slot(sim, stock_id, stack_size);
}

fn set_every_slot(
    sim: &mut Simulation,
    stock_id: crate::rolling_stock::RollingStockId,
    count: u16,
) {
    let catalog = sim.world.prototypes.clone();
    let coal = factory_data::item_id_by_name(&catalog, "coal");
    let stock = sim
        .rolling_stock
        .stock
        .get_mut(&stock_id)
        .expect("the wagon is placed");
    let inventory = stock
        .inventory
        .as_mut()
        .expect("a cargo wagon declares an inventory");
    let slot_count = inventory.slots().len();
    let slot = ItemSlot::from_stack(
        &catalog,
        ItemStack::new(&catalog, coal, count).expect("coal forms a valid stack"),
    )
    .expect("a wagon slot accepts coal");
    *inventory = Inventory::from_slots(&catalog, vec![slot; slot_count])
        .expect("the slots were built against this catalog");
}
