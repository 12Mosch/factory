use bevy::prelude::*;
use factory_sim::{SimCommand, SimCommandEffect, SimCommandError};

use crate::audio::SoundEvent;
use crate::build::resources::{BuildPlacementState, BuildPlacementStatus};
use crate::placement::build::{
    build_status_from_error, construction_status_from_error, entity_display_name,
};
use crate::resources::SimResource;
use crate::simulation::SimCommandResult;
use crate::ui::formatting::format_item_display_name;
use crate::ui::inventory_panel::slot_transfer_error_message;
use crate::ui::resources::InventoryTransferFeedback;

const ITEM_GAIN_MESSAGE_SECONDS: f32 = 3.0;

#[derive(Resource, Default)]
pub(crate) struct ItemGainFeedback {
    message: Option<String>,
    timer: Timer,
}

impl ItemGainFeedback {
    fn show(&mut self, message: String) {
        self.message = Some(message);
        self.timer = Timer::from_seconds(ITEM_GAIN_MESSAGE_SECONDS, TimerMode::Once);
    }

    fn tick(&mut self, delta: std::time::Duration) -> Option<String> {
        self.timer.tick(delta);
        self.timer
            .is_finished()
            .then(|| self.message.take())
            .flatten()
    }
}

/// Frame-side feedback for commands the fixed tick applied: click and
/// placement sounds, transfer error messages, and build placement status.
pub(crate) fn handle_sim_command_results(
    mut results: MessageReader<SimCommandResult>,
    sim: Res<SimResource>,
    mut feedback: ResMut<InventoryTransferFeedback>,
    mut build_state: ResMut<BuildPlacementState>,
    mut item_gain_feedback: ResMut<ItemGainFeedback>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    for outcome in results.read() {
        if let Ok(SimCommandEffect::PlayerItemGained {
            item_id,
            amount,
            total,
        }) = outcome.result
        {
            let item_name = format_item_display_name(sim.read().catalog(), item_id);
            let message = item_gain_message(amount, &item_name, total);
            build_state.last_status = BuildPlacementStatus::Placed(message.clone());
            item_gain_feedback.show(message);
        }

        match (&outcome.command, &outcome.result) {
            (SimCommand::TransferSlot { .. }, Ok(_)) => {
                feedback.message = None;
                sounds.write(SoundEvent::UiClick);
            }
            (SimCommand::TransferSlot { .. }, Err(SimCommandError::Transfer(error))) => {
                feedback.message = Some(slot_transfer_error_message(sim.read().catalog(), *error));
            }
            (SimCommand::StartManualCraft(_), Ok(_))
            | (SimCommand::SelectAssemblerRecipe { .. }, Ok(_))
            | (SimCommand::EnqueueResearch(_), Ok(_))
            | (SimCommand::RemoveQueuedResearch { .. }, Ok(_))
            | (SimCommand::MoveQueuedResearch { .. }, Ok(_)) => {
                sounds.write(SoundEvent::UiClick);
            }
            (SimCommand::PlaceEntityFromPlayerInventory { prototype_id, .. }, result) => {
                let sim = sim.read();
                let catalog = sim.catalog();
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
                            entity_display_name(sim.read().catalog(), *prototype_id)
                                .unwrap_or_else(|| "Building".to_string())
                        ))
                    }
                    Err(SimCommandError::Construction(error)) => {
                        sounds.write(SoundEvent::PlaceError);
                        construction_status_from_error(sim.read().catalog(), *error)
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
                        construction_status_from_error(sim.read().catalog(), *error)
                    }
                    Err(_) => continue,
                };
            }
            (SimCommand::CancelGhost { .. } | SimCommand::DeleteBlueprint { .. }, result) => {
                match result {
                    Ok(_) => {
                        sounds.write(SoundEvent::UiClick);
                    }
                    Err(SimCommandError::Construction(error)) => {
                        sounds.write(SoundEvent::PlaceError);
                        build_state.last_status =
                            construction_status_from_error(sim.read().catalog(), *error);
                    }
                    Err(_) => continue,
                }
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
                    Ok(SimCommandEffect::PlayerItemGained { .. }) => {
                        sounds.write(SoundEvent::Place);
                        continue;
                    }
                    Ok(_) => BuildPlacementStatus::Placed("Deconstructed".to_string()),
                    Err(SimCommandError::Construction(error)) => {
                        sounds.write(SoundEvent::PlaceError);
                        construction_status_from_error(sim.read().catalog(), *error)
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
                        construction_status_from_error(sim.read().catalog(), *error)
                    }
                    Err(_) => continue,
                };
            }
            (SimCommand::RenameBlueprint { name, .. }, result) => {
                build_state.last_status = match result {
                    Ok(_) => {
                        sounds.write(SoundEvent::UiClick);
                        BuildPlacementStatus::Placed(format!("Renamed to {name}"))
                    }
                    Err(SimCommandError::Construction(error)) => {
                        sounds.write(SoundEvent::PlaceError);
                        construction_status_from_error(sim.read().catalog(), *error)
                    }
                    Err(_) => continue,
                };
            }
            _ => {}
        }
    }
}

pub(crate) fn expire_item_gain_feedback(
    time: Res<Time>,
    mut item_gain_feedback: ResMut<ItemGainFeedback>,
    mut build_state: ResMut<BuildPlacementState>,
) {
    let Some(message) = item_gain_feedback.tick(time.delta()) else {
        return;
    };

    if build_state.last_status == BuildPlacementStatus::Placed(message) {
        build_state.last_status = BuildPlacementStatus::Ready;
    }
}

fn item_gain_message(amount: u32, item_name: &str, total: u32) -> String {
    format!("+{amount} {item_name} ({total})")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ITEM_GAIN_MESSAGE_SECONDS, ItemGainFeedback, item_gain_message};

    #[test]
    fn item_gain_message_includes_amount_name_and_total() {
        assert_eq!(
            item_gain_message(1, "Stone furnace", 4),
            "+1 Stone furnace (4)"
        );
    }

    #[test]
    fn item_gain_feedback_expires_after_three_seconds() {
        let mut feedback = ItemGainFeedback::default();
        feedback.show("+1 Stone furnace (4)".to_string());

        assert_eq!(feedback.tick(Duration::from_secs(2)), None);
        assert_eq!(
            feedback.tick(Duration::from_secs_f32(ITEM_GAIN_MESSAGE_SECONDS - 2.0)),
            Some("+1 Stone furnace (4)".to_string())
        );
    }
}
