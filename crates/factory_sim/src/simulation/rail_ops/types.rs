use std::collections::BTreeMap;

use crate::entities::Direction;
use crate::ids::EntityId;
use crate::rail::{RailNetworkSnapshot, RailPieceGeometry, RailPoint};
use crate::simulation::SmallVec;

/// One placed rail as the graph builder sees it. Every rail piece owns exactly
/// one edge, so the entity id is the edge's identity.
#[derive(Clone, Copy, Debug)]
pub(super) struct RailPieceInput {
    pub(super) entity_id: EntityId,
    pub(super) geometry: RailPieceGeometry,
}

/// One rail piece as an edge of the graph.
///
/// The edge is undirected — a train may run a piece either way — but it
/// remembers the heading at each end, which is what makes it directed-capable:
/// arriving at `nodes[0]` means arriving travelling `headings[0]`, and leaving
/// the far end means leaving travelling `headings[1]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) struct RailEdge {
    pub(in crate::simulation) entity_id: EntityId,
    pub(in crate::simulation) nodes: [usize; 2],
    pub(in crate::simulation) headings: [Direction; 2],
    /// Where each end sits in the world, copied from the piece's geometry when
    /// the graph is built.
    ///
    /// Here for the same reason `length_fixed` is: a route search needs a
    /// metric over the graph — how far apart two ends are in a straight line is
    /// what makes its heuristic a lower bound on the track between them — and
    /// resolving that from the placed entity and its prototype on every node
    /// expansion would put a catalog lookup and a rotation in the search's inner
    /// loop. The graph is rebuilt wholesale whenever placement changes, so this
    /// is a copy that cannot outlive the geometry it came from.
    pub(in crate::simulation) end_positions: [RailPoint; 2],
    pub(in crate::simulation) length_fixed: i64,
    pub(in crate::simulation) network_id: u32,
}

/// One junction of the graph: the rail ends that meet at a single point.
///
/// The point itself is not stored. A node exists precisely because two ends
/// resolved to the same position, and where an end is remains a question for the
/// piece's geometry, which is the one description of that; keeping a second copy
/// here is how the two would eventually disagree.
///
/// `ends` names the edges that touch this node and which of their two ends does
/// the touching. Ends only *connect* when their headings oppose, so the node is
/// where that rule is applied rather than a promise that everything touching it
/// is joined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::simulation) struct RailNode {
    pub(in crate::simulation) ends: SmallVec<[RailNodeEnd; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) struct RailNodeEnd {
    pub(in crate::simulation) edge_index: usize,
    /// Which of the edge's two ends meets here.
    pub(in crate::simulation) end_index: usize,
}

/// The rail graph: nodes at rail ends, one edge per placed piece, grouped into
/// connected networks.
///
/// Entirely derived from the placed rails. It is rebuilt when placement changes
/// and never saved.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct RailGraph {
    pub(in crate::simulation) nodes: Vec<RailNode>,
    pub(in crate::simulation) edges: Vec<RailEdge>,
    pub(in crate::simulation) networks: Vec<RailNetworkSnapshot>,
    pub(in crate::simulation) edge_indices_by_entity: BTreeMap<EntityId, usize>,
}

impl RailGraph {
    pub(in crate::simulation) fn edge_for_entity(&self, entity_id: EntityId) -> Option<&RailEdge> {
        let index = *self.edge_indices_by_entity.get(&entity_id)?;
        self.edges.get(index)
    }

    /// The rail joined to `end_index` of `edge`, or `None` for a free end.
    pub(in crate::simulation) fn neighbor(
        &self,
        edge: &RailEdge,
        end_index: usize,
    ) -> Option<EntityId> {
        self.neighbor_end(edge, end_index)
            .and_then(|(edge_index, _)| self.edges.get(edge_index))
            .map(|edge| edge.entity_id)
    }

    /// The edge joined to `end_index` of `edge`, and which of *its* two ends
    /// does the joining, or `None` for a free end.
    ///
    /// The first of [`RailGraph::neighbor_ends`], which is the whole answer for
    /// the pieces that exist today: two ends at one point facing the same way
    /// are two rails laid over each other, which placement refuses, so an end
    /// can only ever be joined to one other end. Anything that must keep
    /// working when a junction piece lands — route search does — asks for every
    /// joined end instead.
    pub(in crate::simulation) fn neighbor_end(
        &self,
        edge: &RailEdge,
        end_index: usize,
    ) -> Option<(usize, usize)> {
        self.neighbor_ends(edge, end_index).next()
    }

    /// Every edge joined to `end_index` of `edge`, and which of *its* two ends
    /// does the joining. Empty for a free end.
    ///
    /// Two ends at one node are joined exactly when their headings oppose, so
    /// this is the connection rule itself rather than a cached answer to it.
    /// Which end is reached matters to anything travelling the graph: arriving
    /// at the neighbour's end 0 means entering it at distance zero and running
    /// forwards, and arriving at end 1 means entering at its far end and
    /// running back down it.
    ///
    /// Yielded in the node's own end order, which the builder fills in entity id
    /// order, so a caller that has to break a tie between two branches breaks it
    /// the same way on every machine and every replay.
    pub(in crate::simulation) fn neighbor_ends<'graph>(
        &'graph self,
        edge: &'graph RailEdge,
        end_index: usize,
    ) -> impl Iterator<Item = (usize, usize)> + 'graph {
        let heading = edge.headings[end_index];
        self.nodes
            .get(edge.nodes[end_index])
            .into_iter()
            .flat_map(|node| node.ends.iter())
            .filter_map(move |other| {
                let other_edge = self.edges.get(other.edge_index)?;
                (other_edge.entity_id != edge.entity_id
                    && other_edge.headings[other.end_index] == heading.opposite())
                .then_some((other.edge_index, other.end_index))
            })
    }
}
