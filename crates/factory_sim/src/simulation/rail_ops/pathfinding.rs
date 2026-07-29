//! Route search over the rail graph.
//!
//! The shape is the enemy pathfinder's ([`crate::simulation::enemy`]): A* with a
//! preallocated scratch, a cap on the expansions one search may spend, and a
//! per-tick budget above it so a search can never spike a frame. What differs is
//! the space being searched, and the difference is the whole of this module.
//!
//! * **The node is a direction, not a place.** A train arriving at a junction
//!   cannot leave the way it came without stopping and reversing, so the search
//!   state is *an edge together with the end it is running toward* — `(edge,
//!   exit)` — and every rail is two states rather than one. A search over bare
//!   edges would happily route a train through a junction and straight back out
//!   of it, which is a manoeuvre no train can make.
//! * **Turning around is a move with a price.** Reversing is a transition from
//!   `(edge, exit)` to `(edge, other exit)`: the train runs to the end, stops,
//!   and comes back down the same rail. It costs that rail's length like any
//!   other traversal, plus [`TRAIN_REVERSAL_PENALTY_FIXED`], which is what stops
//!   the search from treating a there-and-back as free just because the track
//!   happens to be short.
//! * **The graph is small and the yards are dense.** Cost scales with junctions,
//!   not with track length: a hundred-tile main line is a handful of expansions
//!   and a yard of the same length can be hundreds. That is why the cap is on
//!   expansions rather than on distance.
//!
//! Nothing here allocates. The scratch lives on the routing subsystem across
//! searches, and the only thing that comes back with an allocation of its own is
//! the route itself, which is durable state a train then owns.
//!
//! Ties are broken by state index — edge order, which the graph builder takes
//! from entity id order — so two routes of equal cost always resolve the same
//! way, on every machine and every replay.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use crate::ids::EntityId;
use crate::rail::RailPoint;
use crate::rolling_stock::{RailPosition, RailTarget, TrainRoute, TrainRouteLeg};

use super::types::{RailEdge, RailGraph};

/// Marks a state nothing led to: the seeds the search starts from, and every
/// state it never reached.
const NO_PREDECESSOR: u32 = u32::MAX;

/// What a train is asking for, and what the search may spend answering.
pub(in crate::simulation) struct RailRouteRequest<'graph> {
    pub(in crate::simulation) graph: &'graph RailGraph,
    /// Where the train is now, including which way it faces: a route that
    /// starts by driving forwards and one that starts by reversing out are
    /// different routes with different costs.
    pub(in crate::simulation) start: RailPosition,
    pub(in crate::simulation) target: RailTarget,
    pub(in crate::simulation) reversal_penalty_fixed: i64,
    pub(in crate::simulation) occupied_penalty_fixed: i64,
    /// Rails something is standing on, ascending. Sorted because the search asks
    /// about them by binary search rather than by building a set it would have
    /// to allocate.
    pub(in crate::simulation) occupied: &'graph [EntityId],
    /// Rails the searching train is itself standing on, ascending. Occupancy is
    /// about what is in the way, and a train is never in its own way — so these
    /// are the rails the penalty above is not charged for.
    ///
    /// Kept apart from `occupied` rather than subtracted out of it because the
    /// occupancy is gathered once for the whole tick and read by every search,
    /// while what to leave out of it differs for each of them.
    pub(in crate::simulation) exempt: &'graph [EntityId],
    pub(in crate::simulation) max_expansions: usize,
}

/// How a route search ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::simulation) enum RailRouteOutcome {
    Found(TrainRoute),
    /// No route exists: the destination is on other track, or the rail named by
    /// either end of the request is no longer there.
    Unreachable,
    /// The search ran out of expansions before it could answer. Distinct from
    /// [`RailRouteOutcome::Unreachable`] because it says nothing about whether a
    /// route exists — only that this railway is larger than one search may
    /// walk.
    Exhausted,
}

/// The buffers one search reuses.
///
/// Sized by the graph on every call and never freed between them, so a search
/// on a settled railway allocates nothing at all. `best_cost` and `came_from`
/// are indexed by state — `edge_index * 2 + exit` — rather than keyed by one,
/// which is what keeps the inner loop free of a map lookup.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct RailRouteScratch {
    open: BinaryHeap<Reverse<(i64, i64, u32)>>,
    best_cost: Vec<i64>,
    came_from: Vec<u32>,
    /// States of the found route, goal first, before it is turned into legs.
    trail: Vec<u32>,
}

