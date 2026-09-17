//! Sending a train somewhere: planning a route, driving it, and throwing it
//! away when the track under it changes.
//!
//! Three rules hold this together.
//!
//! * **A destination outlives a plan.** [`Train::destination`] is what the train
//!   was asked for; [`Train::route`] is the plan it is currently driving. A
//!   train with a destination and no route is one waiting for a search: track
//!   was pulled up under its plan, or the tick's expansion budget was already
//!   spent, or its last search ran out of expansions without an answer. Nothing
//!   special has to happen for any of those: the pass that plans routes finds it
//!   in exactly that state on a later tick. Only a search that comes back with
//!   *no route exists* takes the destination away.
//! * **Following a route is O(1) per tick.** A leg carries the distance still to
//!   run rather than the distance already covered, so a train spends its travel
//!   down against it instead of measuring back up the track it came along. What
//!   a train has to know each tick is how far to the next stop, and that is a
//!   subtraction.
//! * **The route can only stop a train early, never carry it further.** The leg
//!   distance clips the step the same way the end of the line, a signal, and
//!   other stock do, so a train stops exactly at its mark instead of rolling past
//!   it and being dragged back. Which of the four limits bound the step is what
//!   tells the difference between arriving, waiting for someone to move, waiting
//!   for a signal, and running out of track.
//!
//! Manual driving wins: a drive command clears whatever the train was doing, so
//! the debug throttle keys are never fighting a plan the player cannot see.

use crate::ids::EntityId;
use crate::rolling_stock::{
    RailPosition, RailTarget, RollingStockSubsystem, TRAIN_OCCUPIED_RAIL_PENALTY_FIXED,
    TRAIN_REVERSAL_PENALTY_FIXED, TrainControlError, TrainId, TrainThrottle,
};
use crate::simulation::rail_ops::{
    RailBlockPartition, RailGraph, RailRouteOutcome, RailRouteRequest, RailRouteScratch,
};
use crate::simulation::*;
use factory_data::PrototypeCatalog;
use serde::{Deserialize, Serialize};

use super::traversal::edges_along;
use super::{TrainStep, TrainStepLimit, braking_distance_fixed, travel};

/// Node expansions every route search in one tick may spend between them.
///
/// Trains re-path rarely — on a new destination, on a route invalidated by
/// track changing, and later on a signal-driven reroute — so this is a budget
/// for the unusual tick where several of them do it at once. What it buys is
/// that no single tick can spend an unbounded amount of time in here, however
/// many trains want a plan.
const ROUTE_EXPANSIONS_PER_TICK: usize = 8_192;

/// Node expansions one route search may spend before giving up.
///
/// Two states per rail, so this covers a railway of two thousand pieces even if
/// the search has to look at all of it. A search that wants more is deferred
/// rather than truncated: half a route is not a route.
const ROUTE_MAX_EXPANSIONS: usize = 4_096;

/// Maximum unfinished searches retained at once.
///
/// Each frontier owns two graph-sized cost tables. Limiting the pool to the
/// number of full slices a tick can advance bounds retained routing memory at
/// O(graph) rather than O(waiting trains × graph). Pending and fresh searches
/// share the same round-robin queue, so the bound cannot turn retained work into
/// starvation for a later cheap query.
const ROUTE_PENDING_SEARCH_LIMIT: usize = ROUTE_EXPANSIONS_PER_TICK / ROUTE_MAX_EXPANSIONS;

/// The reusable half of train routing: search scratch, the tick's remaining
/// expansion budget, and where in the trains the last tick got to.
///
/// Most fields are derived scratch. Unfinished searches are the exception: the
/// bounded `pending` pool survives saves and participates in simulation identity
/// because its progress determines when and which route is produced.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct TrainRouting {
    scratch: RailRouteScratch,
    /// Every rail of every block some train holds, with the train holding it,
    /// ordered by train and then by rail.
    ///
    /// Blocks rather than the rails stock physically covers, because a block is
    /// the unit a train waits for: a route through the far end of a block
    /// somebody is in is a route that stops at the signal in front of it. On
    /// unsignalled track the whole railway is one block and every rail is
    /// charged alike, which leaves the ranking where it was.
    ///
    /// Gathered once per tick rather than once per search: it is proportional to
    /// the track the world's trains hold, and rebuilding it for each train that
    /// wants a route would make a tick where many of them do quadratic in it.
    /// The per-train order is what makes the second question — which of these
    /// rails are a given train's own — a range in this rather than a second scan
    /// over everything. Held here and never freed, so the gathering is not also
    /// an allocation.
    held_rails: Vec<(TrainId, EntityId)>,
    /// The track each train holds, as `(train, block)` pairs, ascending and
    /// deduplicated.
    ///
    /// The blocks are collected and made unique *before* their rails are
    /// expanded, which is the difference between a train paying for its blocks
    /// once and paying for them once per piece of stock and once per rail under
    /// each piece. On a long unsignalled network — one block over the whole
    /// railway — the second is thousands of entries per train for the same
    /// answer.
    ///
    /// The second element is a block key, or — for a rail the partition does not
    /// know — the rail itself. The two cannot be confused: a block's key is one
    /// of that block's own rails, so a rail belonging to no block is no block's
    /// key either.
    held_blocks: Vec<(TrainId, EntityId)>,
    /// Every rail in `held_rails`, ascending and deduplicated: what the search
    /// asks its occupancy question of.
    occupied: Vec<EntityId>,
    /// Rails more than one train holds, ascending.
    ///
    /// What stops "a train is not in its own way" from becoming "a train is in
    /// nobody's way". Two trains in one block both hold every rail of it, and
    /// exempting the searching train's own rails wholesale would take the
    /// penalty off track the *other* train is standing in — on an unsignalled
    /// railway, where the single block is every train's own, off the whole
    /// railway at once.
    shared: Vec<EntityId>,
    /// Rails the train currently being planned for holds and no other train
    /// does, ascending. Subtracted from the occupancy above, because a train is
    /// not in its own way.
    exempt: Vec<EntityId>,
    /// Trains that have somewhere to be and no plan for getting there, in id
    /// order. Refilled each tick the pass runs.
    ///
    /// The cursor the pass resumes from is *not* here: which trains a tick with
    /// more searches than budget plans for decides what those trains do next, so
    /// it is durable state on the rolling-stock subsystem rather than something
    /// a save could forget.
    waiting: Vec<TrainId>,
    remaining_expansions: usize,
    /// Marks the tick's targets buffer, so one search can hand the goals to the
    /// pathfinder without allocating a vector per call.
    targets: Vec<RailTarget>,
    /// Whether the rails above describe *this* tick.
    ///
    /// Gathering them is proportional to the track the world's trains stand on,
    /// so it is done once, lazily, on the first search of a tick — and not at
    /// all on the great majority of ticks, where every train already has a plan
    /// and nothing asks.
    held_rails_ready: bool,
    /// Searches that reached their per-tick slice limit, keyed by train so the
    /// next eligible tick resumes their frontier instead of starting over.
    pub(in crate::simulation) pending: BTreeMap<TrainId, PendingTrainRouteSearch>,
}

