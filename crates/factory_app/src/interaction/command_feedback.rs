use bevy::prelude::*;
use factory_sim::{SimCommand, SimCommandEffect, SimCommandError};

use crate::audio::SoundEvent;
use crate::build::resources::{BuildPlacementState, BuildPlacementStatus};
use crate::placement::build::{
    build_status_from_error, construction_status_from_error, entity_display_name,
};
use crate::resources::SimResource;
use crate::simulation::SimCommandResult;
use crate::ui::inventory_panel::slot_transfer_error_message;
use crate::ui::resources::InventoryTransferFeedback;

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
            (SimCommand::PlaceEntityFromPlayerInventory { prototype_id, .. }, result) => {
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
            }
            (SimCommand::PlaceGhost { prototype_id, .. }, result) => {
                build_state.last_status = match result {
                    Ok(_) => {
                        sounds.write(SoundEvent::Place);
                        BuildPlacementStatus::Placed(format!(
                            "Planned {}",
                            entity_display_name(sim.sim.catalog(), *prototype_id)
                                .unwrap_or_else(|| "Building".to_string())
                        ))
                    }
                    Err(SimCommandError::Construction(error)) => {
                        sounds.write(SoundEvent::PlaceError);
                        construction_status_from_error(sim.sim.catalog(), *error)
                    }
                    Err(_) => continue,
                };
            }
            (SimCommand::BuildGhost { .. }, result) => {
                build_state.last_status = match result {
                    Ok(_) => {
                        sounds.write(SoundEvent::Place);
                        BuildPlacementStatus::Placed("Built ghost".to_string())
                    }
                    Err(SimCommandError::Construction(error)) => {
                        sounds.write(SoundEvent::PlaceError);
                        construction_status_from_error(sim.sim.catalog(), *error)
                    }
                    Err(_) => continue,
                };
            }
            (SimCommand::CancelGhost { .. }, Ok(_))
            | (SimCommand::DeleteBlueprint { .. }, Ok(_)) => {
                sounds.write(SoundEvent::UiClick);
            }
            (
                SimCommand::MarkDeconstruction { .. },
                Ok(SimCommandEffect::DeconstructionMarked {
                    marked,
                    ghosts_removed,
                }),
            ) => {
                sounds.write(SoundEvent::UiClick);
                let mut message = format!("Marked {marked} for deconstruction");
                if *ghosts_removed > 0 {
                    message.push_str(&format!(", removed {ghosts_removed} ghosts"));
                }
                build_state.last_status = BuildPlacementStatus::Placed(message);
            }
            (
                SimCommand::CancelDeconstruction { .. },
                Ok(SimCommandEffect::DeconstructionCancelled { cancelled }),
            ) => {
                sounds.write(SoundEvent::UiClick);
                build_state.last_status =
                    BuildPlacementStatus::Placed(format!("Unmarked {cancelled}"));
            }
            (SimCommand::DeconstructEntity { .. }, result) => {
                build_state.last_status = match result {
                    Ok(_) => {
                        sounds.write(SoundEvent::Place);
                        BuildPlacementStatus::Placed("Deconstructed".to_string())
                    }
                    Err(SimCommandError::Construction(error)) => {
                        sounds.write(SoundEvent::PlaceError);
                        construction_status_from_error(sim.sim.catalog(), *error)
                    }
                    Err(_) => continue,
                };
            }
            (
                SimCommand::PasteBlueprint { .. },
                Ok(SimCommandEffect::BlueprintPasted { placed, skipped }),
            ) => {
                if *placed > 0 {
                    sounds.write(SoundEvent::Place);
                } else {
                    sounds.write(SoundEvent::PlaceError);
                }
                let mut message = format!("Pasted {placed} ghosts");
                if *skipped > 0 {
                    message.push_str(&format!(" ({skipped} skipped)"));
                }
                build_state.last_status = if *placed > 0 {
                    BuildPlacementStatus::Placed(message)
                } else {
                    BuildPlacementStatus::CannotPlace(message)
                };
            }
            (SimCommand::SaveBlueprint { name, .. }, result) => {
                build_state.last_status = match result {
                    Ok(_) => {
                        sounds.write(SoundEvent::UiClick);
                        BuildPlacementStatus::Placed(format!("Saved {name}"))
                    }
                    Err(SimCommandError::Construction(error)) => {
                        sounds.write(SoundEvent::PlaceError);
                        construction_status_from_error(sim.sim.catalog(), *error)
                    }
                    Err(_) => continue,
                };
            }
            _ => {}
        }
    }
}
