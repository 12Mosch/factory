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
use super::rolling_stock::{
    fuel_train, place_stock, unlock_with_prerequisites, world_with_rail_run,
};
use crate::rolling_stock::{
    Train, TrainId, TrainSchedule, TrainScheduleEntry, TrainThrottle, TrainWaitCondition,
    TrainWaitConditionGroup,
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

/// Puts a stop on the tile beside `rail` and gives it a name and a limit.
///
/// Beside rather than on: a stop is an ordinary placed entity that binds to the
/// track next to it, so every test here builds its station the way a player
/// would. Either side will do — which one is free depends on the terrain the
/// fixture found room on — and the binding rule picks the same rail from both.
fn stop_at(sim: &mut Simulation, name: &str, rail: EntityId, train_limit: u32) -> EntityId {
    let prototype_id =
        factory_data::entity_prototype_id_by_name(&sim.world.prototypes, "train_stop");
    let (rail_x, rail_y) = sim
        .entities
        .placed_entity(rail)
        .map(|placed| (placed.x, placed.y))
        .expect("the run's rails are placed");
    let stop = [1, -1]
        .into_iter()
        .find_map(|dx| {
            crate::placement::place(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id,
                    x: rail_x + dx,
                    y: rail_y,
                    direction: Direction::North,
                },
            )
            .ok()
        })
        .expect("one of the tiles beside the track takes a stop");
    sim.rename_train_stop(stop, name)
        .expect("a fresh stop takes a name");
    sim.set_train_stop_limit(stop, train_limit)
        .expect("a fresh stop takes a train limit");
    // The mark a stop puts on the track is derived with the rail graph, so it
    // exists from the next rebuild on — which the placement above has already
    // asked for.
    sim.ensure_rail_graph();
    assert_eq!(
        sim.train_stop_target(stop),
        Some(RailTarget::new(rail, rail_middle(sim, rail))),
        "a stop beside a rail marks the middle of it"
    );
    stop
}

/// One piece of track laid past the end of the run and joined to nothing.
///
/// Three empty rows of gap: enough that neither end meets the run's, so the two
/// are separate railways, and enough that a stop beside this piece is well out
/// of binding reach of the run's last rail.
fn disconnected_rail(sim: &mut Simulation, rails: &[EntityId]) -> EntityId {
    let prototype_id =
        factory_data::entity_prototype_id_by_name(&sim.world.prototypes, "rail_straight");
    let (x, y) = sim
        .entities
        .placed_entity(*rails.last().expect("the run has rails"))
        .map(|placed| (placed.x, placed.y))
        .expect("the run's rails are placed");
    let rail = crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id,
            x,
            y: y + 5,
            direction: Direction::North,
        },
    )
    .expect("the column the run was laid in is clear past the end of it");
    sim.ensure_rail_graph();
    assert_ne!(
        sim.rail_network_id_for_entity(rail),
        sim.rail_network_id_for_entity(rails[0]),
        "the gap is wide enough that the two are separate railways"
    );
    rail
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

fn run_until_waiting(sim: &mut Simulation, train_id: TrainId, stop_id: EntityId) {
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
fn removing_the_last_stop_of_a_name_drops_the_entries_naming_it() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let north = stop_at(&mut sim, "North", rails[20], 1);
    stop_at(&mut sim, "South", rails[3], 1);
    sim.set_train_schedule(
        train_id,
        schedule(vec![entry("North", &[FOREVER]), entry("South", &[FOREVER])]),
    )
    .expect("the train takes a schedule");
    run_until_waiting(&mut sim, train_id, north);

    crate::entity_mutation::remove(&mut sim, north).expect("the stop exists");

    let stranded = train(&sim, train_id);
    assert_eq!(stranded.scheduled_stop, None);
    assert_eq!(
        stranded
            .schedule
            .entries
            .iter()
            .map(|entry| entry.stop_name.as_str())
            .collect::<Vec<_>>(),
        ["South"],
        "the entry naming a station that no longer exists is dropped"
    );
    assert_eq!(
        stranded.schedule.current, 0,
        "the cursor moves onto the entry that survives it"
    );
    run_until(&mut sim, |sim| {
        position(sim, train_id) == RailPosition::new(rails[3], rail_middle(sim, rails[3]), true)
    });
}

