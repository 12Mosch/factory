use super::*;

impl Simulation {
    pub fn machine_statuses(&self) -> MachineStatusSnapshot {
        let mut groups = Vec::new();
        let mut total_by_status = BTreeMap::<MachineStatus, usize>::new();

        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::MiningDrill,
            self.entities
                .burner_mining_drills
                .iter()
                .map(|(entity_id, state)| self.burner_mining_drill_status(*entity_id, state)),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::Furnace,
            self.entities
                .furnaces
                .values()
                .map(|state| self.furnace_status(state)),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::AssemblingMachine,
            self.entities
                .assembling_machines
                .iter()
                .map(|(entity_id, state)| self.assembler_status(*entity_id, state)),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::Lab,
            self.entities
                .labs
                .iter()
                .map(|(entity_id, state)| self.lab_status(*entity_id, state)),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::Boiler,
            self.entities
                .boilers
                .iter()
                .map(|(entity_id, state)| self.boiler_status(*entity_id, state)),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::SteamEngine,
            self.entities
                .steam_engines
                .keys()
                .map(|entity_id| self.steam_engine_status(*entity_id)),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::OffshorePump,
            self.entities
                .offshore_pumps
                .keys()
                .map(|entity_id| self.offshore_pump_status(*entity_id)),
        );

