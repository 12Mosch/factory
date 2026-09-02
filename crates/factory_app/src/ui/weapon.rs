use bevy::prelude::*;
use bevy::scene::ScenePatch;
use factory_sim::{PlayerWeaponError, SimCommand, SimCommandError};

use crate::input::bindings::{ActionBindings, InputAction, KeyDisplayNames};
use crate::resources::SimResource;
use crate::simulation::SimCommandResult;
use crate::ui::formatting::format_item_display_name;

const FEEDBACK_TICKS: u64 = 120;

#[derive(Component, Default, Clone)]
pub struct WeaponPanelText;

#[derive(Resource, Default)]
pub struct WeaponUiState {
    feedback: Option<(PlayerWeaponError, u64)>,
    rendered: String,
}

/// Builds the retained bottom-left weapon status panel.
fn weapon_panel_scene() -> impl Scene {
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            bottom: Val::Px(86.0),
            width: Val::Px(246.0),
            min_height: Val::Px(78.0),
            padding: UiRect::all(Val::Px(8.0)),
            border: UiRect::all(Val::Px(1.0)),
        }
        BackgroundColor(Color::srgba(0.02, 0.025, 0.027, 0.9))
        BorderColor::all(Color::srgba(0.36, 0.38, 0.34, 0.82))
        GlobalZIndex(1800)
        Children [(
            Text("WEAPON: NONE")
            TextFont { font_size: FontSize::Px(11.0) }
            TextColor(Color::srgb(0.76, 0.86, 0.7))
            WeaponPanelText
        )]
    }
}

/// Spawns the weapon panel when render assets are available.
pub fn setup_weapon_ui(
    mut commands: Commands,
    asset_server: Option<Res<AssetServer>>,
    scene_patches: Option<Res<Assets<ScenePatch>>>,
) {
    if asset_server.is_some() && scene_patches.is_some() {
        commands.spawn_scene(weapon_panel_scene());
    }
}

/// Retains player-facing feedback for failed weapon commands.
pub fn handle_weapon_command_results(
    mut results: MessageReader<SimCommandResult>,
    sim: Res<SimResource>,
    mut state: ResMut<WeaponUiState>,
) {
    for outcome in results.read() {
        if !matches!(
            outcome.command,
            SimCommand::CyclePlayerWeapon | SimCommand::AttackWithPlayerWeapon { .. }
        ) {
            continue;
        }
        state.feedback = match outcome.result {
            Err(SimCommandError::Weapon(error)) => {
                Some((error, sim.read().tick_count() + FEEDBACK_TICKS))
            }
            _ => None,
        };
    }
}

/// Synchronizes selected weapon, ammunition, cooldown, and binding labels.
pub fn sync_weapon_ui(
    sim: Res<SimResource>,
    bindings: Res<ActionBindings>,
    key_names: Res<KeyDisplayNames>,
    mut state: ResMut<WeaponUiState>,
    mut texts: Query<&mut Text, With<WeaponPanelText>>,
) {
    let sim = sim.read();
    if state
        .feedback
        .is_some_and(|(_, expires)| sim.tick_count() > expires)
    {
        state.feedback = None;
    }
    let status = sim.player_weapon_status();
    let weapon = status.selected_weapon.map_or_else(
        || "NONE".to_string(),
        |item_id| format_item_display_name(sim.catalog(), item_id).to_uppercase(),
    );
    let readiness = if status.cooldown_remaining_ticks == 0 {
        "READY".to_string()
    } else {
        format!("COOLDOWN: {}t", status.cooldown_remaining_ticks)
    };
    let total_shots = u64::from(status.loaded_shots).saturating_add(status.reserve_shots);
    let select_key = bindings.display_name(InputAction::CycleWeapon, &key_names);
    let fire_key = bindings.display_name(InputAction::FireWeapon, &key_names);
    let mut next = format!(
        "WEAPON: {weapon}\nAMMO: {total_shots}  ·  {readiness}\n{select_key} SELECT  ·  {fire_key} FIRE"
    );
    if let Some((error, _)) = state.feedback {
        next.push('\n');
        next.push_str(weapon_error_text(error));
    }
    if next == state.rendered {
        return;
    }
    state.rendered.clone_from(&next);
    for mut text in &mut texts {
        **text = next.clone();
    }
}

/// Maps every deterministic weapon failure to concise HUD copy.
fn weapon_error_text(error: PlayerWeaponError) -> &'static str {
    match error {
        PlayerWeaponError::NoWeaponsAvailable => "NO CARRIED WEAPON",
        PlayerWeaponError::NoWeaponSelected => "SELECT A WEAPON",
        PlayerWeaponError::WeaponUnavailable(_) => "SELECTED WEAPON IS MISSING",
        PlayerWeaponError::NoAmmunition => "OUT OF AMMUNITION",
        PlayerWeaponError::CoolingDown { .. } => "WEAPON COOLING DOWN",
        PlayerWeaponError::NoHostileTarget => "NO HOSTILE TARGET",
        PlayerWeaponError::OutOfRange { .. } => "TARGET OUT OF RANGE",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensures every simulation-level attack failure remains visible to the
    /// player rather than silently dropping input.
    #[test]
    fn every_weapon_failure_has_player_facing_copy() {
        let failures = [
            PlayerWeaponError::NoWeaponsAvailable,
            PlayerWeaponError::NoWeaponSelected,
            PlayerWeaponError::WeaponUnavailable(factory_data::ItemId::new(1)),
            PlayerWeaponError::NoAmmunition,
            PlayerWeaponError::CoolingDown { remaining_ticks: 1 },
            PlayerWeaponError::NoHostileTarget,
            PlayerWeaponError::OutOfRange { range_tiles: 10 },
        ];
        assert!(
            failures
                .into_iter()
                .all(|failure| !weapon_error_text(failure).is_empty())
        );
    }
}
