use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::audio::SoundEvent;
use crate::save_load::{SaveLoadTab, SaveLoadWindowState};
use crate::simulation::AppPauseState;
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

/// Always-available HUD control for players who prefer pointer input.
#[derive(Component)]
pub struct PauseHudButton;

#[derive(Component)]
pub struct PauseHudRoot;

/// Non-interactive banner that remains visible while paused menus replace the
/// main pause menu (settings and save/load).
#[derive(Component)]
pub struct PauseIndicatorRoot;

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

type PauseHudButtons<'w, 's> =
    Query<'w, 's, &'static Interaction, (Changed<Interaction>, With<Button>, With<PauseHudButton>)>;

#[derive(SystemParam)]
pub(crate) struct PauseMenuResources<'w> {
    pause: ResMut<'w, PauseMenuState>,
    app_pause: ResMut<'w, AppPauseState>,
    save_load: ResMut<'w, SaveLoadWindowState>,
    confirmation: ResMut<'w, NewWorldConfirmation>,
    setup: ResMut<'w, WorldSetupState>,
    next: ResMut<'w, NextState<AppMode>>,
    sounds: MessageWriter<'w, SoundEvent>,
}

/// Handles actions that stay within or leave the pause-menu navigation flow.
pub(crate) fn handle_pause_menu_buttons(
    mut buttons: PauseMenuButtons,
    mut resources: PauseMenuResources,
) {
    if !resources.pause.open {
        return;
    }

    for (interaction, button) in &mut buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        resources.sounds.write(SoundEvent::UiClick);
        match button.action {
            PauseMenuAction::Resume => {
                resources.pause.open = false;
                resources.app_pause.resume();
                resources.confirmation.awaiting_confirmation = false;
            }
            PauseMenuAction::SaveLoad => {
                resources.pause.open = false;
                resources.confirmation.awaiting_confirmation = false;
                resources.save_load.open = true;
                resources.save_load.tab = SaveLoadTab::Save;
                resources.save_load.refresh_on_open = true;
            }
            PauseMenuAction::NewWorld if resources.confirmation.awaiting_confirmation => {
                resources.pause.open = false;
                resources.app_pause.resume();
                resources.confirmation.awaiting_confirmation = false;
                resources.setup.allow_cancel = true;
                resources.next.set(AppMode::WorldSetup);
            }
            PauseMenuAction::NewWorld => {
                resources.confirmation.awaiting_confirmation = true;
            }
        }
    }
}

/// Creates the visible pointer control that opens the pause menu.
pub(crate) fn setup_pause_hud(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(14.0),
                bottom: Val::Px(14.0),
                ..default()
            },
            GlobalZIndex(1900),
            PauseHudRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Button,
                Node {
                    height: Val::Px(32.0),
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.025, 0.030, 0.029, 0.94)),
                BorderColor::all(Color::srgb(0.40, 0.48, 0.36)),
                PauseHudButton,
            ))
            .with_child((
                Text::new("PAUSE"),
                TextFont::from_font_size(11.0),
                TextColor(Color::srgb(0.88, 0.94, 0.78)),
            ));
        });
}

/// Opens pause through the HUD control; Resume inside the menu is the matching
/// pointer-driven transition back to the running state.
pub(crate) fn handle_pause_hud_button(
    mut buttons: PauseHudButtons,
    mut app_pause: ResMut<AppPauseState>,
    mut pause: ResMut<PauseMenuState>,
    mut confirmation: ResMut<NewWorldConfirmation>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    if app_pause.is_paused() {
        return;
    }
    if buttons
        .iter_mut()
        .any(|interaction| *interaction == Interaction::Pressed)
    {
        app_pause.pause();
        pause.open = true;
        confirmation.awaiting_confirmation = false;
        sounds.write(SoundEvent::UiClick);
    }
}

/// Keeps the running-state HUD button out of the way while a pause surface is open.
pub(crate) fn sync_pause_hud(
    app_pause: Res<AppPauseState>,
    mut roots: Query<&mut Visibility, With<PauseHudRoot>>,
) {
    if !app_pause.is_changed() {
        return;
    }
    let visibility = if app_pause.is_paused() {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    for mut current in &mut roots {
        *current = visibility;
    }
}

/// Shows a compact pause banner when settings or save/load hide the main menu.
pub(crate) fn sync_pause_indicator(
    mut commands: Commands,
    app_pause: Res<AppPauseState>,
    pause_menu: Res<PauseMenuState>,
    existing: Query<Entity, With<PauseIndicatorRoot>>,
) {
    if !app_pause.is_changed() && !pause_menu.is_changed() {
        return;
    }
    let should_show = app_pause.is_paused() && !pause_menu.open;
    if !should_show {
        for entity in &existing {
            commands.entity(entity).despawn();
        }
        return;
    }
    if !existing.is_empty() {
        return;
    }

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::ZERO,
                right: Val::ZERO,
                top: Val::Px(14.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(3000),
            Pickable::IGNORE,
            PauseIndicatorRoot,
        ))
        .with_child((
            Text::new("PAUSED"),
            TextFont::from_font_size(14.0),
            TextColor(Color::srgb(0.95, 0.91, 0.64)),
            BackgroundColor(Color::srgba(0.025, 0.030, 0.029, 0.94)),
            Node {
                padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgb(0.65, 0.58, 0.30)),
            Pickable::IGNORE,
        ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indicator_follows_pause_while_the_main_menu_is_hidden() {
        let mut app = App::new();
        app.init_resource::<AppPauseState>()
            .init_resource::<PauseMenuState>()
            .add_systems(Update, sync_pause_indicator);

        app.world_mut().resource_mut::<AppPauseState>().pause();
        app.update();
        assert_eq!(indicator_count(&mut app), 1);

        app.world_mut().resource_mut::<PauseMenuState>().open = true;
        app.update();
        assert_eq!(indicator_count(&mut app), 0);

        app.world_mut().resource_mut::<PauseMenuState>().open = false;
        app.update();
        assert_eq!(indicator_count(&mut app), 1);

        app.world_mut().resource_mut::<AppPauseState>().resume();
        app.update();
        assert_eq!(indicator_count(&mut app), 0);
    }

    fn indicator_count(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut indicators = world.query_filtered::<Entity, With<PauseIndicatorRoot>>();
        indicators.iter(world).count()
    }
}
