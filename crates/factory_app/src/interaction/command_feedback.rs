use bevy::prelude::*;
use factory_sim::{SimCommand, SimCommandError};

use crate::audio::SoundEvent;
use crate::placement::build::{build_status_from_error, entity_display_name};
use crate::build::resources::{BuildPlacementState, BuildPlacementStatus};
use crate::resources::SimResource;
use crate::ui::resources::InventoryTransferFeedback;
use crate::simulation::SimCommandResult;
use crate::ui::inventory_panel::slot_transfer_error_message;

/// Frame-side feedback for commands the fixed tick applied: click and
/// placement sounds, transfer error messages, and build placement status.
pub(crate) fn handle_sim_command_results(
    mut results: MessageReader<SimCommandResult>,
    sim: Res<SimResource>,
    mut feedback: ResMut<InventoryTransferFeedback>,
    mut build_state: ResMut<BuildPlacementState>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    for outcome in results.read() {
        match (&outcome.command, &outcome.result) {
            (SimCommand::TransferSlot { .. }, Ok(_)) => {
                feedback.message = None;
                sounds.write(SoundEvent::UiClick);
            }
            (SimCommand::TransferSlot { .. }, Err(SimCommandError::Transfer(error))) => {
                feedback.message = Some(slot_transfer_error_message(sim.sim.catalog(), *error));
            }
            (SimCommand::StartManualCraft(_), Ok(_))
            | (SimCommand::SelectAssemblerRecipe { .. }, Ok(_))
            | (SimCommand::EnqueueResearch(_), Ok(_))
            | (SimCommand::RemoveQueuedResearch { .. }, Ok(_))
            | (SimCommand::MoveQueuedResearch { .. }, Ok(_)) => {
                sounds.write(SoundEvent::UiClick);
            }
            (
                SimCommand::PlaceEntityFromPlayerInventory {
                    prototype_id,
                    item_id,
                    ..
                },
                result,
            ) => {
                let catalog = sim.sim.catalog();
                let status = match result {
                    Ok(_) => {
                        sounds.write(SoundEvent::Place);
                        BuildPlacementStatus::Placed(format!(
                            "Placed {}",
                            entity_display_name(catalog, *prototype_id)
                                .unwrap_or_else(|| "Building".to_string())
                        ))
                    }
                    Err(SimCommandError::Build(error)) => {
                        sounds.write(SoundEvent::PlaceError);
                        build_status_from_error(catalog, *error)
                    }
                    Err(_) => continue,
                };
                build_state.last_status = status;
                if build_state
                    .selected
                    .is_some_and(|selection| selection.item_id == *item_id)
                    && sim.sim.player_inventory().count(*item_id) == 0
                {
                    build_state.selected = None;
                }
            }
            _ => {}
        }
    }
}
