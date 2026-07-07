use super::*;

impl Simulation {
    pub fn advance_transport_belts(&mut self) {
        self.refresh_transport_lane_graph();
        self.transport
            .visit_states
            .begin_tick((self.entities.next_entity_id as usize).saturating_mul(4));

        let mut advancement = TransportBeltAdvancement::new(
            &mut self.entities,
            &self.transport.graph,
            &mut self.transport.visit_states,
        );
        advancement.process_all_lanes();
    }
}
