use super::research_ops::{
    add_research_units_to_state, can_select_research_in_state,
    promote_next_queued_research_in_state,
};
use super::*;

impl Simulation {
    pub fn new(seed: u64, prototypes: PrototypeCatalog) -> Self {
        let world = WorldSim::new(seed, prototypes);
        let research = ResearchState::from_catalog(&world.prototypes);
        let entities = EntityStore::new_test_entities(seed);
        let player = find_player_start(&world, &entities.occupancy)
            .expect("test world should contain a walkable player start");
        let mut player_inventory = Inventory::player();
        let burner_mining_drill = item_id(&world.prototypes, "burner_mining_drill");
        let stone_furnace = item_id(&world.prototypes, "stone_furnace");
        player_inventory
            .insert(&world.prototypes, burner_mining_drill, 1)
            .expect("player starting inventory should accept burner mining drill");
        player_inventory
            .insert(&world.prototypes, stone_furnace, 1)
            .expect("player starting inventory should accept stone furnace");

        let mut sim = Self {
            tick: 0,
            entity_topology_revision: 0,
            revealed_revision: 0,
            world,
            chart: ChartState::default(),
            entities,
            player,
            player_inventory,
            manual_mining_progress: None,
            crafting_queue: CraftingQueue::default(),
            research,
            power: PowerSubsystem::default(),
            fluids: FluidSubsystem::default(),
            statistics: StatisticsSubsystem::default(),
            transport: TransportLaneCache::default(),
        };
        sim.reveal_chunks_around_player();
        sim
    }

    pub fn new_test_world(seed: u64) -> Self {
        Self::new(
            seed,
            PrototypeCatalog::load_base().expect("base prototype catalog should load"),
        )
    }

    pub fn new_seeded(seed: u64) -> Self {
        Self::new_test_world(seed)
    }

    pub fn tick(&mut self) {
        crate::tick::advance_simulation(self);
    }

    pub(crate) fn advance_one_tick<P: TickProfiler>(&mut self, profiler: &mut P) {
        self.tick += 1;
        self.advance_statistics_to_current_tick();
        self.reveal_chunks_around_player();
        profiler.measure(ProfilePhase::EntityMotion, || {
            self.entities.advance(Tick(self.tick), self.world.seed);
        });
        profiler.measure(ProfilePhase::Belts, || self.advance_transport_belts());
        profiler.measure(ProfilePhase::Fluids, || self.advance_fluids_before_power());
        profiler.measure(ProfilePhase::PowerRebuild, || self.refresh_power_state());

        let machines = profiler.begin();
        self.advance_burner_mining_drills(profiler);
        self.advance_furnaces(profiler);
        self.advance_assembling_machines(profiler);
        self.advance_labs(profiler);
        profiler.finish(ProfilePhase::Machines, machines);

        let inserters = profiler.begin();
        self.advance_inserters(profiler);
        profiler.finish(ProfilePhase::Inserters, inserters);

        profiler.measure(ProfilePhase::ManualCrafting, || {
            self.advance_manual_crafting();
        });
    }

    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    pub fn entity_topology_revision(&self) -> u64 {
        self.entity_topology_revision
    }

    pub(crate) fn bump_entity_topology_revision(&mut self) {
        self.entity_topology_revision = self.entity_topology_revision.wrapping_add(1);
    }

    pub fn revealed_revision(&self) -> u64 {
        self.revealed_revision
    }

    pub fn current_tick(&self) -> Tick {
        Tick(self.tick)
    }

    pub fn seed(&self) -> u64 {
        self.world.seed
    }

    pub fn prototype_count(&self) -> usize {
        self.world.prototypes.item_count()
    }

    pub fn state_hash(&self) -> u64 {
        let mut hasher = StableHasher::default();
        "factory-sim-state-v1".hash(&mut hasher);
        self.tick.hash(&mut hasher);
        self.world.seed.hash(&mut hasher);
        prototype_hash(&self.world.prototypes).hash(&mut hasher);
        self.world.chunks.hash(&mut hasher);
        self.chart.hash(&mut hasher);
        self.statistics.items.hash(&mut hasher);
        self.statistics.fluids.hash(&mut hasher);
        self.statistics.power.hash(&mut hasher);
        self.entities.hash(&mut hasher);
        self.player.hash(&mut hasher);
        self.player_inventory.hash(&mut hasher);
        self.manual_mining_progress.hash(&mut hasher);
        self.crafting_queue.hash(&mut hasher);
        self.research.active.hash(&mut hasher);
        self.research.queue.hash(&mut hasher);
        self.research.technologies.hash(&mut hasher);
        self.power.summary.hash(&mut hasher);
        self.power.networks.hash(&mut hasher);
        self.power.entity_statuses.hash(&mut hasher);
        self.fluids.networks.hash(&mut hasher);
        hasher.finish()
    }

