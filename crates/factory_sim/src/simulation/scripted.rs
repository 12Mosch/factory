use super::*;

pub fn scripted_inputs_for_red_science_factory() -> Vec<SimulationInput> {
    vec![SimulationInput::BuildRedScienceResearchFixture]
}

impl Simulation {
    pub fn new_scripted_red_science_factory() -> Self {
        let mut sim = Self::new_seeded(123);
        sim.apply_input(SimulationInput::BuildRedScienceResearchFixture);
        sim
    }

    pub fn apply_input(&mut self, input: SimulationInput) {
        match input {
            SimulationInput::BuildRedScienceResearchFixture => {
                self.build_red_science_research_fixture();
            }
        }
    }

    fn build_red_science_research_fixture(&mut self) {
        let automation = factory_data::technology_id_by_name(&self.world.prototypes, "automation");
        if !self.research.is_unlocked("automation") {
            self.select_research(automation)
                .expect("automation research should be selectable");
        }

        let lab = factory_data::entity_prototype_id_by_name(&self.world.prototypes, "lab");
        let lab_id = self
            .first_placeable_entity(lab, Direction::North)
            .and_then(|(x, y)| self.place_entity(lab, x, y, Direction::North).ok())
            .expect("scripted red science fixture should be able to place a lab");
        let science_pack =
            factory_data::item_id_by_name(&self.world.prototypes, "automation_science_pack");
        self.entities
            .lab_state_mut(lab_id)
            .expect("placed lab should expose lab state")
            .inventory
            .insert(&self.world.prototypes, science_pack, 10)
            .expect("scripted lab inventory should accept research packs");
    }

    fn first_placeable_entity(
        &self,
        prototype_id: EntityPrototypeId,
        direction: Direction,
    ) -> Option<(i32, i32)> {
        for chunk in self.world.chunks.values() {
            for (index, _) in chunk.tiles.iter().enumerate() {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                let x = chunk.coord.x * CHUNK_SIZE + local_x;
                let y = chunk.coord.y * CHUNK_SIZE + local_y;

                if self.can_place_entity(prototype_id, x, y, direction).is_ok() {
                    return Some((x, y));
                }
            }
        }

        None
    }
}
