use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::audio::SoundEvent;
use crate::input::build::{select_build_selection, technology_window_open};
use crate::placement::build::buildable_prototypes;
use crate::build::resources::{
    BuildMenuState, BuildPlacementState, BuildPlacementStatus, BuildSelection, HotbarState,
};
use crate::resources::SimResource;
use crate::ui::resources::{OpenContainer, TechnologyWindowState};
use crate::ui::build_bar::{BuildMenuButton, slot_key_label};
use crate::ui::window_sync::{WindowRootQuery, sync_window};
use crate::utils::compact_item_name;
use factory_sim::Simulation;

const CELL_WIDTH: f32 = 150.0;
const CELL_HEIGHT: f32 = 62.0;
const CELL_GAP: f32 = 6.0;
const GRID_COLUMNS: f32 = 5.0;
// A definite grid width is required for correct flex-wrap sizing: without it
// the wrapped height is under-measured and the panel collapses.
const GRID_WIDTH: f32 = GRID_COLUMNS * CELL_WIDTH + (GRID_COLUMNS - 1.0) * CELL_GAP;

#[derive(Component)]
pub(crate) struct BuildMenuSelectButton {
    selection: BuildSelection,
}

#[derive(Component)]
pub(crate) struct BuildMenuHotbarToggleButton {
    selection: BuildSelection,
}

#[derive(Component)]
pub(crate) struct BuildMenuCloseButton;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildMenuSnapshot {
    entries: Vec<BuildMenuEntry>,
    message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BuildMenuEntry {
    selection: BuildSelection,
    display_name: String,
    compact_name: String,
    count: String,
    unlocked: bool,
    hotbar_slot: Option<usize>,
}

type BuildMenuToggleInteractionQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (Changed<Interaction>, With<Button>, With<BuildMenuButton>),
>;
type BuildMenuCloseInteractionQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<BuildMenuCloseButton>,
    ),
>;
type BuildMenuSelectInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static BuildMenuSelectButton),
    (Changed<Interaction>, With<Button>),
>;
type BuildMenuHotbarToggleInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static BuildMenuHotbarToggleButton),
    (Changed<Interaction>, With<Button>),
>;

#[derive(SystemParam)]
pub(crate) struct BuildMenuButtonState<'w> {
    sim: Res<'w, SimResource>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    menu: ResMut<'w, BuildMenuState>,
    hotbar: ResMut<'w, HotbarState>,
    build_state: ResMut<'w, BuildPlacementState>,
    open_container: ResMut<'w, OpenContainer>,
    sounds: MessageWriter<'w, SoundEvent>,
}

pub(crate) fn handle_build_menu_buttons(
    mut toggle_interactions: BuildMenuToggleInteractionQuery,
    mut close_interactions: BuildMenuCloseInteractionQuery,
    mut select_interactions: BuildMenuSelectInteractionQuery,
    mut hotbar_toggle_interactions: BuildMenuHotbarToggleInteractionQuery,
    mut state: BuildMenuButtonState,
) {
    // The build menu itself blocks world input, so unlike the build bar this
    // only guards against the technology window layering on top of the menu.
    if technology_window_open(state.technology_window.as_deref()) {
        return;
    }

    let toggle_pressed = toggle_interactions
        .iter_mut()
        .any(|interaction| *interaction == Interaction::Pressed);
    let close_pressed = close_interactions
        .iter_mut()
        .any(|interaction| *interaction == Interaction::Pressed);
    if toggle_pressed || close_pressed {
        state.sounds.write(SoundEvent::UiClick);
        if state.menu.open {
            state.menu.open = false;
            state.menu.message = None;
        } else if toggle_pressed {
            state.menu.open = true;
            state.menu.message = None;
            state.build_state.selected = None;
            state.open_container.entity_id = None;
        }
        return;
    }

    if !state.menu.open {
        return;
    }

    for (interaction, button) in &mut select_interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        state.sounds.write(SoundEvent::UiClick);
        if select_build_selection(
            &state.sim.sim,
            state.technology_window.as_deref(),
            &mut state.build_state,
            button.selection,
        ) {
            state.menu.open = false;
            state.menu.message = None;
        } else {
            state.menu.message = status_message(&state.build_state.last_status);
        }
    }

    for (interaction, button) in &mut hotbar_toggle_interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        state.sounds.write(SoundEvent::UiClick);
        if state.hotbar.remove(button.selection)
            || state
                .hotbar
                .assign_to_first_empty(button.selection)
                .is_some()
        {
            state.menu.message = None;
        } else {
            state.menu.message =
                Some("Hotbar is full - remove a building from it first".to_string());
        }
    }
}

fn status_message(status: &BuildPlacementStatus) -> Option<String> {
    match status {
        BuildPlacementStatus::Ready => None,
        BuildPlacementStatus::Placed(message)
        | BuildPlacementStatus::CannotPlace(message)
        | BuildPlacementStatus::MissingInventory(message)
        | BuildPlacementStatus::Locked(message) => Some(message.clone()),
    }
}

pub(crate) fn sync_build_menu(
    mut commands: Commands,
    sim: Res<SimResource>,
    hotbar: Res<HotbarState>,
    state: Res<BuildMenuState>,
    mut roots: WindowRootQuery<BuildMenuSnapshot>,
) {
    sync_window(
        &mut commands,
        &mut roots,
        state.open,
        sim.is_changed() || hotbar.is_changed() || state.is_changed(),
        || build_menu_snapshot(&sim.sim, &hotbar, &state),
        build_menu_root,
        spawn_build_menu_contents,
    );
}

