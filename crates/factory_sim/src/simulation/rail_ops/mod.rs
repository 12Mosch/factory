mod geometry;
mod graph_builder;
mod types;

pub use geometry::rail_geometry_in_footprint;
pub(in crate::simulation) use types::RailGraph;

use super::*;
use crate::rails::{
    RailEndpointConnections, RailNetworkSnapshot, RailPieceGeometry, RailPlacementPreview,
};
use geometry::{placed_rail_geometry, rail_geometry_for_placed};
use graph_builder::build_rail_graph_from_nodes;
use types::{RailEdgeNode, RailEndpointRef};

impl Simulation {
    pub(super) fn invalidate_rail_graph(&mut self) {
        self.rails.invalidate();
    }

    /// Whether placing or destroying this prototype can change rail
    /// connectivity. Only track pieces define the graph, so nothing else pays
    /// for a rebuild.
    pub(super) fn prototype_affects_rail_graph(
        &self,
        prototype: &factory_data::EntityPrototype,
    ) -> bool {
        prototype.rail.is_some()
    }

    /// Rebuilds the rail graph if a placement has invalidated it.
    ///
    /// Rails have no per-tick work of their own, so this is called once at the
    /// top of the tick rather than from a solve: the graph is what presentation
    /// and placement previews read, and both want it settled.
    pub(in crate::simulation) fn ensure_rail_graph(&mut self) {
        if !self.rails.graph_dirty {
            return;
        }

        let graph = build_rail_graph_from_nodes(&self.rail_edge_nodes());
        self.rails.replace_graph(graph);
        #[cfg(test)]
        {
            self.rails.graph_rebuilds += 1;
        }
    }

    /// Placed rail pieces in ascending entity id order, which is the order the
    /// graph's edges keep so an edge can be found by entity id.
    fn rail_edge_nodes(&self) -> Vec<RailEdgeNode> {
        self.entities
            .placed_entities
            .values()
            .filter_map(|placed| {
                let prototype = self.world.prototypes.entity(placed.prototype_id)?;
                Some(RailEdgeNode {
                    entity_id: placed.id,
                    geometry: rail_geometry_for_placed(placed, prototype)?,
                })
            })
            .collect()
    }

    /// Travel geometry of a placed rail piece in world fixed-point units, or
    /// `None` when the entity is not track.
    ///
    /// Read straight off the prototype rather than out of the graph, so it is
    /// answerable the instant a piece is placed and never reports a stale path.
    pub fn rail_piece_geometry(&self, entity_id: EntityId) -> Option<RailPieceGeometry> {
        let placed = self.entities.placed_entity(entity_id)?;
        let prototype = self.world.prototypes.entity(placed.prototype_id)?;
        rail_geometry_for_placed(placed, prototype)
    }

    /// Network a rail piece belongs to, or `None` when it is not track.
    ///
    /// Answered from the settled graph, so between a placement and the rebuild
    /// that follows it this reports `None` rather than a network that no longer
    /// exists — the same contract [`Simulation::robot_network_id_for_entity`]
    /// keeps.
    pub fn rail_network_id_for_entity(&self, entity_id: EntityId) -> Option<u32> {
        self.rails.network_ids_by_entity.get(&entity_id).copied()
    }

    /// Every connected run of track, in ascending network id order.
    pub fn rail_networks(&self) -> Vec<RailNetworkSnapshot> {
        self.rails
            .graph
            .networks
            .iter()
            .map(|network| RailNetworkSnapshot {
                network_id: network.network_id,
                entities: network
                    .edges
                    .iter()
                    .map(|edge| self.rails.graph.edges[*edge as usize].entity_id)
                    .collect(),
                total_length_fixed: network.total_length_fixed,
            })
            .collect()
    }

    /// Both ends of a placed piece with the track each one actually joins.
    ///
    /// This is what a connectivity overlay draws: it distinguishes an end that
    /// continues into another piece from one that merely touches it, which is
    /// the distinction a player cannot see from the sprites alone.
    pub fn rail_endpoint_connections(
        &self,
        entity_id: EntityId,
    ) -> Option<[RailEndpointConnections; 2]> {
        let edge_index = self.rails.graph.edge_index(entity_id)?;
        Some([
            self.endpoint_connections(edge_index, 0),
            self.endpoint_connections(edge_index, 1),
        ])
    }

    fn endpoint_connections(&self, edge_index: u32, endpoint: u8) -> RailEndpointConnections {
        let edge = &self.rails.graph.edges[edge_index as usize];
        let node = &self.rails.graph.nodes[edge.nodes[usize::from(endpoint)] as usize];
        let own = edge.endpoint(endpoint, node.position);

        RailEndpointConnections {
            endpoint: own,
            connected: self.track_joining(own, Some(edge_index)),
        }
    }

    fn graph_endpoint(&self, reference: RailEndpointRef) -> crate::rails::RailEndpoint {
        let graph = &self.rails.graph;
        let edge = &graph.edges[reference.edge as usize];
        let node = &graph.nodes[edge.nodes[usize::from(reference.endpoint)] as usize];
        edge.endpoint(reference.endpoint, node.position)
    }

    /// What a rail piece would join if it were placed at `(x, y)` facing
    /// `direction`, without placing anything.
    ///
    /// Returns `None` for prototypes that are not track, so the build preview
    /// can ask about whatever the player has in hand.
    pub fn rail_placement_preview(
        &self,
        prototype_id: EntityPrototypeId,
        x: WorldTileCoord,
        y: WorldTileCoord,
        direction: Direction,
    ) -> Option<RailPlacementPreview> {
        let prototype = self.world.prototypes.entity(prototype_id)?;
        let geometry = placed_rail_geometry(prototype, x, y, direction)?;
        let endpoints = geometry.endpoints.map(|endpoint| RailEndpointConnections {
            endpoint,
            connected: self.track_joining(endpoint, None),
        });

        Some(RailPlacementPreview {
            endpoints,
            curve: geometry.curve,
            length_fixed: geometry.length_fixed,
        })
    }

    /// Placed pieces whose own end continues the run out of `endpoint`, in
    /// ascending entity id order.
    ///
    /// `exclude` is the edge the endpoint belongs to, when it has one; a piece
    /// is never reported as joining itself. A previewed placement has no edge
    /// yet and passes `None`, which is what lets one lookup answer both "what am
    /// I attached to" and "what would I attach to".
    fn track_joining(
        &self,
        endpoint: crate::rails::RailEndpoint,
        exclude: Option<u32>,
    ) -> Vec<EntityId> {
        let graph = &self.rails.graph;
        let Ok(node_index) = graph
            .nodes
            .binary_search_by_key(&endpoint.position, |node| node.position)
        else {
            return Vec::new();
        };

        let mut connected = graph.nodes[node_index]
            .endpoints
            .iter()
            .filter(|reference| Some(reference.edge) != exclude)
            .filter(|reference| endpoint.joins(self.graph_endpoint(**reference)))
            .map(|reference| graph.edges[reference.edge as usize].entity_id)
            .collect::<Vec<_>>();
        connected.sort_unstable();
        connected.dedup();
        connected
    }
}
