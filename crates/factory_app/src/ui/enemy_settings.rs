use bevy::prelude::*;

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::settings::{SettingsTab, SettingsWindowState};
use factory_sim::{EnemyDifficultyPreset, SimCommand, SimulationConfig};

#[derive(Component, Clone, Copy)]
pub struct EnemyPresetButton {
    pub preset: EnemyDifficultyPreset,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EnemySettingsSnapshot {
    config: SimulationConfig,
}

type EnemyPresetButtonQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static EnemyPresetButton),
    (Changed<Interaction>, With<Button>),
>;

pub(crate) fn handle_enemy_settings_buttons(
    mut buttons: EnemyPresetButtonQuery,
    mut window: ResMut<SettingsWindowState>,
    mut sounds: MessageWriter<SoundEvent>,
    mut sim_commands: MessageWriter<SimCommandRequest>,
) {
    if !window.open || window.active_tab != SettingsTab::Gameplay {
        return;
    }

    for (interaction, button) in &mut buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        window.pending_values.enemy_preset = button.preset;
        window.dirty = true;
        sounds.write(SoundEvent::UiClick);
        sim_commands.write(SimCommandRequest(SimCommand::SetEnemyRuntimeSettings(
            button.preset.config().runtime,
        )));
    }
}

pub(crate) fn enemy_settings_snapshot(sim: &SimResource) -> EnemySettingsSnapshot {
    EnemySettingsSnapshot {
        config: sim.read().enemy_settings(),
    }
}

pub(crate) fn spawn_enemy_settings_content(
    modal: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &EnemySettingsSnapshot,
) {
    modal.spawn((
        Text::new("Enemy difficulty"),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.94, 0.95, 0.90)),
    ));
    modal.spawn((
        Text::new(format!(
            "World: {}% density · {} tile safe radius (immutable)",
            snapshot.config.world.base_density_percent,
            snapshot.config.world.starting_safe_radius_tiles
        )),
        TextFont::from_font_size(12.0),
        TextColor(Color::srgb(0.65, 0.68, 0.65)),
    ));
    modal.spawn((
        Text::new(format!(
            "Runtime: {}% strength · {}% pollution · {}% evolution",
            snapshot.config.runtime.strength_percent,
            snapshot.config.runtime.pollution_sensitivity_percent,
            snapshot.config.runtime.evolution_rate_percent
        )),
        TextFont::from_font_size(12.0),
    ));
    modal
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|row| {
            for (preset, label) in [
                (EnemyDifficultyPreset::Peaceful, "Peaceful"),
                (EnemyDifficultyPreset::Standard, "Standard"),
                (EnemyDifficultyPreset::Aggressive, "Aggressive"),
            ] {
                spawn_preset_button(row, label, preset);
            }
        });
}

fn spawn_preset_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    preset: EnemyDifficultyPreset,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(92.0),
                height: Val::Px(32.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.95)),
            BorderColor::all(Color::srgba(0.44, 0.43, 0.39, 0.70)),
            EnemyPresetButton { preset },
        ))
        .with_child((
            Text::new(label),
            TextFont::from_font_size(13.0),
            TextColor(Color::WHITE),
        ));
}
