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
        x: WorldTileCoord,
        y: WorldTileCoord,
        direction: Direction,
    },
    /// Plans an entity as a ghost without consuming items.
    PlaceGhost {
        prototype_id: EntityPrototypeId,
        x: WorldTileCoord,
        y: WorldTileCoord,
        direction: Direction,
    },
    CancelGhost {
        ghost_id: GhostId,
    },
    /// Manually builds a planned ghost from the player inventory.
    BuildGhost {
        ghost_id: GhostId,
    },
    /// Deconstruction planner: marks every entity intersecting the tile
    /// rectangle for deconstruction and cancels ghosts in the area.
    MarkDeconstruction {
        min_x: WorldTileCoord,
        min_y: WorldTileCoord,
        max_x: WorldTileCoord,
        max_y: WorldTileCoord,
    },
    CancelDeconstruction {
        min_x: WorldTileCoord,
        min_y: WorldTileCoord,
        max_x: WorldTileCoord,
        max_y: WorldTileCoord,
    },
    /// Manually deconstructs a marked entity into the player inventory.
    DeconstructEntity {
        entity_id: EntityId,
    },
    /// Repairs a damaged entity near the player, consuming repair packs.
    /// Sent repeatedly while the repair input is held.
    RepairEntity {
        entity_id: EntityId,
    },
    /// Places ghosts for the given blueprint entries with the blueprint
    /// origin at `(x, y)`; blocked entries are skipped.
    PasteBlueprint {
        entities: Vec<BlueprintEntity>,
        x: WorldTileCoord,
        y: WorldTileCoord,
    },
    /// Captures the tile rectangle into the blueprint library.
    SaveBlueprint {
        name: String,
        min_x: WorldTileCoord,
        min_y: WorldTileCoord,
        max_x: WorldTileCoord,
        max_y: WorldTileCoord,
    },
    DeleteBlueprint {
        index: usize,
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
    Construction(ConstructionError),
    Repair(RepairError),
}

/// State a command produced beyond the mutation itself, for consumers that
/// react to the outcome (e.g. UI feedback).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimCommandEffect {
    None,
    EntityPlaced(EntityId),
    GhostPlaced(GhostId),
    EntityDeconstructed(EntityId),
    DeconstructionMarked {
        marked: usize,
        ghosts_removed: usize,
    },
    DeconstructionCancelled {
        cancelled: usize,
    },
    BlueprintPasted {
        placed: usize,
        skipped: usize,
    },
    BlueprintSaved {
        index: usize,
    },
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
            SimCommand::PlaceGhost {
                prototype_id,
                x,
                y,
                direction,
            } => {
                let ghost_id = construction_ops::place_ghost(
                    self,
                    GhostPlacementRequest {
                        prototype_id,
                        x,
                        y,
                        direction,
                        recipe: None,
                    },
                )
                .map_err(SimCommandError::Construction)?;
                Ok(SimCommandEffect::GhostPlaced(ghost_id))
            }
            SimCommand::CancelGhost { ghost_id } => {
                construction_ops::cancel_ghost(self, ghost_id)
                    .map_err(SimCommandError::Construction)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::BuildGhost { ghost_id } => {
                let entity_id = construction_ops::build_ghost_from_player_inventory(self, ghost_id)
                    .map_err(SimCommandError::Construction)?;
                Ok(SimCommandEffect::EntityPlaced(entity_id))
            }
            SimCommand::MarkDeconstruction {
                min_x,
                min_y,
                max_x,
                max_y,
            } => {
                let (marked, ghosts_removed) = construction_ops::mark_area_for_deconstruction(
                    self, min_x, min_y, max_x, max_y,
                );
                Ok(SimCommandEffect::DeconstructionMarked {
                    marked,
                    ghosts_removed,
                })
            }
            SimCommand::CancelDeconstruction {
                min_x,
                min_y,
                max_x,
                max_y,
            } => {
                let cancelled = construction_ops::cancel_deconstruction_in_area(
                    self, min_x, min_y, max_x, max_y,
                );
                Ok(SimCommandEffect::DeconstructionCancelled { cancelled })
            }
            SimCommand::DeconstructEntity { entity_id } => {
                construction_ops::deconstruct_marked(self, entity_id)
                    .map_err(SimCommandError::Construction)?;
                Ok(SimCommandEffect::EntityDeconstructed(entity_id))
            }
            SimCommand::RepairEntity { entity_id } => {
                self.repair_entity(entity_id)
                    .map_err(SimCommandError::Repair)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::PasteBlueprint { ref entities, x, y } => {
                let (placed, skipped) =
                    construction_ops::paste_blueprint_ghosts(self, entities, x, y);
                Ok(SimCommandEffect::BlueprintPasted { placed, skipped })
            }
            SimCommand::SaveBlueprint {
                ref name,
                min_x,
                min_y,
                max_x,
                max_y,
            } => {
                let index = construction_ops::save_blueprint_from_area(
                    self, name, min_x, min_y, max_x, max_y,
                )
                .map_err(SimCommandError::Construction)?;
                Ok(SimCommandEffect::BlueprintSaved { index })
            }
            SimCommand::DeleteBlueprint { index } => {
                construction_ops::delete_blueprint(self, index)
                    .map_err(SimCommandError::Construction)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::BuildRedScienceResearchFixture => {
                self.build_red_science_research_fixture();
                Ok(SimCommandEffect::None)
            }
        }
    }
}