/// Durable work for one unfinished route query.
///
/// The graph itself is derived and rebuilt on load, but its edge ordering is
/// deterministic. The frontier can therefore be saved using graph-local state
/// indices and resumed once that graph has been rebuilt.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub(in crate::simulation) struct PendingTrainRouteSearch {
    start: RailPosition,
    target: RailTarget,
    occupied: Vec<EntityId>,
    exempt: Vec<EntityId>,
    scratch: RailRouteScratch,
}

impl PartialEq for TrainRouting {
    fn eq(&self, other: &Self) -> bool {
        self.pending == other.pending
    }
}

impl Hash for TrainRouting {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pending.hash(state);
    }
}

impl TrainRouting {
    pub(in crate::simulation) fn from_pending(
        pending: BTreeMap<TrainId, PendingTrainRouteSearch>,
    ) -> Self {
        Self {
            pending,
            ..Self::default()
        }
    }

    pub(in crate::simulation) fn begin_tick(&mut self) {
        self.remaining_expansions = ROUTE_EXPANSIONS_PER_TICK;
        self.held_rails_ready = false;
    }

    fn has_matching_pending(
        &self,
        train_id: TrainId,
        start: RailPosition,
        target: RailTarget,
    ) -> bool {
        self.pending
            .get(&train_id)
            .is_some_and(|pending| pending.start == start && pending.target == target)
    }

    /// Returns a completed or invalidated frontier's allocation to the one-shot
    /// scratch slot, retaining whichever set of buffers is larger.
    fn recycle_scratch(&mut self, scratch: RailRouteScratch) {
        if scratch.buffer_capacity() > self.scratch.buffer_capacity() {
            self.scratch = scratch;
        }
    }

    fn remove_pending(&mut self, train_id: TrainId) {
        if let Some(pending) = self.pending.remove(&train_id) {
            self.recycle_scratch(pending.scratch);
        }
    }

    fn clear_pending(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        for search in pending.into_values() {
            self.recycle_scratch(search.scratch);
        }
    }

    #[cfg(test)]
    pub(in crate::simulation) fn scratch_buffer_capacity_for_test(&self) -> usize {
        self.scratch.buffer_capacity()
    }

    /// Validates durable unfinished work after the rail graph has been rebuilt.
    pub(in crate::simulation) fn invalid_pending_train(
        &self,
        graph: &RailGraph,
        rolling_stock: &RollingStockSubsystem,
    ) -> Option<TrainId> {
        if self.pending.len() > ROUTE_PENDING_SEARCH_LIMIT {
            return self.pending.keys().next().copied();
        }
        for (train_id, pending) in &self.pending {
            let Some(train) = rolling_stock.train(*train_id) else {
                return Some(*train_id);
            };
            let Some(start) = train
                .stock
                .first()
                .and_then(|stock_id| rolling_stock.get(*stock_id))
                .map(|stock| stock.position)
            else {
                return Some(*train_id);
            };
            let sorted_unique = |rails: &[EntityId]| rails.windows(2).all(|pair| pair[0] < pair[1]);
            let invalid = train.route.is_some()
                || train.destination != Some(pending.target)
                || train.route_search_exhausted_at != Some(pending.start)
                || start != pending.start
                || !sorted_unique(&pending.occupied)
                || !sorted_unique(&pending.exempt)
                || pending
                    .occupied
                    .iter()
                    .any(|rail| graph.edge_for_entity(*rail).is_none())
                || pending
                    .exempt
                    .iter()
                    .any(|rail| pending.occupied.binary_search(rail).is_err())
                || !pending
                    .scratch
                    .is_valid_pending_search(graph, pending.start, pending.target);
            if invalid {
                return Some(*train_id);
            }
        }
        None
    }

    #[cfg(test)]
    pub(in crate::simulation) fn corrupt_pending_frontier_for_test(&mut self, train_id: TrainId) {
        self.pending
            .get_mut(&train_id)
            .expect("the test created pending work for this train")
            .scratch
            .corrupt_frontier_state_for_test();
    }

    /// Whether this tick can still pay for a whole search.
    ///
    /// Asked before a search's own inputs are gathered rather than inside it, so
    /// a train the tick is about to defer costs nothing to defer.
    pub(in crate::simulation) fn can_search(&self) -> bool {
        self.remaining_expansions >= ROUTE_MAX_EXPANSIONS
    }

