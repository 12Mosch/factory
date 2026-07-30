//! Blocks: the rail graph cut into the sections a train reserves.
//!
//! A block is a maximal run of track joined without a signal standing between.
//! That is the whole definition, and everything else here follows from it:
//!
//! * **It is derived data over the graph, not a second graph.** The partition is
//!   a disjoint set over the same edges, unioned across the same joins, with the
//!   joins at signal positions left out. It is rebuilt exactly when the graph is,
//!   which is why placing or rotating a signal invalidates the graph rather than
//!   only the partition — one cache, one rebuild.
//! * **A block is named by the lowest rail entity id it contains.** Indices into
//!   the partition mean something else one placement later and nothing at all
//!   across a save; a train's claim is durable state and has to name what it
//!   holds by something the world itself carries.
//! * **A signal is a point and a heading, not a tile.** Which end of which rail
//!   a signal governs is answered once, when the partition is built, from the
//!   tile the signal stands on. Travel through that point the other way is
//!   governed by whatever signal faces the other side — or by nothing, which is
//!   what makes a single signal a one-way boundary.
//!
//! The cut is applied to the whole node rather than to the one join through it.
//! In any world placement can produce a node carries at most one join per
//! heading pair, because two ends at one point facing the same way are two
//! pieces laid over each other and placement refuses them; so cutting the node
//! and cutting its join are the same thing, and the node is the cheaper
//! statement.

use std::collections::{BTreeMap, BTreeSet};

use factory_data::RailSignalKind;

use crate::entities::Direction;
use crate::ids::EntityId;
use crate::rail::{RailBlockSnapshot, RailPoint, RailSignalSnapshot};
use crate::simulation::disjoint_set::DisjointSet;

use super::types::RailGraph;

/// One placed signal as the partition builder sees it: already resolved to the
/// rail end it stands at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) struct RailSignalInput {
    pub(in crate::simulation) entity_id: EntityId,
    pub(in crate::simulation) kind: RailSignalKind,
    pub(in crate::simulation) position: RailPoint,
    pub(in crate::simulation) heading: Direction,
}

/// One block of the partition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::simulation) struct RailBlock {
    pub(in crate::simulation) snapshot: RailBlockSnapshot,
    /// The rails in this block, ascending. Ascending rather than in travel
    /// order because a block has no travel order — it is a set of track, and the
    /// order a train runs it in is the route's business.
    pub(in crate::simulation) edges: Vec<EntityId>,
}

/// The rail graph cut into blocks at the signal positions.
///
/// Rebuilt with the graph and never saved. Blocks are held in ascending key
/// order, so every walk over the partition is a function of the world rather
/// than of iteration order.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct RailBlockPartition {
    blocks: Vec<RailBlock>,
    block_indices_by_key: BTreeMap<EntityId, usize>,
    block_indices_by_edge: BTreeMap<EntityId, usize>,
    /// Signals in ascending entity id order.
    signals: Vec<RailSignalSnapshot>,
    signal_indices_by_entity: BTreeMap<EntityId, usize>,
    /// The signal governing each crossing: a point together with the heading a
    /// train takes through it.
    signal_indices_by_crossing: BTreeMap<(RailPoint, Direction), usize>,
    /// Points that carry a signal at all, whichever way it faces. This is the
    /// set the cut is taken at, and it is deliberately not the same question as
    /// the one above: a boundary exists because a signal stands there, and
    /// whether a *particular* direction may cross it is decided separately.
    boundary_points: BTreeSet<RailPoint>,
}

impl RailBlockPartition {
    pub(in crate::simulation) fn blocks(&self) -> &[RailBlock] {
        &self.blocks
    }

    pub(in crate::simulation) fn block(&self, key: EntityId) -> Option<&RailBlock> {
        let index = *self.block_indices_by_key.get(&key)?;
        self.blocks.get(index)
    }

    /// The block a rail belongs to, or `None` when the rail is not track the
    /// partition knows about.
    pub(in crate::simulation) fn block_for_edge(&self, edge: EntityId) -> Option<&RailBlock> {
        let index = *self.block_indices_by_edge.get(&edge)?;
        self.blocks.get(index)
    }