/// The cheapest way found so far of arriving on the destination rail.
///
/// Arrival is priced when a route *enters* the destination rather than when it
/// has run the whole of it, because a train stops at the mark rather than at the
/// far end: entering a rail from one end and from the other are two different
/// distances to the same point, and a search that ignored that would sometimes
/// pick the wrong end to come in from.
#[derive(Clone, Copy, Debug)]
struct RailArrival {
    predecessor: u32,
    /// Which end of the destination rail the route comes in through.
    entry_end: usize,
    cost: i64,
}

impl RailRouteScratch {
    /// Searches for a route, and reports what it spent doing so.
    ///
    /// The expansion count is returned even when nothing was found: it is what
    /// the caller charges against the tick's budget, and a failed search is not
    /// a free one.
    pub(in crate::simulation) fn find_route(
        &mut self,
        request: &RailRouteRequest<'_>,
    ) -> (RailRouteOutcome, usize) {
        let graph = request.graph;
        let (Some(&start_index), Some(&target_index)) = (
            graph.edge_indices_by_entity.get(&request.start.edge),
            graph.edge_indices_by_entity.get(&request.target.edge),
        ) else {
            return (RailRouteOutcome::Unreachable, 0);
        };
        let start_edge = graph.edges[start_index];
        let target_edge = graph.edges[target_index];
        // Track that is not joined to the train's own is not track it can reach,
        // and answering that from the network ids costs one comparison. Without
        // it the search would have to exhaust everything reachable — the one
        // case where a bounded search cannot tell "no" from "not yet".
        if start_edge.network_id != target_edge.network_id {
            return (RailRouteOutcome::Unreachable, 0);
        }
        // A train sent to the rail it is already standing on has a route without
        // a search: the stretch of that rail between it and the mark. That is
        // only the *cheapest* route when it needs no reversal, though — turning
        // round costs a penalty a way round the railway might not pay — so a
        // direct route that reverses is kept as the one to beat and priced
        // against whatever the search comes back with.
        let direct = match start_index == target_index {
            false => None,
            true => {
                let direct = route_along_one_rail(
                    request.start,
                    request.target,
                    request.reversal_penalty_fixed,
                );
                if !direct.reverses {
                    return (RailRouteOutcome::Found(direct.route), 0);
                }
                Some(direct)
            }
        };

        self.reset(graph.edges.len());
        self.seed(request, start_index, &start_edge, &target_edge);

        let mut expansions = 0;
        let mut exhausted = false;
        let mut arrival: Option<RailArrival> = None;
        while let Some(Reverse((estimate, cost, state))) = self.open.pop() {
            // Every route still open costs at least its estimate, so once that
            // reaches what getting there already costs — by arriving, or by
            // turning round on the spot — nothing left can improve on it.
            let best_known = arrival
                .map(|found| found.cost)
                .into_iter()
                .chain(direct.as_ref().map(|direct| direct.cost))
                .min();
            if best_known.is_some_and(|limit| estimate >= limit) {
                break;
            }
            if cost > self.best_cost[state as usize] {
                continue;
            }
            if expansions == request.max_expansions {
                exhausted = true;
                break;
            }
            expansions += 1;

            let (edge_index, exit_end) = decode(state);
            let edge = graph.edges[edge_index];
            // Onward through everything joined to the end being run toward, and
            // then back down this very rail, which is what a reversal is.
            let onward = graph
                .neighbor_ends(&edge, exit_end)
                .map(|(next_index, arrival_end)| (next_index, arrival_end, 0));
            for (next_index, entry_end, penalty) in onward.chain(std::iter::once((
                edge_index,
                exit_end,
                request.reversal_penalty_fixed,
            ))) {
                self.relax(
                    request,
                    &target_edge,
                    target_index,
                    state,
                    cost + penalty,
                    next_index,
                    entry_end,
                    &mut arrival,
                );
            }
        }

        // The way round only wins if it really costs less than turning round on
        // the spot; a tie goes to the direct route, which is the shorter
        // description of the same journey.
        let arrival = arrival.filter(|found| {
            direct
                .as_ref()
                .is_none_or(|direct| found.cost < direct.cost)
        });
        match (arrival, direct) {
            (Some(arrival), _) => (
                RailRouteOutcome::Found(self.rebuild(request, &start_edge, &target_edge, arrival)),
                expansions,
            ),
            // A search that ran out while a route was already in hand has still
            // found one: it only failed to prove that nothing beats it.
            (None, Some(direct)) => (RailRouteOutcome::Found(direct.route), expansions),
            (None, None) if exhausted => (RailRouteOutcome::Exhausted, expansions),
            (None, None) => (RailRouteOutcome::Unreachable, expansions),
        }
    }