    pub fn world(&self) -> &WorldSim {
        &self.world
    }

    pub fn ensure_chunk_generated(&mut self, coord: ChunkCoord) -> bool {
        self.world.ensure_chunk_generated(coord)
    }

    pub fn entities(&self) -> &EntityStore {
        &self.entities
    }

    pub fn power_summary(&self) -> PowerSummary {
        self.power.summary
    }

    pub fn power_networks(&self) -> &[PowerNetworkSnapshot] {
        &self.power.networks
    }

    pub fn fluid_networks(&self) -> &[FluidNetworkSnapshot] {
        &self.fluids.networks
    }

    pub fn entity_power_status(&self, entity_id: EntityId) -> Option<EntityPowerStatus> {
        self.power.entity_statuses.get(&entity_id).copied()
    }

    pub fn player(&self) -> PlayerState {
        self.player
    }

    pub fn player_inventory(&self) -> &Inventory {
        &self.player_inventory
    }

    pub fn crafting_queue(&self) -> &CraftingQueue {
        &self.crafting_queue
    }

    pub fn player_inventory_mut(&mut self) -> &mut Inventory {
        &mut self.player_inventory
    }

    pub fn manual_mining_progress(&self) -> Option<ManualMiningProgress> {
        self.manual_mining_progress
    }

    pub fn catalog(&self) -> &PrototypeCatalog {
        &self.world.prototypes
    }

    pub fn is_technology_unlocked(&self, technology_id: TechnologyId) -> bool {
        self.research
            .technology_state(technology_id)
            .is_some_and(|state| state.unlocked)
    }

    pub fn technology_progress(&self, technology_id: TechnologyId) -> Option<u32> {
        self.research
            .technology_state(technology_id)
            .map(|state| state.progress_units)
    }

    pub fn active_research(&self) -> Option<TechnologyId> {
        self.research.active
    }

    pub fn research_queue(&self) -> &[TechnologyId] {
        &self.research.queue
    }

    pub fn select_research(&mut self, technology_id: TechnologyId) -> Result<(), ResearchError> {
        self.can_select_research(technology_id)?;
        self.research.active = Some(technology_id);
        self.research
            .queue
            .retain(|queued_id| *queued_id != technology_id);
        self.prune_invalid_research_queue();
        Ok(())
    }

    pub fn can_enqueue_research(&self, technology_id: TechnologyId) -> Result<(), ResearchError> {
        let mut proposed_queue = self.research.queue.clone();
        proposed_queue.push(technology_id);
        self.validate_research_queue_order(&proposed_queue)
    }

    pub fn enqueue_research(&mut self, technology_id: TechnologyId) -> Result<(), ResearchError> {
        self.can_enqueue_research(technology_id)?;
        self.research.queue.push(technology_id);
        self.promote_next_queued_research()?;
        Ok(())
    }

    pub fn remove_queued_research(
        &mut self,
        index: usize,
    ) -> Result<Vec<TechnologyId>, ResearchError> {
        if index >= self.research.queue.len() {
            return Err(ResearchError::InvalidQueueIndex { index });
        }

        Ok(self.remove_queued_research_and_dependents(index))
    }

    pub fn move_queued_research(
        &mut self,
        from_index: usize,
        to_index: usize,
    ) -> Result<(), ResearchError> {
        let len = self.research.queue.len();
        if from_index >= len {
            return Err(ResearchError::InvalidQueueIndex { index: from_index });
        }
        if to_index >= len {
            return Err(ResearchError::InvalidQueueIndex { index: to_index });
        }
        if from_index == to_index {
            return Ok(());
        }

        let mut proposed_queue = self.research.queue.clone();
        let technology_id = proposed_queue.remove(from_index);
        proposed_queue.insert(to_index, technology_id);
        self.validate_research_queue_order(&proposed_queue)?;
        self.research.queue = proposed_queue;
        Ok(())
    }

    pub fn add_research_units(
        &mut self,
        units: u32,
    ) -> Result<ResearchProgressResult, ResearchError> {
        add_research_units_to_state(&self.world.prototypes, &mut self.research, units)
    }

    pub fn is_recipe_unlocked(&self, recipe_id: RecipeId) -> bool {
        recipe_is_unlocked(&self.world.prototypes, &self.research, recipe_id)
    }

    pub fn is_entity_unlocked(&self, prototype_id: EntityPrototypeId) -> bool {
        let Some(prototype) = self.world.prototypes.entity(prototype_id) else {
            return false;
        };
        let Some(build_item) = prototype.build_item else {
            return false;
        };

        self.world.prototypes.recipes.iter().any(|recipe| {
            recipe
                .products
                .iter()
                .any(|product| product.item == build_item)
                && self.is_recipe_unlocked(recipe.id)
        })
    }

