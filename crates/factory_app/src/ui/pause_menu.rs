use bevy::prelude::*;

use crate::audio::SoundEvent;
use crate::save_load::{SaveLoadTab, SaveLoadWindowState};
use crate::ui::settings::SettingsMenuButton;
use crate::ui::window_sync::{WindowRootQuery, sync_window};
use crate::world_setup::{AppMode, WorldSetupState};

/// Open state for the compact in-game pause menu.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PauseMenuState {
    pub open: bool,
}

/// Confirmation state for leaving the current world.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NewWorldConfirmation {
    pub awaiting_confirmation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseMenuAction {
    Resume,
    SaveLoad,
    NewWorld,
}

#[derive(Component)]
pub struct PauseMenuActionButton {
    pub action: PauseMenuAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PauseMenuSnapshot {
    new_world_confirmation: bool,
}

type PauseMenuButtons<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static PauseMenuActionButton),
    (Changed<Interaction>, With<Button>),
>;

/// Handles actions that stay within or leave the pause-menu navigation flow.
pub(crate) fn handle_pause_menu_buttons(
    mut buttons: PauseMenuButtons,
    mut pause: ResMut<PauseMenuState>,
    mut save_load: ResMut<SaveLoadWindowState>,
    mut confirmation: ResMut<NewWorldConfirmation>,
    mut setup: ResMut<WorldSetupState>,
    mut next: ResMut<NextState<AppMode>>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    if !pause.open {
        return;
    }

    for (interaction, button) in &mut buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        sounds.write(SoundEvent::UiClick);
        match button.action {
            PauseMenuAction::Resume => {
                pause.open = false;
                confirmation.awaiting_confirmation = false;
            }
            PauseMenuAction::SaveLoad => {
                pause.open = false;
                confirmation.awaiting_confirmation = false;
                save_load.open = true;
                save_load.tab = SaveLoadTab::Save;
                save_load.refresh_on_open = true;
            }
            PauseMenuAction::NewWorld if confirmation.awaiting_confirmation => {
                pause.open = false;
                confirmation.awaiting_confirmation = false;
                setup.allow_cancel = true;
                next.set(AppMode::WorldSetup);
            }
            PauseMenuAction::NewWorld => {
                confirmation.awaiting_confirmation = true;
            }
        }
    }
}

/// Reconciles the compact pause menu with its current confirmation state.
pub(crate) fn sync_pause_menu(
    mut commands: Commands,
    pause: Res<PauseMenuState>,
    mut confirmation: ResMut<NewWorldConfirmation>,
    mut roots: WindowRootQuery<PauseMenuSnapshot>,
) {
    if !pause.open && confirmation.awaiting_confirmation {
        confirmation.awaiting_confirmation = false;
    }
    sync_window(
        &mut commands,
        &mut roots,
        pause.open,
        pause.is_changed() || confirmation.is_changed(),
        || PauseMenuSnapshot {
            new_world_confirmation: confirmation.awaiting_confirmation,
        },
        pause_menu_root,
        spawn_pause_menu,
    );
}

fn pause_menu_root() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::ZERO,
            right: Val::ZERO,
            top: Val::ZERO,
            bottom: Val::ZERO,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.62)),
        GlobalZIndex(2600),
    )
}

fn spawn_pause_menu(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &PauseMenuSnapshot,
) {
    root.spawn((
        Node {
            width: Val::Vw(88.0),
            max_width: Val::Px(440.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            padding: UiRect::all(Val::Px(20.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.025, 0.030, 0.029, 0.99)),
        BorderColor::all(Color::srgb(0.40, 0.48, 0.36)),
    ))
    .with_children(|menu| {
        menu.spawn((
            Text::new("PAUSED"),
            TextFont::from_font_size(22.0),
            TextColor(Color::srgb(0.88, 0.94, 0.78)),
            Node {
                margin: UiRect::bottom(Val::Px(6.0)),
                ..default()
            },
        ));
        spawn_action_button(menu, "Resume", PauseMenuAction::Resume, false);
        spawn_action_button(menu, "Save / Load", PauseMenuAction::SaveLoad, false);
        spawn_settings_button(menu);
        spawn_action_button(
            menu,
            if snapshot.new_world_confirmation {
                "Confirm New World"
            } else {
                "New World"
            },
            PauseMenuAction::NewWorld,
            true,
        );
        if snapshot.new_world_confirmation {
            menu.spawn((
                Text::new("Unsaved progress in the current world will be lost."),
                TextFont::from_font_size(11.0),
                TextColor(Color::srgb(0.96, 0.52, 0.38)),
            ));
        }
    });
}

fn spawn_action_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    action: PauseMenuAction,
    destructive: bool,
) {
    parent
        .spawn((
            Button,
            pause_button_node(),
            BackgroundColor(if destructive {
                Color::srgb(0.17, 0.07, 0.05)
            } else {
                Color::srgb(0.08, 0.10, 0.09)
            }),
            BorderColor::all(if destructive {
                Color::srgb(0.72, 0.28, 0.18)
            } else {
                Color::srgb(0.38, 0.45, 0.34)
            }),
            PauseMenuActionButton { action },
        ))
        .with_child((Text::new(label), TextFont::from_font_size(12.0)));
}

fn spawn_settings_button(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    parent
        .spawn((
            Button,
            pause_button_node(),
            BackgroundColor(Color::srgb(0.08, 0.10, 0.09)),
            BorderColor::all(Color::srgb(0.38, 0.45, 0.34)),
            SettingsMenuButton,
        ))
        .with_child((Text::new("Settings"), TextFont::from_font_size(12.0)));
}

fn pause_button_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Px(36.0),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    }
}
