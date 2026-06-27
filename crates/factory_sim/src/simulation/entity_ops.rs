use super::*;

impl Simulation {
    pub fn can_place_entity_from_player_inventory(
        &self,
        prototype_id: EntityPrototypeId,
        item_id: ItemId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> Result<EntityFootprint, PlayerBuildError> {
        let prototype = self
            .world
            .prototypes
            .entities
            .get(prototype_id.index())
            .filter(|prototype| prototype.id == prototype_id)
            .ok_or(PlayerBuildError::MissingPrototype(prototype_id))?;
        if prototype.entity_kind == EntityKind::ResourcePatch {
            return Err(PlayerBuildError::MissingBuildItem { prototype_id });
        }

        let item = self
            .world
            .prototypes
            .items
            .get(item_id.index())
            .filter(|item| item.id == item_id)
            .ok_or(PlayerBuildError::MissingBuildItem { prototype_id })?;
        if item.name != prototype.name {
            return Err(PlayerBuildError::ItemDoesNotBuildEntity {
                item_id,
                prototype_id,
            });
        }
        if self.player_inventory.count(item_id) == 0 {
            return Err(PlayerBuildError::InsufficientInventory { item_id });
        }

        self.can_place_entity(prototype_id, x, y, direction)
            .map_err(PlayerBuildError::Build)
    }

    pub fn place_entity_from_player_inventory(
        &mut self,
        prototype_id: EntityPrototypeId,
        item_id: ItemId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> Result<EntityId, PlayerBuildError> {
        self.can_place_entity_from_player_inventory(prototype_id, item_id, x, y, direction)?;

        let entity_id = self
            .place_entity(prototype_id, x, y, direction)
            .map_err(PlayerBuildError::Build)?;
        self.player_inventory
            .remove(item_id, 1)
            .expect("validated player build item should remain removable");

        Ok(entity_id)
    }

    pub fn can_place_entity(
        &self,
        prototype_id: EntityPrototypeId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> Result<EntityFootprint, BuildError> {
        let footprint = self.world.entity_footprint(prototype_id, x, y, direction)?;
        let prototype = self
            .world
            .prototypes
            .entities
            .get(prototype_id.index())
            .filter(|prototype| prototype.id == prototype_id)
            .ok_or(BuildError::MissingPrototype(prototype_id))?;
        self.world
            .validate_entity_footprint_for_prototype(prototype, &footprint)?;
        self.validate_footprint_clear_of_player(&footprint)?;
        self.entities
            .occupancy
            .validate_available(&footprint, None)?;

        Ok(footprint)
    }

    pub fn place_entity(
        &mut self,
        prototype_id: EntityPrototypeId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> Result<EntityId, BuildError> {
        let footprint = self.can_place_entity(prototype_id, x, y, direction)?;
        let prototype = &self.world.prototypes.entities[prototype_id.index()];
        let inventory_slot_count = (prototype.entity_kind == EntityKind::Chest)
            .then_some(prototype.inventory_slot_count)
            .flatten();
        let burner_mining_drill = burner_mining_drill_state_for_prototype(prototype);
        let furnace = furnace_state_for_prototype(prototype);
        let assembling_machine = assembling_machine_state_for_prototype(prototype);
        let lab = lab_state_for_prototype(prototype);
        let transport_belt = transport_belt_segment_for_prototype(prototype, direction);
        let inserter = inserter_state_for_prototype(prototype);
        Ok(self.entities.reserve_entity(EntityReservation {
            prototype_id,
            x,
            y,
            direction,
            footprint,
            inventory_slot_count,
            burner_mining_drill,
            furnace,
            assembling_machine,
            lab,
            transport_belt,
            inserter,
        }))
    }

    pub fn rotate_entity(
        &mut self,
        entity_id: EntityId,
        direction: Direction,
    ) -> Result<(), BuildError> {
        let entity = self
            .entities
            .placed_entity(entity_id)
            .cloned()
            .ok_or(BuildError::MissingEntity(entity_id))?;
        let footprint =
            self.world
                .entity_footprint(entity.prototype_id, entity.x, entity.y, direction)?;
        let prototype = self
            .world
            .prototypes
            .entities
            .get(entity.prototype_id.index())
            .filter(|prototype| prototype.id == entity.prototype_id)
            .ok_or(BuildError::MissingPrototype(entity.prototype_id))?;

        self.world
            .validate_entity_footprint_for_prototype(prototype, &footprint)?;
        self.validate_footprint_clear_of_player(&footprint)?;
        self.entities
            .occupancy
            .validate_available(&footprint, Some(entity_id))?;
        self.entities
            .update_entity_footprint(entity_id, direction, footprint)
    }

    pub fn remove_entity(&mut self, entity_id: EntityId) -> Option<PlacedEntity> {
        self.entities.remove_placed_entity(entity_id)
    }

    pub fn destroy_entity_to_player_inventory(
        &mut self,
        entity_id: EntityId,
    ) -> Result<PlacedEntity, EntityDestroyError> {
        let placed = self
            .entities
            .placed_entity(entity_id)
            .cloned()
            .ok_or(EntityDestroyError::MissingEntity(entity_id))?;
        let recovery_stacks = self.entity_recovery_stacks(&placed)?;
        let mut player_inventory = self.player_inventory.clone();

        for stack in recovery_stacks {
            player_inventory
                .insert(&self.world.prototypes, stack.item_id, stack.count)
                .map_err(|error| match error {
                    InventoryError::InsufficientSpace => {
                        EntityDestroyError::InsufficientInventory {
                            item_id: stack.item_id,
                        }
                    }
                    InventoryError::UnknownItem => EntityDestroyError::UnknownItem(stack.item_id),
                    InventoryError::InsufficientItems => {
                        unreachable!("destroy recovery only inserts items")
                    }
                })?;
        }

        let removed = self
            .entities
            .remove_placed_entity(entity_id)
            .expect("validated placed entity should still be removable");
        self.player_inventory = player_inventory;
        self.manual_mining_progress = None;

        Ok(removed)
    }

    fn entity_recovery_stacks(
        &self,
        placed: &PlacedEntity,
    ) -> Result<Vec<ItemStack>, EntityDestroyError> {
        let mut stacks = Vec::new();
        stacks.push(ItemStack {
            item_id: self.build_item_for_entity(placed.prototype_id)?,
            count: 1,
        });

        if let Some(inventory) = self.entities.entity_inventories.get(&placed.id) {
            push_inventory_stacks(&mut stacks, inventory);
        }
        if let Some(state) = self.entities.burner_mining_drills.get(&placed.id) {
            push_optional_stack(&mut stacks, state.energy.fuel_slot);
            push_optional_stack(&mut stacks, state.output_slot);
        }
        if let Some(state) = self.entities.furnaces.get(&placed.id) {
            push_optional_stack(&mut stacks, state.input_slot);
            push_optional_stack(&mut stacks, state.energy.fuel_slot);
            push_optional_stack(&mut stacks, state.output_slot);
        }
        if let Some(state) = self.entities.assembling_machines.get(&placed.id) {
            push_inventory_stacks(&mut stacks, &state.input_inventory);
            push_inventory_stacks(&mut stacks, &state.output_inventory);
        }
        if let Some(state) = self.entities.labs.get(&placed.id) {
            push_inventory_stacks(&mut stacks, &state.inventory);
        }
        if let Some(segment) = self.entities.transport_belts.get(&placed.id) {
            stacks.extend(segment.lanes.iter().flat_map(|lane| {
                lane.items.iter().map(|item| ItemStack {
                    item_id: item.item_id,
                    count: 1,
                })
            }));
        }
        if let Some(InserterState::Holding { item }) = self.entities.inserters.get(&placed.id) {
            stacks.push(*item);
        }

        Ok(stacks)
    }

    fn build_item_for_entity(
        &self,
        prototype_id: EntityPrototypeId,
    ) -> Result<ItemId, EntityDestroyError> {
        let prototype = self
            .world
            .prototypes
            .entities
            .get(prototype_id.index())
            .filter(|prototype| prototype.id == prototype_id)
            .ok_or(EntityDestroyError::MissingBuildItem { prototype_id })?;

        self.world
            .prototypes
            .items
            .iter()
            .find(|item| item.name == prototype.name)
            .map(|item| item.id)
            .ok_or(EntityDestroyError::MissingBuildItem { prototype_id })
    }

    fn validate_footprint_clear_of_player(
        &self,
        footprint: &EntityFootprint,
    ) -> Result<(), BuildError> {
        let player_tile = self.player.tile_position();
        if footprint.contains_tile(player_tile.0, player_tile.1) {
            return Err(BuildError::TileBlocked {
                x: player_tile.0,
                y: player_tile.1,
            });
        }

        Ok(())
    }

    pub fn entity_inventory(&self, entity_id: EntityId) -> Result<&Inventory, ContainerError> {
        self.entities.entity_inventory(entity_id)
    }

    pub fn entity_inventory_mut(
        &mut self,
        entity_id: EntityId,
    ) -> Result<&mut Inventory, ContainerError> {
        self.entities.entity_inventory_mut(entity_id)
    }

    pub fn transfer_player_slot_to_entity(
        &mut self,
        entity_id: EntityId,
        player_slot_index: usize,
    ) -> Result<(), ContainerError> {
        let stack = stack_in_slot(&self.player_inventory, player_slot_index)?;
        if self.entities.labs.contains_key(&entity_id)
            && !lab_can_accept_item(&self.world.prototypes, stack.item_id)
        {
            return Err(ContainerError::InvalidItem(stack.item_id));
        }
        let entity_inventory = self.entities.entity_inventory(entity_id)?;
        ensure_inventory_can_accept(&self.world.prototypes, entity_inventory, stack)?;

        self.player_inventory.slots[player_slot_index] = None;
        self.entities
            .entity_inventory_mut(entity_id)?
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(ContainerError::from)
    }

    pub fn transfer_entity_slot_to_player(
        &mut self,
        entity_id: EntityId,
        entity_slot_index: usize,
    ) -> Result<(), ContainerError> {
        let stack = {
            let entity_inventory = self.entities.entity_inventory(entity_id)?;
            stack_in_slot(entity_inventory, entity_slot_index)?
        };
        ensure_inventory_can_accept(&self.world.prototypes, &self.player_inventory, stack)?;

        self.entities.entity_inventory_mut(entity_id)?.slots[entity_slot_index] = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(ContainerError::from)
    }

    pub fn burner_drill_state(
        &self,
        entity_id: EntityId,
    ) -> Result<&BurnerMiningDrillState, BurnerDrillError> {
        self.entities.burner_drill_state(entity_id)
    }

    pub fn transfer_player_slot_to_burner_drill_fuel(
        &mut self,
        entity_id: EntityId,
        player_slot_index: usize,
    ) -> Result<(), BurnerDrillError> {
        let stack = self
            .player_inventory
            .slots
            .get(player_slot_index)
            .ok_or(BurnerDrillError::InvalidSlot {
                slot_index: player_slot_index,
            })?
            .ok_or(BurnerDrillError::EmptySlot {
                slot_index: player_slot_index,
            })?;

        if fuel_value_joules(&self.world.prototypes, stack.item_id).is_none() {
            return Err(BurnerDrillError::InvalidFuel(stack.item_id));
        }

        let state = self.entities.burner_drill_state(entity_id)?;
        if !burner_fuel_slot_can_accept(&self.world.prototypes, state.energy.fuel_slot, stack) {
            return Err(BurnerDrillError::InsufficientSpace);
        }

        self.player_inventory.slots[player_slot_index] = None;
        let state = self.entities.burner_drill_state_mut(entity_id)?;
        insert_into_single_slot(&mut state.energy.fuel_slot, stack);

        Ok(())
    }

    pub fn transfer_burner_drill_fuel_to_player(
        &mut self,
        entity_id: EntityId,
    ) -> Result<(), BurnerDrillError> {
        let stack = self
            .entities
            .burner_drill_state(entity_id)?
            .energy
            .fuel_slot
            .ok_or(BurnerDrillError::EmptySlot {
                slot_index: BURNER_MINING_DRILL_FUEL_SLOT_INDEX,
            })?;
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(BurnerDrillError::InsufficientSpace);
        }

        self.entities
            .burner_drill_state_mut(entity_id)?
            .energy
            .fuel_slot = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(BurnerDrillError::from)
    }

    pub fn transfer_burner_drill_output_to_player(
        &mut self,
        entity_id: EntityId,
    ) -> Result<(), BurnerDrillError> {
        let stack = self
            .entities
            .burner_drill_state(entity_id)?
            .output_slot
            .ok_or(BurnerDrillError::EmptySlot {
                slot_index: BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX,
            })?;
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(BurnerDrillError::InsufficientSpace);
        }

        self.entities.burner_drill_state_mut(entity_id)?.output_slot = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(BurnerDrillError::from)
    }

    pub fn furnace_state(&self, entity_id: EntityId) -> Result<&FurnaceState, FurnaceError> {
        self.entities.furnace_state(entity_id)
    }

    pub fn belt_segment(&self, entity_id: EntityId) -> Result<&BeltSegment, BeltError> {
        self.entities.belt_segment(entity_id)
    }

    pub fn inserter_state(&self, entity_id: EntityId) -> Result<&InserterState, InserterError> {
        self.entities.inserter_state(entity_id)
    }

    pub fn lab_state(&self, entity_id: EntityId) -> Result<&LabState, LabError> {
        self.entities.lab_state(entity_id)
    }

    pub fn insert_item_onto_belt(
        &mut self,
        entity_id: EntityId,
        lane_index: usize,
        item_id: ItemId,
    ) -> Result<(), BeltError> {
        self.entities
            .insert_item_onto_belt(entity_id, lane_index, item_id)
    }

    pub fn transfer_player_slot_to_furnace_input(
        &mut self,
        entity_id: EntityId,
        player_slot_index: usize,
    ) -> Result<(), FurnaceError> {
        let stack = self
            .player_inventory
            .slots
            .get(player_slot_index)
            .ok_or(FurnaceError::InvalidSlot {
                slot_index: player_slot_index,
            })?
            .ok_or(FurnaceError::EmptySlot {
                slot_index: player_slot_index,
            })?;

        if first_matching_smelting_recipe(&self.world.prototypes, stack.item_id).is_none() {
            return Err(FurnaceError::InvalidInput(stack.item_id));
        }

        let state = self.entities.furnace_state(entity_id)?;
        if !input_slot_can_accept(&self.world.prototypes, state.input_slot, stack) {
            return Err(FurnaceError::InsufficientSpace);
        }

        self.player_inventory.slots[player_slot_index] = None;
        let state = self.entities.furnace_state_mut(entity_id)?;
        insert_into_single_slot(&mut state.input_slot, stack);

        Ok(())
    }

    pub fn transfer_player_slot_to_furnace_fuel(
        &mut self,
        entity_id: EntityId,
        player_slot_index: usize,
    ) -> Result<(), FurnaceError> {
        let stack = self
            .player_inventory
            .slots
            .get(player_slot_index)
            .ok_or(FurnaceError::InvalidSlot {
                slot_index: player_slot_index,
            })?
            .ok_or(FurnaceError::EmptySlot {
                slot_index: player_slot_index,
            })?;

        if fuel_value_joules(&self.world.prototypes, stack.item_id).is_none() {
            return Err(FurnaceError::InvalidFuel(stack.item_id));
        }

        let state = self.entities.furnace_state(entity_id)?;
        if !burner_fuel_slot_can_accept(&self.world.prototypes, state.energy.fuel_slot, stack) {
            return Err(FurnaceError::InsufficientSpace);
        }

        self.player_inventory.slots[player_slot_index] = None;
        let state = self.entities.furnace_state_mut(entity_id)?;
        insert_into_single_slot(&mut state.energy.fuel_slot, stack);

        Ok(())
    }

    pub fn transfer_furnace_input_to_player(
        &mut self,
        entity_id: EntityId,
    ) -> Result<(), FurnaceError> {
        let stack =
            self.entities
                .furnace_state(entity_id)?
                .input_slot
                .ok_or(FurnaceError::EmptySlot {
                    slot_index: FURNACE_INPUT_SLOT_INDEX,
                })?;
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(FurnaceError::InsufficientSpace);
        }

        self.entities.furnace_state_mut(entity_id)?.input_slot = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(FurnaceError::from)
    }

    pub fn transfer_furnace_fuel_to_player(
        &mut self,
        entity_id: EntityId,
    ) -> Result<(), FurnaceError> {
        let stack = self
            .entities
            .furnace_state(entity_id)?
            .energy
            .fuel_slot
            .ok_or(FurnaceError::EmptySlot {
                slot_index: FURNACE_FUEL_SLOT_INDEX,
            })?;
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(FurnaceError::InsufficientSpace);
        }

        self.entities.furnace_state_mut(entity_id)?.energy.fuel_slot = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(FurnaceError::from)
    }

