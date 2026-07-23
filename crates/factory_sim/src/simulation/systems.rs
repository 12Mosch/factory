use super::*;

impl Simulation {
    pub fn advance_transport_belts(&mut self) {
        self.refresh_transport_lane_graph();
        let run_count = self.transport.graph.run_count();
        self.transport.visit_states.begin_tick(run_count);
        self.transport.active_runs.begin_tick(run_count);

        let mut advancement = TransportBeltAdvancement::new(
            &mut self.entities,
            &self.transport.graph,
            &mut self.transport.visit_states,
            &mut self.transport.active_runs,
            &mut self.transport.item_revision,
            &mut self.transport.item_revisions_by_entity,
        );
        advancement.process_active_runs();
        self.transport.active_runs.finish_tick();
    }
}