/// The schedule is a loop, so an entry naming a vanished station is a dead end
/// whether the train is on it now or comes to it a lap later. Stepping past only
/// the entry being served would leave the one behind it to strand the train on
/// the next time round — a fault that shows up long after the stop was removed.
#[test]
fn removing_a_stop_served_later_in_the_loop_drops_its_entry_too() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let south = stop_at(&mut sim, "South", rails[3], 1);
    let north = stop_at(&mut sim, "North", rails[20], 1);
    sim.set_train_schedule(
        train_id,
        schedule(vec![entry("South", &[FOREVER]), entry("North", &[FOREVER])]),
    )
    .expect("the train takes a schedule");
    run_until_waiting(&mut sim, train_id, south);

    // North is removed while the train is serving South, so the entry naming it
    // is the one *after* the cursor rather than the one under it.
    crate::entity_mutation::remove(&mut sim, north).expect("the stop exists");

    let waiting = train(&sim, train_id);
    assert_eq!(
        waiting
            .schedule
            .entries
            .iter()
            .map(|entry| entry.stop_name.as_str())
            .collect::<Vec<_>>(),
        ["South"],
        "the entry naming the removed station goes with it, wherever the cursor is"
    );
    assert_eq!(
        waiting.schedule.current, 0,
        "the entry being served is untouched and still the current one"
    );
    assert_eq!(
        waiting.scheduled_stop,
        Some(south),
        "the train keeps the platform it is standing at"
    );
    sim.validate()
        .expect("a world one stop lighter is still a valid world");
}

/// While another stop still answers to the name, removing one is not the end of
/// the entry: the train simply goes to the platform that is left.
#[test]
fn removing_one_of_two_stops_sharing_a_name_keeps_the_schedule_on_it() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let far = stop_at(&mut sim, "North", rails[20], 1);
    let near = stop_at(&mut sim, "North", rails[16], 1);
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    run_until_waiting(&mut sim, train_id, near);

    crate::entity_mutation::remove(&mut sim, near).expect("the stop exists");

    assert_eq!(
        train(&sim, train_id).schedule.current,
        0,
        "the name is still served, so the entry still is"
    );
    run_until_waiting(&mut sim, train_id, far);
    assert_eq!(
        position(&sim, train_id),
        RailPosition::new(rails[20], rail_middle(&sim, rails[20]), true)
    );
}

/// Two platforms of one station, and the train takes the one it can *get to*
/// most cheaply rather than the first in id order. The far platform is placed
/// first here for exactly that reason: an id-order pick would send the train
/// past the near one and on up the line.
#[test]
fn a_train_takes_the_platform_that_is_cheapest_to_reach() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let far = stop_at(&mut sim, "North", rails[22], 1);
    let near = stop_at(&mut sim, "North", rails[12], 1);
    assert!(far < near, "the far platform is the lower stop id");
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");

    run_until_waiting(&mut sim, train_id, near);

    assert_eq!(
        position(&sim, train_id),
        RailPosition::new(rails[12], rail_middle(&sim, rails[12]), true),
        "the nearer platform is the one it stopped at"
    );
}

/// A platform already booked to capacity is not a candidate at all, so a second
/// train takes the other one — even though it is the dearer of the two.
#[test]
fn a_full_platform_sends_the_next_train_to_the_other_one() {
    let (mut sim, rails) = world_with_rail_run(24);
    let first_stock = place_stock(&mut sim, &rails, 4, "locomotive").expect("the first fits");
    let second_stock = place_stock(&mut sim, &rails, 12, "locomotive").expect("the second fits");
    let first = sim.rolling_stock_piece(first_stock).expect("placed").train;
    let second = sim.rolling_stock_piece(second_stock).expect("placed").train;
    assert_ne!(first, second, "the two locomotives are two trains");
    fuel_train(&mut sim, first, 50);
    fuel_train(&mut sim, second, 50);
    let far = stop_at(&mut sim, "North", rails[22], 1);
    let near = stop_at(&mut sim, "North", rails[16], 1);
    for train_id in [first, second] {
        sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
            .expect("the train takes a schedule");
    }

    sim.tick();

    assert_eq!(
        train(&sim, first).scheduled_stop,
        Some(near),
        "the lower train id books first, and books the platform it can reach cheapest"
    );
    assert_eq!(
        train(&sim, second).scheduled_stop,
        Some(far),
        "the second train takes the platform the first one left"
    );
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

/// Renaming the platform a train has booked is not a change to that train's
/// orders: its entry still names the old station, and running on to the renamed
/// platform would have it load at a station its schedule no longer asks for.
/// The place goes back, and the train takes one that still answers.
#[test]
fn renaming_a_platform_a_train_booked_sends_it_to_one_that_still_answers() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let far = stop_at(&mut sim, "North", rails[20], 1);
    let near = stop_at(&mut sim, "North", rails[16], 1);
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    run_until_waiting(&mut sim, train_id, near);

    sim.rename_train_stop(near, "Depot")
        .expect("the stop takes a name");

    let renamed = train(&sim, train_id);
    assert_eq!(
        renamed.scheduled_stop, None,
        "the platform it booked no longer answers to the entry it is serving"
    );
    assert_eq!(renamed.schedule_arrival_tick, None, "so it is not waiting");
    assert_eq!(
        renamed.schedule.entries[0].stop_name, "North",
        "the other platform still bears the name, so the schedule is untouched"
    );
    run_until_waiting(&mut sim, train_id, far);
    sim.validate()
        .expect("a train sent to the other platform is a valid world");
}