    pub fn transfer_furnace_output_to_player(
        &mut self,
        entity_id: EntityId,
    ) -> Result<(), FurnaceError> {
        let stack =
            self.entities
                .furnace_state(entity_id)?
                .output_slot
                .ok_or(FurnaceError::EmptySlot {
                    slot_index: FURNACE_OUTPUT_SLOT_INDEX,
                })?;
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(FurnaceError::InsufficientSpace);
        }

        self.entities.furnace_state_mut(entity_id)?.output_slot = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(FurnaceError::from)
    }

    pub fn assembler_state(
        &self,
        entity_id: EntityId,
    ) -> Result<&AssemblingMachineState, AssemblerError> {
        self.entities.assembler_state(entity_id)
    }

    pub fn select_assembler_recipe(
        &mut self,
        entity_id: EntityId,
        recipe_id: RecipeId,
    ) -> Result<(), AssemblerError> {
        let recipe = self
            .world
            .prototypes
            .recipes
            .get(recipe_id.index())
            .filter(|recipe| recipe.id == recipe_id)
            .ok_or(AssemblerError::MissingRecipe(recipe_id))?;
        if recipe.category != CraftingCategory::Crafting {
            return Err(AssemblerError::InvalidRecipe(recipe_id));
        }
        if !self.is_recipe_unlocked(recipe_id) {
            return Err(AssemblerError::RecipeLocked(recipe_id));
        }

        let state = self.entities.assembler_state_mut(entity_id)?;
        if state.selected_recipe == Some(recipe_id) {
            return Ok(());
        }
        if !assembler_is_empty_for_recipe_change(state) {
            return Err(AssemblerError::RecipeChangeRequiresEmpty { entity_id });
        }

        state.selected_recipe = Some(recipe_id);
        state.crafting_progress_ticks = 0;
        state.crafting_required_ticks = assembler_required_ticks(
            recipe.crafting_time_ticks,
            state.crafting_speed_numerator,
            state.crafting_speed_denominator,
        );

        Ok(())
    }

    pub fn can_select_assembler_recipe(
        &self,
        entity_id: EntityId,
        recipe_id: RecipeId,
    ) -> Result<bool, AssemblerError> {
        let recipe = self
            .world
            .prototypes
            .recipes
            .get(recipe_id.index())
            .filter(|recipe| recipe.id == recipe_id)
            .ok_or(AssemblerError::MissingRecipe(recipe_id))?;
        if recipe.category != CraftingCategory::Crafting {
            return Err(AssemblerError::InvalidRecipe(recipe_id));
        }
        if !self.is_recipe_unlocked(recipe_id) {
            return Ok(false);
        }

        let state = self.entities.assembler_state(entity_id)?;
        Ok(state.selected_recipe == Some(recipe_id) || assembler_is_empty_for_recipe_change(state))
    }

    pub fn assembler_ingredient_status(
        &self,
        entity_id: EntityId,
    ) -> Result<Vec<AssemblerIngredientStatus>, AssemblerError> {
        let state = self.entities.assembler_state(entity_id)?;
        let Some(recipe) = selected_assembler_recipe(&self.world.prototypes, state) else {
            return if let Some(recipe_id) = state.selected_recipe {
                Err(AssemblerError::MissingRecipe(recipe_id))
            } else {
                Ok(Vec::new())
            };
        };
        if recipe.category != CraftingCategory::Crafting {
            return Err(AssemblerError::InvalidRecipe(recipe.id));
        }

        Ok(recipe
            .ingredients
            .iter()
            .map(|ingredient| {
                let required = u32::from(ingredient.amount);
                let available = state.input_inventory.count(ingredient.item);
                AssemblerIngredientStatus {
                    item: ingredient.item,
                    required,
                    available,
                    missing: required.saturating_sub(available),
                }
            })
            .collect())
    }

    pub fn transfer_player_slot_to_assembler_input(
        &mut self,
        entity_id: EntityId,
        player_slot_index: usize,
    ) -> Result<(), AssemblerError> {
        let stack = self
            .player_inventory
            .slots
            .get(player_slot_index)
            .ok_or(AssemblerError::InvalidSlot {
                slot_index: player_slot_index,
            })?
            .ok_or(AssemblerError::EmptySlot {
                slot_index: player_slot_index,
            })?;
        let state = self.entities.assembler_state(entity_id)?;
        if !assembler_input_can_accept(&self.world.prototypes, state, stack) {
            return Err(AssemblerError::InvalidInput(stack.item_id));
        }
        if !state
            .input_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(AssemblerError::InsufficientSpace);
        }

        self.player_inventory.slots[player_slot_index] = None;
        self.entities
            .assembler_state_mut(entity_id)?
            .input_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(AssemblerError::from)
    }

    pub fn transfer_assembler_input_slot_to_player(
        &mut self,
        entity_id: EntityId,
        slot_index: usize,
    ) -> Result<(), AssemblerError> {
        let stack = {
            let state = self.entities.assembler_state(entity_id)?;
            stack_in_assembler_inventory_slot(&state.input_inventory, slot_index)?
        };
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(AssemblerError::InsufficientSpace);
        }

        self.entities
            .assembler_state_mut(entity_id)?
            .input_inventory
            .slots[slot_index] = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(AssemblerError::from)
    }

    pub fn transfer_assembler_output_slot_to_player(
        &mut self,
        entity_id: EntityId,
        slot_index: usize,
    ) -> Result<(), AssemblerError> {
        let stack = {
            let state = self.entities.assembler_state(entity_id)?;
            stack_in_assembler_inventory_slot(&state.output_inventory, slot_index)?
        };
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(AssemblerError::InsufficientSpace);
        }

        self.entities
            .assembler_state_mut(entity_id)?
            .output_inventory
            .slots[slot_index] = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(AssemblerError::from)
    }
}

fn push_inventory_stacks(stacks: &mut Vec<ItemStack>, inventory: &Inventory) {
    stacks.extend(inventory.slots.iter().flatten().copied());
}

fn push_optional_stack(stacks: &mut Vec<ItemStack>, stack: Option<ItemStack>) {
    if let Some(stack) = stack {
        stacks.push(stack);
    }
}
