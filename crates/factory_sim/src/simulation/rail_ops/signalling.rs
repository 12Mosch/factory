//! Reservation: who holds which block, and how far that lets a train run.
//!
//! A train reserves the block it is standing in and the block ahead of it, and
//! may run as far as the far end of what it holds. Everything a signalled
//! railway does follows from those two sentences plus the rule a chain signal
//! adds, and this module is where all three live.
//!
//! "Ahead" is every direction the train may travel *this tick*, which is one
//! direction except while a reversal is being commanded — the one moment the way
//! a train is rolling and the way it is being driven disagree, and tractive force
//! may flip its velocity inside the tick. Both are walked and both are claimed
//! then, because a limit for the direction the train did not end up taking is a
//! limit that does not apply, and an unlimited step across a red boundary is a
//! collision.
//!
//! Four properties hold it together.
//!
//! * **A claim survives the tick that made it.** The reservation index is
//!   rebuilt every tick, but it is *seeded* from the claims the trains already
//!   hold, so a train never re-competes for a block it has. Without that, the
//!   fixed resolution order below would let a low-numbered train take a block
//!   out from under a train that had already been let into it — which is a
//!   collision, not a scheduling decision. This is also the reason a claim is
//!   durable state on the train and not derived: where a train is says nothing
//!   about which block it was let into next.
//! * **Claims resolve in train id order.** Two trains reaching one junction on
//!   the same tick always resolve the same way on every machine and every
//!   replay, because the order is the train ids and not the order a map happened
//!   to be walked. Same rule the circuit network states in the header of
//!   [`crate::circuits`].
//! * **Being somewhere beats having booked it.** The pass claims occupied blocks
//!   first, so a train physically standing in a block holds it whatever any
//!   saved claim said. A train is never displaced from where it already is.
//! * **A chain signal is all or nothing.** The blocks from a chain signal up to
//!   and including the first ordinary signal beyond it are claimed together or
//!   not at all, so a train that could not get all the way through a junction
//!   waits *before* it instead of stopping inside it.
//!
//! Deadlock — two trains that have each claimed what the other needs — is a
//! state a player builds, not a bug. Both trains stop at their signals and stay
//! stopped: the pass is a bounded walk per train with no retry and no
//! re-search, so nothing spins, nothing is corrupted, and pulling up a rail is
//! all it takes to recover.

use std::collections::{BTreeMap, BTreeSet};

use factory_data::RailSignalKind;

use crate::ids::EntityId;
use crate::rail::RailSignalAspect;
use crate::rolling_stock::{RailPosition, TrainId, TrainThrottle};
use crate::simulation::*;

use super::blocks::RailBlockPartition;
use super::types::RailGraph;

/// How far ahead of itself a train looks for the signal it must stop at, in
/// fixed-point units of track.
///
/// A bound on the walk rather than a rule about railways: the walk runs once per
/// train per tick, and without a limit a train on a long unsignalled stretch
/// would pay for the whole of it every tick. A hundred tiles is more than an
/// order of magnitude beyond both a tick's travel (about one tile) and the
/// distance a train needs to stop (about six), so a signal is always found long
/// before it matters — and a train that has not found one yet is simply not
/// limited by one, which the end of the line and the stock ahead still are.
const SIGNAL_LOOKAHEAD_REACH_FIXED: i64 = 100 * crate::POSITION_SCALE;

/// How far a walk already inside a chain run may go on looking for the signal
/// that resolves it, in fixed-point units of track.
///
/// Larger than the bound above, because an unresolved chain commits nothing and a
/// chain run the walk gave up on early would hold trains at its entrance for as
/// long as the yard beyond it was deeper than the bound. Still a constant, and
/// that is the point: without one, a chain signal followed by a long unsignalled
/// stretch would walk to the end of the line every tick, for every train parked
/// behind it, which is a per-tick cost proportional to the size of the railway.
///
/// Five hundred tiles is far past any junction worth calling one, so in practice
/// this bounds the pathological layout and never the real ones.
const SIGNAL_CHAIN_REACH_FIXED: i64 = 5 * SIGNAL_LOOKAHEAD_REACH_FIXED;