/// The mirror of it: renaming the *last* platform of a station renames the
/// station, so the schedules follow it and the train already on its way keeps
/// the place it booked rather than being sent round again for no reason.
#[test]
fn renaming_the_last_platform_of_a_station_keeps_the_train_that_booked_it() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let north = stop_at(&mut sim, "North", rails[20], 1);
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    run_until_waiting(&mut sim, train_id, north);

    sim.rename_train_stop(north, "Depot")
        .expect("the stop takes a name");

    let renamed = train(&sim, train_id);
    assert_eq!(
        renamed.scheduled_stop,
        Some(north),
        "the station was renamed, not closed"
    );
    assert!(renamed.is_waiting_at_scheduled_stop());
    assert_eq!(renamed.schedule.entries[0].stop_name, "Depot");
}

/// A platform on a railway the train is not on is not a platform it can be
/// booked into, however few of them there are and whatever the tick's search
/// budget has left.
///
/// Booking it and finding out afterwards is the trap: the routing pass answers
/// "no way there" by stepping the schedule past the entry, so a train would skip
/// a station rather than wait for one it can reach. It waits instead — the same
/// thing it does for a station with no track beside it at all.
#[test]
fn a_platform_on_another_railway_is_not_booked() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let siding = disconnected_rail(&mut sim, &rails);
    let stranded = stop_at(&mut sim, "North", siding, 1);
    sim.set_train_schedule(
        train_id,
        schedule(vec![entry("North", &[FOREVER]), entry("South", &[FOREVER])]),
    )
    .expect("the train takes a schedule");

    for _ in 0..10 {
        sim.tick();
    }

    let waiting = train(&sim, train_id);
    assert_eq!(
        waiting.scheduled_stop, None,
        "the only platform of that name is on track this train cannot reach"
    );
    assert_eq!(waiting.destination, None);
    assert_eq!(
        waiting.schedule.current, 0,
        "and the entry naming it is not stepped past, which would skip the station"
    );
    sim.validate()
        .expect("a train waiting for a platform it can reach is a valid world");

    // Lay a platform of the same name on the train's own railway and it goes.
    let reachable = stop_at(&mut sim, "North", rails[20], 1);
    assert!(
        stranded < reachable,
        "the unreachable platform is the lower stop id, so it is the one a \
         fallback would have taken"
    );
    run_until_waiting(&mut sim, train_id, reachable);
}

/// A platform that moves takes the train booked into it with it.
///
/// The mark is derived from the track beside the stop, so it can move while the
/// stop never does — here by pulling up the rail it marked and leaving it the
/// one beside that. A train aimed at the old mark would run out its old route
/// and call that an arrival on track the station no longer marks, so the claim
/// goes back and the train is aimed again.
#[test]
fn a_stop_that_binds_to_other_track_aims_its_train_again() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let north = stop_at(&mut sim, "North", rails[20], 1);
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    // One tick to book the platform and be given a route to it. The train is
    // still a third of the way down the run, so the rail that goes is track it
    // is heading for rather than track it is standing on.
    sim.tick();
    assert_eq!(train(&sim, train_id).scheduled_stop, Some(north));
    let marked = sim.train_stop_target(north).expect("the stop marks a rail");

    crate::entity_mutation::remove(&mut sim, rails[20]);
    sim.tick();

    let moved = sim
        .train_stop_target(north)
        .expect("the rail beside the mined one is still within reach");
    assert_ne!(moved, marked, "the platform is on other track now");
    // Given back and taken again within the tick, because the schedule pass
    // runs straight after the rebuild that moved the mark: what is worth
    // asserting is not that the claim blinked but that the train is aimed at
    // where the platform is *now*.
    let aimed = train(&sim, train_id);
    assert_eq!(aimed.scheduled_stop, Some(north));
    assert_eq!(
        aimed.destination,
        Some(moved),
        "the train is sent to the mark that moved, not the one it was given"
    );

    // And it goes on to serve the station rather than idling at the mark it was
    // given. It comes to rest short of the new mark rather than on it, because
    // pulling up that rail put the end of the line within half a train of it —
    // the limitation `advance_train_route` names — so what is asserted is that
    // it arrives at all.
    run_until_waiting(&mut sim, train_id, north);
    sim.validate()
        .expect("a train at a platform that moved is a valid world");
}

