use super::*;

mod dispatch;

/// A player-issued simulation mutation. All interactive changes to the
/// simulation are expressed as commands and applied at a tick boundary via
/// [`Simulation::apply_command`], so a recorded command stream fully
/// determines the simulation's evolution (replays, scripted end-to-end tests,
/// lockstep multiplayer).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum SimCommand {
    /// Requests recovery at the next tick; remaining commands still see a dead player.
    RespawnPlayer,
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
    PlayerDead,
    PlayerAlive,
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

impl SimCommand {
    /// Used by both frame-side collection and authoritative command dispatch.
    /// New gameplay commands require a living player unless explicitly exempted.
    pub fn requires_living_player(&self) -> bool {
        !matches!(self, Self::RespawnPlayer | Self::SetEnemyRuntimeSettings(_))
    }
}

impl Simulation {
    pub fn apply_command(
        &mut self,
        command: &SimCommand,
    ) -> Result<SimCommandEffect, SimCommandError> {
        if matches!(command, SimCommand::RespawnPlayer) {
            if !self.player.is_dead() {
                return Err(SimCommandError::PlayerAlive);
            }
            self.player.respawn_requested = true;
            return Ok(SimCommandEffect::None);
        }
        if self.player.is_dead() && command.requires_living_player() {
            return Err(SimCommandError::PlayerDead);
        }
        dispatch::apply(self, command)
    }
}
