use super::*;

mod circuit;
mod construction;
mod fixtures;
mod player;
mod production;
mod rail;

pub(super) fn apply(
    sim: &mut Simulation,
    command: &SimCommand,
) -> Result<SimCommandEffect, SimCommandError> {
    match command {
        SimCommand::RespawnPlayer => unreachable!("respawn handled at command boundary"),
        SimCommand::SetEnemyRuntimeSettings(_)
        | SimCommand::MovePlayer { .. }
        | SimCommand::SetManualMiningTarget(_)
        | SimCommand::CyclePlayerWeapon
        | SimCommand::AttackWithPlayerWeapon { .. }
        | SimCommand::RepairEntity { .. }
        | SimCommand::EquipArmor { .. }
        | SimCommand::UnequipArmor
        | SimCommand::InstallEquipment { .. }
        | SimCommand::RemoveEquipment { .. } => player::apply(sim, command),

        SimCommand::StartManualCraft(_)
        | SimCommand::CancelManualCraft { .. }
        | SimCommand::MoveManualCraft { .. }
        | SimCommand::SelectAssemblerRecipe { .. }
        | SimCommand::EnqueueResearch(_)
        | SimCommand::RemoveQueuedResearch { .. }
        | SimCommand::MoveQueuedResearch { .. }
        | SimCommand::TransferSlot { .. }
        | SimCommand::TransferRollingStockSlot { .. }
        | SimCommand::SetRollingStockSlotFilter { .. }
        | SimCommand::SetLogisticRequest { .. } => production::apply(sim, command),

        SimCommand::PlaceEntityFromPlayerInventory { .. }
        | SimCommand::PlaceTileFromPlayerInventory { .. }
        | SimCommand::PlaceGhost { .. }
        | SimCommand::CancelGhost { .. }
        | SimCommand::BuildGhost { .. }
        | SimCommand::MarkDeconstruction { .. }
        | SimCommand::CancelDeconstruction { .. }
        | SimCommand::DeconstructEntity { .. }
        | SimCommand::PasteBlueprint { .. }
        | SimCommand::SaveBlueprint { .. }
        | SimCommand::DeleteBlueprint { .. }
        | SimCommand::RenameBlueprint { .. } => construction::apply(sim, command),

        SimCommand::ConnectCircuitWire { .. }
        | SimCommand::DisconnectCircuitWire { .. }
        | SimCommand::DisconnectAllCircuitWires { .. }
        | SimCommand::SetCircuitCondition { .. }
        | SimCommand::SetCircuitReadContents { .. }
        | SimCommand::SetEntityOutputSignal { .. }
        | SimCommand::SetConstantCombinatorSlot { .. }
        | SimCommand::SetConstantCombinatorEnabled { .. }
        | SimCommand::ConfigureArithmeticCombinator { .. }
        | SimCommand::ConfigureDeciderCombinator { .. } => circuit::apply(sim, command),

        SimCommand::PlaceRollingStockFromPlayerInventory { .. }
        | SimCommand::MineRollingStock { .. }
        | SimCommand::SetTrainThrottle { .. }
        | SimCommand::SetTrainManual { .. }
        | SimCommand::SetTrainDestination { .. }
        | SimCommand::ClearTrainDestination { .. }
        | SimCommand::SetTrainSchedule { .. }
        | SimCommand::RenameTrainStop { .. }
        | SimCommand::SetTrainStopLimit { .. }
        | SimCommand::SetTrainStopLimitSignal { .. } => rail::apply(sim, command),

        SimCommand::BuildRedScienceResearchFixture
        | SimCommand::BuildChemicalScienceFactoryFixture
        | SimCommand::RunChemicalScienceFactoryProgram => fixtures::apply(sim, command),
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