    fn reset(&mut self, edge_count: usize) {
        let state_count = edge_count * 2;
        self.open.clear();
        self.best_cost.clear();
        self.best_cost.resize(state_count, i64::MAX);
        self.came_from.clear();
        self.came_from.resize(state_count, NO_PREDECESSOR);
        self.trail.clear();
    }

    /// Opens the search with the two ways a train can leave where it stands:
    /// forwards, and — for the price of a reversal — backwards.
    ///
    /// Both seeds sit on the rail the train is already on, so neither pays that
    /// rail's occupancy penalty: the train is what is standing on it.
    fn seed(
        &mut self,
        request: &RailRouteRequest<'_>,
        start_index: usize,
        start_edge: &RailEdge,
        target_edge: &RailEdge,
    ) {
        for exit_end in [0, 1] {
            let reversal = if exit_end == usize::from(request.start.forward) {
                0
            } else {
                request.reversal_penalty_fixed
            };
            let cost =
                distance_to_end(start_edge, request.start.distance_fixed, exit_end) + reversal;
            let state = encode(start_index, exit_end);
            self.best_cost[state as usize] = cost;
            self.open.push(Reverse((
                cost + heuristic(start_edge, exit_end, target_edge),
                cost,
                state,
            )));
        }
    }

    /// Prices one step out of `state` and either records an arrival or opens the
    /// state it leads to.
    #[allow(clippy::too_many_arguments)]
    fn relax(
        &mut self,
        request: &RailRouteRequest<'_>,
        target_edge: &RailEdge,
        target_index: usize,
        state: u32,
        cost: i64,
        next_index: usize,
        entry_end: usize,
        arrival: &mut Option<RailArrival>,
    ) {
        let next = request.graph.edges[next_index];
        let penalty = if request.occupied.binary_search(&next.entity_id).is_ok()
            && request.exempt.binary_search(&next.entity_id).is_err()
        {
            request.occupied_penalty_fixed
        } else {
            0
        };

        if next_index == target_index {
            // The destination is where the route ends, so it is priced to the
            // mark and never opened: a route that ran on past it would only be
            // a longer way of getting there.
            let cost = cost
                + distance_from_end(target_edge, request.target.distance_fixed, entry_end)
                + penalty;
            let candidate = RailArrival {
                predecessor: state,
                entry_end,
                cost,
            };
            if arrival.is_none_or(|found| {
                (candidate.cost, candidate.predecessor, candidate.entry_end)
                    < (found.cost, found.predecessor, found.entry_end)
            }) {
                *arrival = Some(candidate);
            }
            return;
        }

        let cost = cost + next.length_fixed + penalty;
        // Entering through one end means running toward the other.
        let next_state = encode(next_index, 1 - entry_end);
        if cost < self.best_cost[next_state as usize] {
            self.best_cost[next_state as usize] = cost;
            self.came_from[next_state as usize] = state;
            self.open.push(Reverse((
                cost + heuristic(&next, 1 - entry_end, target_edge),
                cost,
                next_state,
            )));
        }
    }