/// How many signals one lookahead may walk through.
///
/// What makes a chain of chain signals terminate. A ring of them with no
/// ordinary signal anywhere on it is a railway a player can build, and the walk
/// has to come back from it.
const SIGNAL_LOOKAHEAD_LIMIT: usize = 32;

/// One signal the lookahead reached, and what it would take to pass it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SignalAhead {
    /// Track between the train's leading end and the signal.
    distance_fixed: i64,
    kind: RailSignalKind,
    /// The block the signal would admit the train into.
    ///
    /// `None` covers the two ways a boundary can be impassable: no signal faces
    /// this way across it — which is what makes a single signal a one-way
    /// boundary — or there is no track beyond it to be let into. Neither is
    /// claimable, so both stop the train, and the claim rule needs no special
    /// case for either.
    guarded_block: Option<EntityId>,
}

/// This tick's reservation state: who holds what, what each signal shows, and
/// how far each train may run.
///
/// Derived, runtime-only, and rebuilt every tick from the trains and the
/// partition. The buffers are held rather than freed, so the pass allocates
/// nothing once a railway has settled.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct RailSignalling {
    /// Blocks some train's stock is standing in, as `(block, train)`. A pair
    /// rather than a map because a block really can hold two trains — a wagon
    /// dropped onto occupied track — and a map would have to pick one of them.
    occupancy: BTreeSet<(EntityId, TrainId)>,
    /// Who holds each block. Exclusive by construction: a claim is only ever
    /// inserted where there is none.
    claims: BTreeMap<EntityId, TrainId>,
    aspects: BTreeMap<EntityId, RailSignalAspect>,
    /// How far each train may still run before the signal it has not been let
    /// past, keyed by the direction along the train's own facing that the
    /// allowance was measured in.
    ///
    /// Keyed by direction rather than carrying one, because a train being
    /// reversed has an allowance each way and the step has to read the one that
    /// matches the way it actually moved. An absent entry is a direction the pass
    /// did not walk, which is only ever a direction the train cannot travel.
    limits: BTreeMap<(TrainId, bool), i64>,
    /// The signals ahead of the train being planned for, in travel order.
    signals_ahead: Vec<SignalAhead>,
    /// The reservation being built for the train being planned for, ascending.
    next_blocks: Vec<EntityId>,
    /// The reservation that train held coming into the tick, ascending.
    previous_blocks: Vec<EntityId>,
}

impl_runtime_only_identity!(RailSignalling);

impl RailSignalling {
    pub(in crate::simulation) fn clear(&mut self) {
        self.occupancy.clear();
        self.claims.clear();
        self.aspects.clear();
        self.limits.clear();
    }

    /// How far `train_id` may run toward `forward`, or `None` when no signal
    /// limits it that way.
    pub(in crate::simulation) fn limit(&self, train_id: TrainId, forward: bool) -> Option<i64> {
        self.limits.get(&(train_id, forward)).copied()
    }

    pub(in crate::simulation) fn aspect(&self, signal: EntityId) -> Option<RailSignalAspect> {
        self.aspects.get(&signal).copied()
    }

    /// The train holding `block`, if any.
    pub(in crate::simulation) fn claimant(&self, block: EntityId) -> Option<TrainId> {
        self.claims.get(&block).copied()
    }

    /// Whether any train's stock is standing in `block`.
    fn is_occupied(&self, block: EntityId) -> bool {
        self.occupants(block).next().is_some()
    }

    /// Every train standing in `block`, ascending.
    fn occupants(&self, block: EntityId) -> impl Iterator<Item = TrainId> + '_ {
        self.occupancy
            .range((block, TrainId::new(0))..=(block, TrainId::new(u64::MAX)))
            .map(|(_, train_id)| *train_id)
    }

    /// Whether `block` is one this train may take: free track, or track it
    /// already holds.
    fn is_claimable(&self, block: Option<EntityId>, train_id: TrainId) -> bool {
        block.is_some_and(|block| {
            self.claims
                .get(&block)
                .is_none_or(|holder| *holder == train_id)
        })
    }
}

