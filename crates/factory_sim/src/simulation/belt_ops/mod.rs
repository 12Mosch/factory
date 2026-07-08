use super::*;

mod advancement;
mod cache;
mod geometry;
mod lane_access;
mod types;

pub(super) use advancement::TransportBeltAdvancement;
pub(super) use advancement::insert_lane_item_at_entry;
pub(super) use cache::TransportLaneCache;
#[allow(unused_imports)]
pub(super) use geometry::{direction_tile_delta, splitter_port_tiles};
pub(super) use lane_access::belt_lane_can_accept_position;
pub(super) use types::TransportLaneKey;

impl Simulation {
    pub fn insert_item_onto_belt(
        &mut self,
        entity_id: EntityId,
        lane_index: usize,
        item_id: ItemId,
    ) -> Result<(), BeltError> {
        self.entities
            .insert_item_onto_belt(entity_id, lane_index, item_id)?;
        self.transport.mark_active(TransportLaneKey::Belt {
            entity_id,
            lane_index,
        });
        Ok(())
    }

    pub(super) fn prototype_affects_transport_lane_graph(
        &self,
        prototype: &factory_data::EntityPrototype,
    ) -> bool {
        (prototype.entity_kind == EntityKind::TransportBelt && prototype.transport_belt.is_some())
            || (prototype.entity_kind == EntityKind::Splitter && prototype.splitter.is_some())
    }

    pub(super) fn invalidate_transport_lane_graph(&mut self) {
        self.transport.invalidate();
    }

    pub(super) fn refresh_transport_lane_graph(&mut self) {
        self.transport.refresh(&self.entities);
    }

    #[cfg(test)]
    pub(super) fn transport_lane_graph_rebuild_count(&self) -> u64 {
        self.transport.rebuilds
    }
}