/// A stop's mark on the track is derived from the rail beside it, so mining
/// that rail leaves the stop standing with nothing to serve — rather than
/// leaving it pointing at track that is not there, which is the state a durable
/// mark would have to be pruned out of.
///
/// The train booked into it has to be let go of, or it would hold a place at a
/// platform it can never reach against every train that could.
#[test]
fn a_stop_whose_rail_is_mined_keeps_its_name_and_loses_its_platform() {
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
    // tested is the platform going rather than the train being stranded on the
    // track that went with it.
    sim.tick();
    assert_eq!(train(&sim, train_id).scheduled_stop, Some(north));

    // The whole of the track within reach of the stop, because the rule is
    // "the nearest rail": pulling up only the one it marks would hand it the
    // one beside that, which is a station shifting up the line rather than a
    // station with no platform.
    for rail in [rails[19], rails[20], rails[21]] {
        crate::entity_mutation::remove(&mut sim, rail);
    }
    sim.tick();

    assert_eq!(
        sim.train_stops().count(),
        2,
        "the stop is a placed entity and outlives the track beside it"
    );
    assert_eq!(
        sim.train_stop_target(north),
        None,
        "with no rail beside it, it marks nothing"
    );
    let train_now = train(&sim, train_id);
    assert_eq!(
        train_now.scheduled_stop, None,
        "the train gives back a place it can no longer be sent to"
    );
    assert_eq!(
        train_now
            .schedule
            .entries
            .iter()
            .map(|entry| entry.stop_name.as_str())
            .collect::<Vec<_>>(),
        ["North", "South"],
        "the station still exists, so the entry naming it does too"
    );
    sim.validate()
        .expect("a world whose track was pulled up is still a valid world");
    // The entry it cannot serve is not a dead end either: with nothing to
    // claim, the cursor stays where it is and the train comes quietly to a
    // stand, waiting for a platform to answer to the name again.
    run_until(&mut sim, |sim| {
        sim.train(train_id)
            .is_some_and(|train| train.destination.is_none() && train.is_stationary())
    });
    assert_eq!(train(&sim, train_id).schedule.current, 0);
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

/// A wildcard channel stands for an iteration over a whole network, which is
/// not something one train can wait on: a train let go by "anything above zero"
/// could not say what let it go. Refused where the schedule is set, rather than
/// evaluated to something arbitrary at the station.
#[test]
fn a_wait_on_a_wildcard_channel_is_refused() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    stop_at(&mut sim, "North", rails[20], 1);
    let each = virtual_signal(&sim, "signal_each");

    assert_eq!(
        sim.set_train_schedule(
            train_id,
            schedule(vec![entry(
                "North",
                &[TrainWaitCondition::Circuit(
                    crate::circuits::CircuitCondition {
                        left: each,
                        comparator: crate::circuits::Comparator::Greater,
                        right: crate::circuits::SignalOperand::Constant(0),
                    }
                )]
            )]),
        ),
        Err(crate::rolling_stock::TrainControlError::WildcardSignal(
            each
        ))
    );
    assert!(train(&sim, train_id).schedule.entries.is_empty());
}

/// The condition the connector on a stop exists for: a train held at a platform
/// until the factory behind it says otherwise.
///
/// Both halves are checked, because only one of them is obvious. A stop nothing
/// is wired to reads every channel as zero, so "green > 0" holds the train there
/// indefinitely; wiring a source that says otherwise is what releases it.
#[test]
fn a_circuit_condition_holds_a_train_until_the_stop_s_network_says_go() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let north = stop_at(&mut sim, "North", rails[20], 1);
    let green = virtual_signal(&sim, "signal_green");
    sim.set_train_schedule(
        train_id,
        schedule(vec![
            entry(
                "North",
                &[TrainWaitCondition::Circuit(
                    crate::circuits::CircuitCondition {
                        left: green,
                        comparator: crate::circuits::Comparator::Greater,
                        right: crate::circuits::SignalOperand::Constant(0),
                    },
                )],
            ),
            entry("South", &[FOREVER]),
        ]),
    )
    .expect("the train takes a schedule");
    run_until_waiting(&mut sim, train_id, north);

    for _ in 0..60 {
        sim.tick();
    }
    assert_eq!(
        train(&sim, train_id).schedule.current,
        0,
        "an unwired stop publishes nothing, and nothing is not above zero"
    );

    wire_constant_source(&mut sim, north, green, 1);
    sim.tick();
    sim.tick();

    assert_eq!(
        train(&sim, train_id).schedule.current,
        1,
        "the network said go, so the train left"
    );
}