    /// Turns the states the search settled on into the legs a train drives.
    ///
    /// Walking the predecessors backwards gives the rails in reverse order; the
    /// legs are then cut where the route runs back down the rail it just ran up,
    /// which is the only thing a reversal can look like.
    fn rebuild(
        &mut self,
        request: &RailRouteRequest<'_>,
        start_edge: &RailEdge,
        target_edge: &RailEdge,
        arrival: RailArrival,
    ) -> TrainRoute {
        let graph = request.graph;
        self.trail.clear();
        let mut state = arrival.predecessor;
        loop {
            self.trail.push(state);
            let predecessor = self.came_from[state as usize];
            if predecessor == NO_PREDECESSOR {
                break;
            }
            state = predecessor;
        }
        self.trail.reverse();

        let (_, seed_exit) = decode(self.trail[0]);
        let mut legs = VecDeque::new();
        let mut edges = Vec::with_capacity(self.trail.len() + 1);
        // The first leg drives forwards exactly when the search left down the
        // end the train is already facing.
        let mut forward = seed_exit == usize::from(request.start.forward);
        let mut distance_fixed =
            distance_to_end(start_edge, request.start.distance_fixed, seed_exit);
        edges.push(start_edge.entity_id);

        for pair in self.trail.windows(2) {
            let (previous_index, _) = decode(pair[0]);
            let (next_index, _) = decode(pair[1]);
            let next = graph.edges[next_index];
            edges.push(next.entity_id);
            if next_index == previous_index {
                legs.push_back(TrainRouteLeg {
                    distance_fixed,
                    forward,
                });
                forward = !forward;
                distance_fixed = next.length_fixed;
            } else {
                distance_fixed += next.length_fixed;
            }
        }

        // The destination rail is entered but never traversed: the train stops
        // on it, at the mark.
        edges.push(target_edge.entity_id);
        distance_fixed += distance_from_end(
            target_edge,
            request.target.distance_fixed,
            arrival.entry_end,
        );
        legs.push_back(TrainRouteLeg {
            distance_fixed,
            forward,
        });

        TrainRoute { legs, edges }
    }
}

/// The route for a train already standing on the rail it was sent to, and what
/// it costs.
///
/// `reverses` is what decides whether the search has to run at all. A mark ahead
/// of the train is reached by driving to it and nothing else can be shorter —
/// any other route runs the rest of this rail first and then more. A mark behind
/// it is a different matter: driving to it means turning round, and turning
/// round has a price that a way round the railway need not pay.
struct DirectRoute {
    route: TrainRoute,
    cost: i64,
    reverses: bool,
}

fn route_along_one_rail(
    start: RailPosition,
    target: RailTarget,
    reversal_penalty_fixed: i64,
) -> DirectRoute {
    let delta = target.distance_fixed - start.distance_fixed;
    let forward = (delta >= 0) == start.forward;
    DirectRoute {
        route: TrainRoute {
            legs: VecDeque::from([TrainRouteLeg {
                distance_fixed: delta.abs(),
                forward,
            }]),
            edges: vec![start.edge],
        },
        cost: delta.abs() + if forward { 0 } else { reversal_penalty_fixed },
        reverses: !forward,
    }
}

/// Track left between a point on `edge` and the end being run toward.
fn distance_to_end(edge: &RailEdge, distance_fixed: i64, end_index: usize) -> i64 {
    if end_index == 1 {
        edge.length_fixed - distance_fixed
    } else {
        distance_fixed
    }
}

/// Track between the end a route enters `edge` through and a point on it.
fn distance_from_end(edge: &RailEdge, distance_fixed: i64, end_index: usize) -> i64 {
    if end_index == 0 {
        distance_fixed
    } else {
        edge.length_fixed - distance_fixed
    }
}

/// A lower bound on the track left between the end of a traversal and the
/// destination.
///
/// The straight line to the *nearer end of the destination rail*, rather than to
/// the mark itself. A route can only arrive on a rail through one of its ends,
/// so the distance to whichever is nearer can never exceed the track that is
/// actually left — which is what makes the estimate admissible, and therefore
/// what makes the first arrival the search settles on the cheapest one. Straight
/// track is exactly its chord and a curve is longer than its own, so no rail can
/// undercut it either.
fn heuristic(edge: &RailEdge, exit_end: usize, target: &RailEdge) -> i64 {
    let from = edge.end_positions[exit_end];
    straight_line(from, target.end_positions[0]).min(straight_line(from, target.end_positions[1]))
}

/// Straight-line distance in fixed-point units, widened before squaring so a
/// railway in a far-off chunk cannot wrap, and rounded down so the estimate
/// stays a lower bound.
fn straight_line(from: RailPoint, to: RailPoint) -> i64 {
    let dx = i128::from(from.x) - i128::from(to.x);
    let dy = i128::from(from.y) - i128::from(to.y);
    (dx * dx + dy * dy).isqrt() as i64
}