impl Simulation {
    /// What a signal is showing, for the renderer and for a circuit connector
    /// wired to it.
    ///
    /// `None` for anything that is not a signal, and for a signal the partition
    /// has not seen yet — which is the tick a signal is placed on, before the
    /// graph it cuts has been rebuilt.
    pub fn rail_signal_aspect(&self, entity_id: EntityId) -> Option<RailSignalAspect> {
        self.rails.signalling.aspect(entity_id)
    }

    /// A placed signal as the block partition sees it: what it is, which point
    /// and heading it governs, and the blocks it stands between.
    ///
    /// `None` for anything that is not a signal, and for a signal with no track
    /// beside it to bind to.
    pub fn rail_signal(&self, entity_id: EntityId) -> Option<crate::rail::RailSignalSnapshot> {
        debug_assert!(
            !self.rails.graph_dirty,
            "rail graph must be ensured before querying signals"
        );
        self.rails.blocks.signal(entity_id).copied()
    }

    /// The blocks the placed signals cut the track into.
    pub fn rail_blocks(&self) -> impl Iterator<Item = &crate::rail::RailBlockSnapshot> {
        debug_assert!(
            !self.rails.graph_dirty,
            "rail graph must be ensured before querying blocks"
        );
        self.rails
            .blocks
            .blocks()
            .iter()
            .map(|block| &block.snapshot)
    }

    /// The block a rail belongs to, named by the lowest rail entity id in it.
    pub fn rail_block_key(&self, rail: EntityId) -> Option<EntityId> {
        self.rails.blocks.block_key_for_edge(rail)
    }

    /// The train holding a block, if one does.
    pub fn rail_block_claimant(&self, block: EntityId) -> Option<TrainId> {
        self.rails.signalling.claimant(block)
    }

    /// Resolves every train's claim on the track for this tick, and works out
    /// what each signal is showing.
    ///
    /// Runs after the routes are planned — a train's plan is what says which way
    /// it is about to go — and before any train is stepped, because the step
    /// reads the allowance this produces.
    pub(in crate::simulation) fn advance_rail_signals(&mut self) {
        self.rails.signalling.clear();
        if self.rolling_stock.trains.is_empty() && self.rails.blocks.signals().is_empty() {
            return;
        }

        self.collect_block_occupancy();
        self.seed_block_claims();
        let train_ids = self
            .rolling_stock
            .trains
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for train_id in train_ids {
            self.reserve_train_blocks(train_id);
        }
        self.refresh_signal_aspects();
    }

    /// Records which block every piece of stock is standing in, and hands each
    /// occupied block to its occupier.
    ///
    /// Occupancy is claimed before anything else is even looked at, which is
    /// what makes "a train holds the block it is in" unconditional. A saved
    /// claim on a block someone is standing in loses to the standing, and the
    /// loser drops it in its own step below.
    fn collect_block_occupancy(&mut self) {
        let Simulation {
            rails,
            rolling_stock,
            world,
            ..
        } = self;
        for stock in rolling_stock.iter() {
            let train = stock.train;
            rolling_stock_ops::push_stock_rails(&rails.graph, &world.prototypes, stock, |rail| {
                if let Some(block) = rails.blocks.block_key_for_edge(rail) {
                    rails.signalling.occupancy.insert((block, train));
                }
            });
        }
        // Lowest occupier wins a block two trains are somehow both in. That can
        // only come of stock put down on track someone else is on, which
        // placement refuses; the rule is here so the index is a function of the
        // world even then.
        let occupied = rails
            .signalling
            .occupancy
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for (block, train_id) in occupied {
            rails.signalling.claims.entry(block).or_insert(train_id);
        }
    }

    /// Seeds the index with the claims the trains were already holding.
    ///
    /// This is what stops a train from having to win its block again every tick.
    /// A key that no longer names a block is simply not seeded — the block it
    /// named was split or joined by track changing, and the claim went with it.
    fn seed_block_claims(&mut self) {
        let Simulation {
            rails,
            rolling_stock,
            ..
        } = self;
        for train in rolling_stock.trains.values() {
            for key in &train.reserved_blocks {
                if rails.blocks.block(*key).is_some() {
                    rails.signalling.claims.entry(*key).or_insert(train.id);
                }
            }
        }
    }