    /// Searches for a route, or defers when this tick can no longer pay for a
    /// whole search.
    ///
    /// Deferral is a `None`, and it is deliberately not an outcome of its own:
    /// the caller does nothing either way, and the train asks again next tick
    /// because it still has a destination and no route.
    pub(in crate::simulation) fn plan(
        &mut self,
        graph: &RailGraph,
        start: RailPosition,
        marks: &[RailTarget],
    ) -> Option<RailRouteOutcome> {
        if !self.can_search() {
            return None;
        }
        let Self {
            scratch,
            occupied,
            exempt,
            remaining_expansions,
            targets,
            ..
        } = self;
        targets.clear();
        targets.extend_from_slice(marks);
        let (outcome, expansions) = scratch.find_route(&RailRouteRequest {
            graph,
            start,
            targets,
            reversal_penalty_fixed: TRAIN_REVERSAL_PENALTY_FIXED,
            occupied_penalty_fixed: TRAIN_OCCUPIED_RAIL_PENALTY_FIXED,
            occupied,
            exempt,
            max_expansions: ROUTE_MAX_EXPANSIONS,
        });
        *remaining_expansions = remaining_expansions.saturating_sub(expansions);
        Some(outcome)
    }

    /// Advances one train's route query by one bounded slice.
    ///
    /// A matching unfinished query owns the occupancy snapshot with which it
    /// began, so moving trains elsewhere cannot change costs halfway through an
    /// A* search. A changed start or target replaces that query with a new one.
    pub(in crate::simulation) fn plan_for_train(
        &mut self,
        train_id: TrainId,
        graph: &RailGraph,
        start: RailPosition,
        target: RailTarget,
    ) -> Option<RailRouteOutcome> {
        if !self.can_search() {
            return None;
        }

        let matches = self.has_matching_pending(train_id, start, target);
        if matches {
            let pending = self
                .pending
                .get_mut(&train_id)
                .expect("the matching route query exists");
            let (outcome, expansions) = pending.scratch.continue_route(&RailRouteRequest {
                graph,
                start: pending.start,
                targets: std::slice::from_ref(&pending.target),
                reversal_penalty_fixed: TRAIN_REVERSAL_PENALTY_FIXED,
                occupied_penalty_fixed: TRAIN_OCCUPIED_RAIL_PENALTY_FIXED,
                occupied: &pending.occupied,
                exempt: &pending.exempt,
                max_expansions: ROUTE_MAX_EXPANSIONS,
            });
            self.remaining_expansions = self.remaining_expansions.saturating_sub(expansions);
            if !matches!(outcome, RailRouteOutcome::Exhausted) {
                self.remove_pending(train_id);
            }
            return Some(outcome);
        }

        self.remove_pending(train_id);
        let (outcome, expansions) = self.scratch.find_route(&RailRouteRequest {
            graph,
            start,
            targets: std::slice::from_ref(&target),
            reversal_penalty_fixed: TRAIN_REVERSAL_PENALTY_FIXED,
            occupied_penalty_fixed: TRAIN_OCCUPIED_RAIL_PENALTY_FIXED,
            occupied: &self.occupied,
            exempt: &self.exempt,
            max_expansions: ROUTE_MAX_EXPANSIONS,
        });
        self.remaining_expansions = self.remaining_expansions.saturating_sub(expansions);
        if matches!(outcome, RailRouteOutcome::Exhausted)
            && self.pending.len() < ROUTE_PENDING_SEARCH_LIMIT
        {
            self.pending.insert(
                train_id,
                PendingTrainRouteSearch {
                    start,
                    target,
                    occupied: self.occupied.clone(),
                    exempt: self.exempt.clone(),
                    scratch: std::mem::take(&mut self.scratch),
                },
            );
        }
        Some(outcome)
    }

    /// Collects what is held where, once for the tick: every rail of every block
    /// a train is standing in or has been let into, and which train holds it.
    ///
    /// Both halves are gathered, because they answer at different moments. A
    /// train that has just been put on the track holds no claim yet and is still
    /// in somebody's way; a train that has been let into the block ahead is in
    /// the way of everyone else before it gets there.
    fn collect_held_rails(
        &mut self,
        graph: &RailGraph,
        partition: &RailBlockPartition,
        rolling_stock: &RollingStockSubsystem,
        prototypes: &PrototypeCatalog,
    ) {
        // Which blocks first, made unique, and only then the rails in them. A
        // piece of stock covers several rails and a train several pieces, so
        // expanding as the rails are visited would re-expand one block once per
        // rail under every piece standing in it.
        self.held_blocks.clear();
        for stock in rolling_stock.iter() {
            let train = stock.train;
            push_stock_rails(graph, prototypes, stock, |rail| {
                // A rail the partition has not seen — one mined out from under a
                // wagon, before the pruning pass reaches it — stands for itself.
                // It is still what is in the way.
                let held = partition.block_key_for_edge(rail).unwrap_or(rail);
                self.held_blocks.push((train, held));
            });
        }
        for train in rolling_stock.trains() {
            self.held_blocks
                .extend(train.reserved_blocks.iter().map(|key| (train.id, *key)));
        }
        self.held_blocks.sort_unstable();
        self.held_blocks.dedup();

        self.held_rails.clear();
        for (train, held) in &self.held_blocks {
            match partition.block(*held) {
                Some(block) => self
                    .held_rails
                    .extend(block.edges.iter().map(|edge| (*train, *edge))),
                None => self.held_rails.push((*train, *held)),
            }
        }
        // By train first, so one train's rails are a contiguous run, and by rail
        // within that, which is the order the search reads them in. Blocks do not
        // overlap, so the only duplicates left are a train's claim on a block it
        // is also standing in.
        self.held_rails.sort_unstable();
        self.held_rails.dedup();

        self.occupied.clear();
        self.occupied
            .extend(self.held_rails.iter().map(|(_, rail)| *rail));
        self.occupied.sort_unstable();
        // A rail appearing twice here is a rail two *trains* hold: `held_rails`
        // is already unique per `(train, rail)`, so a repeat cannot be one train
        // counted twice. Taken before the dedup because the dedup is what would
        // destroy the evidence.
        self.shared.clear();
        self.shared.extend(
            self.occupied
                .windows(2)
                .filter(|pair| pair[0] == pair[1])
                .map(|pair| pair[0]),
        );
        self.shared.dedup();
        self.occupied.dedup();
    }

