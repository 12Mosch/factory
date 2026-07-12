use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use factory_data::PrototypeCatalog;
use factory_sim::{EnemyDifficultyPreset, Simulation, SimulationConfig};

use crate::save_load::{
    LoadState, PendingSaveJobs, SaveLoadConfig, SaveLoadStatus, SaveSlotKind, enter_swapped_world,
    load_slot, slot_display_name, slot_exists,
};

#[derive(States, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AppMode {
    WorldSetup,
    #[default]
    InGame,
}

#[derive(Resource, Default)]
pub struct StartInWorldSetup;

#[derive(Resource, Clone, Debug)]
pub struct WorldSetupState {
    pub seed_text: String,
    pub config: SimulationConfig,
    pub validation_error: Option<String>,
    /// True when the setup screen was opened from a running game (New
    /// World), so it offers a way back that keeps the current session. At
    /// application startup there is no session to return to.
    pub allow_cancel: bool,
}

impl Default for WorldSetupState {
    fn default() -> Self {
        Self {
            seed_text: "123".into(),
            config: SimulationConfig::default(),
            validation_error: None,
            allow_cancel: false,
        }
    }
}

#[derive(Component)]
pub struct WorldSetupRoot;
#[derive(Component)]
pub struct WorldSetupSeedText;
#[derive(Component)]
pub struct WorldSetupErrorText;
#[derive(Component)]
pub struct WorldSetupSettingsText;
#[derive(Component, Clone, Copy)]
pub enum WorldSetupAction {
    Randomize,
    SelectPreset(EnemyDifficultyPreset),
    Density(i16),
    SafeRadius(i16),
    Strength(i16),
    PollutionSensitivity(i16),
    EvolutionRate(i16),
    RaidRate(i16),
    ExpansionRate(i16),
    ToggleRaids,
    ToggleExpansion,
    LoadSlot(SaveSlotKind),
    Start,
    /// Return to the running game without touching the simulation.
    Cancel,
}

