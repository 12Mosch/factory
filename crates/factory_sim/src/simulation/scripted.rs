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
            .fuel_slot = Some(ItemStack {
            item_id: coal,
            count: 10,
        });
        self.entities
            .lab_state_mut(lab_id)
            .expect("placed lab should expose lab state")
            .inventory
            .insert(&self.world.prototypes, science_pack, 35)
            .expect("scripted lab inventory should accept research packs");
    }

    fn build_basic_power_fixture(&mut self) -> Option<(i32, i32, EntityId)> {
        let pump =
            factory_data::entity_prototype_id_by_name(&self.world.prototypes, "offshore_pump");
        let boiler = factory_data::entity_prototype_id_by_name(&self.world.prototypes, "boiler");
        let steam_engine =
            factory_data::entity_prototype_id_by_name(&self.world.prototypes, "steam_engine");
        let pole = factory_data::entity_prototype_id_by_name(
            &self.world.prototypes,
            "small_electric_pole",
        );

        for (x, y) in self.all_tile_coords() {
            let lab_x = x + 8;
            let lab_y = y + 1;
            let source_pole = (x + 5, y + 4);
            let lab_pole = (x + 9, y + 5);
            if crate::placement::validate(
                self,
                crate::placement::EntityPlacementRequest {
                    prototype_id: pump,
                    x,
                    y,
                    direction: Direction::North,
                },
            )
            .is_err()
                || crate::placement::validate(
                    self,
                    crate::placement::EntityPlacementRequest {
                        prototype_id: boiler,
                        x,
                        y: y + 1,
                        direction: Direction::North,
                    },
                )
                .is_err()
                || crate::placement::validate(
                    self,
                    crate::placement::EntityPlacementRequest {
                        prototype_id: steam_engine,
                        x: x + 2,
                        y: y + 1,
                        direction: Direction::North,
                    },
                )
                .is_err()
                || crate::placement::validate(
                    self,
                    crate::placement::EntityPlacementRequest {
                        prototype_id: pole,
                        x: source_pole.0,
                        y: source_pole.1,
                        direction: Direction::North,
                    },
                )
                .is_err()
                || crate::placement::validate(
                    self,
                    crate::placement::EntityPlacementRequest {
                        prototype_id: pole,
                        x: lab_pole.0,
                        y: lab_pole.1,
                        direction: Direction::North,
                    },
                )
                .is_err()
                || crate::placement::validate(
                    self,
                    crate::placement::EntityPlacementRequest {
                        prototype_id: factory_data::entity_prototype_id_by_name(
                            &self.world.prototypes,
                            "lab",
                        ),
                        x: lab_x,
                        y: lab_y,
                        direction: Direction::North,
                    },
                )
                .is_err()
            {
                continue;
            }

            crate::placement::place(
                self,
                crate::placement::EntityPlacementRequest {
                    prototype_id: pump,
                    x,
                    y,
                    direction: Direction::North,
                },
            )
            .ok()?;
            let boiler_id = crate::placement::place(
                self,
                crate::placement::EntityPlacementRequest {
                    prototype_id: boiler,
                    x,
                    y: y + 1,
                    direction: Direction::North,
                },
            )
            .ok()?;
            crate::placement::place(
                self,
                crate::placement::EntityPlacementRequest {
                    prototype_id: steam_engine,
                    x: x + 2,
                    y: y + 1,
                    direction: Direction::North,
                },
            )
            .ok()?;
            crate::placement::place(
                self,
                crate::placement::EntityPlacementRequest {
                    prototype_id: pole,
                    x: source_pole.0,
                    y: source_pole.1,
                    direction: Direction::North,
                },
            )
            .ok()?;
            crate::placement::place(
                self,
                crate::placement::EntityPlacementRequest {
                    prototype_id: pole,
                    x: lab_pole.0,
                    y: lab_pole.1,
                    direction: Direction::North,
                },
            )
            .ok()?;

            return Some((lab_x, lab_y, boiler_id));
        }

        None
    }

    fn all_tile_coords(&self) -> Vec<(i32, i32)> {
        self.world
            .chunks
            .values()
            .flat_map(|chunk| {
                chunk.tiles.iter().enumerate().map(move |(index, _)| {
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    (
                        chunk.coord.x * CHUNK_SIZE + local_x,
                        chunk.coord.y * CHUNK_SIZE + local_y,
                    )
                })
            })
            .collect()
    }
}