    /// Takes the rails `train_id` holds *alone* out of what the tick already
    /// gathered, which the search then reads as track nothing is in the way on.
    ///
    /// Alone is the operative word. The exemption exists because a train is not
    /// in its own way — without it a long train would be steered off its own
    /// branch by the penalty for being where it already is — but a rail another
    /// train also holds is a rail something *is* in the way on, and exempting it
    /// would hide that. On an unsignalled railway, where one block is every
    /// train's own, it would hide every train in the world from every other.
    ///
    /// A range of the tick's own index rather than another walk over the world's
    /// stock: this runs once per train that wants a route, and a full scan here
    /// would put back the quadratic the tick-wide gathering took out.
    pub(in crate::simulation) fn collect_exempt_rails(&mut self, train_id: TrainId) {
        let first = self
            .held_rails
            .partition_point(|(train, _)| *train < train_id);
        let past = self
            .held_rails
            .partition_point(|(train, _)| *train <= train_id);
        let Self {
            held_rails,
            shared,
            exempt,
            ..
        } = self;
        exempt.clear();
        exempt.extend(
            held_rails[first..past]
                .iter()
                .map(|(_, rail)| *rail)
                .filter(|rail| shared.binary_search(rail).is_err()),
        );
    }
}

/// The waiting trains in the order this tick plans for them: id order, resumed
/// just after the one planned for last and wrapping round once.
///
/// The rotation is what makes the budget fair. A train whose search costs the
/// whole of it — a destination across a railway larger than one search may walk
/// — would otherwise be reached first on every tick and deny every train behind
/// it a plan for as long as it kept asking.
fn planning_order(
    waiting: &[TrainId],
    planned_last: Option<TrainId>,
) -> impl Iterator<Item = &TrainId> {
    let resume = planned_last.map_or(0, |last| waiting.partition_point(|id| *id <= last));
    waiting[resume..].iter().chain(&waiting[..resume])
}

/// Records what a search came back with on the train that asked for it.
///
/// The two failures are not the same failure, and the difference is the whole
/// of this function. `Unreachable` is an answer: there is no way there, so the
/// train stops asking, and because it is no longer a routed train nothing else
/// will steer it — hence the brake here. `Exhausted` is the absence of an
/// answer — the search ran out of expansions, which says nothing about whether
/// a route exists — so the destination is kept and tried again on a later tick.
/// Throwing it away would strand a train whose destination is perfectly
/// reachable, on the very railways big enough to need the cap. It needs no
/// brake of its own: a routed train without a plan is braked by
/// [`Simulation::steer_train`], which is the one place that rule lives.
fn record_route_outcome(
    train: &mut crate::rolling_stock::Train,
    outcome: RailRouteOutcome,
    searched_from: RailPosition,
) {
    // Whatever the answer, it is an answer to the question the last one could
    // not finish, so what was remembered about that one goes.
    train.route_search_exhausted_at = None;
    match outcome {
        RailRouteOutcome::Found { route, .. } => train.route = Some(route),
        RailRouteOutcome::Unreachable => {
            train.destination = None;
            train.route = None;
            train.throttle = TrainThrottle::Brake;
            // A stop there is no way to is a stop this train will not serve, so
            // it gives back the place it was holding there and its schedule steps
            // past the entry. Merely giving the claim back would leave the
            // schedule naming the same unreachable station, which the assignment
            // pass would book again and search for again on every tick that
            // follows; stepping past it is what makes the answer stick until the
            // schedule comes round to the entry again.
            if train.scheduled_stop.is_some() {
                train.release_scheduled_stop();
                train.schedule.advance();
            }
        }
        RailRouteOutcome::Exhausted => {
            train.route = None;
            // The matching frontier is retained by the routing subsystem. The
            // start is durable so save/load and query invalidation can tell
            // which unfinished question that frontier answers.
            train.route_search_exhausted_at = Some(searched_from);
        }
    }
}

/// Reports every rail one piece of stock lies on.
///
/// The body is walked from its back end forward over its own length — the same
/// extent [`super::stock_ends`] measures, so occupancy covers exactly the track
/// the piece stands on and not a unit more of it at either end.
///
/// Shared with the signalling pass rather than written twice: which block a piece
/// of stock is in and which rails a route is charged for are the same question
/// asked one step apart, and two walks would eventually disagree about a wagon
/// sitting exactly on a joint.
pub(in crate::simulation) fn push_stock_rails(
    graph: &RailGraph,
    prototypes: &PrototypeCatalog,
    stock: &crate::rolling_stock::RollingStock,
    visit: impl FnMut(EntityId),
) {
    let Some(half) = prototypes
        .entity(stock.prototype_id)
        .and_then(|prototype| prototype.rolling_stock)
        .map(|rolling_stock| i64::from(rolling_stock.length_fixed) / 2)
    else {
        return;
    };
    let back = travel(graph, stock.position, -half).position;
    edges_along(graph, back, half * 2, visit);
}