impl RailRouteScratch {
    /// What each buffer has room for, which is what a second search must not
    /// have to change.
    #[cfg(test)]
    fn capacities(&self) -> [usize; 4] {
        [
            self.open.capacity(),
            self.best_cost.capacity(),
            self.came_from.capacity(),
            self.trail.capacity(),
        ]
    }
}

const fn encode(edge_index: usize, exit_end: usize) -> u32 {
    (edge_index * 2 + exit_end) as u32
}

const fn decode(state: u32) -> (usize, usize) {
    (state as usize / 2, state as usize % 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Direction;
    use crate::ids::EntityId;
    use crate::rolling_stock::{TRAIN_OCCUPIED_RAIL_PENALTY_FIXED, TRAIN_REVERSAL_PENALTY_FIXED};
    use crate::simulation::rail_ops::graph_builder::build_rail_graph_from_pieces;
    use crate::simulation::rail_ops::test_graphs::{
        CORNER_FIXED, STRAIGHT_FIXED, piece, straight, straight_run,
    };

    fn rail(entity_id: u64) -> EntityId {
        EntityId::new(entity_id)
    }

    fn request<'graph>(
        graph: &'graph RailGraph,
        start: RailPosition,
        target: RailTarget,
    ) -> RailRouteRequest<'graph> {
        RailRouteRequest {
            graph,
            start,
            target,
            reversal_penalty_fixed: TRAIN_REVERSAL_PENALTY_FIXED,
            occupied_penalty_fixed: TRAIN_OCCUPIED_RAIL_PENALTY_FIXED,
            occupied: &[],
            exempt: &[],
            max_expansions: 4_096,
        }
    }

    fn search(request: &RailRouteRequest<'_>) -> (RailRouteOutcome, usize) {
        RailRouteScratch::default().find_route(request)
    }

    fn route(request: &RailRouteRequest<'_>) -> TrainRoute {
        match search(request).0 {
            RailRouteOutcome::Found(route) => route,
            other => panic!("expected a route, found {other:?}"),
        }
    }

    fn legs(route: &TrainRoute) -> Vec<(i64, bool)> {
        route
            .legs
            .iter()
            .map(|leg| (leg.distance_fixed, leg.forward))
            .collect()
    }

    fn edges(route: &TrainRoute) -> Vec<u64> {
        route.edges.iter().map(|edge| edge.raw()).collect()
    }

    /// A stem with two branches off its far end, the second of them a corner.
    ///
    /// The branches meet the stem but not each other: both leave the junction on
    /// the same side of it, so their ends there face the same way and the rule
    /// that joins ends does not join them. That is what makes this a junction
    /// rather than a run — and getting from one branch to the other means going
    /// back down the stem.
    fn junction_graph() -> RailGraph {
        let junction = RailPoint::new(512, STRAIGHT_FIXED);
        build_rail_graph_from_pieces(&[
            straight(1, RailPoint::new(512, 0), Direction::North, STRAIGHT_FIXED),
            straight(2, junction, Direction::North, STRAIGHT_FIXED),
            piece(
                3,
                junction,
                Direction::North,
                RailPoint::new(2_048, 3_584),
                Direction::East,
                CORNER_FIXED,
            ),
        ])
    }

    /// Two branches of the same length between the same pair of junctions, so
    /// the two routes across them cost exactly the same.
    ///
    /// Laid over each other, which placement refuses and a player could never
    /// build. What is under test is which of two equal routes the search
    /// settles on, and a fixture that made them differ by a unit of track would
    /// be testing arithmetic instead.
    fn parallel_branches_graph() -> RailGraph {
        let first = RailPoint::new(512, STRAIGHT_FIXED);
        let second = RailPoint::new(512, 2 * STRAIGHT_FIXED);
        build_rail_graph_from_pieces(&[
            straight(1, RailPoint::new(512, 0), Direction::North, STRAIGHT_FIXED),
            straight(2, first, Direction::North, STRAIGHT_FIXED),
            straight(3, first, Direction::North, STRAIGHT_FIXED),
            straight(4, second, Direction::North, STRAIGHT_FIXED),
        ])
    }

    /// A closed loop of four straights and four corners, laid out the way track
    /// that comes back on itself really is: every corner is longer than the line
    /// across its own ends.
    fn loop_graph() -> RailGraph {
        build_rail_graph_from_pieces(&[
            straight(1, RailPoint::new(512, 0), Direction::North, STRAIGHT_FIXED),
            piece(
                2,
                RailPoint::new(512, 2_048),
                Direction::North,
                RailPoint::new(2_048, 3_584),
                Direction::East,
                CORNER_FIXED,
            ),
            straight(
                3,
                RailPoint::new(2_048, 3_584),
                Direction::East,
                STRAIGHT_FIXED,
            ),
            piece(
                4,
                RailPoint::new(4_096, 3_584),
                Direction::East,
                RailPoint::new(5_632, 2_048),
                Direction::South,
                CORNER_FIXED,
            ),
            straight(
                5,
                RailPoint::new(5_632, 2_048),
                Direction::South,
                STRAIGHT_FIXED,
            ),
            piece(
                6,
                RailPoint::new(5_632, 0),
                Direction::South,
                RailPoint::new(4_096, -1_536),
                Direction::West,
                CORNER_FIXED,
            ),
            straight(
                7,
                RailPoint::new(4_096, -1_536),
                Direction::West,
                STRAIGHT_FIXED,
            ),
            piece(
                8,
                RailPoint::new(2_048, -1_536),
                Direction::West,
                RailPoint::new(512, 0),
                Direction::North,
                CORNER_FIXED,
            ),
        ])
    }

    #[test]
    fn a_route_down_a_run_is_one_leg_of_the_track_between() {
        let graph = build_rail_graph_from_pieces(&straight_run(1, 4));
        let start = RailPosition::new(rail(1), 512, true);
        let target = RailTarget::new(rail(4), 1_024);

        let route = route(&request(&graph, start, target));

        // Out of the first rail, over the two between, and a thousand units into
        // the fourth.
        assert_eq!(
            legs(&route),
            vec![(STRAIGHT_FIXED - 512 + 2 * STRAIGHT_FIXED + 1_024, true)]
        );
        assert_eq!(edges(&route), vec![1, 2, 3, 4]);
    }

    /// A destination behind the train is one leg driven backwards, not a
    /// there-and-back: reversing is something a train does where it stands.
    #[test]
    fn a_destination_behind_the_train_is_driven_in_reverse() {
        let graph = build_rail_graph_from_pieces(&straight_run(1, 4));
        let start = RailPosition::new(rail(4), 512, true);
        let target = RailTarget::new(rail(1), 1_024);

        let route = route(&request(&graph, start, target));

        assert_eq!(
            legs(&route),
            vec![(512 + 2 * STRAIGHT_FIXED + (STRAIGHT_FIXED - 1_024), false)]
        );
        assert_eq!(edges(&route), vec![4, 3, 2, 1]);
    }

    /// A mark ahead of the train on the rail it is standing on is reached by
    /// driving to it, and no search is needed to know that: every other route
    /// runs the rest of this rail first and then more. Answering it for nothing
    /// is what keeps a train sent where it stands off the tick's budget.
    #[test]
    fn a_destination_ahead_on_the_rail_underneath_needs_no_search() {
        let graph = build_rail_graph_from_pieces(&straight_run(1, 2));
        let start = RailPosition::new(rail(1), 500, true);

        let (outcome, expansions) =
            search(&request(&graph, start, RailTarget::new(rail(1), 1_500)));

        assert_eq!(expansions, 0);
        let RailRouteOutcome::Found(route) = outcome else {
            panic!("a train on its destination rail has a route");
        };
        assert_eq!(legs(&route), vec![(1_000, true)]);
        assert_eq!(edges(&route), vec![1]);
    }

    /// A mark *behind* the train is a different question, because reaching it
    /// means turning round. On a line that goes nowhere else there is nothing
    /// cheaper, so the answer is still the stretch of rail between the two.
    #[test]
    fn a_destination_behind_on_the_rail_underneath_is_driven_back_to() {
        let graph = build_rail_graph_from_pieces(&straight_run(1, 2));
        let start = RailPosition::new(rail(1), 1_500, true);

        let route = route(&request(&graph, start, RailTarget::new(rail(1), 500)));

        assert_eq!(legs(&route), vec![(1_000, false)]);
        assert_eq!(edges(&route), vec![1]);
    }

    /// The same question on a loop has a different answer: going round costs
    /// less than the reversal does, so the train drives on past the end of its
    /// own rail and comes back to the mark from the other side.
    #[test]
    fn a_destination_behind_on_a_loop_is_reached_by_going_round() {
        let graph = loop_graph();
        let start = RailPosition::new(rail(1), 1_500, true);

        let route = route(&request(&graph, start, RailTarget::new(rail(1), 500)));

        assert_eq!(route.legs.len(), 1, "going round needs no reversal");
        assert!(route.current_leg().expect("a route has a leg").forward);
        assert_eq!(edges(&route), vec![1, 2, 3, 4, 5, 6, 7, 8, 1]);
    }

    /// The manoeuvre the whole direction-aware state space exists for: a train
    /// on one branch cannot swing straight onto the other, so it runs down the
    /// stem, stops, and backs onto it.
    #[test]
    fn crossing_between_two_branches_runs_out_and_backs_in() {
        let graph = junction_graph();
        let start = RailPosition::new(rail(2), 1_024, false);
        let target = RailTarget::new(rail(3), 1_206);

        let route = route(&request(&graph, start, target));

        assert_eq!(
            legs(&route),
            vec![
                (1_024 + STRAIGHT_FIXED, true),
                (STRAIGHT_FIXED + 1_206, false),
            ]
        );
        // The stem appears twice because the route runs down it and back up it.
        assert_eq!(edges(&route), vec![2, 1, 1, 3]);
    }

    /// Going round is cheaper than turning round: the loop is far shorter than
    /// what a reversal costs, so the train keeps driving forwards past its own
    /// starting point rather than backing up to a destination just behind it.
    #[test]
    fn a_short_loop_is_preferred_to_turning_round() {
        let graph = loop_graph();
        let start = RailPosition::new(rail(1), 1_024, true);
        let target = RailTarget::new(rail(7), 1_024);

        let route = route(&request(&graph, start, target));

        assert_eq!(route.legs.len(), 1);
        assert!(route.current_leg().expect("a route has a leg").forward);
        assert_eq!(edges(&route), vec![1, 2, 3, 4, 5, 6, 7]);
    }

    /// The same railway and the same train, priced with a reversal that costs
    /// about one rail instead of a hundred tiles: now backing up is the shorter
    /// way round, which is the reversal penalty doing the only job it has.
    #[test]
    fn a_cheap_reversal_sends_the_train_back_rather_than_round() {
        let graph = loop_graph();
        let start = RailPosition::new(rail(1), 1_024, true);
        let target = RailTarget::new(rail(7), 1_024);

        let route = route(&RailRouteRequest {
            reversal_penalty_fixed: STRAIGHT_FIXED,
            ..request(&graph, start, target)
        });

        assert_eq!(route.legs.len(), 1);
        assert!(!route.current_leg().expect("a route has a leg").forward);
        assert_eq!(edges(&route), vec![1, 8, 7]);
    }

    /// Two routes of exactly equal cost resolve to the lower rail, every time.
    /// The graph builder numbers edges in the order it is handed the pieces, and
    /// the simulation hands them over in entity id order, so this is the tie
    /// being broken by the world rather than by iteration.
    #[test]
    fn equal_cost_branches_resolve_the_same_way_every_time() {
        let start = RailPosition::new(rail(1), 1_024, true);
        let target = RailTarget::new(rail(4), 1_024);

        for _ in 0..2 {
            let graph = parallel_branches_graph();
            let route = route(&request(&graph, start, target));
            assert_eq!(edges(&route), vec![1, 2, 4]);
        }
    }

    /// Track someone else is standing on costs more to plan over, so an
    /// otherwise equal branch beside it wins.
    #[test]
    fn a_rail_under_another_train_pushes_the_route_onto_the_branch_beside_it() {
        let graph = parallel_branches_graph();
        let start = RailPosition::new(rail(1), 1_024, true);
        let target = RailTarget::new(rail(4), 1_024);

        let route = route(&RailRouteRequest {
            occupied: &[rail(2)],
            ..request(&graph, start, target)
        });

        assert_eq!(edges(&route), vec![1, 3, 4]);
    }

    /// A train is not in its own way. The rails it is standing on are occupied
    /// — by it — so without the exemption a long train would be steered off its
    /// own branch by the penalty for being where it already is.
    #[test]
    fn a_rail_the_searching_train_stands_on_costs_it_nothing() {
        let graph = parallel_branches_graph();
        let start = RailPosition::new(rail(1), 1_024, true);
        let target = RailTarget::new(rail(4), 1_024);

        let route = route(&RailRouteRequest {
            occupied: &[rail(2)],
            exempt: &[rail(2)],
            ..request(&graph, start, target)
        });

        assert_eq!(edges(&route), vec![1, 2, 4]);
    }

    /// Track that is not joined to the train's own is answered without a search
    /// at all: a bounded search could not tell "there is no way there" from "I
    /// did not finish looking".
    #[test]
    fn a_destination_on_a_separate_railway_is_unreachable() {
        let mut pieces = straight_run(1, 2);
        pieces.push(straight(
            3,
            RailPoint::new(8_192, 0),
            Direction::North,
            STRAIGHT_FIXED,
        ));
        let graph = build_rail_graph_from_pieces(&pieces);

        let (outcome, expansions) = search(&request(
            &graph,
            RailPosition::new(rail(1), 0, true),
            RailTarget::new(rail(3), 1_024),
        ));

        assert_eq!(outcome, RailRouteOutcome::Unreachable);
        assert_eq!(expansions, 0);
    }

    /// A search that runs out of expansions says so rather than reporting no
    /// route, and it stops at its cap rather than overrunning it.
    #[test]
    fn a_search_that_runs_out_of_expansions_reports_it() {
        let graph = build_rail_graph_from_pieces(&straight_run(1, 32));

        let (outcome, expansions) = search(&RailRouteRequest {
            max_expansions: 2,
            ..request(
                &graph,
                RailPosition::new(rail(1), 0, true),
                RailTarget::new(rail(32), 1_024),
            )
        });

        assert_eq!(outcome, RailRouteOutcome::Exhausted);
        assert_eq!(expansions, 2);
    }

    /// The property the scratch exists for: once a search has run on a railway,
    /// searching it again asks nothing of the allocator. Capacity is the exact
    /// statement of that — a buffer that did not grow was not reallocated —
    /// where an allocation count measured through a whole tick could only be an
    /// approximate one.
    #[test]
    fn a_second_search_reuses_the_first_one_s_buffers() {
        let graph = loop_graph();
        let mut scratch = RailRouteScratch::default();
        let first = request(
            &graph,
            RailPosition::new(rail(1), 1_024, true),
            RailTarget::new(rail(5), 1_024),
        );
        scratch.find_route(&first);
        let capacities = scratch.capacities();

        // A different journey over the same railway, so the second search is a
        // real one rather than a repeat that could be answered from anything
        // left lying about.
        scratch.find_route(&request(
            &graph,
            RailPosition::new(rail(3), 512, false),
            RailTarget::new(rail(7), 1_024),
        ));

        assert_eq!(scratch.capacities(), capacities);
    }

    /// The estimate that guides the search must never exceed the track that is
    /// really left, or the first arrival it settles on would not be the cheapest
    /// one. Every rail is at least as long as the line across its own ends, so
    /// the distance to the nearer end of the destination can only undercount.
    #[test]
    fn the_estimate_never_exceeds_the_route_it_estimates() {
        let graph = loop_graph();
        let target = graph
            .edge_for_entity(rail(5))
            .copied()
            .expect("the loop holds the destination rail");

        for edge in &graph.edges {
            for exit_end in [0, 1] {
                let estimate = heuristic(edge, exit_end, &target);
                let route = route(&request(
                    &graph,
                    // Standing at the very end of this rail, facing out of it,
                    // is where its estimate is measured from.
                    RailPosition::new(edge.entity_id, edge.length_fixed * exit_end as i64, true),
                    RailTarget::new(rail(5), target.length_fixed / 2),
                ));
                let travelled = route.legs.iter().map(|leg| leg.distance_fixed).sum::<i64>();
                assert!(
                    estimate <= travelled,
                    "rail {} end {exit_end} estimates {estimate} against {travelled} of track",
                    edge.entity_id.raw()
                );
            }
        }
    }
}
