use super::*;

impl Simulation {
    pub(super) fn refresh_production_status_revision(&mut self) {
        let mut next = std::mem::take(&mut self.production_map_status_scratch);
        next.clear();
        let fluids = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes).fluids;

        for (entity_id, state) in &self.entities.mining_drills {
            push_production_map_status(
                &mut next,
                *entity_id,
                self.mining_drill_status(*entity_id, state),
            );
        }
        for (entity_id, state) in &self.entities.furnaces {
            push_production_map_status(
                &mut next,
                *entity_id,
                self.furnace_status(*entity_id, state),
            );
        }
        for (entity_id, state) in &self.entities.assembling_machines {
            push_production_map_status(
                &mut next,
                *entity_id,
                self.assembler_status(*entity_id, state),
            );
        }
        for (entity_id, state) in &self.entities.labs {
            push_production_map_status(&mut next, *entity_id, self.lab_status(*entity_id, state));
        }
        for (entity_id, state) in &self.entities.boilers {
            push_production_map_status(
                &mut next,
                *entity_id,
                self.boiler_status(*entity_id, state, fluids.water, fluids.steam),
            );
        }
        for entity_id in self.entities.steam_engines.keys() {
            push_production_map_status(
                &mut next,
                *entity_id,
                self.steam_engine_status(*entity_id, fluids.steam),
            );
        }
        for entity_id in self.entities.offshore_pumps.keys() {
            push_production_map_status(
                &mut next,
                *entity_id,
                self.offshore_pump_status(*entity_id, fluids.water),
            );
        }
        for entity_id in self.entities.pumpjacks.keys() {
            push_production_map_status(&mut next, *entity_id, self.pumpjack_status(*entity_id));
        }
        for (entity_id, state) in &self.entities.nuclear_reactors {
            push_production_map_status(
                &mut next,
                *entity_id,
                self.nuclear_reactor_status(*entity_id, state),
            );
        }
        for entity_id in self.entities.heat_exchangers.keys() {
            push_production_map_status(
                &mut next,
                *entity_id,
                self.heat_exchanger_status(*entity_id, fluids.water, fluids.steam),
            );
        }
        for entity_id in self.entities.roboports.keys() {
            push_production_map_status(&mut next, *entity_id, self.roboport_status(*entity_id));
        }