impl Simulation {
    /// Sends a train to a rail, stopping in the middle of it.
    ///
    /// The search does not run here. A command lands on a tick boundary and the
    /// rail graph is rebuilt inside the tick, so planning here would either
    /// route over a graph that is about to change or force a rebuild out of
    /// turn; what this does is record where the train is going and let the
    /// routing pass plan it against the track that actually exists.
    pub fn set_train_destination(
        &mut self,
        train_id: TrainId,
        rail: EntityId,
    ) -> Result<(), TrainControlError> {
        let geometry = self
            .rail_piece_geometry(rail)
            .ok_or(TrainControlError::NotRail(rail))?;
        let train = self
            .rolling_stock
            .trains
            .get_mut(&train_id)
            .ok_or(TrainControlError::MissingTrain(train_id))?;
        train.destination = Some(RailTarget::new(rail, geometry.length_fixed / 2));
        train.route = None;
        train.route_search_exhausted_at = None;
        // Being sent somewhere by hand replaces where the schedule was sending
        // it, so the place it had booked at a stop goes back.
        train.release_scheduled_stop();
        Ok(())
    }

    /// Cancels where a train was going. It brakes rather than coasting: a train
    /// whose orders were withdrawn should come to a stop, not keep rolling
    /// toward a destination nobody is steering it to any more.
    pub fn clear_train_destination(&mut self, train_id: TrainId) -> Result<(), TrainControlError> {
        let train = self
            .rolling_stock
            .trains
            .get_mut(&train_id)
            .ok_or(TrainControlError::MissingTrain(train_id))?;
        train.destination = None;
        train.route = None;
        train.route_search_exhausted_at = None;
        train.throttle = TrainThrottle::Brake;
        train.release_scheduled_stop();
        Ok(())
    }

    /// Plans a route for every train that has somewhere to be and no way of
    /// getting there, until the tick's budget runs out.
    ///
    /// One pass for the whole tick rather than a search wedged into each train's
    /// step, because the budget and the occupancy are both tick-wide: the rails
    /// something is standing on are gathered once here and read by every search,
    /// and where the pass stopped last tick is where it starts this one.
    ///
    /// Round-robin rather than id order. A train whose search costs the whole
    /// budget every tick — a destination across a railway larger than one search
    /// may walk — would otherwise deny every train behind it a plan forever,
    /// which is the difference between one train not moving and none of them
    /// moving.
    pub(in crate::simulation) fn plan_train_routes(&mut self) {
        // Anything that changed the query clears the train's durable exhaustion
        // marker. Position changes are included: a train still braking must
        // finish stopping before a fresh query is worth starting.
        let pending_ids = self
            .train_routing
            .pending
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for train_id in pending_ids {
            let keep = self
                .train_routing
                .pending
                .get(&train_id)
                .is_some_and(|pending| {
                    self.rolling_stock.train(train_id).is_some_and(|train| {
                        train.route.is_none()
                            && train.destination == Some(pending.target)
                            && train.route_search_exhausted_at == Some(pending.start)
                    }) && self.train_search_position(train_id) == Some(pending.start)
                });
            if !keep {
                self.train_routing.remove_pending(train_id);
            }
        }

        let mut waiting = std::mem::take(&mut self.train_routing.waiting);
        waiting.clear();
        waiting.extend(
            self.rolling_stock
                .trains()
                .filter(|train| train.is_routed() && train.route.is_none())
                .map(|train| train.id)
                .collect::<Vec<_>>(),
        );
        waiting.retain(|train_id| !self.search_must_wait_for_train_to_stop(*train_id));
        if waiting.is_empty() || !self.train_routing.can_search() {
            self.train_routing.waiting = waiting;
            return;
        }

        // Pending and fresh queries stay in one round-robin. A full pending pool
        // may make a newly exhausted search repeat later rather than retaining a
        // third graph-sized frontier, but it must never keep a cheap new query
        // from using its turn. Matching resumes use their frozen occupancy
        // snapshots, so only fresh turns gather current occupancy.
        for train_id in planning_order(&waiting, self.rolling_stock.planned_last) {
            if !self.train_routing.can_search() {
                break;
            }
            self.rolling_stock.planned_last = Some(*train_id);
            self.plan_train_route(*train_id);
        }
        self.train_routing.waiting = waiting;
    }

    /// Gathers what track the world's trains hold, once per tick and only if
    /// something is going to search.
    ///
    /// Both callers — picking between the platforms of a station, and planning
    /// the route to wherever a train was sent — read the same occupancy, and it
    /// is proportional to the track under the world's stock. Doing it once is
    /// what keeps a tick where many trains want a plan from being quadratic in
    /// that track; doing it lazily is what keeps the ordinary tick, where every
    /// train already has a plan, from paying for it at all.
    pub(in crate::simulation) fn ensure_train_occupancy(&mut self) {
        if self.train_routing.held_rails_ready {
            return;
        }
        let Simulation {
            train_routing,
            rails,
            rolling_stock,
            world,
            ..
        } = self;
        train_routing.collect_held_rails(
            &rails.graph,
            &rails.blocks,
            rolling_stock,
            &world.prototypes,
        );
        train_routing.held_rails_ready = true;
    }