    /// Rebuilds one train's reservation: the blocks it is standing in, the
    /// blocks it may take ahead, and how far that lets it run.
    fn reserve_train_blocks(&mut self, train_id: TrainId) {
        let Some(train) = self.rolling_stock.train(train_id) else {
            return;
        };
        // Both halves are read before anything is written, because the walk
        // needs the train as it stands and the writing below borrows it mutably.
        let leading_ends = train_reservation_directions(train).map(|forward| {
            forward.and_then(|forward| {
                self.train_leading_end(train, forward)
                    .map(|leading_end| (forward, leading_end))
            })
        });

        let Simulation {
            rails,
            rolling_stock,
            ..
        } = self;
        let signalling = &mut rails.signalling;
        signalling.next_blocks.clear();
        // Swapped out rather than read in place: the old list is what says which
        // claims have to be given back, and the new one is built beside it. The
        // swap hands the train the buffer the last one was held in, so the two
        // rotate between the trains instead of being reallocated every tick.
        let train = rolling_stock
            .trains
            .get_mut(&train_id)
            .expect("the train was just read");
        std::mem::swap(&mut train.reserved_blocks, &mut signalling.previous_blocks);
        train.reserved_blocks.clear();

        // The blocks the train is standing in are always part of its reservation,
        // whether or not it is going anywhere: a train another one was let into
        // the block of would be driven into.
        let RailSignalling {
            occupancy,
            claims,
            next_blocks,
            ..
        } = signalling;
        for (block, occupant) in occupancy.iter() {
            if *occupant == train_id && claims.get(block).is_none_or(|holder| *holder == train_id) {
                claims.insert(*block, train_id);
                next_blocks.push(*block);
            }
        }

        let mut limits = [None; 2];
        for (index, (forward, leading_end)) in leading_ends.into_iter().flatten().enumerate() {
            limits[index] = self
                .claim_blocks_ahead(train_id, leading_end)
                .map(|allowance_fixed| (forward, allowance_fixed));
        }

        let Simulation {
            rails,
            rolling_stock,
            ..
        } = self;
        rails.signalling.next_blocks.sort_unstable();
        rails.signalling.next_blocks.dedup();
        // Whatever the train held and no longer does goes back, so the tick a
        // train stops wanting a block is the tick another train can have it.
        for block in &rails.signalling.previous_blocks {
            if rails.signalling.next_blocks.binary_search(block).is_err()
                && rails.signalling.claims.get(block) == Some(&train_id)
            {
                rails.signalling.claims.remove(block);
            }
        }
        for (forward, allowance_fixed) in limits.into_iter().flatten() {
            rails
                .signalling
                .limits
                .insert((train_id, forward), allowance_fixed);
        }
        if let Some(train) = rolling_stock.trains.get_mut(&train_id) {
            train
                .reserved_blocks
                .extend_from_slice(&rails.signalling.next_blocks);
        }
    }