pub fn build_world_setup_ui(
    mut commands: Commands,
    existing: Query<Entity, With<WorldSetupRoot>>,
    saves: Res<SaveLoadConfig>,
    setup: Res<WorldSetupState>,
) {
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    commands
        .spawn((
            WorldSetupRoot,
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
            BackgroundColor(Color::srgb(0.025, 0.03, 0.035)),
            GlobalZIndex(5000),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(620.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(14.0),
                    padding: UiRect::all(Val::Px(24.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.06, 0.07, 0.075)),
                BorderColor::all(Color::srgb(0.3, 0.38, 0.32)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("NEW WORLD"),
                    TextFont::from_font_size(28.0),
                    TextColor(Color::srgb(0.85, 0.94, 0.78)),
                ));
                panel.spawn((Text::new("Existing saves"), TextFont::from_font_size(14.0)));
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(6.0),
                        row_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|row| {
                        for slot in [
                            SaveSlotKind::Manual(1),
                            SaveSlotKind::Manual(2),
                            SaveSlotKind::Manual(3),
                            SaveSlotKind::Quick,
                            SaveSlotKind::Auto,
                        ] {
                            let label = if slot_exists(&saves, slot) {
                                format!("Load {}", slot_display_name(slot))
                            } else {
                                format!("{} empty", slot_display_name(slot))
                            };
                            spawn_button(row, &label, WorldSetupAction::LoadSlot(slot));
                        }
                    });
                panel.spawn((
                    Text::new("Seed: 123"),
                    WorldSetupSeedText,
                    TextFont::from_font_size(16.0),
                ));
                spawn_button(panel, "Randomize", WorldSetupAction::Randomize);
                panel.spawn((
                    Text::new("Enemy difficulty"),
                    TextFont::from_font_size(15.0),
                ));
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        ..default()
                    })
                    .with_children(|row| {
                        for (preset, label) in [
                            (EnemyDifficultyPreset::Peaceful, "Peaceful"),
                            (EnemyDifficultyPreset::Standard, "Standard"),
                            (EnemyDifficultyPreset::Aggressive, "Aggressive"),
                        ] {
                            spawn_button(row, label, WorldSetupAction::SelectPreset(preset));
                        }
                    });
                panel.spawn((
                    Text::new("Advanced controls (world values lock after Start)"),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::srgb(0.65, 0.68, 0.65)),
                ));
                panel.spawn((
                    Text::new(""),
                    WorldSetupSettingsText,
                    TextFont::from_font_size(12.0),
                    TextColor(Color::srgb(0.78, 0.84, 0.76)),
                ));
                for (label, down, up) in [
                    (
                        "Base density ±25%",
                        WorldSetupAction::Density(-25),
                        WorldSetupAction::Density(25),
                    ),
                    (
                        "Safe radius ±16",
                        WorldSetupAction::SafeRadius(-16),
                        WorldSetupAction::SafeRadius(16),
                    ),
                    (
                        "Strength ±10%",
                        WorldSetupAction::Strength(-10),
                        WorldSetupAction::Strength(10),
                    ),
                    (
                        "Pollution sensitivity ±25%",
                        WorldSetupAction::PollutionSensitivity(-25),
                        WorldSetupAction::PollutionSensitivity(25),
                    ),
                    (
                        "Evolution rate ±25%",
                        WorldSetupAction::EvolutionRate(-25),
                        WorldSetupAction::EvolutionRate(25),
                    ),
                    (
                        "Raid frequency ±25%",
                        WorldSetupAction::RaidRate(-25),
                        WorldSetupAction::RaidRate(25),
                    ),
                    (
                        "Expansion frequency ±25%",
                        WorldSetupAction::ExpansionRate(-25),
                        WorldSetupAction::ExpansionRate(25),
                    ),
                ] {
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(8.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Text::new(label),
                                Node {
                                    width: Val::Px(250.0),
                                    ..default()
                                },
                                TextFont::from_font_size(12.0),
                            ));
                            spawn_button(row, "−", down);
                            spawn_button(row, "+", up);
                        });
                }
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        ..default()
                    })
                    .with_children(|row| {
                        spawn_button(row, "Toggle raids", WorldSetupAction::ToggleRaids);
                        spawn_button(row, "Toggle expansion", WorldSetupAction::ToggleExpansion);
                    });
                panel.spawn((
                    Text::new(""),
                    WorldSetupErrorText,
                    TextFont::from_font_size(13.0),
                    TextColor(Color::srgb(1.0, 0.4, 0.35)),
                ));
                spawn_button(panel, "Start", WorldSetupAction::Start);
                if setup.allow_cancel {
                    spawn_button(panel, "Back to game", WorldSetupAction::Cancel);
                }
            });
        });
}

pub fn cleanup_world_setup(mut commands: Commands, roots: Query<Entity, With<WorldSetupRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

fn spawn_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    action: WorldSetupAction,
) {
    parent
        .spawn((
            Button,
            Node {
                height: Val::Px(36.0),
                min_width: Val::Px(110.0),
                padding: UiRect::horizontal(Val::Px(12.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.13, 0.17, 0.14)),
            BorderColor::all(Color::srgb(0.38, 0.48, 0.38)),
            action,
        ))
        .with_child((Text::new(label), TextFont::from_font_size(14.0)));
}

pub fn handle_world_setup_seed_input(
    mut inputs: MessageReader<KeyboardInput>,
    mut setup: ResMut<WorldSetupState>,
) {
    for input in inputs.read() {
        if input.state != ButtonState::Pressed {
            continue;
        }
        if input.key_code == KeyCode::Backspace {
            setup.seed_text.pop();
            setup.validation_error = None;
            continue;
        }
        if let Some(text) = &input.text {
            setup
                .seed_text
                .extend(text.chars().filter(char::is_ascii_digit));
            setup.validation_error = None;
        }
    }
}

type SetupButtons<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static WorldSetupAction),
    (Changed<Interaction>, With<Button>),