    pub fn available_recipes(
        &self,
        category: CraftingCategory,
    ) -> Vec<&factory_data::RecipePrototype> {
        self.world
            .prototypes
            .recipes
            .iter()
            .filter(|recipe| recipe.category == category && self.is_recipe_unlocked(recipe.id))
            .collect()
    }

    fn can_select_research(&self, technology_id: TechnologyId) -> Result<(), ResearchError> {
        can_select_research_in_state(&self.world.prototypes, &self.research, technology_id)
    }

    fn promote_next_queued_research(&mut self) -> Result<(), ResearchError> {
        promote_next_queued_research_in_state(&self.world.prototypes, &mut self.research)
    }

    fn validate_research_queue_order(&self, queue: &[TechnologyId]) -> Result<(), ResearchError> {
        let mut available = self.researched_technology_ids();
        if let Some(active_id) = self.research.active {
            let state = self.technology_research_state(active_id)?;
            if state.unlocked {
                return Err(ResearchError::AlreadyResearched(active_id));
            }
            available.push(active_id);
        }
        let mut seen = Vec::new();

        for technology_id in queue {
            if self.research.active == Some(*technology_id) {
                return Err(ResearchError::AlreadyActive(*technology_id));
            }
            if seen.contains(technology_id) {
                return Err(ResearchError::AlreadyQueued(*technology_id));
            }

            let technology = self.technology_by_id(*technology_id)?;
            let state = self.technology_research_state(*technology_id)?;
            if state.unlocked {
                return Err(ResearchError::AlreadyResearched(*technology_id));
            }

            for prerequisite_id in &technology.prerequisites {
                if !available.contains(prerequisite_id) {
                    return Err(ResearchError::PrerequisiteLocked {
                        technology_id: *technology_id,
                        prerequisite_id: *prerequisite_id,
                    });
                }
            }

            seen.push(*technology_id);
            available.push(*technology_id);
        }

        Ok(())
    }

    fn remove_queued_research_and_dependents(&mut self, index: usize) -> Vec<TechnologyId> {
        let old_queue = std::mem::take(&mut self.research.queue);
        let mut available = self.researched_technology_ids();
        if let Some(active_id) = self.research.active {
            available.push(active_id);
        }
        let mut new_queue = Vec::with_capacity(old_queue.len().saturating_sub(1));
        let mut removed = Vec::new();

        for (candidate_index, technology_id) in old_queue.into_iter().enumerate() {
            if candidate_index == index {
                removed.push(technology_id);
                continue;
            }

            if self.queued_technology_prerequisites_satisfied(technology_id, &available) {
                available.push(technology_id);
                new_queue.push(technology_id);
            } else {
                removed.push(technology_id);
            }
        }

        self.research.queue = new_queue;
        removed
    }

    fn prune_invalid_research_queue(&mut self) {
        let old_queue = std::mem::take(&mut self.research.queue);
        let mut available = self.researched_technology_ids();
        if let Some(active_id) = self.research.active {
            available.push(active_id);
        }
        let mut new_queue = Vec::with_capacity(old_queue.len());

        for technology_id in old_queue {
            if self.queued_technology_prerequisites_satisfied(technology_id, &available) {
                available.push(technology_id);
                new_queue.push(technology_id);
            }
        }

        self.research.queue = new_queue;
    }

    fn queued_technology_prerequisites_satisfied(
        &self,
        technology_id: TechnologyId,
        available: &[TechnologyId],
    ) -> bool {
        let Ok(technology) = self.technology_by_id(technology_id) else {
            return false;
        };
        let Ok(state) = self.technology_research_state(technology_id) else {
            return false;
        };
        !state.unlocked
            && self.research.active != Some(technology_id)
            && technology
                .prerequisites
                .iter()
                .all(|prerequisite_id| available.contains(prerequisite_id))
    }

    fn researched_technology_ids(&self) -> Vec<TechnologyId> {
        self.research
            .technologies
            .iter()
            .filter(|state| state.unlocked)
            .map(|state| state.technology_id)
            .collect()
    }

    fn technology_by_id(
        &self,
        technology_id: TechnologyId,
    ) -> Result<&factory_data::TechnologyPrototype, ResearchError> {
        self.world
            .prototypes
            .technology(technology_id)
            .ok_or(ResearchError::MissingTechnology(technology_id))
    }

    fn technology_research_state(
        &self,
        technology_id: TechnologyId,
    ) -> Result<&TechnologyResearchState, ResearchError> {
        self.research
            .technology_state(technology_id)
            .ok_or(ResearchError::MissingTechnology(technology_id))
    }
}