    /// Sets a routed train's throttle for this tick.
    ///
    /// Runs before the train is stepped, so the throttle the step reads is the
    /// one this leg asks for.
    ///
    /// A train with no leg to drive brakes, however it came to be in that
    /// state: the tick's budget was spent before its search, its last search ran
    /// out of expansions, the track under its plan was pulled up, or its
    /// schedule found no platform it could be sent to at all. Whatever throttle
    /// it was last driving on belonged to a plan that no longer exists, and
    /// holding it would run the train on toward a mark nothing is measuring any
    /// more. It costs a routed train a little speed while it waits, and that is
    /// the right trade: a train under no plan should not be moving under one.
    ///
    /// The exception is the train being driven by hand, whose throttle *is* the
    /// plan. That is the only reason this asks about manual control rather than
    /// simply braking everything without a leg: a train handed back to its
    /// schedule while every platform is full has no plan and no driver, and left
    /// alone it would keep accelerating on the throttle its driver walked away
    /// from.
    pub(in crate::simulation) fn steer_train(&mut self, train_id: TrainId) {
        let Some(train) = self.rolling_stock.train(train_id) else {
            return;
        };
        let Some(leg) = train.route.as_ref().and_then(|route| route.current_leg()) else {
            if !train.manual
                && let Some(train) = self.rolling_stock.trains.get_mut(&train_id)
            {
                train.throttle = TrainThrottle::Brake;
            }
            return;
        };
        // Braking is decided against the distance left rather than against a
        // point on the map, and against the model's own stopping distance rather
        // than a guessed margin, so a heavy train starts braking earlier than a
        // light one without either of them being told to.
        //
        // A signal it has not been let past is the nearer of the two marks
        // whenever there is one, so a train comes to rest *at* a red signal
        // rather than being clipped to a standstill against it — which is the
        // difference between a train waiting at a signal and a train that hit it.
        let forces = self.train_forces_now(train_id).unwrap_or_default();
        let remaining_fixed = self
            .signal_allowance_fixed(train_id, leg.forward)
            .map_or(leg.distance_fixed, |allowance| {
                leg.distance_fixed.min(allowance)
            });
        let throttle = if remaining_fixed <= braking_distance_fixed(train.velocity, forces) {
            TrainThrottle::Brake
        } else if leg.forward {
            TrainThrottle::Forward
        } else {
            TrainThrottle::Reverse
        };
        if let Some(train) = self.rolling_stock.trains.get_mut(&train_id) {
            train.throttle = throttle;
        }
    }

    /// Whether an unfinished query must wait for its train to finish braking.
    ///
    /// A train told to brake passes through a great many places on the way down.
    /// Its saved frontier only describes the place it started from, while
    /// restarting from every intermediate position would spend a full slice on
    /// each. Once it is stationary, the matching frontier resumes or a new
    /// query starts from where it came to rest.
    ///
    /// What deliberately does not count as a change is other trains moving:
    /// occupancy shifts what a route costs rather than whether one can be found
    /// inside the cap, and treating it as a change would mean re-searching every
    /// tick — the very thing this exists to prevent.
    fn search_must_wait_for_train_to_stop(&self, train_id: TrainId) -> bool {
        let Some(train) = self.rolling_stock.train(train_id) else {
            return false;
        };
        train.route_search_exhausted_at.is_some() && !train.is_stationary()
    }

    /// Where a search for this train would start: its leading piece's position.
    pub(super) fn train_search_position(&self, train_id: TrainId) -> Option<RailPosition> {
        let train = self.rolling_stock.train(train_id)?;
        self.rolling_stock
            .get(*train.stock.first()?)
            .map(|stock| stock.position)
    }

    /// Searches for a route from where the train stands to where it was sent.
    ///
    /// The route is measured from the train's leading piece. Every piece of a
    /// train advances by the same distance, so which piece the plan is measured
    /// from only fixes where "arrived" leaves the train standing, and the front
    /// is the piece a destination is naturally about.
    fn plan_train_route(&mut self, train_id: TrainId) {
        let (Some(start), Some(target)) = (
            self.train_search_position(train_id),
            self.rolling_stock
                .train(train_id)
                .and_then(|train| train.destination),
        ) else {
            return;
        };

        let resuming = self
            .train_routing
            .has_matching_pending(train_id, start, target);
        if !resuming {
            self.ensure_train_occupancy();
        }
        let Simulation {
            train_routing,
            rails,
            rolling_stock,
            ..
        } = self;
        if !resuming {
            train_routing.collect_exempt_rails(train_id);
        }
        let Some(outcome) = train_routing.plan_for_train(train_id, &rails.graph, start, target)
        else {
            return;
        };
        if let Some(train) = rolling_stock.trains.get_mut(&train_id) {
            record_route_outcome(train, outcome, start);
        }
    }

