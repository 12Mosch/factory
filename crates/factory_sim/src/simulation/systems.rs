use super::*;

impl Simulation {
    pub fn advance_transport_belts(&mut self) {
        self.refresh_transport_lane_graph();
        let required_lane_slots = (self.entities.next_entity_id as usize).saturating_mul(4);
        self.transport.visit_states.begin_tick(required_lane_slots);
        self.transport.active_lanes.begin_tick(required_lane_slots);

        let mut advancement = TransportBeltAdvancement::new(
            &mut self.entities,
            &self.transport.graph,
            &mut self.transport.visit_states,
            &mut self.transport.active_lanes,
            &mut self.transport.item_revision,
            &mut self.transport.item_revisions_by_entity,
        );
        advancement.process_active_lanes();
        self.transport.active_lanes.finish_tick();
    }
}
