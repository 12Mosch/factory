use super::*;

pub(super) fn apply(
    sim: &mut Simulation,
    command: &SimCommand,
) -> Result<SimCommandEffect, SimCommandError> {
    match command {
        SimCommand::PlaceRollingStockFromPlayerInventory {
            prototype_id,
            item_id,
            x,
            y,
        } => {
            let stock_id = sim
                .place_rolling_stock_from_player_inventory(*prototype_id, *item_id, *x, *y)
                .map_err(SimCommandError::RollingStockPlacement)?;
            Ok(SimCommandEffect::RollingStockPlaced(stock_id))
        }
        SimCommand::MineRollingStock { stock_id } => {
            sim.mine_rolling_stock(*stock_id)
                .map_err(SimCommandError::RollingStockMining)?;
            Ok(SimCommandEffect::RollingStockMined)
        }
        SimCommand::SetTrainThrottle { train_id, throttle } => {
            sim.set_train_throttle(*train_id, *throttle)
                .map_err(SimCommandError::TrainControl)?;
            Ok(SimCommandEffect::None)
        }
        SimCommand::SetTrainManual { train_id, manual } => {
            sim.set_train_manual(*train_id, *manual)
                .map_err(SimCommandError::TrainControl)?;
            Ok(SimCommandEffect::None)
        }
        SimCommand::SetTrainDestination { train_id, rail } => {
            sim.set_train_destination(*train_id, *rail)
                .map_err(SimCommandError::TrainControl)?;
            Ok(SimCommandEffect::None)
        }
        SimCommand::ClearTrainDestination { train_id } => {
            sim.clear_train_destination(*train_id)
                .map_err(SimCommandError::TrainControl)?;
            Ok(SimCommandEffect::None)
        }
        SimCommand::SetTrainSchedule { train_id, schedule } => {
            sim.set_train_schedule(*train_id, schedule.clone())
                .map_err(SimCommandError::TrainControl)?;
            Ok(SimCommandEffect::None)
        }
        SimCommand::RenameTrainStop { stop, name } => {
            sim.rename_train_stop(*stop, name.clone())
                .map_err(SimCommandError::TrainControl)?;
            Ok(SimCommandEffect::None)
        }
        SimCommand::SetTrainStopLimit { stop, train_limit } => {
            sim.set_train_stop_limit(*stop, *train_limit)
                .map_err(SimCommandError::TrainControl)?;
            Ok(SimCommandEffect::None)
        }
        SimCommand::SetTrainStopLimitSignal { stop, signal } => {
            sim.set_train_stop_limit_signal(*stop, *signal)
                .map_err(SimCommandError::TrainControl)?;
            Ok(SimCommandEffect::None)
        }
        _ => unreachable!("non-rail command routed to rail dispatcher"),
    }
}