    /// Spends a tick's travel against the route and retires legs that are done.
    ///
    /// Runs after the step, and is told what bound it. A leg the *track* cut
    /// short is a leg that will never be finished, because a leg is measured to
    /// the train's centre while what stops the train is its nose: it comes to
    /// rest against the buffer with distance still on the leg, and no further
    /// driving can bring the centre nearer. Such a leg is retired rather than
    /// held open — the unrun part carried into the reversal that follows it, or,
    /// on the last leg, ending the journey where the train physically stopped.
    /// Holding it open instead would leave the train with its throttle against a
    /// dead end for ever, burning fuel to stay exactly where it is.
    ///
    /// A mark within half a train of the end of the line is therefore a mark the
    /// train stops short of. The search plans for a point rather than for a
    /// body, which is a limitation worth naming here because this is where it
    /// shows: it is stations, which must land a whole train against a platform,
    /// that will have to teach the plan how long its train is.
    ///
    /// That only holds for a buffer the train ran *at*, though. A train sent
    /// somewhere behind itself keeps rolling the old way while it brakes, and it
    /// can reach the end of the line in that direction — the wrong end, getting
    /// further from its mark with every tick. The track cut that step short too,
    /// but nothing about it was an arrival, so a stall only finishes a leg when
    /// the step it stopped was headed toward that leg's end.
    ///
    /// A leg cut short by other stock is a different matter — that is a leg the
    /// train is still on, and it waits.
    pub(super) fn advance_train_route(&mut self, train_id: TrainId, step: TrainStep) {
        let tick = self.tick;
        let Some(train) = self.rolling_stock.trains.get_mut(&train_id) else {
            return;
        };
        let stationary = train.is_stationary();
        let Some(route) = train.route.as_mut() else {
            return;
        };
        let Some(leg) = route.legs.front_mut() else {
            train.route = None;
            return;
        };
        // Travel away from the leg's end lengthens it. That is not a correction
        // — it is the distance the train now has to make up — and it is why the
        // leg is spent down by signed travel rather than by how far the train
        // moved.
        leg.distance_fixed -= if leg.forward {
            step.travelled_fixed
        } else {
            -step.travelled_fixed
        };
        // A step that moved no distance at all was aimed nowhere, so it cannot
        // have been aimed at the leg's end; the sign is only meaningful when the
        // train asked to go somewhere.
        let stalled_at_leg_end = matches!(step.limit, Some(TrainStepLimit::Track))
            && step.attempted_fixed != 0
            && (step.attempted_fixed > 0) == leg.forward;
        if !stationary || (leg.distance_fixed > 0 && !stalled_at_leg_end) {
            return;
        }

        let unrun = leg.distance_fixed.max(0);
        route.legs.pop_front();
        match route.legs.front_mut() {
            // The next leg runs back down the track this one came up, so track
            // this one could not cover is track the next one no longer has to.
            Some(next) => next.distance_fixed = (next.distance_fixed - unrun).max(0),
            // Nothing left to run: the train is either on its mark or as near it
            // as the end of the line lets it stand. Both are the end of the
            // journey, and it coasts — which for a train that has just come to a
            // stand is doing nothing at all.
            None => {
                train.route = None;
                train.destination = None;
                train.throttle = TrainThrottle::Coast;
                // The one place a scheduled train has arrived. A run of track
                // spent down to nothing while standing still is the only thing
                // that says the train is at the stop it claimed rather than
                // stopped somewhere else with its orders withdrawn, and the wait
                // is timed from here.
                train.arrive_at_scheduled_stop(tick);
            }
        }
    }

    /// How far the route lets the train travel this tick, or `None` when it does
    /// not limit the step at all.
    ///
    /// Only travel *toward* the leg's end is limited. A train still rolling the
    /// other way is getting further from the point it must stop at, and clipping
    /// that would pin it in place instead of letting it come round.
    pub(super) fn route_clearance_fixed(
        &self,
        train_id: TrainId,
        travel_fixed: i64,
    ) -> Option<i64> {
        let leg = self
            .rolling_stock
            .train(train_id)?
            .route
            .as_ref()?
            .current_leg()?;
        let sign = if leg.forward { 1 } else { -1 };
        (travel_fixed.signum() == sign).then(|| sign * leg.distance_fixed.max(0))
    }

    /// Throws away a train's plan while leaving it its destination, so the
    /// routing pass plans it again from wherever it now stands.
    ///
    /// Called whenever a train changes shape. A route is measured from the
    /// train's leading piece and priced for the stock that was coupled when it
    /// was found; a train that has just gained a wagon, lost one, or been cut in
    /// two is not that train, and driving its old plan would run the new one to
    /// the wrong place.
    pub(super) fn discard_train_route(&mut self, train_id: TrainId) {
        if let Some(train) = self.rolling_stock.trains.get_mut(&train_id) {
            train.route = None;
            // A train of a different shape searches from a different place, so
            // whatever its last search could not finish says nothing about what
            // this one would find.
            train.route_search_exhausted_at = None;
        }
    }