        if next != self.production_map_statuses {
            self.production_status_revision = self.production_status_revision.wrapping_add(1);
        }
        std::mem::swap(&mut self.production_map_statuses, &mut next);
        self.production_map_status_scratch = next;
    }

    pub fn machine_statuses(&self) -> MachineStatusSnapshot {
        let mut groups = Vec::new();
        let mut total_by_status = BTreeMap::<MachineStatus, usize>::new();
        let fluids = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes).fluids;

        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::MiningDrill,
            self.entities
                .mining_drills
                .iter()
                .map(|(entity_id, state)| self.mining_drill_status(*entity_id, state)),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::Furnace,
            self.entities
                .furnaces
                .iter()
                .map(|(entity_id, state)| self.furnace_status(*entity_id, state)),
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
            self.entities.boilers.iter().map(|(entity_id, state)| {
                self.boiler_status(*entity_id, state, fluids.water, fluids.steam)
            }),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::SteamEngine,
            self.entities
                .steam_engines
                .keys()
                .map(|entity_id| self.steam_engine_status(*entity_id, fluids.steam)),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::OffshorePump,
            self.entities
                .offshore_pumps
                .keys()
                .map(|entity_id| self.offshore_pump_status(*entity_id, fluids.water)),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::Pumpjack,
            self.entities
                .pumpjacks
                .keys()
                .map(|entity_id| self.pumpjack_status(*entity_id)),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::NuclearReactor,
            self.entities
                .nuclear_reactors
                .iter()
                .map(|(entity_id, state)| self.nuclear_reactor_status(*entity_id, state)),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::HeatExchanger,
            self.entities.heat_exchangers.keys().map(|entity_id| {
                self.heat_exchanger_status(*entity_id, fluids.water, fluids.steam)
            }),
        );
        self.push_status_group(
            &mut groups,
            &mut total_by_status,
            EntityKind::Roboport,
            self.entities
                .roboports
                .keys()
                .map(|entity_id| self.roboport_status(*entity_id)),
        );

        MachineStatusSnapshot {
            groups,
            total_by_status: total_by_status
                .into_iter()
                .map(|(status, count)| MachineStatusCount { status, count })
                .collect(),
        }
    }

    pub fn machine_status_for_entity(&self, entity_id: EntityId) -> Option<MachineStatus> {
        let fluids = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes).fluids;

        if let Some(state) = self.entities.mining_drills.get(&entity_id) {
            return Some(self.mining_drill_status(entity_id, state));
        }
        if let Some(state) = self.entities.furnaces.get(&entity_id) {
            return Some(self.furnace_status(entity_id, state));
        }
        if let Some(state) = self.entities.assembling_machines.get(&entity_id) {
            return Some(self.assembler_status(entity_id, state));
        }
        if let Some(state) = self.entities.labs.get(&entity_id) {
            return Some(self.lab_status(entity_id, state));
        }
        if let Some(state) = self.entities.boilers.get(&entity_id) {
            return Some(self.boiler_status(entity_id, state, fluids.water, fluids.steam));
        }
        if self.entities.steam_engines.contains_key(&entity_id) {
            return Some(self.steam_engine_status(entity_id, fluids.steam));
        }
        if self.entities.offshore_pumps.contains_key(&entity_id) {
            return Some(self.offshore_pump_status(entity_id, fluids.water));
        }
        if self.entities.pumpjacks.contains_key(&entity_id) {
            return Some(self.pumpjack_status(entity_id));
        }
        if let Some(state) = self.entities.nuclear_reactors.get(&entity_id) {
            return Some(self.nuclear_reactor_status(entity_id, state));
        }
        if self.entities.heat_exchangers.contains_key(&entity_id) {
            return Some(self.heat_exchanger_status(entity_id, fluids.water, fluids.steam));
        }
        if self.entities.roboports.contains_key(&entity_id) {
            return Some(self.roboport_status(entity_id));
        }

        None
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
            && let Some(technology) = self.world.prototypes.technology(technology_id)
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

        if self.power.summary.consumption_watts > 0
            && self.power.summary.satisfaction_permyriad < POWER_SATISFACTION_FULL_PERMYRIAD
        {
            candidates.push((
                u64::from(
                    POWER_SATISFACTION_FULL_PERMYRIAD
                        .saturating_sub(self.power.summary.satisfaction_permyriad),
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

    fn mining_drill_status(&self, entity_id: EntityId, state: &MiningDrillState) -> MachineStatus {
        let Some(placed) = self.entities.placed_entity(entity_id) else {
            return MachineStatus::Idle;
        };
        let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
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
        if let Some(status) = self.machine_energy_status(entity_id, &state.energy) {
            return status;
        }
        MachineStatus::Working
    }

    fn furnace_status(&self, entity_id: EntityId, state: &FurnaceState) -> MachineStatus {
        let Some((_, _, _, product)) =
            furnace_work_selection(&self.world.prototypes, &self.research, state.input_slot)
        else {
            return MachineStatus::NoInput;
        };
        if !state
            .output_slot
            .can_insert_item(&self.world.prototypes, product.item, product.amount)
        {
            return MachineStatus::OutputFull;
        }
        if let Some(status) = self.machine_energy_status(entity_id, &state.energy) {
            return status;
        }
        MachineStatus::Working
    }

    /// Energy-starvation status shared by burner-or-electric machines: `None`
    /// when the machine's energy source can currently supply work.
    fn machine_energy_status(
        &self,
        entity_id: EntityId,
        energy: &MachineEnergy,
    ) -> Option<MachineStatus> {
        match energy {
            MachineEnergy::Burner(_) => energy.is_out_of_fuel().then_some(MachineStatus::NoFuel),
            MachineEnergy::Electric => {
                let satisfaction = self
                    .power
                    .entity_statuses
                    .get(&entity_id)
                    .map(|status| status.satisfaction_permyriad)
                    .unwrap_or(0);
                (satisfaction == 0).then_some(MachineStatus::NoPower)
            }
        }
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
        let Some(prototype) = self
            .entities
            .placed_entity(entity_id)
            .and_then(|placed| self.world.prototypes.entity(placed.prototype_id))
        else {
            return MachineStatus::Idle;
        };
        let box_states = self
            .entities
            .fluid_boxes
            .get(&entity_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if fluid_ingredient_box_indices(
            &prototype.fluid_boxes,
            box_states,
            &recipe.fluid_ingredients,
        )
        .is_none()
        {
            return MachineStatus::NoFluid;
        }
        if fluid_product_box_indices(&prototype.fluid_boxes, box_states, &recipe.fluid_products)
            .is_none()
        {
            return MachineStatus::OutputFull;
        }
        if self
            .power
            .entity_statuses
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
        let Some(technology) = self.world.prototypes.technology(technology_id) else {
            return MachineStatus::NoResearch;
        };
        if !lab_has_science_packs(&state.inventory, &technology.science_packs) {
            return MachineStatus::NoInput;
        }
        if self
            .power
            .entity_statuses
            .get(&entity_id)
            .map(|status| status.satisfaction_permyriad)
            .unwrap_or(0)
            == 0
        {
            return MachineStatus::NoPower;
        }
        MachineStatus::Working
    }

    fn boiler_status(
        &self,
        entity_id: EntityId,
        state: &BoilerState,
        water: FluidId,
        steam: FluidId,
    ) -> MachineStatus {
        let Some(placed) = self.entities.placed_entity(entity_id) else {
            return MachineStatus::Idle;
        };
        let Some(boiler) = self
            .world
            .prototypes
            .entity(placed.prototype_id)
            .and_then(|prototype| prototype.boiler.as_ref())
        else {
            return MachineStatus::Idle;
        };
        if let Some(blocker) = self.boiler_fluid_blocker(entity_id, boiler, water, steam) {
            return blocker;
        }
        if state.energy.fuel_slot.is_empty() && state.energy.energy_remaining_joules <= f64::EPSILON
        {
            return MachineStatus::NoFuel;
        }
        MachineStatus::Working
    }

    /// Whatever stops a water-to-steam converter this tick on the fluid side, or
    /// `None` when water is available and the steam has somewhere to go.
    ///
    /// Shared by boilers and heat exchangers: they differ only in where their
    /// energy comes from, so letting the fluid rules drift apart would report two
    /// different statuses for the same blocked pipe.
    fn boiler_fluid_blocker(
        &self,
        entity_id: EntityId,
        boiler: &factory_data::BoilerPrototype,
        water: FluidId,
        steam: FluidId,
    ) -> Option<MachineStatus> {
        // An unnetworked box is itself a blocker, so neither lookup may use `?`:
        // `None` from this helper means "the fluids are fine".
        let Some(water_network_id) =
            self.fluid_network_id_for_box_key(FluidBoxKey::entity(entity_id, 0))
        else {
            return Some(MachineStatus::NoFluid);
        };
        let Some(steam_network_id) =
            self.fluid_network_id_for_box_key(FluidBoxKey::entity(entity_id, 1))
        else {
            return Some(MachineStatus::NoFluid);
        };

        let water_amount = per_tick_milliunits(boiler.water_consumption_per_second_milliunits);
        let steam_amount = per_tick_milliunits(boiler.steam_output_per_second_milliunits);
        if self.fluid_network_total_for_fluid(water_network_id, water) < water_amount {
            return Some(MachineStatus::NoFluid);
        }
        if self.fluid_network_available_capacity_for_fluid(steam_network_id, steam) < steam_amount {
            return Some(MachineStatus::OutputFull);
        }
        None
    }

    fn nuclear_reactor_status(
        &self,
        entity_id: EntityId,
        state: &NuclearReactorState,
    ) -> MachineStatus {
        if state.energy.fuel_slot.is_empty() && state.energy.energy_remaining_joules <= f64::EPSILON
        {
            return MachineStatus::NoFuel;
        }
        // A reactor at maximum temperature holds its fuel; the network is not
        // taking the heat away fast enough.
        let capacity = self.heat_buffer_capacity_joules(entity_id);
        let stored = self
            .entities
            .heat_buffers
            .get(&entity_id)
            .map_or(0, |buffer| buffer.energy_joules);
        if stored >= capacity {
            return MachineStatus::OutputFull;
        }
        // Spent fuel cells with nowhere to go stop the reactor between cells.
        if state.energy.energy_remaining_joules <= f64::EPSILON
            && self.reactor_residue_is_blocked(state)
        {
            return MachineStatus::OutputFull;
        }
        MachineStatus::Working
    }

    /// Whether the next fuel cell's residue has nowhere to go, which is what
    /// stops a fuelled reactor between cells.
    fn reactor_residue_is_blocked(&self, state: &NuclearReactorState) -> bool {
        let Some(fuel_item) = state.energy.fuel_slot.stack().map(|stack| stack.item_id()) else {
            return false;
        };
        let Some(burnt_result) = self
            .world
            .prototypes
            .item(fuel_item)
            .and_then(|item| item.burnt_result)
        else {
            return false;
        };
        !state
            .output_slot
            .can_insert_item(&self.world.prototypes, burnt_result, 1)
    }

    fn heat_exchanger_status(
        &self,
        entity_id: EntityId,
        water: FluidId,
        steam: FluidId,
    ) -> MachineStatus {
        let Some(placed) = self.entities.placed_entity(entity_id) else {
            return MachineStatus::Idle;
        };
        let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
            return MachineStatus::Idle;
        };
        let Some(boiler) = prototype.boiler.as_ref() else {
            return MachineStatus::Idle;
        };
        let Some(heat_source) = prototype.heat_energy_source else {
            return MachineStatus::Idle;
        };
        let Some(heat_buffer) = prototype.heat_buffer.as_ref() else {
            return MachineStatus::Idle;
        };
        if let Some(blocker) = self.boiler_fluid_blocker(entity_id, boiler, water, steam) {
            return blocker;
        }
        let stored = self
            .entities
            .heat_buffers
            .get(&entity_id)
            .map_or(0, |buffer| buffer.energy_joules);
        let minimum = crate::heat::energy_for_temperature(
            heat_source.min_working_temperature_degrees,
            heat_buffer.specific_heat_joules_per_degree,
        )
        .max(heat_source.energy_usage_watts / SIMULATION_TICKS_PER_SECOND);
        if stored < minimum {
            return MachineStatus::NoHeat;
        }
        MachineStatus::Working
    }

    /// A roboport's status is about power, because power is the only thing it
    /// can be short of: unpowered it cannot charge, still filling it is
    /// working, and once its buffer is full it is idle and waiting for robots.
    fn roboport_status(&self, entity_id: EntityId) -> MachineStatus {
        if self
            .power
            .entity_statuses
            .get(&entity_id)
            .is_none_or(|status| status.satisfaction_permyriad == 0)
        {
            return MachineStatus::NoPower;
        }
        if crate::simulation::robot_ops::roboport_is_charging(
            &self.world.prototypes,
            &self.entities,
            entity_id,
        ) {
            return MachineStatus::Working;
        }
        MachineStatus::Idle
    }

    fn steam_engine_status(&self, entity_id: EntityId, steam: FluidId) -> MachineStatus {
        if self.power.summary.consumption_watts == 0 {
            return MachineStatus::Idle;
        }
        let Some(engine) = self.steam_engine_prototype(entity_id) else {
            return MachineStatus::Idle;
        };
        let Some(network_id) = self.fluid_network_id_for_box_key(FluidBoxKey::entity(entity_id, 0))
        else {
            return MachineStatus::NoFluid;
        };
        let required = per_tick_milliunits(engine.steam_consumption_per_second_milliunits);
        if self.fluid_network_total_for_fluid(network_id, steam) < required {
            return MachineStatus::NoFluid;
        }
        MachineStatus::Working
    }

    fn offshore_pump_status(&self, entity_id: EntityId, water: FluidId) -> MachineStatus {
        let Some(network_id) = self.fluid_network_id_for_box_key(FluidBoxKey::entity(entity_id, 0))
        else {
            return MachineStatus::NoFluid;
        };
        if self.fluid_network_available_capacity_for_fluid(network_id, water) == 0 {
            return MachineStatus::OutputFull;
        }
        MachineStatus::Working
    }

    fn pumpjack_status(&self, entity_id: EntityId) -> MachineStatus {
        let Some(placed) = self.entities.placed_entity(entity_id) else {
            return MachineStatus::Idle;
        };
        let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
            return MachineStatus::Idle;
        };
        let Some(pumpjack) = prototype.pumpjack.as_ref() else {
            return MachineStatus::Idle;
        };
        if !placed.footprint.tiles().into_iter().any(|(x, y)| {
            self.world
                .tile_at(x, y)
                .and_then(|tile| tile.resource)
                .is_some_and(|resource| resource.resource_item == pumpjack.resource_item)
        }) {
            return MachineStatus::NoInput;
        }
        let Some(capacity_milliunits) = prototype
            .fluid_boxes
            .first()
            .map(|fluid_box| fluid_box.capacity_milliunits)
        else {
            return MachineStatus::NoFluid;
        };
        let Some(state) = self
            .entities
            .fluid_boxes
            .get(&entity_id)
            .and_then(|boxes| boxes.first())
        else {
            return MachineStatus::NoFluid;
        };
        if state
            .fluid_id
            .is_some_and(|fluid| fluid != pumpjack.output_fluid)
            || state.amount_milliunits >= capacity_milliunits
        {
            return MachineStatus::OutputFull;
        }
        if self
            .power
            .entity_statuses
            .get(&entity_id)
            .map(|status| status.satisfaction_permyriad)
            .unwrap_or(0)
            == 0
        {
            return MachineStatus::NoPower;
        }
        MachineStatus::Working
    }

    fn steam_engines_are_starved(&self) -> bool {
        if self.entities.steam_engines.is_empty()
            || self.power.summary.consumption_watts == 0
            || self.power.summary.available_production_watts >= self.power.summary.consumption_watts
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
            let Some(network_id) =
                self.fluid_network_id_for_box_key(FluidBoxKey::entity(*entity_id, 0))
            else {
                return true;
            };
            let required = per_tick_milliunits(engine.steam_consumption_per_second_milliunits);
            self.fluid_network_total_for_fluid(network_id, steam) < required
        })
    }
}

fn push_production_map_status(
    statuses: &mut Vec<(EntityId, u8)>,
    entity_id: EntityId,
    status: MachineStatus,
) {
    let display_class = match status {
        MachineStatus::Working | MachineStatus::Idle => return,
        MachineStatus::NoPower => 0,
        _ => 1,
    };
    statuses.push((entity_id, display_class));
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
        .item(item_id)
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
