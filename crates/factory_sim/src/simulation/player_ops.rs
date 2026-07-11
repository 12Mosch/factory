use super::*;

impl Simulation {
    pub fn move_player(&mut self, direction_x: f32, direction_y: f32, delta_seconds: f32) {
        if delta_seconds <= 0.0 {
            return;
        }

        let direction_length = (direction_x * direction_x + direction_y * direction_y).sqrt();
        if direction_length <= f32::EPSILON {
            return;
        }

        let distance = PLAYER_MOVEMENT_SPEED_TILES_PER_SECOND * delta_seconds;
        self.move_player_by_tiles(
            direction_x / direction_length * distance,
            direction_y / direction_length * distance,
        );
    }

    pub fn move_player_by_tiles(&mut self, delta_x_tiles: f32, delta_y_tiles: f32) {
        let delta_x = tiles_to_fixed(delta_x_tiles);
        let delta_y = tiles_to_fixed(delta_y_tiles);

        self.try_move_player_axis(delta_x, 0);
        self.try_move_player_axis(0, delta_y);
    }

    pub fn can_player_occupy_tile(&self, x: WorldTileCoord, y: WorldTileCoord) -> bool {
        player_can_occupy_tile(&self.world, &self.entities.occupancy, x, y)
    }

    pub fn update_manual_mining(&mut self, target: Option<ManualMiningTarget>) {
        let Some(target) = target else {
            self.manual_mining_progress = None;
            return;
        };

        if !self.is_valid_manual_mining_target(target) {
            self.manual_mining_progress = None;
            return;
        }

        let mut progress = match self.manual_mining_progress {
            Some(progress) if progress.target == target => progress,
            _ => ManualMiningProgress {
                target,
                progress_ticks: 0,
                required_ticks: MANUAL_MINING_TICKS_PER_ITEM,
            },
        };

        if progress.progress_ticks < progress.required_ticks {
            progress.progress_ticks += 1;
        }

        if progress.progress_ticks < progress.required_ticks {
            self.manual_mining_progress = Some(progress);
            return;
        }

        if let Some(entity_id) = self.entities.occupancy.entity_at(target.x, target.y) {
            match crate::entity_mutation::destroy_to_player_inventory(self, entity_id) {
                Ok(_) => {
                    self.manual_mining_progress = if self.is_valid_manual_mining_target(target) {
                        Some(ManualMiningProgress {
                            target,
                            progress_ticks: 0,
                            required_ticks: MANUAL_MINING_TICKS_PER_ITEM,
                        })
                    } else {
                        None
                    };
                }
                Err(EntityDestroyError::InsufficientInventory { .. })
                | Err(EntityDestroyError::UnknownItem(_))
                | Err(EntityDestroyError::MissingBuildItem { .. }) => {
                    self.manual_mining_progress = Some(progress);
                }
                Err(EntityDestroyError::MissingEntity(_)) => {
                    self.manual_mining_progress = None;
                }
            }
            return;
        }

        let resource_item = self
            .world
            .tile_at(target.x, target.y)
            .and_then(|tile| tile.resource.map(|resource| resource.resource_item));
        let Some(resource_item) = resource_item else {
            self.manual_mining_progress = None;
            return;
        };

        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, resource_item, 1)
        {
            self.manual_mining_progress = Some(progress);
            return;
        }