    /// Drops the plans that ran over track which is no longer there.
    ///
    /// Called when the rail graph is invalidated, which is the one moment a
    /// route can stop describing the world. A train whose route went is left
    /// with its destination, so the routing pass plans it again from where it
    /// now stands; a train whose *destination* went has nowhere to be sent and
    /// stops.
    pub(in crate::simulation) fn invalidate_train_routes(&mut self) {
        self.train_routing.clear_pending();
        if !self
            .rolling_stock
            .trains()
            .any(|train| train.is_routed() || train.route.is_some())
        {
            return;
        }
        let train_ids = self
            .rolling_stock
            .trains
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for train_id in train_ids {
            // Track changing is the one thing that can turn a search which ran
            // out of expansions into one that does not, so every train waiting
            // on that answer asks again.
            if let Some(train) = self.rolling_stock.trains.get_mut(&train_id) {
                train.route_search_exhausted_at = None;
            }
            let Some(train) = self.rolling_stock.trains.get(&train_id) else {
                continue;
            };
            // A rail that is gone from the entity store is gone: nothing else
            // can remove one, and asking the store is a lookup rather than the
            // geometry resolve asking the catalog would be. Deliberately not a
            // question for the rail graph, which is mid-rebuild.
            let placed = |entity_id: &EntityId| self.entities.placed_entity(*entity_id).is_some();
            let destination_gone = train
                .destination
                .is_some_and(|destination| !placed(&destination.edge));
            let route_gone = train
                .route
                .as_ref()
                .is_some_and(|route| !route.edges.iter().all(placed));
            if !destination_gone && !route_gone {
                continue;
            }
            let Some(train) = self.rolling_stock.trains.get_mut(&train_id) else {
                continue;
            };
            train.route = None;
            if destination_gone {
                train.destination = None;
                train.throttle = TrainThrottle::Brake;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The budget is per tick and a search is all-or-nothing, so a tick that has
    /// already spent most of its allowance defers rather than starting a search
    /// it might not be able to finish. The train keeps its destination, so the
    /// next tick — with a full budget again — plans it.
    #[test]
    fn a_tick_that_has_spent_its_budget_defers_rather_than_searching() {
        let mut routing = TrainRouting::default();
        routing.begin_tick();
        assert!(routing.can_search());

        routing.remaining_expansions = ROUTE_MAX_EXPANSIONS - 1;
        assert!(!routing.can_search());
        assert_eq!(
            routing.plan(
                &RailGraph::default(),
                RailPosition::new(EntityId::new(1), 0, true),
                &[RailTarget::new(EntityId::new(2), 0)],
            ),
            None,
            "a deferred search reports nothing rather than a failure to find a route"
        );

        routing.begin_tick();
        assert!(
            routing.can_search(),
            "the next tick pays for searches again"
        );
    }

    fn train(raw: u64) -> crate::rolling_stock::Train {
        crate::rolling_stock::Train {
            id: TrainId::new(raw),
            stock: Vec::new(),
            velocity: 0,
            travel_remainder: 0,
            throttle: TrainThrottle::Forward,
            destination: Some(RailTarget::new(EntityId::new(raw), 0)),
            route: None,
            route_search_exhausted_at: None,
            schedule: Default::default(),
            schedule_arrival_tick: None,
            schedule_last_activity_tick: None,
            schedule_activity_cargo: None,
            scheduled_stop: None,
            reserved_blocks: Vec::new(),
            manual: false,
        }
    }

    /// A search that ran out of expansions has not answered the question, so the
    /// train keeps where it was going and is planned for again later. Giving up
    /// here would strand a train whose destination is perfectly reachable on
    /// exactly the railways that are big enough to reach the cap. Its throttle
    /// is not touched here because it is still a routed train, and a routed
    /// train without a plan is braked by the steering pass.
    #[test]
    fn an_exhausted_search_keeps_the_destination_and_an_unreachable_one_does_not() {
        let searched_from = RailPosition::new(EntityId::new(7), 512, true);
        let mut exhausted = train(1);
        record_route_outcome(&mut exhausted, RailRouteOutcome::Exhausted, searched_from);
        assert!(exhausted.destination.is_some());
        assert_eq!(exhausted.route, None);
        assert_eq!(
            exhausted.route_search_exhausted_at,
            Some(searched_from),
            "where it asked from is half of what makes it the same question"
        );

        let mut unreachable = train(2);
        record_route_outcome(
            &mut unreachable,
            RailRouteOutcome::Unreachable,
            searched_from,
        );
        assert_eq!(unreachable.destination, None);
        assert_eq!(unreachable.route, None);
        assert_eq!(unreachable.throttle, TrainThrottle::Brake);
    }

    /// A train's own rails come out of the index the tick already gathered,
    /// rather than out of a second walk over the world's stock — the walk that
    /// would put back the quadratic the tick-wide gathering took out.
    #[test]
    fn exempt_rails_are_the_train_s_own_range_of_the_ticks_index() {
        let mut routing = TrainRouting {
            held_rails: vec![
                (TrainId::new(1), EntityId::new(10)),
                (TrainId::new(1), EntityId::new(11)),
                (TrainId::new(4), EntityId::new(20)),
            ],
            ..TrainRouting::default()
        };

        routing.collect_exempt_rails(TrainId::new(1));
        assert_eq!(routing.exempt, vec![EntityId::new(10), EntityId::new(11)]);

        routing.collect_exempt_rails(TrainId::new(4));
        assert_eq!(routing.exempt, vec![EntityId::new(20)]);

        // A train standing on nothing the index knows about — one whose stock
        // has no rolling-stock metadata — exempts nothing rather than the range
        // beside where it would have been.
        routing.collect_exempt_rails(TrainId::new(9));
        assert!(routing.exempt.is_empty());
    }

    /// A rail two trains hold is not one either of them is exempt from. The
    /// exemption is there because a train is not in its own way; a rail somebody
    /// else is also standing in is a rail something *is* in the way on, and
    /// exempting it would take the penalty off the whole of an unsignalled
    /// railway — where one block is every train's own.
    #[test]
    fn a_rail_two_trains_hold_is_exempt_for_neither() {
        let shared = EntityId::new(11);
        let mut routing = TrainRouting {
            held_rails: vec![
                (TrainId::new(1), EntityId::new(10)),
                (TrainId::new(1), shared),
                (TrainId::new(4), shared),
                (TrainId::new(4), EntityId::new(20)),
            ],
            shared: vec![shared],
            ..TrainRouting::default()
        };

        routing.collect_exempt_rails(TrainId::new(1));
        assert_eq!(routing.exempt, vec![EntityId::new(10)]);

        routing.collect_exempt_rails(TrainId::new(4));
        assert_eq!(routing.exempt, vec![EntityId::new(20)]);
    }

    /// Planning resumes after the train it last reached and wraps round, so a
    /// train that spends the whole budget every tick cannot hold the queue
    /// behind it still.
    #[test]
    fn planning_resumes_after_the_train_it_last_reached() {
        let waiting = [TrainId::new(1), TrainId::new(4), TrainId::new(9)];
        let order = |planned_last| {
            planning_order(&waiting, planned_last)
                .map(|train_id| train_id.raw())
                .collect::<Vec<_>>()
        };

        assert_eq!(order(None), vec![1, 4, 9]);
        assert_eq!(order(Some(TrainId::new(1))), vec![4, 9, 1]);
        assert_eq!(order(Some(TrainId::new(9))), vec![1, 4, 9]);
        // A train that has since arrived, been mined, or been cut in two is no
        // longer waiting; the pass resumes at the next one that is.
        assert_eq!(order(Some(TrainId::new(5))), vec![9, 1, 4]);
        assert_eq!(order(Some(TrainId::new(99))), vec![1, 4, 9]);
    }
}