/// A stop reports what the train standing at it carries. This is the one way a
/// wagon's contents reach a circuit at all — a wagon is not a placed entity, so
/// no wire can be attached to one.
#[test]
fn a_stop_publishes_what_the_train_standing_at_it_carries() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let wagon = place_stock(&mut sim, &rails, 6, "cargo_wagon").expect("the wagon fits");
    let north = stop_at(&mut sim, "North", rails[20], 1);
    let coal = factory_data::item_id_by_name(&sim.world.prototypes, "coal");
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    // Wired to a combinator so the stop has a network to publish onto, and
    // reading turned on so it publishes at all.
    let red = virtual_signal(&sim, "signal_red");
    wire_constant_source(&mut sim, north, red, 1);
    sim.set_circuit_read_contents(north, true)
        .expect("a stop reads contents");
    assert_eq!(
        sim.rolling_stock_piece(wagon).expect("placed").train,
        train_id,
        "the wagon coupled onto the locomotive"
    );
    put_one_item_in_every_slot(&mut sim, wagon);

    run_until_waiting(&mut sim, train_id, north);
    sim.tick();

    let carried = sim
        .circuit_signals_at_entity(north)
        .value(crate::circuits::SignalId::Item(coal));
    assert_eq!(
        carried,
        i32::try_from(
            sim.rolling_stock_piece(wagon)
                .expect("the wagon is placed")
                .inventory
                .as_ref()
                .expect("a cargo wagon declares an inventory")
                .slots()
                .len()
        )
        .expect("a wagon has a small number of slots"),
        "one coal in every slot of the wagon standing here"
    );
}

/// Tanks are read the same way wagons are, and in the same whole units the
/// fluid UI and every other tank's connector report — a tank holding a fraction
/// of a unit reads as nothing rather than as a thousand of something.
#[test]
fn a_stop_publishes_the_fluid_the_tanks_standing_at_it_hold() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    // A fluid wagon sits behind a technology of its own that a locomotive does
    // not need, and placement goes through the ordinary gate.
    unlock_with_prerequisites(&mut sim, "fluid_wagon");
    let tanker = place_stock(&mut sim, &rails, 6, "fluid_wagon").expect("the tank wagon fits");
    let north = stop_at(&mut sim, "North", rails[20], 1);
    let water = factory_data::BasePrototypeIds::from_catalog(&sim.world.prototypes)
        .fluids
        .water;
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    let red = virtual_signal(&sim, "signal_red");
    wire_constant_source(&mut sim, north, red, 1);
    sim.set_circuit_read_contents(north, true)
        .expect("a stop reads contents");
    assert_eq!(
        sim.rolling_stock_piece(tanker).expect("placed").train,
        train_id,
        "the tank wagon coupled onto the locomotive"
    );
    // Not a round number of units, so the reading has to be the truncated
    // quotient rather than anything that happens to agree with it.
    fill_tank(&mut sim, tanker, water, 12_345_678);

    run_until_waiting(&mut sim, train_id, north);
    sim.tick();

    assert_eq!(
        sim.circuit_signals_at_entity(north)
            .value(crate::circuits::SignalId::Fluid(water)),
        12_345,
        "the tank standing here holds twelve thousand three hundred and forty five whole units"
    );
}

/// A train that has booked the platform is not standing on it. Publishing on the
/// strength of the claim alone would have a station reporting goods that are
/// still out on the main line, and a train rolling through a station it does not
/// serve would flicker onto the network as it passed.
#[test]
fn a_stop_publishes_nothing_for_a_train_that_has_not_arrived() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let wagon = place_stock(&mut sim, &rails, 6, "cargo_wagon").expect("the wagon fits");
    let north = stop_at(&mut sim, "North", rails[20], 1);
    let coal = factory_data::item_id_by_name(&sim.world.prototypes, "coal");
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    let red = virtual_signal(&sim, "signal_red");
    wire_constant_source(&mut sim, north, red, 1);
    sim.set_circuit_read_contents(north, true)
        .expect("a stop reads contents");
    put_one_item_in_every_slot(&mut sim, wagon);

    // Out on the run with the claim in hand: the stop is spoken for and the
    // cargo is aboard, but the train is not there yet.
    run_until(&mut sim, |sim| {
        sim.train(train_id)
            .is_some_and(|train| train.scheduled_stop == Some(north) && train.velocity > 0)
    });
    assert!(
        !train(&sim, train_id).is_waiting_at_scheduled_stop(),
        "the train is still on its way"
    );
    assert_eq!(
        sim.circuit_signals_at_entity(north)
            .value(crate::circuits::SignalId::Item(coal)),
        0,
        "a claim is not an arrival, and an empty platform carries no cargo"
    );

    // And once it does arrive, the same network says so.
    run_until_waiting(&mut sim, train_id, north);
    sim.tick();
    assert!(
        sim.circuit_signals_at_entity(north)
            .value(crate::circuits::SignalId::Item(coal))
            > 0,
        "the train that arrived publishes what it carries"
    );
}

