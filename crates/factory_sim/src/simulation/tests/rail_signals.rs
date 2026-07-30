//! Signals in a world: the blocks they cut, the claims trains take on them, and
//! the nasty cases a signalled railway exists to survive.
//!
//! The partition and the lookahead walk are tested against hand-built graphs
//! beside the code that builds them. What these tests are about is real track,
//! real signals placed against it through the ordinary placement path, and real
//! trains stopping where they should.

use super::super::*;
use super::rolling_stock::{fuel_train, place_stock};
use super::support::*;
use crate::rail::RailSignalAspect;
use crate::rolling_stock::{RollingStockId, TrainId};

/// Long enough for a locomotive to run any of these fixtures twice over, which
/// is the point at which a train that has not got there is not going to.
const SETTLE_TICKS: usize = 2_000;

/// A run of two-tile straights with room beside it for signals, and the tiles a
/// signal can be dropped on.
///
/// The origin is chosen so that every rail *and* every signal tile validates
/// there, because a signal is refused unless a rail joint is already beside it
/// and the origin search cannot ask that question before the track exists. A
/// one-tile stand-in is validated on the signal tiles instead, which is the same
/// question about terrain and occupancy.
fn world_with_signalled_run(piece_count: usize) -> (Simulation, Vec<EntityId>) {
    let mut sim = Simulation::new_test_world(123);
    let straight = entity_id_by_name(&sim.world.prototypes, "rail_straight");
    let stand_in = entity_id_by_name(&sim.world.prototypes, "chest");

    let placeable = |sim: &Simulation, prototype_id, x, y| {
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
    // Clear ground two columns either side of the run as well as the run itself,
    // so a signal can go on either side of any joint and a test can relay the
    // track around one.
    let (origin_x, origin_y) = all_tile_coords(&sim.world)
        .into_iter()
        .find(|(x, y)| {
            (0..piece_count as WorldTileCoord * 2).all(|row| {
                placeable(&sim, straight, *x, *y + row)
                    && (-2..=2)
                        .filter(|column| *column != 0)
                        .all(|column| placeable(&sim, stand_in, *x + column, *y + row))
            })
        })
        .expect("the test world holds a signalled run somewhere");

    let rails = (0..piece_count as WorldTileCoord)
        .map(|index| {
            place_at(
                &mut sim,
                straight,
                origin_x,
                origin_y + index * 2,
                Direction::North,
            )
        })
        .collect();
    sim.tick();
    (sim, rails)
}

/// Puts a signal beside the joint at the near end of `rails[rail_index]`,
/// governing travel `direction`.
///
/// A northbound signal stands on the east side and a southbound one on the west,
/// which is only a convention this file follows — what decides which way a signal
/// governs is the direction it is placed in, not the side it is on.
fn place_signal(
    sim: &mut Simulation,
    rails: &[EntityId],
    rail_index: usize,
    direction: Direction,
    name: &str,
) -> EntityId {
    let prototype_id = entity_id_by_name(&sim.world.prototypes, name);
    let footprint = sim
        .entities
        .placed_entity(rails[rail_index])
        .expect("the run's rails are placed")
        .footprint;
    let offset_x = if direction == Direction::North { 1 } else { -1 };
    let signal = place_at(
        sim,
        prototype_id,
        footprint.x + offset_x,
        footprint.y,
        direction,
    );
    sim.tick();
    signal
}

/// A locomotive on `rail_index` of the run, fuelled and ready to drive.
fn place_locomotive(
    sim: &mut Simulation,
    rails: &[EntityId],
    rail_index: usize,
) -> (RollingStockId, TrainId) {
    let stock_id =
        place_stock(sim, rails, rail_index, "locomotive").expect("a locomotive fits on the run");
    let train_id = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive was just placed")
        .train;
    fuel_train(sim, train_id, 50);
    (stock_id, train_id)
}

fn block_of(sim: &Simulation, rail: EntityId) -> EntityId {
    sim.rail_block_key(rail).expect("a placed rail has a block")
}

fn reserved(sim: &Simulation, train_id: TrainId) -> Vec<EntityId> {
    sim.train(train_id)
        .expect("the train exists")
        .reserved_blocks
        .clone()
}

/// Runs until nothing moves any more, so a test can assert about where trains
/// came to rest rather than about a moment part way there.
fn run_until_settled(sim: &mut Simulation) {
    let mut still = 0;
    for _ in 0..SETTLE_TICKS {
        sim.tick();
        if sim.trains().all(|train| train.is_stationary()) {
            still += 1;
            // Several ticks of stillness, because a train that has just reversed
            // passes through a stationary tick on its way to moving the other
            // way.
            if still == 30 {
                return;
            }
        } else {
            still = 0;
        }
    }
}

#[test]
fn a_signal_cuts_the_run_it_stands_beside_into_two_blocks() {
    let (mut sim, rails) = world_with_signalled_run(6);
    assert_eq!(
        sim.rail_blocks().count(),
        1,
        "unsignalled track is one block"
    );

    place_signal(&mut sim, &rails, 3, Direction::North, "rail_signal");

    assert_eq!(sim.rail_blocks().count(), 2);
    assert_eq!(block_of(&sim, rails[0]), block_of(&sim, rails[2]));
    assert_eq!(block_of(&sim, rails[3]), block_of(&sim, rails[5]));
    assert_ne!(block_of(&sim, rails[2]), block_of(&sim, rails[3]));
}

/// The signal knows which stretch it lets a train into, and the block partition
/// is derived rather than saved: pulling the signal up joins the two halves back
/// together.
#[test]
fn removing_a_signal_joins_its_two_blocks_again() {
    let (mut sim, rails) = world_with_signalled_run(6);
    let signal = place_signal(&mut sim, &rails, 3, Direction::North, "rail_signal");
    assert_eq!(
        sim.rail_signal(signal)
            .expect("a placed signal is in the partition")
            .guarded_block,
        Some(block_of(&sim, rails[3]))
    );

    crate::entity_mutation::remove(&mut sim, signal).expect("a placed signal can be removed");
    sim.tick();

    assert_eq!(sim.rail_blocks().count(), 1);
    assert_eq!(sim.rail_signal(signal), None);
}

/// A signal needs a rail joint beside it, aligned with the way it faces. Without
/// that there is no crossing for it to govern — and a signal that governed
/// nothing would still cut the partition, quietly splitting a railway in two.
#[test]
fn a_signal_is_refused_where_no_aligned_joint_is_beside_it() {
    let (sim, rails) = world_with_signalled_run(6);
    let prototype_id = entity_id_by_name(&sim.world.prototypes, "rail_signal");
    let footprint = sim
        .entities
        .placed_entity(rails[3])
        .expect("the run's rails are placed")
        .footprint;
    let request = |x, y, direction| crate::placement::EntityPlacementRequest {
        prototype_id,
        x,
        y,
        direction,
    };

    // Across the run rather than along it: the joint is there, but no rail
    // leaves it running east.
    assert_eq!(
        crate::placement::validate(&sim, request(footprint.x + 1, footprint.y, Direction::East)),
        Err(BuildError::NeedsAlignedRail { prototype_id })
    );
    // Nowhere near the track at all.
    assert_eq!(
        crate::placement::validate(
            &sim,
            request(footprint.x + 4, footprint.y, Direction::North)
        ),
        Err(BuildError::NeedsAlignedRail { prototype_id })
    );
    // Beside the far end of the run, where the joint is a buffer stop: nothing
    // continues past it, so there is no block to be let into.
    let last = sim
        .entities
        .placed_entity(rails[5])
        .expect("the run's rails are placed")
        .footprint;
    assert_eq!(
        crate::placement::validate(&sim, request(last.x + 1, last.y + 2, Direction::North)),
        Err(BuildError::NeedsAlignedRail { prototype_id })
    );
    // And the placement the fixture relies on really is allowed.
    assert!(
        crate::placement::validate(
            &sim,
            request(footprint.x + 1, footprint.y, Direction::North)
        )
        .is_ok()
    );
}

/// Two signals cannot govern one crossing the same way round. Facing opposite
/// ways at the same joint is the ordinary two-way line and is allowed.
#[test]
fn one_crossing_takes_one_signal_each_way() {
    let (mut sim, rails) = world_with_signalled_run(6);
    let northbound = place_signal(&mut sim, &rails, 3, Direction::North, "rail_signal");
    let prototype_id = entity_id_by_name(&sim.world.prototypes, "rail_signal");
    let footprint = sim
        .entities
        .placed_entity(rails[3])
        .expect("the run's rails are placed")
        .footprint;

    // The west side of the same joint, facing the same way: refused, and the
    // error names the signal already there.
    assert_eq!(
        crate::placement::validate(
            &sim,
            crate::placement::EntityPlacementRequest {
                prototype_id,
                x: footprint.x - 1,
                y: footprint.y,
                direction: Direction::North,
            },
        ),
        Err(BuildError::EntityOccupied {
            x: footprint.x,
            y: footprint.y,
            entity_id: northbound,
        })
    );

    // Facing the other way is a second signal over one boundary, which is how a
    // two-way line is signalled.
    place_signal(&mut sim, &rails, 3, Direction::South, "rail_signal");
    assert_eq!(sim.rail_blocks().count(), 2);
}

/// Placement settles whether a signal is aligned with the track under it, but
/// track can be mined and relaid under a signal that never moved. A signal left
/// facing across a joint it no longer runs along must drop out of the partition
/// rather than cut a boundary it governs in neither direction — otherwise
/// building a line past an idle-looking signal silently breaks it.
#[test]
fn a_signal_left_facing_across_relaid_track_governs_nothing_and_cuts_nothing() {
    let (mut sim, rails) = world_with_signalled_run(6);
    let signal = place_signal(&mut sim, &rails, 3, Direction::North, "rail_signal");
    let footprint = sim
        .entities
        .placed_entity(signal)
        .expect("the signal is placed")
        .footprint;
    assert!(
        sim.rail_signal(signal).is_some(),
        "it governs the north-south run it was placed against"
    );

    for rail in &rails {
        crate::entity_mutation::remove(&mut sim, *rail).expect("a placed rail can be removed");
    }
    sim.tick();
    assert_eq!(sim.rail_signal(signal), None, "its track went with it");

    // An east-west line one row over, joined at a point within the signal's
    // binding reach. The signal still faces north; the track there runs east and
    // west, so there is no crossing for it to govern.
    let straight = entity_id_by_name(&sim.world.prototypes, "rail_straight");
    let west = place_at(
        &mut sim,
        straight,
        footprint.x - 2,
        footprint.y + 1,
        Direction::East,
    );
    let east = place_at(
        &mut sim,
        straight,
        footprint.x,
        footprint.y + 1,
        Direction::East,
    );
    sim.tick();

    assert_eq!(
        sim.rail_piece_connections(west)[1],
        Some(east),
        "the fixture really does join the two pieces at the point beside the signal"
    );
    assert_eq!(sim.rail_signal(signal), None);
    assert_eq!(sim.rail_signal_aspect(signal), None);
    assert_eq!(
        sim.rail_blocks().count(),
        1,
        "the relaid line is one block: a signal that governs nothing cuts nothing"
    );
    assert_eq!(block_of(&sim, west), block_of(&sim, east));
    sim.validate()
        .expect("a railway with an orphaned signal beside it is a valid world");
}

/// A train holds the block it stands in and the one ahead, and nothing further:
/// that is the whole reservation rule, and it is what leaves room for a second
/// train two blocks back.
#[test]
fn a_train_holds_the_block_it_is_in_and_the_one_ahead() {
    let (mut sim, rails) = world_with_signalled_run(20);
    place_signal(&mut sim, &rails, 8, Direction::North, "rail_signal");
    place_signal(&mut sim, &rails, 14, Direction::North, "rail_signal");
    let (_, train_id) = place_locomotive(&mut sim, &rails, 3);
    sim.tick();

    // Even with nowhere to be, a train holds where it stands: another train let
    // into that block would drive into it.
    assert_eq!(
        reserved(&sim, train_id),
        vec![block_of(&sim, rails[0])],
        "a train with no orders still holds the block it is standing in"
    );

    sim.set_train_destination(train_id, rails[18])
        .expect("the train takes a destination");
    sim.tick();
    sim.tick();

    let mut expected = vec![block_of(&sim, rails[0]), block_of(&sim, rails[8])];
    expected.sort_unstable();
    assert_eq!(reserved(&sim, train_id), expected);
    assert!(
        !reserved(&sim, train_id).contains(&block_of(&sim, rails[14])),
        "the block beyond the next signal is not held yet"
    );
}

/// A train that has arrived and has nowhere left to go gives back the block it
/// was let into ahead, so a siding a train has finished with is a siding
/// somebody else can be let into.
#[test]
fn an_arrived_train_gives_back_the_block_ahead_of_it() {
    let (mut sim, rails) = world_with_signalled_run(20);
    place_signal(&mut sim, &rails, 8, Direction::North, "rail_signal");
    let (_, train_id) = place_locomotive(&mut sim, &rails, 3);
    sim.set_train_destination(train_id, rails[5])
        .expect("the train takes a destination");
    sim.tick();
    sim.tick();
    assert!(
        reserved(&sim, train_id).contains(&block_of(&sim, rails[8])),
        "on its way it was let into the block ahead"
    );

    run_until_settled(&mut sim);

    assert_eq!(
        reserved(&sim, train_id),
        vec![block_of(&sim, rails[0])],
        "arrived and idle, it holds only where it stands"
    );
    assert_eq!(sim.rail_block_claimant(block_of(&sim, rails[8])), None);
}

/// The aspect a signal shows follows the block beyond it: nothing there is
/// clear, somebody on their way in is reserved, somebody standing in it is
/// blocked. All three, in the order one approach produces them.
#[test]
fn a_signal_shows_the_state_of_the_block_beyond_it() {
    let (mut sim, rails) = world_with_signalled_run(20);
    let signal = place_signal(&mut sim, &rails, 8, Direction::North, "rail_signal");
    assert_eq!(
        sim.rail_signal_aspect(signal),
        Some(RailSignalAspect::Clear)
    );

    let (_, train_id) = place_locomotive(&mut sim, &rails, 3);
    sim.set_train_destination(train_id, rails[16])
        .expect("the train takes a destination");
    sim.tick();
    sim.tick();
    assert_eq!(
        sim.rail_signal_aspect(signal),
        Some(RailSignalAspect::Reserved),
        "the train has been let in but is still on the approach side"
    );

    run_until_settled(&mut sim);
    assert_eq!(
        sim.rail_signal_aspect(signal),
        Some(RailSignalAspect::Blocked),
        "now it is standing in there"
    );
}

/// A signal only governs the way it faces. Travel the other way across the same
/// boundary is governed by nothing at all, which makes the boundary impassable
/// that way — a single signal turns a stretch of track one-way, which is exactly
/// what a signal is for.
#[test]
fn a_boundary_nothing_faces_is_one_a_train_cannot_cross() {
    let (mut sim, rails) = world_with_signalled_run(20);
    place_signal(&mut sim, &rails, 8, Direction::North, "rail_signal");
    let (stock_id, train_id) = place_locomotive(&mut sim, &rails, 14);

    // Sent back down the run, against the way the only signal faces.
    sim.set_train_destination(train_id, rails[2])
        .expect("the train takes a destination");
    run_until_settled(&mut sim);

    let stopped = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive is on the track")
        .position;
    assert_eq!(
        sim.rail_block_key(stopped.edge),
        Some(block_of(&sim, rails[8])),
        "the train is held on its own side of the boundary"
    );
    assert!(
        sim.train(train_id)
            .expect("the train exists")
            .destination
            .is_some(),
        "it keeps its orders: the way through may open later"
    );
    sim.validate()
        .expect("a train held at a boundary is a valid world");
}

/// Two trains meeting head-on on single track. One of them is let into the
/// middle block and the other is held at its signal; then the first cannot get
/// out of the middle either, because the block beyond it is where the second one
/// is standing.
///
/// That is deadlock, and it is a railway the player built rather than a bug.
/// What is under test is that it is *stable*: both trains stop, they stay
/// stopped, nothing drove through anything, the world still validates, and the
/// claim on the middle block resolved to the lower train id — the same way on
/// every machine and every replay.
#[test]
fn two_trains_meeting_head_on_deadlock_and_stay_stopped() {
    let (mut sim, rails) = world_with_signalled_run(24);
    // The middle block signalled from both ends, which is how a two-way single
    // line is signalled and what makes either train able to be let into it.
    for rail_index in [8, 16] {
        place_signal(
            &mut sim,
            &rails,
            rail_index,
            Direction::North,
            "rail_signal",
        );
        place_signal(
            &mut sim,
            &rails,
            rail_index,
            Direction::South,
            "rail_signal",
        );
    }
    let (northbound_stock, northbound) = place_locomotive(&mut sim, &rails, 3);
    let (southbound_stock, southbound) = place_locomotive(&mut sim, &rails, 20);
    assert!(northbound < southbound, "ids follow placement order");

    sim.set_train_destination(northbound, rails[21])
        .expect("the train takes a destination");
    sim.set_train_destination(southbound, rails[2])
        .expect("the train takes a destination");
    run_until_settled(&mut sim);

    // Neither got where it was going, and both are still asking.
    for train_id in [northbound, southbound] {
        let train = sim.train(train_id).expect("the train exists");
        assert!(train.is_stationary());
        assert!(
            train.destination.is_some(),
            "a deadlocked train keeps its orders"
        );
    }
    // The middle went to the lower train id. Which one wins is not the point —
    // that the answer is a function of the world is.
    assert_eq!(
        sim.rail_block_claimant(block_of(&sim, rails[8])),
        Some(northbound)
    );

    // Nobody drove through anybody: the two bodies are on their own sides of the
    // railway still.
    let north_at = sim
        .rolling_stock_piece(northbound_stock)
        .expect("the locomotive is on the track")
        .position;
    let south_at = sim
        .rolling_stock_piece(southbound_stock)
        .expect("the locomotive is on the track")
        .position;
    assert_ne!(
        sim.rail_block_key(north_at.edge),
        sim.rail_block_key(south_at.edge)
    );
    sim.validate()
        .expect("a deadlocked railway is a valid world");

    // And it stays that way: another second of ticking moves nobody and changes
    // nobody's claim, so nothing is retrying, re-searching, or oscillating.
    let settled = [northbound, southbound].map(|train_id| {
        (
            reserved(&sim, train_id),
            sim.rolling_stock_piece(if train_id == northbound {
                northbound_stock
            } else {
                southbound_stock
            })
            .expect("the locomotive is on the track")
            .position,
        )
    });
    for _ in 0..60 {
        sim.tick();
    }
    for (index, train_id) in [northbound, southbound].into_iter().enumerate() {
        assert_eq!(reserved(&sim, train_id), settled[index].0);
        assert_eq!(
            sim.rolling_stock_piece(if train_id == northbound {
                northbound_stock
            } else {
                southbound_stock
            })
            .expect("the locomotive is on the track")
            .position,
            settled[index].1
        );
    }
}

/// A chain signal in front of a full block does not let a train through, and —
/// the whole point of it — holds the train *before* itself rather than in the
/// stretch beyond.
#[test]
fn a_chain_signal_in_front_of_a_full_block_holds_the_train_before_it() {
    let (mut sim, rails) = world_with_signalled_run(24);
    let chain = place_signal(&mut sim, &rails, 8, Direction::North, "chain_signal");
    place_signal(&mut sim, &rails, 12, Direction::North, "rail_signal");
    // Standing in the block past the ordinary signal, so the chain's onward path
    // cannot clear.
    place_locomotive(&mut sim, &rails, 17);
    let (stock_id, train_id) = place_locomotive(&mut sim, &rails, 3);

    sim.set_train_destination(train_id, rails[21])
        .expect("the train takes a destination");
    run_until_settled(&mut sim);

    let stopped = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive is on the track")
        .position;
    assert_eq!(
        sim.rail_block_key(stopped.edge),
        Some(block_of(&sim, rails[0])),
        "held on the approach side of the chain signal, not inside the stretch it guards"
    );
    assert!(
        !reserved(&sim, train_id).contains(&block_of(&sim, rails[8])),
        "a chain that cannot clear takes nothing, so the junction stays free"
    );
    assert_eq!(
        sim.rail_signal_aspect(chain),
        Some(RailSignalAspect::Clear),
        "the block the chain guards is empty — what holds the train is the signal beyond it"
    );
    sim.validate()
        .expect("a train held at a chain signal is a valid world");
}

/// A chain signal with no ordinary signal anywhere beyond it never clears. There
/// is nothing that could clear it — a chain signal asks the signal past it, and
/// there is none — so the train waits at its entrance rather than being let into
/// a stretch nothing is protecting the far end of.
#[test]
fn a_chain_signal_with_nothing_beyond_it_never_clears() {
    let (mut sim, rails) = world_with_signalled_run(20);
    place_signal(&mut sim, &rails, 8, Direction::North, "chain_signal");
    let (stock_id, train_id) = place_locomotive(&mut sim, &rails, 3);

    sim.set_train_destination(train_id, rails[16])
        .expect("the train takes a destination");
    run_until_settled(&mut sim);

    let stopped = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive is on the track")
        .position;
    assert_eq!(
        sim.rail_block_key(stopped.edge),
        Some(block_of(&sim, rails[0])),
        "held on the approach side, because nothing past the chain could clear it"
    );
    assert_eq!(
        reserved(&sim, train_id),
        vec![block_of(&sim, rails[0])],
        "an unresolved chain commits nothing"
    );
    sim.validate()
        .expect("a train held at an unresolved chain is a valid world");
}

/// The same railway with an ordinary signal where the chain was: now the train
/// is let into the stretch and comes to rest inside it. This is what a chain
/// signal exists to avoid, and stating it here is what makes the test above
/// about the chain rule rather than about the geometry.
#[test]
fn an_ordinary_signal_in_the_same_place_lets_the_train_into_the_stretch() {
    let (mut sim, rails) = world_with_signalled_run(24);
    place_signal(&mut sim, &rails, 8, Direction::North, "rail_signal");
    place_signal(&mut sim, &rails, 12, Direction::North, "rail_signal");
    place_locomotive(&mut sim, &rails, 17);
    let (stock_id, train_id) = place_locomotive(&mut sim, &rails, 3);

    sim.set_train_destination(train_id, rails[21])
        .expect("the train takes a destination");
    run_until_settled(&mut sim);

    let stopped = sim
        .rolling_stock_piece(stock_id)
        .expect("the locomotive is on the track")
        .position;
    assert_eq!(
        sim.rail_block_key(stopped.edge),
        Some(block_of(&sim, rails[8])),
        "let in as far as the next signal, and stopped there"
    );
    assert!(reserved(&sim, train_id).contains(&block_of(&sim, rails[8])));
}

/// Track being mined is the one moment a claim can stop naming the stretch of
/// railway it was taken on. Every claim goes back, and the train takes what it
/// is standing in again on the next tick — never displaced from where it
/// physically is.
#[test]
fn mining_track_gives_every_claim_back_and_the_train_retakes_what_it_stands_in() {
    let (mut sim, rails) = world_with_signalled_run(20);
    place_signal(&mut sim, &rails, 8, Direction::North, "rail_signal");
    let (_, train_id) = place_locomotive(&mut sim, &rails, 3);
    sim.set_train_destination(train_id, rails[16])
        .expect("the train takes a destination");
    sim.tick();
    sim.tick();
    assert!(!reserved(&sim, train_id).is_empty());

    // A rail well ahead of the train, so the train itself survives the mining
    // and only its claims are in question.
    crate::entity_mutation::remove(&mut sim, rails[14]).expect("a placed rail can be removed");

    assert_eq!(
        reserved(&sim, train_id),
        vec![],
        "invalidating the graph hands every block back"
    );
    sim.validate()
        .expect("a world mid-placement holds no claims and is still valid");

    sim.tick();
    let standing_in = sim
        .train(train_id)
        .expect("the train exists")
        .stock
        .first()
        .and_then(|stock_id| sim.rolling_stock_piece(*stock_id))
        .map(|stock| stock.position.edge)
        .expect("the train is on the track");
    assert!(
        reserved(&sim, train_id).contains(&block_of(&sim, standing_in)),
        "the next tick hands the train back the block it is standing in"
    );
    sim.validate().expect("a re-taken claim is a valid world");
}

/// Claims are durable state, so a save taken with a train part way toward a
/// block loads with the same train holding the same track. Rebuilding them would
/// be a second resolution whose answer could differ from the one the trains were
/// driving on.
#[test]
fn claims_survive_a_save_and_the_partition_is_rebuilt_around_them() {
    let (mut sim, rails) = world_with_signalled_run(20);
    place_signal(&mut sim, &rails, 8, Direction::North, "rail_signal");
    place_signal(&mut sim, &rails, 14, Direction::North, "rail_signal");
    let (_, train_id) = place_locomotive(&mut sim, &rails, 3);
    sim.set_train_destination(train_id, rails[16])
        .expect("the train takes a destination");
    for _ in 0..30 {
        sim.tick();
    }
    let before = reserved(&sim, train_id);
    assert!(!before.is_empty());

    let bytes = crate::save_to_bytes(&sim).expect("a signalled railway should save");
    let loaded = crate::load_from_bytes(&bytes).expect("a signalled railway should load");

    assert_eq!(sim.state_hash(), loaded.state_hash());
    assert_eq!(reserved(&loaded, train_id), before);
    assert_eq!(loaded.rail_blocks().count(), sim.rail_blocks().count());

    // The index over the claims is derived, so it is the first tick that rebuilds
    // it — from the claims the save carried, which is the whole reason they are
    // saved.
    let mut loaded = loaded;
    loaded.tick();
    for block in before {
        assert_eq!(loaded.rail_block_claimant(block), Some(train_id));
    }
}

/// A signal reports its aspect onto the circuit networks it is wired to, on a
/// channel the player picks — the same mechanism an accumulator's charge uses,
/// because it is the same shape of reading.
#[test]
fn a_wired_signal_publishes_its_aspect() {
    let (mut sim, rails) = world_with_signalled_run(20);
    let signal = place_signal(&mut sim, &rails, 8, Direction::North, "rail_signal");
    let channel = SignalId::Virtual(
        sim.world
            .prototypes
            .virtual_signals
            .iter()
            .find(|virtual_signal| virtual_signal.name == "signal_a")
            .expect("the catalog defines signal_a")
            .id,
    );
    let reader = place_reader_wired_to(&mut sim, signal);
    sim.set_circuit_read_contents(signal, true)
        .expect("a signal publishes its aspect");
    sim.set_entity_output_signal(signal, Some(channel))
        .expect("a signal has a reading to publish");
    sim.tick();

    assert_eq!(
        sim.circuit_signals_at_entity(reader).value(channel),
        RailSignalAspect::Clear.circuit_value()
    );

    place_locomotive(&mut sim, &rails, 14);
    // Twice: circuits resolve before the rails do, so a network carries the
    // aspect the previous tick's reservation settled — the one-tick delay every
    // circuit source has.
    sim.tick();
    sim.tick();

    assert_eq!(
        sim.circuit_signals_at_entity(reader).value(channel),
        RailSignalAspect::Blocked.circuit_value()
    );
}

/// A chest wired to `entity_id` with a red wire, so a test can read what that
/// entity publishes off the network they share.
fn place_reader_wired_to(sim: &mut Simulation, entity_id: EntityId) -> EntityId {
    let catalog = sim.world.prototypes.clone();
    let red_wire = factory_data::item_id_by_name(&catalog, "red_wire");
    sim.player_inventory
        .insert(&catalog, red_wire, 10)
        .expect("the player inventory should accept wire");
    let chest = entity_id_by_name(&catalog, "chest");
    let footprint = sim
        .entities
        .placed_entity(entity_id)
        .expect("the signal is placed")
        .footprint;
    let reader = place_at(sim, chest, footprint.x, footprint.y + 1, Direction::North);
    sim.connect_circuit_wire(
        CircuitNode::new(entity_id, ConnectorPort::Single),
        CircuitNode::new(reader, ConnectorPort::Single),
        WireColor::Red,
    )
    .expect("a signal and a chest beside it are within wire reach");
    reader
}

/// A claim is part of simulation identity, and the index over the claims is not.
///
/// Both halves matter. The claim decides what a train does next, so two worlds
/// that differ in who holds a junction are two different worlds; the index is
/// rebuilt from the claims every tick, so a world that had not rebuilt it yet is
/// the same world as one that had.
#[test]
fn a_claim_is_part_of_simulation_identity_and_the_index_over_it_is_not() {
    let (mut sim, rails) = world_with_signalled_run(20);
    place_signal(&mut sim, &rails, 8, Direction::North, "rail_signal");
    let (_, train_id) = place_locomotive(&mut sim, &rails, 3);
    sim.tick();
    let before = sim.state_hash();

    sim.rolling_stock
        .trains
        .get_mut(&train_id)
        .expect("the train exists")
        .reserved_blocks
        .clear();
    assert_ne!(before, sim.state_hash(), "a claim is durable state");

    // The derived index is not: clearing it leaves the world identical, which is
    // what makes rebuilding it every tick free of consequence.
    let with_claim = crate::save_to_bytes(&sim).expect("a signalled railway should save");
    let loaded = crate::load_from_bytes(&with_claim).expect("a signalled railway should load");
    assert_eq!(sim.state_hash(), loaded.state_hash());
    assert_eq!(
        loaded.rail_block_claimant(block_of(&loaded, rails[0])),
        None
    );
}