        MachineStatusSnapshot {
            groups,
            total_by_status: total_by_status
                .into_iter()
                .map(|(status, count)| MachineStatusCount { status, count })
                .collect(),
        }
    }

    pub fn bottleneck_hints(&self, max: usize) -> BottleneckHintsSnapshot {
        let mut candidates = Vec::<(u64, BottleneckHint)>::new();

        for row in self.item_statistics().rows {
            if row.consumed_last_minute > row.produced_last_minute {
                let deficit = row.consumed_last_minute - row.produced_last_minute;
                candidates.push((
                    deficit,
                    BottleneckHint {
                        kind: BottleneckHintKind::ItemDeficit,
                        subject_item: Some(row.item_id),
                        subject_fluid: None,
                        affected_count: deficit.min(usize::MAX as u64) as usize,
                        message: format!(
                            "{} consumed faster than produced",
                            item_display_name(&self.world.prototypes, row.item_id)
                        ),
                    },
                ));
            }
        }

        if let Some(technology_id) = self.research.active
            && let Some(technology) = self
                .world
                .prototypes
                .technologies
                .get(technology_id.index())
                .filter(|technology| technology.id == technology_id)
        {
            let mut missing_by_pack = BTreeMap::<ItemId, usize>::new();
            for state in self.entities.labs.values() {
                for science_pack in &technology.science_packs {
                    if state.inventory.count(science_pack.item) < u32::from(science_pack.amount) {
                        *missing_by_pack.entry(science_pack.item).or_default() += 1;
                    }
                }
            }
            for (item_id, count) in missing_by_pack {
                candidates.push((
                    count as u64,
                    BottleneckHint {
                        kind: BottleneckHintKind::ResearchMissingScience,
                        subject_item: Some(item_id),
                        subject_fluid: None,
                        affected_count: count,
                        message: format!(
                            "Science labs waiting for {}",
                            item_display_name(&self.world.prototypes, item_id)
                        ),
                    },
                ));
            }
        }

        if self.steam_engines_are_starved() {
            let steam = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes)
                .fluids
                .steam;
            candidates.push((
                self.entities.steam_engines.len() as u64,
                BottleneckHint {
                    kind: BottleneckHintKind::SteamStarved,
                    subject_item: None,
                    subject_fluid: Some(steam),
                    affected_count: self.entities.steam_engines.len(),
                    message: "Steam engines starved of steam".to_string(),
                },
            ));
        }

        if self.power_summary.consumption_watts > 0
            && self.power_summary.satisfaction_permyriad < POWER_SATISFACTION_FULL_PERMYRIAD
        {
            candidates.push((
                u64::from(
                    POWER_SATISFACTION_FULL_PERMYRIAD
                        .saturating_sub(self.power_summary.satisfaction_permyriad),
                ),
                BottleneckHint {
                    kind: BottleneckHintKind::PowerShortage,
                    subject_item: None,
                    subject_fluid: None,
                    affected_count: self.entities.electric_consumers.len(),
                    message: "Power production below demand".to_string(),
                },
            ));
        }

        if !self.entities.labs.is_empty() && self.research.active.is_none() {
            candidates.push((
                self.entities.labs.len() as u64,
                BottleneckHint {
                    kind: BottleneckHintKind::NoActiveResearch,
                    subject_item: None,
                    subject_fluid: None,
                    affected_count: self.entities.labs.len(),
                    message: "No active research selected".to_string(),
                },
            ));
        }

        candidates.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| hint_kind_order(a.1.kind).cmp(&hint_kind_order(b.1.kind)))
                .then_with(|| a.1.message.cmp(&b.1.message))
        });
        BottleneckHintsSnapshot {
            hints: candidates
                .into_iter()
                .take(max)
                .map(|(_, hint)| hint)
                .collect(),
        }
    }

    fn push_status_group(
        &self,
        groups: &mut Vec<MachineStatusGroup>,
        total_by_status: &mut BTreeMap<MachineStatus, usize>,
        kind: EntityKind,
        statuses: impl Iterator<Item = MachineStatus>,
    ) {
        let mut counts_by_status = BTreeMap::<MachineStatus, usize>::new();
        for status in statuses {
            *counts_by_status.entry(status).or_default() += 1;
            *total_by_status.entry(status).or_default() += 1;
        }
        if counts_by_status.is_empty() {
            return;
        }
        groups.push(MachineStatusGroup {
            kind,
            counts: counts_by_status
                .into_iter()
                .map(|(status, count)| MachineStatusCount { status, count })
                .collect(),
        });
    }

    fn burner_mining_drill_status(
        &self,
        entity_id: EntityId,
        state: &BurnerMiningDrillState,
    ) -> MachineStatus {
        let Some(placed) = self.entities.placed_entity(entity_id) else {
            return MachineStatus::Idle;
        };
        let Some(prototype) = self
            .world
            .prototypes
            .entities
            .get(placed.prototype_id.index())
            .filter(|prototype| prototype.id == placed.prototype_id)
        else {
            return MachineStatus::Idle;
        };
        let Some(mining_drill) = prototype.mining_drill.as_ref() else {
            return MachineStatus::Idle;
        };
        let Some((_, resource_item)) =
            first_resource_in_mining_area(&self.world, &placed.footprint, mining_drill)
        else {
            return MachineStatus::NoInput;
        };
        let output_target = drill_output_target(&self.entities, placed);
        if !drill_output_target_can_accept(
            &self.world.prototypes,
            &self.entities,
            output_target,
            state.output_slot,
            resource_item,
            1,
        ) {
            return MachineStatus::OutputFull;
        }
        if state.energy.fuel_slot.is_none() && state.energy.energy_remaining_joules <= f64::EPSILON
        {
            return MachineStatus::NoFuel;
        }
        MachineStatus::Working
    }

    fn furnace_status(&self, state: &FurnaceState) -> MachineStatus {
        let Some((_, _, _, product)) =
            furnace_work_selection(&self.world.prototypes, &self.research, state.input_slot)
        else {
            return MachineStatus::NoInput;
        };
        if !output_slot_can_accept(
            &self.world.prototypes,
            state.output_slot,
            product.item,
            product.amount,
        ) {
            return MachineStatus::OutputFull;
        }
        if state.energy.fuel_slot.is_none() && state.energy.energy_remaining_joules <= f64::EPSILON
        {
            return MachineStatus::NoFuel;
        }
        MachineStatus::Working
    }

    fn assembler_status(
        &self,
        entity_id: EntityId,
        state: &AssemblingMachineState,
    ) -> MachineStatus {
        if state.selected_recipe.is_none() {
            return MachineStatus::NoRecipe;
        }
        let Some(recipe) = selected_assembler_recipe(&self.world.prototypes, &self.research, state)
        else {
            return MachineStatus::NoResearch;
        };
        if !assembler_has_ingredients(&state.input_inventory, &recipe.ingredients) {
            return MachineStatus::NoInput;
        }
        if !assembler_output_can_accept(
            &self.world.prototypes,
            &state.output_inventory,
            &recipe.products,
        ) {
            return MachineStatus::OutputFull;
        }
        if self
            .entity_power_statuses
            .get(&entity_id)
            .map(|status| status.satisfaction_permyriad)
            .unwrap_or(0)
            == 0
        {
            return MachineStatus::NoPower;
        }
        MachineStatus::Working
    }

    fn lab_status(&self, entity_id: EntityId, state: &LabState) -> MachineStatus {
        let Some(technology_id) = state.active_technology.or(self.research.active) else {
            return MachineStatus::NoResearch;
        };
        let Some(technology) = self
            .world
            .prototypes
            .technologies
            .get(technology_id.index())
            .filter(|technology| technology.id == technology_id)
        else {
            return MachineStatus::NoResearch;
        };
        if !lab_has_science_packs(&state.inventory, &technology.science_packs) {
            return MachineStatus::NoInput;
        }
        if self
            .entity_power_statuses
            .get(&entity_id)
            .map(|status| status.satisfaction_permyriad)
            .unwrap_or(0)
            == 0
        {
            return MachineStatus::NoPower;
        }
        MachineStatus::Working
    }

    fn boiler_status(&self, entity_id: EntityId, state: &BoilerState) -> MachineStatus {
        let ids = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes);
        let Some(placed) = self.entities.placed_entity(entity_id) else {
            return MachineStatus::Idle;
        };
        let Some(boiler) = self
            .world
            .prototypes
            .entities
            .get(placed.prototype_id.index())
            .filter(|prototype| prototype.id == placed.prototype_id)
            .and_then(|prototype| prototype.boiler.as_ref())
        else {
            return MachineStatus::Idle;
        };
        let water_amount = per_tick_milliunits(boiler.water_consumption_per_second_milliunits);
        let steam_amount = per_tick_milliunits(boiler.steam_output_per_second_milliunits);
        let Some(water_network_id) = self.fluid_network_id_for_box_key(FluidBoxKey {
            entity_id,
            box_index: 0,
        }) else {
            return MachineStatus::NoFluid;
        };
        let Some(steam_network_id) = self.fluid_network_id_for_box_key(FluidBoxKey {
            entity_id,
            box_index: 1,
        }) else {
            return MachineStatus::NoFluid;
        };
        if self.fluid_network_total_for_fluid(water_network_id, ids.fluids.water) < water_amount {
            return MachineStatus::NoFluid;
        }
        if self.fluid_network_available_capacity_for_fluid(steam_network_id, ids.fluids.steam)
            < steam_amount
        {
            return MachineStatus::OutputFull;
        }
        if state.energy.fuel_slot.is_none() && state.energy.energy_remaining_joules <= f64::EPSILON
        {
            return MachineStatus::NoFuel;
        }
        MachineStatus::Working
    }

    fn steam_engine_status(&self, entity_id: EntityId) -> MachineStatus {
        if self.power_summary.consumption_watts == 0 {
            return MachineStatus::Idle;
        }
        let steam = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes)
            .fluids
            .steam;
        let Some(engine) = self.steam_engine_prototype(entity_id) else {
            return MachineStatus::Idle;
        };
        let Some(network_id) = self.fluid_network_id_for_box_key(FluidBoxKey {
            entity_id,
            box_index: 0,
        }) else {
            return MachineStatus::NoFluid;
        };
        let required = per_tick_milliunits(engine.steam_consumption_per_second_milliunits);
        if self.fluid_network_total_for_fluid(network_id, steam) < required {
            return MachineStatus::NoFluid;
        }
        MachineStatus::Working
    }

    fn offshore_pump_status(&self, entity_id: EntityId) -> MachineStatus {
        let water = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes)
            .fluids
            .water;
        let Some(network_id) = self.fluid_network_id_for_box_key(FluidBoxKey {
            entity_id,
            box_index: 0,
        }) else {
            return MachineStatus::NoFluid;
        };
        if self.fluid_network_available_capacity_for_fluid(network_id, water) == 0 {
            return MachineStatus::OutputFull;
        }
        MachineStatus::Working
    }

    fn steam_engines_are_starved(&self) -> bool {
        if self.entities.steam_engines.is_empty()
            || self.power_summary.consumption_watts == 0
            || self.power_summary.available_production_watts >= self.power_summary.consumption_watts
        {
            return false;
        }
        let steam = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes)
            .fluids
            .steam;
        self.entities.steam_engines.keys().any(|entity_id| {
            let Some(engine) = self.steam_engine_prototype(*entity_id) else {
                return false;
            };
            let Some(network_id) = self.fluid_network_id_for_box_key(FluidBoxKey {
                entity_id: *entity_id,
                box_index: 0,
            }) else {
                return true;
            };
            let required = per_tick_milliunits(engine.steam_consumption_per_second_milliunits);
            self.fluid_network_total_for_fluid(network_id, steam) < required
        })
    }
}

fn hint_kind_order(kind: BottleneckHintKind) -> u8 {
    match kind {
        BottleneckHintKind::ItemDeficit => 0,
        BottleneckHintKind::ResearchMissingScience => 1,
        BottleneckHintKind::SteamStarved => 2,
        BottleneckHintKind::PowerShortage => 3,
        BottleneckHintKind::NoActiveResearch => 4,
    }
}

fn item_display_name(catalog: &PrototypeCatalog, item_id: ItemId) -> String {
    catalog
        .items
        .get(item_id.index())
        .filter(|item| item.id == item_id)
        .map(|item| title_case_identifier(&item.name))
        .unwrap_or_else(|| "Unknown".to_string())
}

fn title_case_identifier(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