/// What a stop reports is what is standing there now. A train that has left
/// takes its cargo off the network with it, rather than leaving the last
/// reading behind for a factory to act on.
#[test]
fn a_departed_train_takes_its_cargo_off_the_stop_s_network() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let wagon = place_stock(&mut sim, &rails, 6, "cargo_wagon").expect("the wagon fits");
    let north = stop_at(&mut sim, "North", rails[20], 1);
    stop_at(&mut sim, "South", rails[3], 1);
    let coal = factory_data::item_id_by_name(&sim.world.prototypes, "coal");
    sim.set_train_schedule(
        train_id,
        schedule(vec![
            entry("North", &[TrainWaitCondition::TimePassed { ticks: 120 }]),
            entry("South", &[FOREVER]),
        ]),
    )
    .expect("the train takes a schedule");
    let red = virtual_signal(&sim, "signal_red");
    wire_constant_source(&mut sim, north, red, 1);
    sim.set_circuit_read_contents(north, true)
        .expect("a stop reads contents");
    put_one_item_in_every_slot(&mut sim, wagon);

    run_until_waiting(&mut sim, train_id, north);
    sim.tick();
    assert!(
        sim.circuit_signals_at_entity(north)
            .value(crate::circuits::SignalId::Item(coal))
            > 0,
        "the train is standing at the stop with a loaded wagon"
    );

    // The wait runs out and the train leaves for the other station.
    run_until(&mut sim, |sim| {
        sim.train(train_id)
            .is_some_and(|train| !train.is_waiting_at_scheduled_stop())
    });
    sim.tick();

    assert_eq!(
        sim.circuit_signals_at_entity(north)
            .value(crate::circuits::SignalId::Item(coal)),
        0,
        "an empty platform reads as an empty signal set, not as the last train's cargo"
    );
    assert_eq!(
        sim.circuit_signals_at_entity(north)
            .value(crate::circuits::SignalId::Virtual(match red {
                crate::circuits::SignalId::Virtual(id) => id,
                _ => unreachable!("the fixture wires a virtual signal"),
            })),
        1,
        "the wire itself is untouched: only the cargo went away"
    );
}

/// The reading is opt-in the way every other connector's is. A stop wired for
/// control alone publishes nothing, and turning the toggle on is what puts the
/// cargo onto the network.
#[test]
fn a_stop_not_asked_to_read_contents_publishes_nothing() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let wagon = place_stock(&mut sim, &rails, 6, "cargo_wagon").expect("the wagon fits");
    let north = stop_at(&mut sim, "North", rails[20], 1);
    let coal = factory_data::item_id_by_name(&sim.world.prototypes, "coal");
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    let red = virtual_signal(&sim, "signal_red");
    wire_constant_source(&mut sim, north, red, 1);
    put_one_item_in_every_slot(&mut sim, wagon);

    run_until_waiting(&mut sim, train_id, north);
    sim.tick();
    assert_eq!(
        sim.circuit_signals_at_entity(north)
            .value(crate::circuits::SignalId::Item(coal)),
        0,
        "a stop nobody asked for a reading from keeps the cargo to itself"
    );

    sim.set_circuit_read_contents(north, true)
        .expect("a stop reads contents");
    sim.tick();

    assert!(
        sim.circuit_signals_at_entity(north)
            .value(crate::circuits::SignalId::Item(coal))
            > 0,
        "asked for the reading, the same standing train publishes it"
    );
}

/// Everything a player configured on a stop is durable, and so is the reading it
/// takes off the train standing at it. The networks themselves are runtime state
/// rebuilt from the wires, so the check that matters is that the loaded world
/// publishes the same cargo onto them without being told what was there before.
#[test]
fn a_stop_and_what_it_publishes_survive_a_save_and_load() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let wagon = place_stock(&mut sim, &rails, 6, "cargo_wagon").expect("the wagon fits");
    let north = stop_at(&mut sim, "North", rails[20], 2);
    let coal = factory_data::item_id_by_name(&sim.world.prototypes, "coal");
    let red = virtual_signal(&sim, "signal_red");
    wire_constant_source(&mut sim, north, red, 1);
    sim.set_circuit_read_contents(north, true)
        .expect("a stop reads contents");
    sim.set_train_stop_limit_signal(north, Some(red))
        .expect("a stop takes a limit channel");
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    put_one_item_in_every_slot(&mut sim, wagon);
    run_until_waiting(&mut sim, train_id, north);
    sim.tick();
    let published = sim
        .circuit_signals_at_entity(north)
        .value(crate::circuits::SignalId::Item(coal));
    assert!(published > 0, "the standing train is being read");

    let before = sim.state_hash();
    let bytes = crate::save_to_bytes(&sim).expect("a world with a wired stop saves");
    let mut loaded = crate::load_from_bytes(&bytes).expect("a world with a wired stop loads");

    assert_eq!(
        before,
        loaded.state_hash(),
        "a stop's name, limit, and limit channel are part of what the world is"
    );
    let stop = loaded.train_stop(north).expect("the stop came back");
    assert_eq!(stop.name, "North");
    assert_eq!(stop.train_limit, 2);
    assert_eq!(stop.train_limit_signal, Some(red));
    assert!(
        loaded
            .circuit_entity_state(north)
            .is_some_and(|state| state.read_contents),
        "the toggle came back with it"
    );
    assert_eq!(
        loaded
            .circuit_signals_at_entity(north)
            .value(crate::circuits::SignalId::Item(coal)),
        published,
        "the reloaded stop reads the train still standing at it"
    );

    loaded.tick();
    sim.tick();
    assert_eq!(
        sim.state_hash(),
        loaded.state_hash(),
        "the two worlds go on being the same world"
    );
    loaded
        .validate()
        .expect("a reloaded wired stop is a valid world");
}