    pub(in crate::simulation) fn block_key_for_edge(&self, edge: EntityId) -> Option<EntityId> {
        self.block_for_edge(edge).map(|block| block.snapshot.key)
    }

    pub(in crate::simulation) fn signals(&self) -> &[RailSignalSnapshot] {
        &self.signals
    }

    pub(in crate::simulation) fn signal(&self, entity_id: EntityId) -> Option<&RailSignalSnapshot> {
        let index = *self.signal_indices_by_entity.get(&entity_id)?;
        self.signals.get(index)
    }

    /// The signal a train crossing `position` while travelling `heading` has to
    /// pass, or `None` when nothing governs that crossing.
    pub(in crate::simulation) fn signal_for_crossing(
        &self,
        position: RailPoint,
        heading: Direction,
    ) -> Option<&RailSignalSnapshot> {
        let index = *self.signal_indices_by_crossing.get(&(position, heading))?;
        self.signals.get(index)
    }

    /// Whether `position` is a block boundary — which is to say whether any
    /// signal stands there.
    pub(in crate::simulation) fn is_boundary(&self, position: RailPoint) -> bool {
        self.boundary_points.contains(&position)
    }
}

/// Builds the block partition from the graph and the signals standing on it.
///
/// `signals` must be in ascending entity id order, which is what makes the
/// signal list — and therefore every tie broken over it — a function of the
/// world.
pub(in crate::simulation) fn build_rail_blocks(
    graph: &RailGraph,
    signals: &[RailSignalInput],
) -> RailBlockPartition {
    let mut partition = RailBlockPartition::default();
    if graph.edges.is_empty() {
        return partition;
    }

    let boundary_points = signals
        .iter()
        .map(|signal| signal.position)
        .collect::<BTreeSet<_>>();
    let point_by_node = node_points(graph);
    partition_blocks(graph, &point_by_node, &boundary_points, &mut partition);
    partition.boundary_points = boundary_points;
    resolve_signals(graph, &point_by_node, signals, &mut partition);
    partition
}

/// Where each node of the graph sits.
///
/// The graph does not store node positions — a node exists because two ends
/// resolved to one point, and where an end is remains the piece geometry's
/// answer — so the partition recovers them from the edges that touch each node.
/// Every node has at least one end on it, so every entry is filled.
fn node_points(graph: &RailGraph) -> Vec<RailPoint> {
    let mut points = vec![RailPoint::default(); graph.nodes.len()];
    for edge in &graph.edges {
        for end_index in 0..2 {
            points[edge.nodes[end_index]] = edge.end_positions[end_index];
        }
    }
    points
}

/// Groups edges into blocks: the graph's own connectivity solve with the joins
/// at signal positions left out.
fn partition_blocks(
    graph: &RailGraph,
    point_by_node: &[RailPoint],
    boundary_points: &BTreeSet<RailPoint>,
    partition: &mut RailBlockPartition,
) {
    let mut disjoint_set = DisjointSet::new(graph.edges.len());
    for (node_index, node) in graph.nodes.iter().enumerate() {
        if boundary_points.contains(&point_by_node[node_index]) {
            continue;
        }
        for (end_index, end) in node.ends.iter().enumerate() {
            for other in node.ends.iter().skip(end_index + 1) {
                let heading = graph.edges[end.edge_index].headings[end.end_index];
                let other_heading = graph.edges[other.edge_index].headings[other.end_index];
                if heading == other_heading.opposite() {
                    disjoint_set.union(end.edge_index, other.edge_index);
                }
            }
        }
    }

    let mut edges_by_key = BTreeMap::<EntityId, Vec<EntityId>>::new();
    for edge_indices in disjoint_set.components().into_values() {
        let mut edges = edge_indices
            .iter()
            .map(|index| graph.edges[*index].entity_id)
            .collect::<Vec<_>>();
        edges.sort_unstable();
        let key = *edges
            .first()
            .expect("a component holds at least one rail piece");
        edges_by_key.insert(key, edges);
    }

    for (key, edges) in edges_by_key {
        let total_length_fixed = edges
            .iter()
            .filter_map(|edge| graph.edge_for_entity(*edge))
            .fold(0_i64, |total, edge| total.saturating_add(edge.length_fixed));
        let index = partition.blocks.len();
        for edge in &edges {
            partition.block_indices_by_edge.insert(*edge, index);
        }
        partition.block_indices_by_key.insert(key, index);
        partition.blocks.push(RailBlock {
            snapshot: RailBlockSnapshot {
                key,
                piece_count: edges.len(),
                total_length_fixed,
                entry_signal_count: 0,
            },
            edges,
        });
    }
}

