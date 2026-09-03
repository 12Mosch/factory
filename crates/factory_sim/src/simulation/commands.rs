use super::*;

/// A player-issued simulation mutation. All interactive changes to the
/// simulation are expressed as commands and applied at a tick boundary via
/// [`Simulation::apply_command`], so a recorded command stream fully
/// determines the simulation's evolution (replays, scripted end-to-end tests,
/// lockstep multiplayer).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum SimCommand {
    SetEnemyRuntimeSettings(EnemyRuntimeSettings),
    MovePlayer {
        direction_x: f32,
        direction_y: f32,
        delta_seconds: f32,
    },
    SetManualMiningTarget(Option<ManualMiningTarget>),
    /// Selects the next held weapon present in the player inventory.
    CyclePlayerWeapon,
    /// Fires the selected weapon at the hostile combatant occupying this tile.
    AttackWithPlayerWeapon {
        x: WorldTileCoord,
        y: WorldTileCoord,
    },
    StartManualCraft(RecipeId),
    CancelManualCraft {
        job_id: CraftingJobId,
    },
    MoveManualCraft {
        job_id: CraftingJobId,
        direction: CraftingQueueMove,
    },
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
    /// The same, against a piece of rolling stock. A separate command rather
    /// than an `EntityId` that might not be one: stock is not a placed entity,
    /// and a command that could name either would be a command the router has
    /// to guess about.
    TransferRollingStockSlot {
        stock_id: RollingStockId,
        panel: InventoryPanel,
        slot_index: usize,
    },
    SetRollingStockSlotFilter {
        stock_id: RollingStockId,
        slot_index: usize,
        filter: Option<ItemId>,
    },
    PlaceEntityFromPlayerInventory {
        prototype_id: EntityPrototypeId,
        item_id: ItemId,
        x: WorldTileCoord,
        y: WorldTileCoord,
        direction: Direction,
    },
    /// Paves the terrain tile at `(x, y)` with the item's tile, consuming one
    /// item. Landfill fills water; stone brick and concrete pave ground.
    PlaceTileFromPlayerInventory {
        item_id: ItemId,
        x: WorldTileCoord,
        y: WorldTileCoord,
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
    EquipArmor {
        inventory_slot: usize,
    },
    UnequipArmor,
    InstallEquipment {
        inventory_slot: usize,
        x: u8,
        y: u8,
    },
    RemoveEquipment {
        x: u8,
        y: u8,
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
    /// Renames a saved blueprint in the library.
    RenameBlueprint {
        index: usize,
        name: String,
    },
    /// Joins two circuit connectors with a wire, consuming one wire item.
    ConnectCircuitWire {
        first: CircuitNode,
        second: CircuitNode,
        color: WireColor,
    },
    /// Cuts one wire, returning its item to the player.
    DisconnectCircuitWire {
        first: CircuitNode,
        second: CircuitNode,
        color: WireColor,
    },
    /// Cuts every wire of one color attached to an entity.
    DisconnectAllCircuitWires {
        entity_id: EntityId,
        color: WireColor,
    },
    /// Sets (or clears) an entity's enable/disable condition.
    SetCircuitCondition {
        entity_id: EntityId,
        condition: Option<CircuitCondition>,
    },
    /// Toggles whether an entity publishes its contents onto its networks.
    SetCircuitReadContents {
        entity_id: EntityId,
        read_contents: bool,
    },
    /// Picks the channel an entity reports its one scalar reading on: an
    /// accumulator's charge percentage, a rail signal's aspect.
    SetEntityOutputSignal {
        entity_id: EntityId,
        signal: Option<SignalId>,
    },
    SetConstantCombinatorSlot {
        entity_id: EntityId,
        slot_index: usize,
        slot: ConstantSignalSlot,
    },
    SetConstantCombinatorEnabled {
        entity_id: EntityId,
        enabled: bool,
    },
    ConfigureArithmeticCombinator {
        entity_id: EntityId,
        left: SignalOperand,
        operation: ArithmeticOperation,
        right: SignalOperand,
        output: Option<SignalId>,
    },
    ConfigureDeciderCombinator {
        entity_id: EntityId,
        left: Option<SignalId>,
        comparator: Comparator,
        right: SignalOperand,
        output: Option<SignalId>,
        output_value: DeciderOutputValue,
    },
    /// Rewrites one configured row of a logistic chest: what a requester or
    /// buffer asks for, or what a storage chest is filtered to.
    SetLogisticRequest {
        entity_id: EntityId,
        slot_index: usize,
        request: LogisticRequest,
    },
    /// Puts a locomotive or wagon on the rail under `(x, y)`. Separate from
    /// [`SimCommand::PlaceEntityFromPlayerInventory`] because rolling stock is
    /// not tile-locked: there is no footprint to reserve, and what comes back
    /// is a [`RollingStockId`] rather than an [`EntityId`].
    PlaceRollingStockFromPlayerInventory {
        prototype_id: EntityPrototypeId,
        item_id: ItemId,
        x: WorldTileCoord,
        y: WorldTileCoord,
    },
    /// Takes one piece of rolling stock off the rails and back into the player
    /// inventory, with its fuel and cargo.
    MineRollingStock {
        stock_id: RollingStockId,
    },
    /// Drives a train by hand, which also takes it off its schedule: a train
    /// cannot be steered by the player and by its orders at once.
    SetTrainThrottle {
        train_id: TrainId,
        throttle: TrainThrottle,
    },
    /// Hands a train back to its schedule, or takes it off again without
    /// touching the throttle.
    SetTrainManual {
        train_id: TrainId,
        manual: bool,
    },
    /// Debug routing command: sends a train to a rail. The route is searched
    /// inside the tick against the rail graph as it then is, so this only
    /// records where the train is going.
    SetTrainDestination {
        train_id: TrainId,
        rail: EntityId,
    },
    /// Cancels wherever a train was going and brakes it.
    ClearTrainDestination {
        train_id: TrainId,
    },
    /// Replaces a train's automatic orders. The whole schedule at once rather
    /// than an edit to one entry: the cursor into it has to move with the list,
    /// and a command per edit would have to describe that move as well.
    SetTrainSchedule {
        train_id: TrainId,
        schedule: TrainSchedule,
    },
    /// Renames a stop, and — when the old name leaves the world with it — the
    /// schedule entries that asked for it.
    RenameTrainStop {
        stop: EntityId,
        name: String,
    },
    /// Sets how many trains a stop admits at once. Refused for zero, which is
    /// what the signal-driven limit below is for.
    SetTrainStopLimit {
        stop: EntityId,
        train_limit: u32,
    },
    /// Picks the channel a stop reads its train limit from, or `None` to go
    /// back to the hand-set number.
    SetTrainStopLimitSignal {
        stop: EntityId,
        signal: Option<SignalId>,
    },
    BuildRedScienceResearchFixture,
    BuildChemicalScienceFactoryFixture,
    /// Applies the chemical science fixture's pending recipe selections as
    /// research unlocks them. Idempotent; scripted runs apply it every tick.
    RunChemicalScienceFactoryProgram,
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
    NuclearReactorFuel,
    NuclearReactorOutput,
    RoboportRobots,
    RoboportMaterial,
    InserterFuel,
    AssemblerInput,
    AssemblerOutput,
    /// A rocket silo's ingredient slots. Finished parts become part of the
    /// rocket; launch products use the separate output panel below.
    RocketSiloInput,
    /// The single launch payload carried by a completed rocket.
    RocketSiloCargo,
    /// Products returned by completed rocket launches.
    RocketSiloOutput,
    Modules,
    /// A cargo wagon's own inventory.
    RollingStockCargo,
    /// A locomotive's fuel slot.
    RollingStockFuel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotTransferError {
    Transfer(ContainerError),
    MiningDrill(MiningDrillError),
    Furnace(FurnaceError),
    Boiler(BoilerError),
    NuclearReactor(NuclearReactorError),
    Roboport(RoboportError),
    Assembler(AssemblerError),
    RocketSilo(RocketSiloError),
    Inserter(InserterError),
    Module(ModuleError),
    RollingStock(RollingStockTransferError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimCommandError {
    EnemyRuntimeSettings(EnemyRuntimeSettingsError),
    Crafting(CraftingError),
    Assembler(AssemblerError),
    Research(ResearchError),
    Transfer(SlotTransferError),
    Build(PlayerBuildError),
    Construction(ConstructionError),
    Repair(RepairError),
    Equipment(PlayerEquipmentError),
    Weapon(PlayerWeaponError),
    TilePlacement(TilePlacementError),
    Circuit(CircuitError),
    LogisticChest(LogisticChestError),
    RollingStockPlacement(RollingStockPlacementError),
    RollingStockMining(RollingStockMiningError),
    TrainControl(TrainControlError),
}

/// State a command produced beyond the mutation itself, for consumers that
/// react to the outcome (e.g. UI feedback).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimCommandEffect {
    None,
    PlayerItemGained {
        item_id: ItemId,
        amount: u32,
        total: u32,
    },
    EntityPlaced(EntityId),
    TilePlaced {
        item_id: ItemId,
        x: WorldTileCoord,
        y: WorldTileCoord,
    },
    GhostPlaced(GhostId),
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
    CircuitWiresRemoved {
        removed: usize,
    },
    RollingStockPlaced(RollingStockId),
    RollingStockMined,
}

impl Simulation {
    pub fn apply_command(
        &mut self,
        command: &SimCommand,
    ) -> Result<SimCommandEffect, SimCommandError> {
        match *command {
            SimCommand::SetEnemyRuntimeSettings(settings) => {
                self.set_enemy_runtime_settings(settings)
                    .map_err(SimCommandError::EnemyRuntimeSettings)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::MovePlayer {
                direction_x,
                direction_y,
                delta_seconds,
            } => {
                self.move_player(direction_x, direction_y, delta_seconds);
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetManualMiningTarget(target) => {
                let gained_item = target.and_then(|target| {
                    if let Some(entity_id) = self.entities.occupancy.entity_at(target.x, target.y) {
                        let placed = self.entities.placed_entity(entity_id)?;
                        entity_recovery_ops::build_item_for_entity(self, placed.prototype_id).ok()
                    } else {
                        self.world
                            .tile_at(target.x, target.y)
                            .and_then(|tile| tile.resource.map(|resource| resource.resource_item))
                    }
                });
                let count_before = gained_item.map(|item_id| self.player_inventory.count(item_id));
                self.update_manual_mining(target);
                Ok(item_gain_effect(self, gained_item, count_before))
            }
            SimCommand::CyclePlayerWeapon => {
                self.cycle_player_weapon()
                    .map_err(SimCommandError::Weapon)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::AttackWithPlayerWeapon { x, y } => {
                self.attack_with_player_weapon(x, y)
                    .map_err(SimCommandError::Weapon)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::StartManualCraft(recipe_id) => {
                self.start_manual_craft(recipe_id)
                    .map_err(SimCommandError::Crafting)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::CancelManualCraft { job_id } => {
                self.cancel_manual_craft(job_id)
                    .map_err(SimCommandError::Crafting)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::MoveManualCraft { job_id, direction } => {
                self.move_manual_craft(job_id, direction)
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
            SimCommand::TransferRollingStockSlot {
                stock_id,
                panel,
                slot_index,
            } => {
                entity_transfer::transfer_rolling_stock_slot(self, stock_id, panel, slot_index)
                    .map_err(|error| {
                        SimCommandError::Transfer(SlotTransferError::RollingStock(error))
                    })?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetRollingStockSlotFilter {
                stock_id,
                slot_index,
                filter,
            } => {
                entity_transfer::set_rolling_stock_slot_filter(self, stock_id, slot_index, filter)
                    .map_err(|error| {
                        SimCommandError::Transfer(SlotTransferError::RollingStock(error))
                    })?;
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
                self.record_early_game_placement(item_id);
                Ok(SimCommandEffect::EntityPlaced(entity_id))
            }
            SimCommand::PlaceTileFromPlayerInventory { item_id, x, y } => {
                tile_placement_ops::place_tile_from_player_inventory(
                    self,
                    TilePlacementRequest { item_id, x, y },
                )
                .map_err(SimCommandError::TilePlacement)?;
                Ok(SimCommandEffect::TilePlaced { item_id, x, y })
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
                let item_id = entity_recovery_ops::build_item_for_entity(
                    self,
                    self.entities
                        .placed_entity(entity_id)
                        .expect("newly built ghost should be placed")
                        .prototype_id,
                )
                .expect("placed entity should have a build item");
                self.record_early_game_placement(item_id);
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
                let item_id = self.entities.placed_entity(entity_id).and_then(|placed| {
                    entity_recovery_ops::build_item_for_entity(self, placed.prototype_id).ok()
                });
                let count_before = item_id.map(|item_id| self.player_inventory.count(item_id));
                construction_ops::deconstruct_marked(self, entity_id)
                    .map_err(SimCommandError::Construction)?;
                Ok(item_gain_effect(self, item_id, count_before))
            }
            SimCommand::RepairEntity { entity_id } => {
                self.repair_entity(entity_id)
                    .map_err(SimCommandError::Repair)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::EquipArmor { inventory_slot } => {
                self.equip_armor(inventory_slot)
                    .map_err(SimCommandError::Equipment)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::UnequipArmor => {
                self.unequip_armor().map_err(SimCommandError::Equipment)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::InstallEquipment {
                inventory_slot,
                x,
                y,
            } => {
                self.install_equipment(inventory_slot, x, y)
                    .map_err(SimCommandError::Equipment)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::RemoveEquipment { x, y } => {
                self.remove_equipment(x, y)
                    .map_err(SimCommandError::Equipment)?;
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
            SimCommand::RenameBlueprint { index, ref name } => {
                construction_ops::rename_blueprint(self, index, name.clone())
                    .map_err(SimCommandError::Construction)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::ConnectCircuitWire {
                first,
                second,
                color,
            } => {
                self.connect_circuit_wire(first, second, color)
                    .map_err(SimCommandError::Circuit)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::DisconnectCircuitWire {
                first,
                second,
                color,
            } => {
                self.disconnect_circuit_wire(first, second, color)
                    .map_err(SimCommandError::Circuit)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::DisconnectAllCircuitWires { entity_id, color } => {
                let removed = self
                    .disconnect_all_circuit_wires(entity_id, color)
                    .map_err(SimCommandError::Circuit)?;
                Ok(SimCommandEffect::CircuitWiresRemoved { removed })
            }
            SimCommand::SetCircuitCondition {
                entity_id,
                condition,
            } => {
                self.set_circuit_condition(entity_id, condition)
                    .map_err(SimCommandError::Circuit)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetCircuitReadContents {
                entity_id,
                read_contents,
            } => {
                self.set_circuit_read_contents(entity_id, read_contents)
                    .map_err(SimCommandError::Circuit)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetEntityOutputSignal { entity_id, signal } => {
                self.set_entity_output_signal(entity_id, signal)
                    .map_err(SimCommandError::Circuit)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetConstantCombinatorSlot {
                entity_id,
                slot_index,
                slot,
            } => {
                self.set_constant_combinator_slot(entity_id, slot_index, slot)
                    .map_err(SimCommandError::Circuit)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetConstantCombinatorEnabled { entity_id, enabled } => {
                self.set_constant_combinator_enabled(entity_id, enabled)
                    .map_err(SimCommandError::Circuit)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::ConfigureArithmeticCombinator {
                entity_id,
                left,
                operation,
                right,
                output,
            } => {
                self.configure_arithmetic_combinator(entity_id, left, operation, right, output)
                    .map_err(SimCommandError::Circuit)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::ConfigureDeciderCombinator {
                entity_id,
                left,
                comparator,
                right,
                output,
                output_value,
            } => {
                self.configure_decider_combinator(
                    entity_id,
                    left,
                    comparator,
                    right,
                    output,
                    output_value,
                )
                .map_err(SimCommandError::Circuit)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetLogisticRequest {
                entity_id,
                slot_index,
                request,
            } => {
                self.set_logistic_request(entity_id, slot_index, request)
                    .map_err(SimCommandError::LogisticChest)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::PlaceRollingStockFromPlayerInventory {
                prototype_id,
                item_id,
                x,
                y,
            } => {
                let stock_id = self
                    .place_rolling_stock_from_player_inventory(prototype_id, item_id, x, y)
                    .map_err(SimCommandError::RollingStockPlacement)?;
                Ok(SimCommandEffect::RollingStockPlaced(stock_id))
            }
            SimCommand::MineRollingStock { stock_id } => {
                self.mine_rolling_stock(stock_id)
                    .map_err(SimCommandError::RollingStockMining)?;
                Ok(SimCommandEffect::RollingStockMined)
            }
            SimCommand::SetTrainThrottle { train_id, throttle } => {
                self.set_train_throttle(train_id, throttle)
                    .map_err(SimCommandError::TrainControl)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetTrainManual { train_id, manual } => {
                self.set_train_manual(train_id, manual)
                    .map_err(SimCommandError::TrainControl)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetTrainDestination { train_id, rail } => {
                self.set_train_destination(train_id, rail)
                    .map_err(SimCommandError::TrainControl)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::ClearTrainDestination { train_id } => {
                self.clear_train_destination(train_id)
                    .map_err(SimCommandError::TrainControl)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetTrainSchedule {
                train_id,
                ref schedule,
            } => {
                self.set_train_schedule(train_id, schedule.clone())
                    .map_err(SimCommandError::TrainControl)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::RenameTrainStop { stop, ref name } => {
                self.rename_train_stop(stop, name.clone())
                    .map_err(SimCommandError::TrainControl)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetTrainStopLimit { stop, train_limit } => {
                self.set_train_stop_limit(stop, train_limit)
                    .map_err(SimCommandError::TrainControl)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::SetTrainStopLimitSignal { stop, signal } => {
                self.set_train_stop_limit_signal(stop, signal)
                    .map_err(SimCommandError::TrainControl)?;
                Ok(SimCommandEffect::None)
            }
            SimCommand::BuildRedScienceResearchFixture => {
                self.build_red_science_research_fixture();
                Ok(SimCommandEffect::None)
            }
            SimCommand::BuildChemicalScienceFactoryFixture => {
                self.build_chemical_science_factory_fixture();
                Ok(SimCommandEffect::None)
            }
            SimCommand::RunChemicalScienceFactoryProgram => {
                self.run_chemical_science_factory_program();
                Ok(SimCommandEffect::None)
            }
        }
    }
}

impl Simulation {
    fn record_early_game_placement(&mut self, item_id: ItemId) {
        let base = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes);
        if item_id == base.items.stone_furnace {
            self.onboarding_progress
                .record_counter(|progress| &mut progress.stone_furnaces_placed, 1);
        } else if item_id == base.items.burner_mining_drill {
            self.onboarding_progress
                .record_counter(|progress| &mut progress.burner_mining_drills_placed, 1);
        } else if item_id == base.items.lab {
            self.onboarding_progress
                .record_counter(|progress| &mut progress.labs_placed, 1);
        }
    }
}

fn item_gain_effect(
    sim: &Simulation,
    item_id: Option<ItemId>,
    count_before: Option<u32>,
) -> SimCommandEffect {
    let Some((item_id, count_before)) = item_id.zip(count_before) else {
        return SimCommandEffect::None;
    };
    let total = sim.player_inventory.count(item_id);
    if total <= count_before {
        SimCommandEffect::None
    } else {
        SimCommandEffect::PlayerItemGained {
            item_id,
            amount: total - count_before,
            total,
        }
    }
}
