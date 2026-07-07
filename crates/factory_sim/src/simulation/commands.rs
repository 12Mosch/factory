use super::*;

/// A player-issued simulation mutation. All interactive changes to the
/// simulation are expressed as commands and applied at a tick boundary via
/// [`Simulation::apply_command`], so a recorded command stream fully
/// determines the simulation's evolution (replays, scripted end-to-end tests,
/// lockstep multiplayer).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum SimCommand {
    MovePlayer {
        direction_x: f32,
        direction_y: f32,
        delta_seconds: f32,
    },
    SetManualMiningTarget(Option<ManualMiningTarget>),
    StartManualCraft(RecipeId),
    SelectAssemblerRecipe {
        entity_id: EntityId,
        recipe_id: RecipeId,
    },
    EnqueueResearch(TechnologyId),
    RemoveQueuedResearch {
        index: usize,
    },
    MoveQueuedResearch {
        from_index: usize,
        to_index: usize,
    },
    TransferSlot {
        entity_id: EntityId,
        panel: InventoryPanel,
        slot_index: usize,
    },
    PlaceEntityFromPlayerInventory {
        prototype_id: EntityPrototypeId,
        item_id: ItemId,
        x: i32,
        y: i32,
        direction: Direction,
    },
    BuildRedScienceResearchFixture,
}

/// An inventory region of the player or an open entity that a slot click can
/// target. Shared between the simulation's transfer dispatch and the UI's
/// slot buttons.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum InventoryPanel {
    Player,
    Container,
    BurnerFuel,
    BurnerOutput,
    FurnaceInput,
    FurnaceFuel,
    FurnaceOutput,
    BoilerFuel,
    AssemblerInput,
    AssemblerOutput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotTransferError {
    Transfer(ContainerError),
    BurnerDrill(BurnerDrillError),
    Furnace(FurnaceError),
    Boiler(BoilerError),
    Assembler(AssemblerError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimCommandError {
    Crafting(CraftingError),
    Assembler(AssemblerError),
    Research(ResearchError),
    Transfer(SlotTransferError),
    Build(PlayerBuildError),
}

/// State a command produced beyond the mutation itself, for consumers that
/// react to the outcome (e.g. UI feedback).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimCommandEffect {
    None,
    EntityPlaced(EntityId),
}

impl Simulation {
    pub fn apply_command(
        &mut self,
        command: &SimCommand,
    ) -> Result<SimCommandEffect, SimCommandError> {
        match *command {
            SimCommand::MovePlayer {
                direction_x,
                direction_y,
                delta_seconds,
            } => {
                self.move_player(direction_x, direction_y, delta_seconds);
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetManualMiningTarget(target) => {
                self.update_manual_mining(target);
                Ok(SimCommandEffect::None)
            }
            SimCommand::StartManualCraft(recipe_id) => {
                self.start_manual_craft(recipe_id)
                    .map_err(SimCommandError::Crafting)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::SelectAssemblerRecipe {
                entity_id,
                recipe_id,
            } => {
                self.select_assembler_recipe(entity_id, recipe_id)
                    .map_err(SimCommandError::Assembler)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::EnqueueResearch(technology_id) => {
                self.enqueue_research(technology_id)
                    .map_err(SimCommandError::Research)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::RemoveQueuedResearch { index } => {
                self.remove_queued_research(index)
                    .map_err(SimCommandError::Research)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::MoveQueuedResearch {
                from_index,
                to_index,
            } => {
                self.move_queued_research(from_index, to_index)
                    .map_err(SimCommandError::Research)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::TransferSlot {
                entity_id,
                panel,
                slot_index,
            } => {
                entity_transfer::transfer_container_slot(self, entity_id, panel, slot_index)
                    .map_err(SimCommandError::Transfer)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::PlaceEntityFromPlayerInventory {
                prototype_id,
                item_id,
                x,
                y,
                direction,
            } => {
                let entity_id = placement::place_from_player_inventory(
                    self,
                    placement::PlayerPlacementRequest {
                        prototype_id,
                        item_id,
                        x,
                        y,
                        direction,
                    },
                )
                .map_err(SimCommandError::Build)?;
                Ok(SimCommandEffect::EntityPlaced(entity_id))
            }
            SimCommand::BuildRedScienceResearchFixture => {
                self.build_red_science_research_fixture();
                Ok(SimCommandEffect::None)
            }
        }
    }
}
