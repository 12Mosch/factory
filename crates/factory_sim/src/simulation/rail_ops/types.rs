use crate::ids::EntityId;
use crate::rails::{RailEndpoint, RailPieceGeometry, RailPoint};

/// One rail piece as the graph builder sees it. Every rail entity is exactly one
/// edge, so the entity id is the edge's identity.
#[derive(Clone, Copy, Debug)]
pub(super) struct RailEdgeNode {
    pub(super) entity_id: EntityId,
    pub(super) geometry: RailPieceGeometry,
}

/// A junction: a world point where one or more rail ends meet.
///
/// Meeting is not the same as joining. Several spurs can reach one point facing
/// different ways; only the ones facing each other are continuous track, so
/// every end reaching the point is recorded and the graph decides per pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::simulation) struct RailGraphNode {
    pub(in crate::simulation) position: RailPoint,
    /// Piece endpoints reaching this point, in ascending `(edge, endpoint)`
    /// order.
    pub(in crate::simulation) endpoints: Vec<RailEndpointRef>,
}

/// An endpoint identified by the edge that owns it and which of its two ends it
/// is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::simulation) struct RailEndpointRef {
    pub(in crate::simulation) edge: u32,
    pub(in crate::simulation) endpoint: u8,
}

/// One piece of track in the graph: an undirected edge between two junctions
/// that a train may traverse either way.
///
/// The direction of travel is not a property of the edge but of the crossing:
/// entering at `nodes[i]` means heading against `outward_directions[i]`, and
/// arriving at the other end means leaving along `outward_directions[1 - i]`.
/// Storing the headings rather than baking in a direction is what keeps one
/// piece of track usable both ways while still letting a future pathfinder
/// reject a reversal at a junction.
///
/// The path itself is not repeated here: it is a function of the piece's
/// prototype and direction, and [`Simulation::rail_piece_geometry`] answers for
/// it. Only the length is copied in, because that is the edge's weight.
///
/// [`Simulation::rail_piece_geometry`]: crate::simulation::Simulation::rail_piece_geometry
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) struct RailGraphEdge {
    pub(in crate::simulation) entity_id: EntityId,
    pub(in crate::simulation) nodes: [u32; 2],
    pub(in crate::simulation) outward_directions: [crate::simulation::Direction; 2],
    pub(in crate::simulation) length_fixed: u32,
}

impl RailGraphEdge {
    pub(in crate::simulation) fn endpoint(&self, index: u8, position: RailPoint) -> RailEndpoint {
        RailEndpoint {
            position,
            direction: self.outward_directions[usize::from(index)],
        }
    }
}

/// A connected run of track, as the builder produces it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::simulation) struct RailNetworkTopology {
    pub(in crate::simulation) network_id: u32,
    /// Member edge indices, in ascending entity id order.
    pub(in crate::simulation) edges: Vec<u32>,
    pub(in crate::simulation) total_length_fixed: u64,
}

/// The settled rail graph: every placed piece, the junctions they meet at, and
/// the connected runs they form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::simulation) struct RailGraph {
    /// Junctions, ordered by position so the numbering is a function of the
    /// world rather than of iteration order.
    pub(in crate::simulation) nodes: Vec<RailGraphNode>,
    /// Pieces, ordered by entity id for the same reason.
    pub(in crate::simulation) edges: Vec<RailGraphEdge>,
    pub(in crate::simulation) networks: Vec<RailNetworkTopology>,
}

impl RailGraph {
    pub(in crate::simulation) fn edge_index(&self, entity_id: EntityId) -> Option<u32> {
        self.edges
            .binary_search_by_key(&entity_id, |edge| edge.entity_id)
            .ok()
            .map(|index| index as u32)
    }
}