        let mined = self
            .world
            .mine_resource_at(target.x, target.y, 1)
            .expect("validated manual mining target should still contain a resource");
        debug_assert_eq!(mined.resource_item, resource_item);
        debug_assert_eq!(mined.amount, 1);
        self.player_inventory
            .insert(&self.world.prototypes, mined.resource_item, 1)
            .expect("manual mining checked inventory capacity before inserting");
        self.record_item_produced(mined.resource_item, u64::from(mined.amount));
        let base = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes);
        if mined.resource_item == base.items.iron_ore {
            self.early_game_progress.iron_ore_manually_mined = self
                .early_game_progress
                .iron_ore_manually_mined
                .saturating_add(u64::from(mined.amount));
            self.early_game_progress.changed();
        }

        self.manual_mining_progress = if self.is_valid_manual_mining_target(target) {
            Some(ManualMiningProgress {
                target,
                progress_ticks: 0,
                required_ticks: MANUAL_MINING_TICKS_PER_ITEM,
            })
        } else {
            None
        };
    }

    pub fn start_manual_craft(&mut self, recipe_id: RecipeId) -> Result<(), CraftingError> {
        let recipe = self
            .world
            .prototypes
            .recipe(recipe_id)
            .ok_or(CraftingError::MissingRecipe(recipe_id))?;

        if !matches!(
            recipe.category,
            CraftingCategory::Crafting | CraftingCategory::Manual
        ) {
            return Err(CraftingError::NotManualRecipe(recipe_id));
        }
        if !self.is_recipe_unlocked(recipe_id) {
            return Err(CraftingError::RecipeLocked(recipe_id));
        }

        for ingredient in &recipe.ingredients {
            let required = recipe
                .ingredients
                .iter()
                .filter(|candidate| candidate.item == ingredient.item)
                .map(|candidate| u32::from(candidate.amount))
                .sum();
            if self.player_inventory.count(ingredient.item) < required {
                return Err(CraftingError::InsufficientIngredients);
            }
        }

        let ingredients = recipe.ingredients.clone();
        let crafting_time_ticks = recipe.crafting_time_ticks;
        for ingredient in &ingredients {
            self.player_inventory
                .remove(ingredient.item, ingredient.amount)
                .expect("manual crafting checked ingredients before removing");
            self.record_item_consumed(ingredient.item, u64::from(ingredient.amount));
        }

        self.crafting_queue.entries.push_back(CraftingJob {
            recipe_id,
            remaining_ticks: crafting_time_ticks,
        });

        Ok(())
    }

    pub(super) fn advance_manual_crafting(&mut self) {
        let Some(job) = self.crafting_queue.entries.front_mut() else {
            return;
        };

        if job.remaining_ticks > 0 {
            job.remaining_ticks -= 1;
        }

        if job.remaining_ticks > 0 {
            return;
        }

        let recipe_id = job.recipe_id;
        let recipe = self
            .world
            .prototypes
            .recipe(recipe_id)
            .expect("queued manual craft should reference an existing recipe");
        let products = recipe.products.clone();
        let mut inventory = self.player_inventory.clone();

        for product in &products {
            if inventory
                .insert(&self.world.prototypes, product.item, product.amount)
                .is_err()
            {
                return;
            }
        }

        self.player_inventory = inventory;
        self.crafting_queue.entries.pop_front();
        for product in &products {
            self.record_item_produced(product.item, u64::from(product.amount));
            let base = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes);
            if product.item == base.items.transport_belt {
                self.early_game_progress.transport_belts_manually_crafted = self
                    .early_game_progress
                    .transport_belts_manually_crafted
                    .saturating_add(u64::from(product.amount));
                self.early_game_progress.changed();
            }
        }
    }

    pub(super) fn is_valid_manual_mining_target(&self, target: ManualMiningTarget) -> bool {
        (self
            .entities
            .occupancy
            .entity_at(target.x, target.y)
            .is_some()
            || self
                .world
                .tile_at(target.x, target.y)
                .and_then(|tile| tile.resource)
                .is_some_and(|resource| {
                    !is_fluid_resource_item(&self.world.prototypes, resource.resource_item)
                }))
            && self.is_manual_mining_target_in_reach(target)
    }

    pub(super) fn is_manual_mining_target_in_reach(&self, target: ManualMiningTarget) -> bool {
        let reach = tiles_to_fixed(MANUAL_MINING_REACH_TILES);
        let dx = self.player.x - tile_center_fixed(target.x);
        let dy = self.player.y - tile_center_fixed(target.y);

        i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy)
            <= i128::from(reach) * i128::from(reach)
    }

    pub(super) fn try_move_player_axis(&mut self, delta_x: i64, delta_y: i64) {
        if delta_x == 0 && delta_y == 0 {
            return;
        }

        let Some(x) = self.player.x.checked_add(delta_x) else {
            return;
        };
        let Some(y) = self.player.y.checked_add(delta_y) else {
            return;
        };
        let candidate = PlayerState {
            x,
            y,
            ..self.player
        };
        let (tile_x, tile_y) = candidate.tile_position();
        let Some(candidate_chunk) = ChunkCoord::from_tile(tile_x, tile_y) else {
            return;
        };
        self.world.ensure_chunk_generated(candidate_chunk);

        if self.can_player_occupy_tile(tile_x, tile_y) {
            self.player = candidate;
        }
    }
}

impl PlayerState {
    pub fn position_tiles(self) -> (f32, f32) {
        (
            self.x as f32 / PLAYER_POSITION_SCALE as f32,
            self.y as f32 / PLAYER_POSITION_SCALE as f32,
        )
    }

    pub fn tile_position(self) -> (WorldTileCoord, WorldTileCoord) {
        (fixed_to_tile(self.x), fixed_to_tile(self.y))
    }

    pub fn x_fixed(self) -> i64 {
        self.x
    }

    pub fn y_fixed(self) -> i64 {
        self.y
    }

    pub(super) fn centered_on_tile<X: Into<WorldTileCoord>, Y: Into<WorldTileCoord>>(
        x: X,
        y: Y,
    ) -> Self {
        Self {
            x: tile_center_fixed(x.into()),
            y: tile_center_fixed(y.into()),
            repair_remaining_health: 0,
        }
    }
}