>;

pub(crate) fn handle_world_setup_buttons(
    mut buttons: SetupButtons,
    mut setup: ResMut<WorldSetupState>,
    mut load_state: LoadState,
) {
    for (interaction, action) in &mut buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            WorldSetupAction::Randomize => {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(123, |duration| duration.as_nanos() as u64);
                setup.seed_text = splitmix64(nanos).to_string();
                setup.validation_error = None;
            }
            WorldSetupAction::SelectPreset(preset) => {
                setup.config = preset.config();
                setup.validation_error = None;
            }
            WorldSetupAction::Density(delta) => {
                setup.config.world.base_density_percent =
                    adjust(setup.config.world.base_density_percent, *delta, 0, 200);
                setup.config.preset = EnemyDifficultyPreset::Custom;
            }
            WorldSetupAction::SafeRadius(delta) => {
                setup.config.world.starting_safe_radius_tiles = adjust(
                    setup.config.world.starting_safe_radius_tiles,
                    *delta,
                    64,
                    320,
                );
                setup.config.preset = EnemyDifficultyPreset::Custom;
            }
            WorldSetupAction::Strength(delta) => {
                setup.config.runtime.strength_percent =
                    adjust(setup.config.runtime.strength_percent, *delta, 50, 200);
                setup.config.preset = EnemyDifficultyPreset::Custom;
            }
            WorldSetupAction::PollutionSensitivity(delta) => {
                setup.config.runtime.pollution_sensitivity_percent = adjust(
                    setup.config.runtime.pollution_sensitivity_percent,
                    *delta,
                    25,
                    200,
                );
                setup.config.preset = EnemyDifficultyPreset::Custom;
            }
            WorldSetupAction::EvolutionRate(delta) => {
                setup.config.runtime.evolution_rate_percent =
                    adjust(setup.config.runtime.evolution_rate_percent, *delta, 25, 200);
                setup.config.preset = EnemyDifficultyPreset::Custom;
            }
            WorldSetupAction::RaidRate(delta) => {
                setup.config.runtime.raid_frequency_percent = adjust(
                    setup.config.runtime.raid_frequency_percent.max(25),
                    *delta,
                    25,
                    200,
                );
                setup.config.preset = EnemyDifficultyPreset::Custom;
            }
            WorldSetupAction::ExpansionRate(delta) => {
                setup.config.runtime.expansion_frequency_percent = adjust(
                    setup.config.runtime.expansion_frequency_percent.max(25),
                    *delta,
                    25,
                    200,
                );
                setup.config.preset = EnemyDifficultyPreset::Custom;
            }
            WorldSetupAction::ToggleRaids => {
                setup.config.runtime.proactive_raids = !setup.config.runtime.proactive_raids;
                setup.config.preset = EnemyDifficultyPreset::Custom;
            }
            WorldSetupAction::ToggleExpansion => {
                setup.config.runtime.expansion = !setup.config.runtime.expansion;
                setup.config.preset = EnemyDifficultyPreset::Custom;
            }
            WorldSetupAction::LoadSlot(_) => {}
            WorldSetupAction::Start => {
                let Ok(seed) = setup.seed_text.parse::<u64>() else {
                    setup.validation_error = Some("Seed must be a decimal u64".into());
                    continue;
                };
                if !setup.config.is_valid() {
                    setup.validation_error =
                        Some("Enemy settings are outside the supported ranges".into());
                    continue;
                }
                let catalog =
                    PrototypeCatalog::load_base().expect("base prototype catalog should load");
                let new_world = Simulation::new_with_config(seed, catalog, setup.config);
                let tick = new_world.tick_count();
                let player_tile = new_world.player().position_tiles();
                if load_state.sim.replace(new_world).is_err() {
                    setup.validation_error = Some("Simulation is busy; try Start again".into());
                    continue;
                }
                enter_swapped_world(&mut load_state, tick, player_tile);
            }
            WorldSetupAction::Cancel => {
                setup.validation_error = None;
                load_state.next_mode.set(AppMode::InGame);
            }
        }
    }
}