    /// Claims the track ahead of a train and reports how far it may run.
    ///
    /// The prefix that has to be claimable in one go is every chain signal ahead
    /// up to and including the first ordinary signal past them. Claim it and the
    /// train may run to the signal *after* that prefix, because everything up to
    /// there is now its own; fail anywhere in it and the train takes nothing and
    /// stops at the first signal, which is the only place a chain run leaves it
    /// safe to wait.
    ///
    /// A list with no ordinary signal in it has no prefix to commit, and that is
    /// the same answer. A chain signal clears only when the signal beyond it does,
    /// so with no such signal to ask there is nothing that could clear it —
    /// whether the chain runs off the end of the railway, or round a loop of
    /// chain signals, or simply deeper than the walk's own bounds. All three fail
    /// closed, which is the property a chain signal exists to provide: committing
    /// the prefix of a chain the walk could not finish is exactly how a train ends
    /// up stopped inside the junction it was supposed to wait outside.
    fn claim_blocks_ahead(&mut self, train_id: TrainId, leading_end: RailPosition) -> Option<i64> {
        let Simulation { rails, .. } = self;
        if rails.blocks.signals().is_empty() {
            return None;
        }
        let RailSubsystem {
            graph,
            blocks,
            signalling,
            ..
        } = rails;
        collect_signals_ahead(graph, blocks, leading_end, &mut signalling.signals_ahead);

        let commit = signalling
            .signals_ahead
            .iter()
            .position(|signal| signal.kind == RailSignalKind::Block)
            .map_or(0, |index| index + 1);
        let claimable = signalling.signals_ahead[..commit]
            .iter()
            .all(|signal| signalling.is_claimable(signal.guarded_block, train_id));

        let RailSignalling {
            signals_ahead,
            claims,
            next_blocks,
            ..
        } = signalling;
        let stop_at = if claimable {
            // Every block in the prefix is a real one: a boundary that guards
            // nothing is never claimable, so `claimable` would have been false.
            for block in signals_ahead[..commit]
                .iter()
                .filter_map(|signal| signal.guarded_block)
            {
                claims.insert(block, train_id);
                next_blocks.push(block);
            }
            signals_ahead.get(commit).copied()
        } else {
            signals_ahead.first().copied()
        };

        stop_at.map(|signal| signal.distance_fixed.max(0))
    }

    /// The leading end of a train going `forward`, oriented so that travelling
    /// forwards from it travels the way the train is going.
    ///
    /// Measured from the leading *piece's* leading end rather than from the
    /// train's middle, because what must not cross a red signal is the nose.
    fn train_leading_end(
        &self,
        train: &crate::rolling_stock::Train,
        forward: bool,
    ) -> Option<RailPosition> {
        let stock_id = if forward {
            *train.stock.first()?
        } else {
            *train.stock.last()?
        };
        let stock = self.rolling_stock.get(stock_id)?;
        let half = i64::from(
            self.world
                .prototypes
                .entity(stock.prototype_id)?
                .rolling_stock?
                .length_fixed,
        ) / 2;
        let lead = if forward { half } else { -half };
        let end = rolling_stock_ops::travel(&self.rails.graph, stock.position, lead).position;
        Some(if forward { end } else { end.reversed() })
    }

    /// Works out what every signal is showing, from the reservation the pass
    /// just settled.
    fn refresh_signal_aspects(&mut self) {
        let RailSubsystem {
            blocks, signalling, ..
        } = &mut self.rails;
        for signal in blocks.signals() {
            let aspect = match signal.guarded_block {
                // Nothing beyond to be let into. The signal is not a boundary a
                // train can ever cross, so it shows the aspect that says so
                // rather than pretending the absence of track is clear.
                None => RailSignalAspect::Blocked,
                Some(block) if signalling.is_occupied(block) => RailSignalAspect::Blocked,
                Some(block) if signalling.claims.contains_key(&block) => RailSignalAspect::Reserved,
                Some(_) => RailSignalAspect::Clear,
            };
            signalling.aspects.insert(signal.entity_id, aspect);
        }
    }
}

/// Every direction along its own facing a train may travel this tick.
///
/// Two of them exactly when the way it is rolling and the way it is being driven
/// disagree, which is a reversal being commanded. Tractive force is the only
/// thing that can change the sign of a velocity ([`super::super::rolling_stock_ops::braking_distance_fixed`]'s
/// model clamps braking and resistance at a standstill), so that is the one case
/// where a tick's travel can come out the opposite way from the velocity it
/// started with — and reserving only one of the two would leave the other end of
/// the train crossing a boundary it had not been let past.
///
/// A train at rest under no orders returns neither, which is correct: it is going
/// nowhere, and it reserves only what it stands on.
fn train_reservation_directions(train: &crate::rolling_stock::Train) -> [Option<bool>; 2] {
    let rolling = (train.velocity != 0).then_some(train.velocity > 0);
    let driven = train
        .route
        .as_ref()
        .and_then(|route| route.current_leg())
        .map(|leg| leg.forward)
        .or(match train.throttle {
            TrainThrottle::Forward => Some(true),
            TrainThrottle::Reverse => Some(false),
            TrainThrottle::Coast | TrainThrottle::Brake => None,
        });

    [
        rolling.or(driven),
        driven.filter(|driven| Some(*driven) != rolling && rolling.is_some()),
    ]
}