/// A limit read off a network is how a station closes itself: a yard that is
/// full stops taking trains rather than queueing them at its throat.
#[test]
fn a_signal_driven_train_limit_closes_a_station() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let north = stop_at(&mut sim, "North", rails[20], 1);
    let red = virtual_signal(&sim, "signal_red");
    let source = wire_constant_source(&mut sim, north, red, 0);
    sim.set_train_stop_limit_signal(north, Some(red))
        .expect("a stop takes a limit channel");
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");

    for _ in 0..10 {
        sim.tick();
    }
    assert_eq!(
        train(&sim, train_id).scheduled_stop,
        None,
        "a station saying nought admits nobody, whatever its hand-set limit says"
    );

    sim.set_constant_combinator_slot(
        source,
        0,
        crate::circuits::ConstantSignalSlot {
            signal: Some(red),
            value: 1,
        },
    )
    .expect("the source takes a new value");

    run_until_waiting(&mut sim, train_id, north);
    sim.validate()
        .expect("a train at a signal-limited station is a valid world");
}

/// A stop's work is taking trains, so the enable condition every controllable
/// connector offers has to switch that off. A condition the network does not
/// satisfy is a station closed, and a connector that accepted one and then
/// ignored it would be a control that looks wired and does nothing.
#[test]
fn an_enable_condition_the_network_fails_closes_a_stop() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let north = stop_at(&mut sim, "North", rails[20], 1);
    let red = virtual_signal(&sim, "signal_red");
    wire_constant_source(&mut sim, north, red, 0);
    sim.set_circuit_condition(
        north,
        Some(crate::circuits::CircuitCondition {
            left: red,
            comparator: crate::circuits::Comparator::Greater,
            right: crate::circuits::SignalOperand::Constant(0),
        }),
    )
    .expect("a stop takes an enable condition");
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");

    for _ in 0..10 {
        sim.tick();
    }

    assert_eq!(sim.train_stop_effective_limit(north), 0);
    assert_eq!(
        train(&sim, train_id).scheduled_stop,
        None,
        "a stop switched off by its condition admits nobody"
    );
}

fn virtual_signal(sim: &Simulation, name: &str) -> crate::circuits::SignalId {
    crate::circuits::SignalId::Virtual(
        sim.world
            .prototypes
            .virtual_signals
            .iter()
            .find(|signal| signal.name == name)
            .unwrap_or_else(|| panic!("the catalog defines virtual signal {name}"))
            .id,
    )
}

/// A constant combinator publishing `value` on `signal`, wired to `stop`.
///
/// Placed on the tile beyond the stop's own, which is well inside a connector's
/// wire reach; whichever side of the track the stop ended up on, the tile
/// further out from the rails is clear ground the fixture already found room in.
fn wire_constant_source(
    sim: &mut Simulation,
    stop: EntityId,
    signal: crate::circuits::SignalId,
    value: i32,
) -> EntityId {
    let catalog = sim.world.prototypes.clone();
    let wire = factory_data::item_id_by_name(&catalog, "red_wire");
    sim.player_inventory
        .insert(&catalog, wire, 10)
        .expect("the player inventory takes wire");
    let (stop_x, stop_y) = sim
        .entities
        .placed_entity(stop)
        .map(|placed| (placed.x, placed.y))
        .expect("the stop is placed");
    let prototype_id = factory_data::entity_prototype_id_by_name(&catalog, "constant_combinator");
    let combinator = [(0, 1), (0, -1), (1, 0), (-1, 0), (1, 1), (-1, -1)]
        .into_iter()
        .find_map(|(dx, dy)| {
            crate::placement::place(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id,
                    x: stop_x + dx,
                    y: stop_y + dy,
                    direction: Direction::North,
                },
            )
            .ok()
        })
        .expect("one of the tiles around the stop takes a combinator");
    sim.set_constant_combinator_slot(
        combinator,
        0,
        crate::circuits::ConstantSignalSlot {
            signal: Some(signal),
            value,
        },
    )
    .expect("the combinator takes a slot");
    sim.connect_circuit_wire(
        crate::circuits::CircuitNode::new(combinator, crate::circuits::ConnectorPort::Output),
        crate::circuits::CircuitNode::new(stop, crate::circuits::ConnectorPort::Single),
        crate::circuits::WireColor::Red,
    )
    .expect("a combinator beside a stop is within reach of it");
    combinator
}