/// Works out which blocks each signal stands between, and counts the signals
/// that admit a train into each block.
///
/// A signal that no track approaches, or that guards nothing, is kept rather
/// than dropped: it still cuts the boundary it stands on, and a partition that
/// quietly forgot it would join two blocks the player has separated.
fn resolve_signals(
    graph: &RailGraph,
    point_by_node: &[RailPoint],
    signals: &[RailSignalInput],
    partition: &mut RailBlockPartition,
) {
    let mut node_by_point = BTreeMap::<RailPoint, usize>::new();
    for (node_index, point) in point_by_node.iter().enumerate() {
        node_by_point.insert(*point, node_index);
    }

    for signal in signals {
        let edge_at = |heading: Direction| {
            let node_index = *node_by_point.get(&signal.position)?;
            let node = graph.nodes.get(node_index)?;
            node.ends
                .iter()
                .find(|end| graph.edges[end.edge_index].headings[end.end_index] == heading)
                .map(|end| graph.edges[end.edge_index].entity_id)
        };
        // A train travelling `heading` leaves the rail whose end here faces that
        // way and enters the one whose end here faces back, because joined ends
        // oppose. That is the whole of the approach/guard asymmetry.
        let approach_block = edge_at(signal.heading).and_then(|edge| {
            partition
                .block_for_edge(edge)
                .map(|block| block.snapshot.key)
        });
        let guarded_block = edge_at(signal.heading.opposite()).and_then(|edge| {
            partition
                .block_for_edge(edge)
                .map(|block| block.snapshot.key)
        });

        let index = partition.signals.len();
        partition.signals.push(RailSignalSnapshot {
            entity_id: signal.entity_id,
            kind: signal.kind,
            position: signal.position,
            heading: signal.heading,
            approach_block,
            guarded_block,
        });
        partition
            .signal_indices_by_entity
            .insert(signal.entity_id, index);
        // Two signals governing one crossing cannot both be honoured, and
        // placement refuses the second. If one ever reaches here the lower
        // entity id keeps the crossing, so the partition is still a function of
        // the world rather than of which signal was written last.
        partition
            .signal_indices_by_crossing
            .entry((signal.position, signal.heading))
            .or_insert(index);

        if let Some(key) = guarded_block
            && let Some(block_index) = partition.block_indices_by_key.get(&key).copied()
        {
            partition.blocks[block_index].snapshot.entry_signal_count += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::rail_ops::graph_builder::build_rail_graph_from_pieces;
    use crate::simulation::rail_ops::test_graphs::{STRAIGHT_FIXED, straight_run};

    fn rail(raw: u64) -> EntityId {
        EntityId::new(raw)
    }

    /// The joint between rail `index` and rail `index + 1` of a north-running
    /// straight run, in world space.
    fn joint(index: i64) -> RailPoint {
        RailPoint::new(512, STRAIGHT_FIXED * index)
    }

    fn signal(entity_id: u64, position: RailPoint, kind: RailSignalKind) -> RailSignalInput {
        RailSignalInput {
            entity_id: rail(entity_id),
            kind,
            position,
            heading: Direction::North,
        }
    }

    /// Unsignalled track is one block however long it is: a boundary exists
    /// because a signal stands there, and nothing else creates one.
    #[test]
    fn unsignalled_track_is_a_single_block() {
        let graph = build_rail_graph_from_pieces(&straight_run(1, 4));

        let partition = build_rail_blocks(&graph, &[]);

        assert_eq!(partition.blocks().len(), 1);
        assert_eq!(partition.blocks()[0].snapshot.key, rail(1));
        assert_eq!(partition.blocks()[0].snapshot.piece_count, 4);
        assert_eq!(
            partition.blocks()[0].snapshot.total_length_fixed,
            4 * STRAIGHT_FIXED
        );
        assert_eq!(partition.block_key_for_edge(rail(3)), Some(rail(1)));
    }

    /// One signal in the middle of a run makes two blocks, each named by the
    /// lowest rail it holds.
    #[test]
    fn a_signal_cuts_the_run_it_stands_on() {
        let graph = build_rail_graph_from_pieces(&straight_run(1, 4));

        let partition = build_rail_blocks(&graph, &[signal(100, joint(2), RailSignalKind::Block)]);

        assert_eq!(partition.blocks().len(), 2);
        assert_eq!(partition.block_key_for_edge(rail(1)), Some(rail(1)));
        assert_eq!(partition.block_key_for_edge(rail(2)), Some(rail(1)));
        assert_eq!(partition.block_key_for_edge(rail(3)), Some(rail(3)));
        assert_eq!(partition.block_key_for_edge(rail(4)), Some(rail(3)));
    }

    /// The signal knows which way round it stands: the block it is approached
    /// from and the block it admits a train into are not interchangeable.
    #[test]
    fn a_signal_names_the_block_it_guards_and_the_one_it_is_approached_from() {
        let graph = build_rail_graph_from_pieces(&straight_run(1, 4));

        let partition = build_rail_blocks(&graph, &[signal(100, joint(2), RailSignalKind::Block)]);
        let signal = partition
            .signal(rail(100))
            .expect("the signal is in the partition");

        assert_eq!(signal.approach_block, Some(rail(1)));
        assert_eq!(signal.guarded_block, Some(rail(3)));
        assert_eq!(
            partition
                .block(rail(3))
                .expect("the far block exists")
                .snapshot
                .entry_signal_count,
            1
        );
        assert_eq!(
            partition
                .block(rail(1))
                .expect("the near block exists")
                .snapshot
                .entry_signal_count,
            0,
            "nothing admits a train into the first block, because nothing faces that way"
        );
    }

    /// A boundary is a boundary whichever way a signal faces, and travel the way
    /// it does not face is governed by nothing at all. That is what makes one
    /// signal a one-way boundary rather than a two-way one.
    #[test]
    fn a_boundary_exists_for_both_directions_but_only_one_is_governed() {
        let graph = build_rail_graph_from_pieces(&straight_run(1, 4));

        let partition = build_rail_blocks(&graph, &[signal(100, joint(2), RailSignalKind::Block)]);

        assert!(partition.is_boundary(joint(2)));
        assert!(
            partition
                .signal_for_crossing(joint(2), Direction::North)
                .is_some()
        );
        assert_eq!(
            partition.signal_for_crossing(joint(2), Direction::South),
            None
        );
        assert!(!partition.is_boundary(joint(1)));
    }

    /// Two signals facing opposite ways at one point are one boundary governed
    /// in both directions — the ordinary way a two-way line is signalled.
    #[test]
    fn signals_facing_both_ways_govern_one_boundary_twice() {
        let graph = build_rail_graph_from_pieces(&straight_run(1, 4));
        let southbound = RailSignalInput {
            heading: Direction::South,
            ..signal(101, joint(2), RailSignalKind::Block)
        };

        let partition = build_rail_blocks(
            &graph,
            &[signal(100, joint(2), RailSignalKind::Block), southbound],
        );

        assert_eq!(partition.blocks().len(), 2);
        let northbound = partition
            .signal_for_crossing(joint(2), Direction::North)
            .expect("a northbound signal stands here");
        let southbound = partition
            .signal_for_crossing(joint(2), Direction::South)
            .expect("a southbound signal stands here");
        assert_eq!(northbound.guarded_block, Some(rail(3)));
        assert_eq!(southbound.guarded_block, Some(rail(1)));
    }
}
