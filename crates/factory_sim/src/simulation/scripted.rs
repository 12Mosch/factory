use super::*;

pub fn scripted_inputs_for_red_science_factory() -> Vec<SimCommand> {
    vec![SimCommand::BuildRedScienceResearchFixture]
}

impl Simulation {
    pub fn new_scripted_red_science_factory() -> Self {
        let mut sim = Self::new_seeded(123);
        for command in scripted_inputs_for_red_science_factory() {
            sim.apply_command(&command)
                .expect("scripted red science commands should apply");
        }
        sim
    }

    pub(super) fn build_red_science_research_fixture(&mut self) {
        let logistics = factory_data::technology_id_by_name(&self.world.prototypes, "logistics");
        let automation = factory_data::technology_id_by_name(&self.world.prototypes, "automation");
        if !self.research.is_unlocked("logistics")
            && self.active_research() != Some(logistics)
            && !self.research_queue().contains(&logistics)
        {
            self.enqueue_research(logistics)
                .expect("logistics research should be queueable");
        }
        if !self.research.is_unlocked("automation")
            && self.active_research() != Some(automation)
            && !self.research_queue().contains(&automation)
        {
            self.enqueue_research(automation)
                .expect("automation research should be queueable after logistics");
        }

        let lab = factory_data::entity_prototype_id_by_name(&self.world.prototypes, "lab");
        let (lab_x, lab_y, boiler_id) = self
            .build_basic_power_fixture()
            .expect("scripted red science fixture should be able to place power");
        let lab_id = crate::placement::place(
            self,
            crate::placement::EntityPlacementRequest {
                prototype_id: lab,
                x: lab_x,
                y: lab_y,
                direction: Direction::North,
            },
        )
        .expect("scripted red science fixture should be able to place a lab");
        let science_pack =
            factory_data::item_id_by_name(&self.world.prototypes, "automation_science_pack");
        let coal = factory_data::item_id_by_name(&self.world.prototypes, "coal");
        self.entities
            .boiler_state_mut(boiler_id)
            .expect("placed boiler should expose boiler state")
            .energy
            .fuel_slot = Some(
            ItemStack::new(&self.world.prototypes, coal, 10)
                .expect("scripted coal should form a valid stack"),
        );
        self.entities
            .lab_state_mut(lab_id)
            .expect("placed lab should expose lab state")
            .inventory
            .insert(&self.world.prototypes, science_pack, 35)
            .expect("scripted lab inventory should accept research packs");
    }

    fn build_basic_power_fixture(&mut self) -> Option<(WorldTileCoord, WorldTileCoord, EntityId)> {
        let pump =
            factory_data::entity_prototype_id_by_name(&self.world.prototypes, "offshore_pump");
        let boiler = factory_data::entity_prototype_id_by_name(&self.world.prototypes, "boiler");
        let steam_engine =
            factory_data::entity_prototype_id_by_name(&self.world.prototypes, "steam_engine");
        let pole = factory_data::entity_prototype_id_by_name(
            &self.world.prototypes,
            "small_electric_pole",
        );
        let lab = factory_data::entity_prototype_id_by_name(&self.world.prototypes, "lab");

        'candidate: for (x, y) in self.all_tile_coords() {
            let lab_x = x + 8;
            let lab_y = y + 1;
            let source_pole = (x + 5, y + 4);
            let lab_pole = (x + 9, y + 5);
            let pump_request = scripted_placement_request(pump, x, y);
            let boiler_request = scripted_placement_request(boiler, x, y + 1);
            let steam_engine_request = scripted_placement_request(steam_engine, x + 2, y + 1);
            let source_pole_request =
                scripted_placement_request(pole, source_pole.0, source_pole.1);
            let lab_pole_request = scripted_placement_request(pole, lab_pole.0, lab_pole.1);
            let lab_request = scripted_placement_request(lab, lab_x, lab_y);
            let fixture_requests = [
                pump_request,
                boiler_request,
                steam_engine_request,
                source_pole_request,
                lab_pole_request,
            ];

            if fixture_requests
                .into_iter()
                .chain([lab_request])
                .any(|request| crate::placement::validate(self, request).is_err())
            {
                continue;
            }

            let mut placed = Vec::with_capacity(fixture_requests.len());
            for request in fixture_requests {
                match crate::placement::place(self, request) {
                    Ok(entity_id) => placed.push(entity_id),
                    Err(_) => {
                        for entity_id in placed {
                            crate::entity_mutation::remove(self, entity_id);
                        }
                        continue 'candidate;
                    }
                }
            }
            let boiler_id = placed[1];

            return Some((lab_x, lab_y, boiler_id));
        }

        None
    }

    fn all_tile_coords(&self) -> Vec<(WorldTileCoord, WorldTileCoord)> {
        self.world
            .chunks
            .values()
            .flat_map(|chunk| {
                chunk.tiles.iter().enumerate().map(move |(index, _)| {
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    chunk.coord.tile_at(local_x, local_y)
                })
            })
            .collect()
    }
}

fn scripted_placement_request(
    prototype_id: EntityPrototypeId,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> crate::placement::EntityPlacementRequest {
    crate::placement::EntityPlacementRequest {
        prototype_id,
        x,
        y,
        direction: Direction::North,
    }
}
