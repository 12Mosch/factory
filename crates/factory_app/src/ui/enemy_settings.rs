use bevy::prelude::*;

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::window_sync::{WindowRootQuery, sync_window};
use factory_sim::{EnemyDifficultyPreset, SimCommand, SimulationConfig};

/// Modal for mid-game enemy difficulty: shows the immutable world settings
/// and lets the player switch the runtime settings between presets. Opened
/// with the N key.
#[derive(Resource, Default)]
pub struct EnemySettingsWindowState {
    pub open: bool,
}

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
    window: Res<EnemySettingsWindowState>,
    mut sounds: MessageWriter<SoundEvent>,
    mut sim_commands: MessageWriter<SimCommandRequest>,
) {
    if !window.open {
        return;
    }

    for (interaction, button) in &mut buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        sounds.write(SoundEvent::UiClick);
        sim_commands.write(SimCommandRequest(SimCommand::SetEnemyRuntimeSettings(
            button.preset.config().runtime,
        )));
    }
}

pub(crate) fn sync_enemy_settings_window(
    mut commands: Commands,
    window: Res<EnemySettingsWindowState>,
    sim: Res<SimResource>,
    mut roots: WindowRootQuery<EnemySettingsSnapshot>,
) {
    sync_window(
        &mut commands,
        &mut roots,
        window.open,
        true,
        || EnemySettingsSnapshot {
            config: sim.read().enemy_settings(),
        },
        enemy_settings_root,
        spawn_enemy_settings_modal,
    );
}

fn enemy_settings_root() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.56)),
        GlobalZIndex(2700),
    )
}

fn spawn_enemy_settings_modal(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &EnemySettingsSnapshot,
) {
    root.spawn((
        Node {
            width: Val::Vw(88.0),
            max_width: Val::Px(420.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(12.0),
            padding: UiRect::all(Val::Px(16.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.030, 0.032, 0.034, 0.98)),
        BorderColor::all(Color::srgba(0.36, 0.39, 0.34, 0.95)),
    ))
    .with_children(|modal| {
        modal.spawn((
            Text::new("Enemy Difficulty"),
            TextFont::from_font_size(20.0),
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