/// Collects the signals ahead of `from`, in travel order.
///
/// `from` must be oriented so that travelling forwards travels the way the train
/// is going. The walk stops as soon as it has what the claim rule needs — the
/// first ordinary signal and one more past it — and otherwise at the end of the
/// line, at [`SIGNAL_LOOKAHEAD_REACH_FIXED`], at [`SIGNAL_LOOKAHEAD_LIMIT`]
/// signals, or once it has crossed more rails than the graph holds, which is
/// what brings it back off a closed loop.
///
/// A list that never reaches an ordinary signal is an unresolved one, and the
/// claim rule commits nothing on it. That is what makes stopping the walk safe
/// under every one of those bounds rather than only under the first.
fn collect_signals_ahead(
    graph: &RailGraph,
    partition: &RailBlockPartition,
    from: RailPosition,
    out: &mut Vec<SignalAhead>,
) {
    out.clear();
    let mut current = from;
    let mut distance_fixed = 0_i64;
    let mut passed_block_signal = false;
    for _ in 0..=graph.edges.len() {
        let Some(edge) = graph.edge_for_entity(current.edge) else {
            return;
        };
        let exit_end = usize::from(current.forward);
        distance_fixed += if current.forward {
            edge.length_fixed - current.distance_fixed
        } else {
            current.distance_fixed
        };
        // The reach bound keeps a train on a long unsignalled stretch from paying
        // for the whole of it every tick: a signal further off than this is one
        // the train cannot get near for many ticks yet, and it will be found from
        // closer.
        //
        // A walk part way through a chain run gets the longer bound instead of
        // the short one. The claim rule needs the signal that resolves the chain
        // and an unresolved chain commits nothing, so the short bound would hold
        // a train at a chain signal for as long as the yard beyond it was deeper
        // than a hundred tiles — but no bound at all would walk to the end of
        // the line every tick behind a chain signal with nothing past it.
        let inside_chain_run = out
            .last()
            .is_some_and(|signal| signal.kind == RailSignalKind::Chain);
        let reach_fixed = if inside_chain_run {
            SIGNAL_CHAIN_REACH_FIXED
        } else {
            SIGNAL_LOOKAHEAD_REACH_FIXED
        };
        if distance_fixed > reach_fixed {
            return;
        }
        let Some((next_index, arrival_end)) = graph.neighbor_end(edge, exit_end) else {
            return;
        };

        let position = edge.end_positions[exit_end];
        if partition.is_boundary(position) {
            let signal = partition.signal_for_crossing(position, edge.headings[exit_end]);
            let kind = signal.map_or(RailSignalKind::Block, |signal| signal.kind);
            out.push(SignalAhead {
                distance_fixed,
                kind,
                guarded_block: signal.and_then(|signal| signal.guarded_block),
            });
            if passed_block_signal || out.len() == SIGNAL_LOOKAHEAD_LIMIT {
                return;
            }
            passed_block_signal = kind == RailSignalKind::Block;
        }

        let next = &graph.edges[next_index];
        let forward = arrival_end == 0;
        current = RailPosition {
            edge: next.entity_id,
            distance_fixed: if forward { 0 } else { next.length_fixed },
            forward,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Direction;
    use crate::rail::RailPoint;
    use crate::simulation::rail_ops::blocks::{RailSignalInput, build_rail_blocks};
    use crate::simulation::rail_ops::graph_builder::build_rail_graph_from_pieces;
    use crate::simulation::rail_ops::test_graphs::{STRAIGHT_FIXED, straight_run};

    fn rail(raw: u64) -> EntityId {
        EntityId::new(raw)
    }

    fn signal(entity_id: u64, joint_index: i64, kind: RailSignalKind) -> RailSignalInput {
        RailSignalInput {
            entity_id: rail(entity_id),
            kind,
            position: RailPoint::new(512, STRAIGHT_FIXED * joint_index),
            heading: Direction::North,
        }
    }

    fn signals_ahead(signals: &[RailSignalInput], start: RailPosition) -> Vec<SignalAhead> {
        let graph = build_rail_graph_from_pieces(&straight_run(1, 8));
        let partition = build_rail_blocks(&graph, signals);
        let mut out = Vec::new();
        collect_signals_ahead(&graph, &partition, start, &mut out);
        out
    }

    /// The walk stops once it has the first ordinary signal and one more past
    /// it, which is everything the claim rule reads. Walking further would cost
    /// a train the whole railway ahead of it every tick.
    #[test]
    fn the_walk_stops_one_signal_past_the_first_ordinary_one() {
        let found = signals_ahead(
            &[
                signal(10, 2, RailSignalKind::Block),
                signal(11, 4, RailSignalKind::Block),
                signal(12, 6, RailSignalKind::Block),
            ],
            RailPosition::new(rail(1), 0, true),
        );

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].distance_fixed, 2 * STRAIGHT_FIXED);
        assert_eq!(found[0].guarded_block, Some(rail(3)));
        assert_eq!(found[1].distance_fixed, 4 * STRAIGHT_FIXED);
    }

    /// Chain signals do not end the walk: what a chain signal needs to know is
    /// whether the signal beyond it clears, so the walk keeps going until it
    /// reaches one that is not a chain.
    #[test]
    fn chain_signals_do_not_end_the_walk() {
        let found = signals_ahead(
            &[
                signal(10, 2, RailSignalKind::Chain),
                signal(11, 4, RailSignalKind::Chain),
                signal(12, 6, RailSignalKind::Block),
            ],
            RailPosition::new(rail(1), 0, true),
        );

        assert_eq!(found.len(), 3);
        assert_eq!(
            found.iter().map(|signal| signal.kind).collect::<Vec<_>>(),
            vec![
                RailSignalKind::Chain,
                RailSignalKind::Chain,
                RailSignalKind::Block
            ]
        );
    }

    /// A boundary nothing faces this way is reported as a signal that guards
    /// nothing, which is exactly how the claim rule already handles a signal it
    /// cannot pass. One-way track needs no case of its own.
    #[test]
    fn a_boundary_facing_the_other_way_guards_nothing() {
        let southbound = RailSignalInput {
            heading: Direction::South,
            ..signal(10, 2, RailSignalKind::Block)
        };

        let found = signals_ahead(&[southbound], RailPosition::new(rail(1), 0, true));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].guarded_block, None);
    }

    /// A chain run the walk could not resolve commits nothing. Reaching the
    /// signal-count bound part way along a chain is indistinguishable, from
    /// inside the claim rule, from a chain that leads nowhere — and committing
    /// the prefix of either is how a train ends up stopped inside the junction
    /// the chain signal was protecting.
    #[test]
    fn a_chain_run_longer_than_the_walk_may_look_stays_unresolved() {
        let signals = (0..40)
            .map(|index| signal(100 + index, index as i64 + 1, RailSignalKind::Chain))
            .collect::<Vec<_>>();
        let graph = build_rail_graph_from_pieces(&straight_run(1, 48));
        let partition = build_rail_blocks(&graph, &signals);
        let mut found = Vec::new();

        collect_signals_ahead(
            &graph,
            &partition,
            RailPosition::new(rail(1), 0, true),
            &mut found,
        );

        assert_eq!(found.len(), SIGNAL_LOOKAHEAD_LIMIT);
        assert!(
            found
                .iter()
                .all(|signal| signal.kind == RailSignalKind::Chain),
            "the walk never reached an ordinary signal, so nothing resolved the chain"
        );
    }

    /// The reach bound must not be what ends a chain walk: a chain run deeper
    /// than the bound would otherwise never resolve, and a train would wait at
    /// its entrance for ever.
    #[test]
    fn the_reach_bound_does_not_cut_a_chain_run_short() {
        let far = SIGNAL_LOOKAHEAD_REACH_FIXED / STRAIGHT_FIXED + 4;
        let graph = build_rail_graph_from_pieces(&straight_run(1, far as usize + 4));
        let partition = build_rail_blocks(
            &graph,
            &[
                signal(100, 1, RailSignalKind::Chain),
                signal(101, far, RailSignalKind::Block),
            ],
        );
        let mut found = Vec::new();

        collect_signals_ahead(
            &graph,
            &partition,
            RailPosition::new(rail(1), 0, true),
            &mut found,
        );

        assert_eq!(
            found.len(),
            2,
            "the ordinary signal beyond the chain is found"
        );
        assert!(found[1].distance_fixed > SIGNAL_LOOKAHEAD_REACH_FIXED);
    }

    /// A chain run is followed further than an ordinary walk, but not for ever:
    /// a chain signal with nothing but empty track past it would otherwise walk
    /// to the end of the line every tick, for every train parked behind it.
    #[test]
    fn a_chain_run_is_still_bounded_by_a_distance() {
        let past = SIGNAL_CHAIN_REACH_FIXED / STRAIGHT_FIXED + 4;
        let graph = build_rail_graph_from_pieces(&straight_run(1, past as usize + 4));
        let partition = build_rail_blocks(
            &graph,
            &[
                signal(100, 1, RailSignalKind::Chain),
                signal(101, past, RailSignalKind::Block),
            ],
        );
        let mut found = Vec::new();

        collect_signals_ahead(
            &graph,
            &partition,
            RailPosition::new(rail(1), 0, true),
            &mut found,
        );

        assert_eq!(
            found.len(),
            1,
            "the walk gave up on the chain rather than following it to the end of the railway"
        );
        assert_eq!(found[0].kind, RailSignalKind::Chain);
    }

    /// A signal further off than the reach bound with no chain run leading to it
    /// is one the walk does give up on: the train cannot get near it for many
    /// ticks, and it will be found from closer.
    #[test]
    fn the_reach_bound_does_cut_an_ordinary_walk_short() {
        let far = SIGNAL_LOOKAHEAD_REACH_FIXED / STRAIGHT_FIXED + 4;
        let graph = build_rail_graph_from_pieces(&straight_run(1, far as usize + 4));
        let partition = build_rail_blocks(&graph, &[signal(100, far, RailSignalKind::Block)]);
        let mut found = Vec::new();

        collect_signals_ahead(
            &graph,
            &partition,
            RailPosition::new(rail(1), 0, true),
            &mut found,
        );

        assert!(found.is_empty());
    }

    /// A train being reversed is going two ways at once as far as reservation is
    /// concerned: tractive force can flip its velocity inside the tick, so the
    /// track has to be claimed whichever way the step comes out.
    #[test]
    fn a_commanded_reversal_reserves_both_ways() {
        let mut train = crate::rolling_stock::Train {
            id: TrainId::new(1),
            stock: Vec::new(),
            velocity: 1,
            travel_remainder: 0,
            throttle: TrainThrottle::Reverse,
            destination: None,
            route: None,
            route_search_exhausted_at: None,
            schedule: Default::default(),
            schedule_arrival_tick: None,
            schedule_last_activity_tick: None,
            scheduled_stop: None,
            reserved_blocks: Vec::new(),
        };
        assert_eq!(
            train_reservation_directions(&train),
            [Some(true), Some(false)]
        );

        // Agreeing on the direction is one direction, whichever way round.
        train.throttle = TrainThrottle::Forward;
        assert_eq!(train_reservation_directions(&train), [Some(true), None]);
        train.velocity = -1;
        train.throttle = TrainThrottle::Reverse;
        assert_eq!(train_reservation_directions(&train), [Some(false), None]);

        // A train at rest takes the direction it is being driven, and one under
        // no orders at all is going nowhere.
        train.velocity = 0;
        assert_eq!(train_reservation_directions(&train), [Some(false), None]);
        train.throttle = TrainThrottle::Brake;
        assert_eq!(train_reservation_directions(&train), [None, None]);
    }

    /// Nothing to find, and the walk says so rather than running to the end of
    /// the line and back.
    #[test]
    fn an_unsignalled_run_reports_no_signals() {
        assert!(signals_ahead(&[], RailPosition::new(rail(1), 0, true)).is_empty());
    }
}