fn build_menu_snapshot(
    sim: &Simulation,
    hotbar: &HotbarState,
    state: &BuildMenuState,
) -> BuildMenuSnapshot {
    let entries = buildable_prototypes(sim.catalog())
        .into_iter()
        .map(|buildable| {
            let selection = buildable.selection();
            BuildMenuEntry {
                selection,
                compact_name: compact_item_name(
                    &buildable.display_name.to_lowercase().replace(' ', "_"),
                ),
                display_name: buildable.display_name,
                count: sim.player_inventory().count(selection.item_id).to_string(),
                unlocked: sim.is_entity_unlocked(selection.prototype_id),
                hotbar_slot: hotbar.slot_of(selection),
            }
        })
        .collect();

    BuildMenuSnapshot {
        entries,
        message: state.message.clone(),
    }
}

fn build_menu_root() -> impl Bundle {
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
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.40)),
        GlobalZIndex(2200),
    )
}

fn spawn_build_menu_contents(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &BuildMenuSnapshot,
) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            padding: UiRect::all(Val::Px(14.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.035, 0.038, 0.040, 0.97)),
        BorderColor::all(Color::srgba(0.44, 0.43, 0.39, 0.70)),
    ))
    .with_children(|panel| {
        spawn_build_menu_header(panel);
        panel.spawn((
            Text::new(
                "Click a building to start placing it. + adds it to the hotbar, - removes it.",
            ),
            TextFont::from_font_size(11.0),
            TextColor(Color::srgb(0.68, 0.70, 0.66)),
        ));
        if let Some(message) = &snapshot.message {
            panel.spawn((
                Text::new(message.clone()),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.98, 0.72, 0.28)),
            ));
        }
        panel
            .spawn((
                Node {
                    width: Val::Px(GRID_WIDTH),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(CELL_GAP),
                    row_gap: Val::Px(CELL_GAP),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|grid| {
                for entry in &snapshot.entries {
                    spawn_build_menu_entry(grid, entry);
                }
            });
    });
}

fn spawn_build_menu_header(panel: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    panel
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|header| {
            header.spawn((
                Text::new("Buildings"),
                TextFont::from_font_size(18.0),
                TextColor(Color::srgb(0.92, 0.93, 0.88)),
            ));
            header
                .spawn((
                    Button,
                    Node {
                        height: Val::Px(26.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        padding: UiRect::horizontal(Val::Px(10.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.95)),
                    BorderColor::all(Color::srgba(0.44, 0.43, 0.39, 0.70)),
                    BuildMenuCloseButton,
                ))
                .with_child((
                    Text::new("Close (Esc)"),
                    TextFont::from_font_size(11.0),
                    TextColor(Color::WHITE),
                ));
        });
}

fn spawn_build_menu_entry(
    grid: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    entry: &BuildMenuEntry,
) {
    let on_hotbar = entry.hotbar_slot.is_some();
    let border_color = if on_hotbar {
        Color::srgba(0.94, 0.66, 0.20, 0.75)
    } else {
        Color::srgba(0.44, 0.43, 0.39, 0.70)
    };
    let name_color = if entry.unlocked {
        Color::WHITE
    } else {
        Color::srgb(0.56, 0.55, 0.52)
    };
    let title = match entry.hotbar_slot {
        Some(slot_index) => format!("{} [{}]", entry.compact_name, slot_key_label(slot_index)),
        None => entry.compact_name.clone(),
    };
    let detail = if entry.unlocked {
        format!("x{}", entry.count)
    } else {
        "Locked".to_string()
    };

    grid.spawn((
        Node {
            width: Val::Px(CELL_WIDTH),
            height: Val::Px(CELL_HEIGHT),
            flex_direction: FlexDirection::Row,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(border_color),
    ))
    .with_children(|cell| {
        cell.spawn((
            Button,
            Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(Val::Px(5.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(if entry.unlocked {
                Color::srgba(0.13, 0.13, 0.13, 0.95)
            } else {
                Color::srgba(0.075, 0.075, 0.075, 0.86)
            }),
            BuildMenuSelectButton {
                selection: entry.selection,
            },
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(title),
                TextFont::from_font_size(12.0),
                TextColor(name_color),
            ));
            button.spawn((
                Text::new(entry.display_name.clone()),
                TextFont::from_font_size(9.0),
                TextColor(Color::srgb(0.68, 0.70, 0.66)),
            ));
            button.spawn((
                Text::new(detail),
                TextFont::from_font_size(10.0),
                TextColor(if entry.unlocked && entry.count != "0" {
                    Color::srgb(0.91, 0.92, 0.86)
                } else {
                    Color::srgb(0.62, 0.58, 0.52)
                }),
            ));
        });
        cell.spawn((
            Button,
            Node {
                width: Val::Px(22.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::left(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(if on_hotbar {
                Color::srgba(0.27, 0.20, 0.10, 0.95)
            } else {
                Color::srgba(0.10, 0.13, 0.10, 0.95)
            }),
            BorderColor::all(border_color),
            BuildMenuHotbarToggleButton {
                selection: entry.selection,
            },
        ))
        .with_child((
            Text::new(if on_hotbar { "-" } else { "+" }),
            TextFont::from_font_size(15.0),
            TextColor(if on_hotbar {
                Color::srgb(0.98, 0.72, 0.28)
            } else {
                Color::srgb(0.56, 0.92, 0.55)
            }),
        ));
    });
}