/// Puts a single item in every slot of a wagon: occupied everywhere, full
/// nowhere.
fn put_one_item_in_every_slot(
    sim: &mut Simulation,
    stock_id: crate::rolling_stock::RollingStockId,
) {
    set_every_slot(sim, stock_id, 1);
}

/// Puts `milliunits` of `fluid` into a tank wagon's one tank.
///
/// Set on the stock directly, the way the slot helpers above load a cargo
/// wagon: what these tests are about is the reading, and pumping the fluid in
/// through a network would only add a fixture that `a_pump_fills_a_stopped_
/// fluid_wagon` already covers.
fn fill_tank(
    sim: &mut Simulation,
    stock_id: crate::rolling_stock::RollingStockId,
    fluid: factory_data::FluidId,
    milliunits: u64,
) {
    let tank = sim
        .rolling_stock
        .stock
        .get_mut(&stock_id)
        .expect("the wagon is placed")
        .fluid_boxes
        .first_mut()
        .expect("a fluid wagon declares a tank");
    tank.fluid_id = Some(fluid);
    tank.amount_milliunits = milliunits;
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

/// The whole of manual control: a train under the player's hand keeps the
/// throttle it was given.
///
/// Without the flag this is exactly what fails. Taking the controls releases the
/// claim, the destination and the route — which leaves the train looking like an
/// idle one to the scheduling pass, so it books the next stop on the same tick
/// and the route it plans drives the throttle straight back over the one that
/// was just asked for.
#[test]
fn a_train_driven_by_hand_keeps_the_controls_it_was_given() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    stop_at(&mut sim, "North", rails[20], 1);
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");

    sim.set_train_throttle(train_id, TrainThrottle::Forward)
        .expect("the player takes the controls");
    for _ in 0..10 {
        sim.tick();
        let train = train(&sim, train_id);
        assert!(train.manual, "the train stayed under the player's hand");
        assert_eq!(
            train.throttle,
            TrainThrottle::Forward,
            "the schedule drove over the throttle the player asked for"
        );
        assert_eq!(
            train.scheduled_stop, None,
            "a hand-driven train booked a platform it is not going to"
        );
    }
    sim.validate()
        .expect("a hand-driven train is a valid world");
}

/// And the way back: the schedule picks the train up again from wherever it was
/// left, rather than from where it was when the player took over.
#[test]
fn handing_a_train_back_lets_its_schedule_have_it_again() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    let north = stop_at(&mut sim, "North", rails[20], 1);
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    sim.set_train_throttle(train_id, TrainThrottle::Brake)
        .expect("the player takes the controls");
    sim.tick();
    assert_eq!(train(&sim, train_id).scheduled_stop, None);

    sim.set_train_manual(train_id, false)
        .expect("the player hands it back");

    run_until_waiting(&mut sim, train_id, north);
    assert!(
        !train(&sim, train_id).manual,
        "it is running its schedule again"
    );
    sim.validate().expect("the world is valid");
}

/// Handing a moving train back to a schedule that cannot place it anywhere.
///
/// The claim fails — every platform is full — so the train gets no destination
/// and no route, and nothing was left to steer it. Without the rule that a
/// train with no leg brakes, it would keep accelerating on the throttle its
/// driver walked away from, under nobody's control at all.
#[test]
fn a_train_handed_back_with_nowhere_to_go_does_not_keep_driving() {
    let (mut sim, rails, train_id) = world_with_a_schedulable_train();
    // The only platform of that name is on track this train cannot reach, so
    // the claim never succeeds however long it waits.
    let siding = disconnected_rail(&mut sim, &rails);
    stop_at(&mut sim, "North", siding, 1);
    sim.set_train_schedule(train_id, schedule(vec![entry("North", &[FOREVER])]))
        .expect("the train takes a schedule");
    sim.set_train_throttle(train_id, TrainThrottle::Forward)
        .expect("the player takes the controls");
    run_until(&mut sim, |sim| train(sim, train_id).velocity != 0);

    sim.set_train_manual(train_id, false)
        .expect("the player hands it back");
    sim.tick();

    let train = train(&sim, train_id);
    assert_eq!(
        train.scheduled_stop, None,
        "this test is only meaningful while the station cannot take the train"
    );
    assert_eq!(
        train.throttle,
        TrainThrottle::Brake,
        "an automatic train with no plan drove on under its old throttle"
    );
    sim.validate().expect("the world is valid");
}
