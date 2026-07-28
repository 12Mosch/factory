use std::collections::BTreeMap;

use crate::entities::Direction;
use crate::ids::EntityId;
use crate::rail::{RailNetworkSnapshot, RailPieceGeometry};
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
    ///
    /// Two ends at one node are joined exactly when their headings oppose, so
    /// this is the connection rule itself rather than a cached answer to it.
    pub(in crate::simulation) fn neighbor(
        &self,
        edge: &RailEdge,
        end_index: usize,
    ) -> Option<EntityId> {
        let node = self.nodes.get(edge.nodes[end_index])?;
        let heading = edge.headings[end_index];
        node.ends.iter().find_map(|other| {
            let other_edge = self.edges.get(other.edge_index)?;
            (other_edge.entity_id != edge.entity_id
                && other_edge.headings[other.end_index] == heading.opposite())
            .then_some(other_edge.entity_id)
        })
    }
}