pub(crate) fn handle_world_setup_load_buttons(
    mut buttons: SetupButtons,
    config: Res<SaveLoadConfig>,
    pending: Res<PendingSaveJobs>,
    mut status: ResMut<SaveLoadStatus>,
    mut load_state: LoadState,
    mut setup: ResMut<WorldSetupState>,
) {
    for (interaction, action) in &mut buttons {
        let WorldSetupAction::LoadSlot(slot) = action else {
            continue;
        };
        if *interaction == Interaction::Pressed
            && !load_slot(*slot, &config, &pending, &mut status, &mut load_state)
        {
            setup.validation_error = status.message.clone();
        }
    }
}

fn adjust(value: u16, delta: i16, minimum: u16, maximum: u16) -> u16 {
    (i32::from(value) + i32::from(delta)).clamp(i32::from(minimum), i32::from(maximum)) as u16
}

type SettingsTextQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<WorldSetupSettingsText>,
        Without<WorldSetupSeedText>,
        Without<WorldSetupErrorText>,
    ),
>;

pub fn sync_world_setup_text(
    setup: Res<WorldSetupState>,
    mut seed: Query<&mut Text, (With<WorldSetupSeedText>, Without<WorldSetupErrorText>)>,
    mut error: Query<&mut Text, With<WorldSetupErrorText>>,
    mut settings: SettingsTextQuery,
) {
    if !setup.is_changed() {
        return;
    }
    for mut text in &mut seed {
        **text = format!("Seed: {}", setup.seed_text);
    }
    for mut text in &mut error {
        **text = setup.validation_error.clone().unwrap_or_default();
    }
    for mut text in &mut settings {
        let config = setup.config;
        **text = format!(
            "{:?} · density {}% · radius {} · strength {}% · pollution {}% · evolution {}% · raids {} @ {}% · expansion {} @ {}%",
            config.preset,
            config.world.base_density_percent,
            config.world.starting_safe_radius_tiles,
            config.runtime.strength_percent,
            config.runtime.pollution_sensitivity_percent,
            config.runtime.evolution_rate_percent,
            if config.runtime.proactive_raids {
                "on"
            } else {
                "off"
            },
            config.runtime.raid_frequency_percent,
            if config.runtime.expansion {
                "on"
            } else {
                "off"
            },
            config.runtime.expansion_frequency_percent
        );
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::SimResource;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[test]
    fn world_setup_mode_blocks_fixed_simulation_ticks() {
        let mut app = App::new();
        app.insert_resource(StartInWorldSetup)
            .add_plugins(MinimalPlugins)
            .add_plugins(crate::FactoryAppPlugin)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
                1.0 / 60.0,
            )));
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<State<AppMode>>().get(),
            &AppMode::WorldSetup
        );
        assert_eq!(app.world().resource::<SimResource>().read().tick_count(), 0);
    }

    #[test]
    fn world_setup_mode_blocks_save_shortcuts() {
        let save_root =
            std::env::temp_dir().join(format!("factory-world-setup-gating-{}", std::process::id()));
        let mut app = App::new();
        app.insert_resource(StartInWorldSetup)
            .add_plugins(MinimalPlugins)
            .add_plugins(crate::FactoryAppPlugin)
            .insert_resource(crate::save_load::SaveLoadConfig {
                root_dir: save_root.clone(),
                autosave_interval_ticks: u64::MAX,
            });
        app.update();

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F5);
        app.update();

        assert!(
            app.world()
                .resource::<crate::save_load::PendingSaveJobs>()
                .is_empty(),
            "the quicksave shortcut must not run on the world-setup screen"
        );
        let _ = std::fs::remove_dir_all(save_root);
    }
}
