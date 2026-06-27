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

        Self {
            tick: 0,
            world,
            entities,
            player,
            player_inventory,
            manual_mining_progress: None,
            crafting_queue: CraftingQueue::default(),
            research,
        }
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

    pub(crate) fn advance_one_tick(&mut self) {
        self.tick += 1;
        self.entities.advance(Tick(self.tick), self.world.seed);
        self.advance_transport_belts();
        self.advance_burner_mining_drills();
        self.advance_furnaces();
        self.advance_assembling_machines();
        self.advance_labs();
        self.advance_inserters();
        self.advance_manual_crafting();
    }

    pub fn tick_count(&self) -> u64 {
        self.tick
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
        self.entities.hash(&mut hasher);
        self.player.hash(&mut hasher);
        self.player_inventory.hash(&mut hasher);
        self.manual_mining_progress.hash(&mut hasher);
        self.crafting_queue.hash(&mut hasher);
        self.research.active.hash(&mut hasher);
        self.research.technologies.hash(&mut hasher);
        hasher.finish()
    }

    pub fn world(&self) -> &WorldSim {
        &self.world
    }

    pub fn entities(&self) -> &EntityStore {
        &self.entities
    }

    pub fn player(&self) -> PlayerState {
        self.player
    }

    pub fn player_inventory(&self) -> &Inventory {
        &self.player_inventory
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
            .technologies
            .get(technology_id.index())
            .filter(|state| state.technology_id == technology_id)
            .is_some_and(|state| state.unlocked)
    }

    pub fn technology_progress(&self, technology_id: TechnologyId) -> Option<u32> {
        self.research
            .technologies
            .get(technology_id.index())
            .filter(|state| state.technology_id == technology_id)
            .map(|state| state.progress_units)
    }

    pub fn select_research(&mut self, technology_id: TechnologyId) -> Result<(), ResearchError> {
        let technology = self
            .world
            .prototypes
            .technologies
            .get(technology_id.index())
            .filter(|technology| technology.id == technology_id)
            .ok_or(ResearchError::MissingTechnology(technology_id))?;
        let state = self
            .research
            .technologies
            .get(technology_id.index())
            .filter(|state| state.technology_id == technology_id)
            .ok_or(ResearchError::MissingTechnology(technology_id))?;
        if state.unlocked {
            return Err(ResearchError::AlreadyResearched(technology_id));
        }

        for prerequisite_id in &technology.prerequisites {
            if !self.is_technology_unlocked(*prerequisite_id) {
                return Err(ResearchError::PrerequisiteLocked {
                    technology_id,
                    prerequisite_id: *prerequisite_id,
                });
            }
        }

        self.research.active = Some(technology_id);
        Ok(())
    }

    pub fn add_research_units(
        &mut self,
        units: u32,
    ) -> Result<ResearchProgressResult, ResearchError> {
        let technology_id = self
            .research
            .active
            .ok_or(ResearchError::NoActiveResearch)?;
        let technology = self
            .world
            .prototypes
            .technologies
            .get(technology_id.index())
            .filter(|technology| technology.id == technology_id)
            .ok_or(ResearchError::MissingTechnology(technology_id))?;
        let state = self
            .research
            .technologies
            .get_mut(technology_id.index())
            .filter(|state| state.technology_id == technology_id)
            .ok_or(ResearchError::MissingTechnology(technology_id))?;

        state.progress_units = state
            .progress_units
            .saturating_add(units)
            .min(technology.required_units);

        if state.progress_units >= technology.required_units {
            state.unlocked = true;
            self.research.active = None;
            Ok(ResearchProgressResult::Completed { technology_id })
        } else {
            Ok(ResearchProgressResult::InProgress {
                technology_id,
                progress_units: state.progress_units,
                required_units: technology.required_units,
            })
        }
    }

    pub fn is_recipe_unlocked(&self, recipe_id: RecipeId) -> bool {
        let is_locked_by_technology =
            self.world.prototypes.technologies.iter().any(|technology| {
                technology.effects.iter().any(|effect| {
                    matches!(effect, TechnologyEffect::UnlockRecipe(unlocked_recipe_id) if *unlocked_recipe_id == recipe_id)
                })
            });
        if !is_locked_by_technology {
            return true;
        }

        self.world.prototypes.technologies.iter().any(|technology| {
            self.is_technology_unlocked(technology.id)
                && technology.effects.iter().any(|effect| {
                    matches!(effect, TechnologyEffect::UnlockRecipe(unlocked_recipe_id) if *unlocked_recipe_id == recipe_id)
                })
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
}
