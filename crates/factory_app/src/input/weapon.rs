use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::SimCommand;

use crate::input::bindings::{ActionInput, InputAction};
use crate::input::panels::world_input_blocked;
use crate::input::resources::{AppInputState, WeaponInput};
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::simulation::SimCommandRequest;
use crate::ui::resources::TechnologyWindowState;

/// Samples combat intent in the rendered frame so the fixed schedule consumes
/// the aim and selection edge the player actually saw.
pub(crate) fn collect_weapon_input(
    actions: ActionInput,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    mut pending: ResMut<WeaponInput>,
) {
    if world_input_blocked(input_state.as_deref())
        || technology_window.as_deref().is_some_and(|state| state.open)
    {
        pending.clear();
        return;
    }

    if actions.just_pressed(InputAction::CycleWeapon) {
        pending.push_cycle();
    }
    pending.fire_held = actions.pressed(InputAction::FireWeapon);
    pending.aim_tile = cursor_tile_from_window(&windows, &cameras);
    if actions.just_pressed(InputAction::FireWeapon)
        && let Some(tile) = pending.aim_tile
    {
        pending.push_shot(tile);
    }
}

/// Converts the retained frame intent into deterministic simulation commands.
pub(crate) fn apply_weapon_input(
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    mut pending: ResMut<WeaponInput>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    if world_input_blocked(input_state.as_deref())
        || technology_window.as_deref().is_some_and(|state| state.open)
    {
        pending.clear();
        return;
    }

    for _ in 0..pending.take_cycles() {
        commands.write(SimCommandRequest(SimCommand::CyclePlayerWeapon));
    }
    let aimed_shot = pending.take_shot().or_else(|| {
        if pending.fire_held {
            pending.aim_tile
        } else {
            None
        }
    });
    if let Some((x, y)) = aimed_shot {
        commands.write(SimCommandRequest(SimCommand::AttackWithPlayerWeapon {
            x,
            y,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensures selection input is bounded and a fast shot keeps its original
    /// cursor tile until the fixed schedule consumes it.
    #[test]
    fn selection_edges_are_counted_and_bounded() {
        let mut input = WeaponInput::default();
        for _ in 0..32 {
            input.push_cycle();
        }
        assert_eq!(input.take_cycles(), 8);
        assert_eq!(input.take_cycles(), 0);
        input.push_shot((1, 2));
        input.push_shot((3, 4));
        assert_eq!(input.take_shot(), Some((1, 2)));
        assert_eq!(input.take_shot(), None);
    }
}
