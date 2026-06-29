use bevy::prelude::*;

use crate::save_load::{
    LOAD_SAVE_SLOTS, LoadState, MANUAL_SAVE_SLOTS, PendingSaveJobs, SaveLoadConfig, SaveLoadStatus,
    SaveLoadStatusKind, SaveLoadTab, SaveLoadWindowState, SaveSlotKind, load_slot, request_save,
    slot_display_name, slot_exists, slot_modified_label,
};

#[derive(Component)]
pub struct SaveLoadRoot {
    snapshot: SaveLoadSnapshot,
}

#[derive(Component)]
pub struct SaveLoadTabButton {
    pub tab: SaveLoadTab,
}

#[derive(Component)]
pub struct SaveSlotButton {
    pub slot: SaveSlotKind,
    pub action: SaveSlotAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaveSlotAction {
    Save,
    Load,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SaveLoadSnapshot {
    window: SaveLoadWindowState,
    status: SaveLoadStatus,
    rows: Vec<SlotRowSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SlotRowSnapshot {
    slot: SaveSlotKind,
    modified_label: String,
    exists: bool,
    pending: bool,
}

type TabButtonQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static SaveLoadTabButton),
    (Changed<Interaction>, With<Button>, Without<SaveSlotButton>),
>;

type SlotButtonQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static SaveSlotButton),
    (Changed<Interaction>, With<Button>),
>;

pub(crate) fn handle_save_load_buttons(
    mut tab_buttons: TabButtonQuery,
    mut slot_buttons: SlotButtonQuery,
    config: Res<SaveLoadConfig>,
    mut pending_jobs: ResMut<PendingSaveJobs>,
    mut status: ResMut<SaveLoadStatus>,
    mut load_state: LoadState,
) {
    if !load_state.window.open {
        return;
    }

    for (interaction, button) in &mut tab_buttons {
        if *interaction == Interaction::Pressed {
            load_state.window.tab = button.tab;
        }
    }

    for (interaction, button) in &mut slot_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        load_state.window.selected_slot = button.slot;
        match button.action {
            SaveSlotAction::Save => {
                request_save(
                    button.slot,
                    &load_state.sim.sim,
                    &config,
                    &mut pending_jobs,
                    &mut status,
                );
            }
            SaveSlotAction::Load => {
                load_slot(button.slot, &config, &mut status, &mut load_state);
            }
        }
    }
}

pub(crate) fn sync_save_load_window(
    mut commands: Commands,
    state: Res<SaveLoadWindowState>,
    config: Res<SaveLoadConfig>,
    pending_jobs: Res<PendingSaveJobs>,
    status: Res<SaveLoadStatus>,
    mut roots: Query<(Entity, &mut SaveLoadRoot, Option<&Children>)>,
) {
    if !state.open {
        for (entity, _, _) in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let snapshot = save_load_snapshot(&state, &config, &pending_jobs, &status);
    let mut roots_iter = roots.iter_mut();
    let Some((root_entity, mut root, children)) = roots_iter.next() else {
        spawn_save_load_window(&mut commands, snapshot);
        return;
    };
    for (duplicate, _, _) in roots_iter {
        commands.entity(duplicate).despawn();
    }
    if root.snapshot == snapshot {
        return;
    }
    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    root.snapshot = snapshot.clone();
    commands
        .entity(root_entity)
        .with_children(|root| spawn_save_load_modal(root, &snapshot));
}

fn save_load_snapshot(
    state: &SaveLoadWindowState,
    config: &SaveLoadConfig,
    pending_jobs: &PendingSaveJobs,
    status: &SaveLoadStatus,
) -> SaveLoadSnapshot {
    SaveLoadSnapshot {
        window: state.clone(),
        status: status.clone(),
        rows: LOAD_SAVE_SLOTS
            .into_iter()
            .map(|slot| SlotRowSnapshot {
                slot,
                modified_label: slot_modified_label(config, slot),
                exists: slot_exists(config, slot),
                pending: pending_jobs.is_slot_pending(slot),
            })
            .collect(),
    }
}

fn spawn_save_load_window(commands: &mut Commands, snapshot: SaveLoadSnapshot) {
    commands
        .spawn((
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
            GlobalZIndex(2600),
            SaveLoadRoot {
                snapshot: snapshot.clone(),
            },
        ))
        .with_children(|root| spawn_save_load_modal(root, &snapshot));
}

fn spawn_save_load_modal(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &SaveLoadSnapshot,
) {
    root.spawn((
        Node {
            width: Val::Px(520.0),
            min_height: Val::Px(360.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            padding: UiRect::all(Val::Px(16.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.030, 0.032, 0.034, 0.98)),
        BorderColor::all(Color::srgba(0.36, 0.39, 0.34, 0.95)),
    ))
    .with_children(|modal| {
        modal.spawn((
            Text::new("Game Menu"),
            TextFont::from_font_size(20.0),
            TextColor(Color::srgb(0.94, 0.95, 0.90)),
        ));
        spawn_tabs(modal, snapshot.window.tab);
        match snapshot.window.tab {
            SaveLoadTab::Save => spawn_save_tab(modal, snapshot),
            SaveLoadTab::Load => spawn_load_tab(modal, snapshot),
        }
        spawn_status(modal, &snapshot.status);
    });
}

fn spawn_tabs(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, selected: SaveLoadTab) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|tabs| {
            for (tab, label) in [(SaveLoadTab::Save, "Save"), (SaveLoadTab::Load, "Load")] {
                tabs.spawn((
                    Button,
                    Node {
                        height: Val::Px(32.0),
                        width: Val::Px(96.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(if tab == selected {
                        Color::srgba(0.22, 0.27, 0.24, 0.98)
                    } else {
                        Color::srgba(0.10, 0.11, 0.11, 0.98)
                    }),
                    BorderColor::all(Color::srgba(0.38, 0.42, 0.36, 0.85)),
                    SaveLoadTabButton { tab },
                ))
                .with_child((
                    Text::new(label),
                    TextFont::from_font_size(13.0),
                    TextColor(Color::WHITE),
                ));
            }
        });
}

fn spawn_save_tab(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &SaveLoadSnapshot,
) {
    for slot in MANUAL_SAVE_SLOTS {
        let row = snapshot.row(slot);
        spawn_slot_row(
            parent,
            row,
            Some((SaveSlotAction::Save, "Save")),
            row.pending,
        );
    }

    for slot in [SaveSlotKind::Quick, SaveSlotKind::Auto] {
        let row = snapshot.row(slot);
        spawn_readonly_row(parent, row);
    }
}

fn spawn_load_tab(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &SaveLoadSnapshot,
) {
    for slot in LOAD_SAVE_SLOTS {
        let row = snapshot.row(slot);
        let action = row.exists.then_some((SaveSlotAction::Load, "Load"));
        spawn_slot_row(parent, row, action, false);
    }
}

fn spawn_slot_row(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    row: &SlotRowSnapshot,
    action: Option<(SaveSlotAction, &'static str)>,
    pending: bool,
) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                min_height: Val::Px(38.0),
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.0),
                padding: UiRect::horizontal(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.055, 0.058, 0.060, 0.90)),
        ))
        .with_children(|row_entity| {
            spawn_slot_name(row_entity, slot_display_name(row.slot));
            spawn_slot_meta(row_entity, &row.modified_label, row.exists);
            if pending {
                spawn_label(row_entity, "Saving", Color::srgb(0.95, 0.78, 0.42));
            } else if let Some((action, label)) = action {
                spawn_action_button(row_entity, row.slot, action, label);
            } else {
                spawn_label(row_entity, "Empty", Color::srgb(0.55, 0.57, 0.54));
            }
        });
}

fn spawn_readonly_row(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    row: &SlotRowSnapshot,
) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                min_height: Val::Px(34.0),
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.0),
                padding: UiRect::horizontal(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.045, 0.048, 0.050, 0.82)),
        ))
        .with_children(|row_entity| {
            spawn_slot_name(row_entity, slot_display_name(row.slot));
            spawn_slot_meta(row_entity, &row.modified_label, row.exists);
            let label = if row.pending {
                "Saving"
            } else if row.slot == SaveSlotKind::Quick {
                "F5"
            } else {
                "Auto"
            };
            spawn_label(row_entity, label, Color::srgb(0.70, 0.72, 0.66));
        });
}

fn spawn_slot_name(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, label: &str) {
    parent.spawn((
        Node {
            width: Val::Px(120.0),
            ..default()
        },
        Text::new(label.to_string()),
        TextFont::from_font_size(13.0),
        TextColor(Color::srgb(0.90, 0.91, 0.86)),
    ));
}

fn spawn_slot_meta(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    exists: bool,
) {
    parent.spawn((
        Node {
            width: Val::Px(250.0),
            ..default()
        },
        Text::new(label.to_string()),
        TextFont::from_font_size(12.0),
        TextColor(if exists {
            Color::srgb(0.76, 0.78, 0.72)
        } else {
            Color::srgb(0.52, 0.54, 0.50)
        }),
    ));
}

fn spawn_action_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    slot: SaveSlotKind,
    action: SaveSlotAction,
    label: &str,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(78.0),
                height: Val::Px(28.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.14, 0.16, 0.14, 0.98)),
            BorderColor::all(Color::srgba(0.46, 0.50, 0.42, 0.88)),
            SaveSlotButton { slot, action },
        ))
        .with_child((
            Text::new(label),
            TextFont::from_font_size(12.0),
            TextColor(Color::WHITE),
        ));
}

fn spawn_label(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, label: &str, color: Color) {
    parent.spawn((
        Node {
            width: Val::Px(78.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Text::new(label.to_string()),
        TextFont::from_font_size(12.0),
        TextColor(color),
    ));
}

fn spawn_status(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, status: &SaveLoadStatus) {
    let (message, color) = match status.message.as_deref() {
        Some(message) => {
            let color = match status.kind {
                SaveLoadStatusKind::Info => Color::srgb(0.86, 0.82, 0.66),
                SaveLoadStatusKind::Success => Color::srgb(0.54, 0.86, 0.56),
                SaveLoadStatusKind::Error => Color::srgb(0.96, 0.40, 0.34),
            };
            (message, color)
        }
        None => ("", Color::srgb(0.86, 0.82, 0.66)),
    };
    parent.spawn((
        Node {
            min_height: Val::Px(22.0),
            align_items: AlignItems::Center,
            ..default()
        },
        Text::new(message.to_string()),
        TextFont::from_font_size(12.0),
        TextColor(color),
    ));
}

impl SaveLoadSnapshot {
    fn row(&self, slot: SaveSlotKind) -> &SlotRowSnapshot {
        self.rows
            .iter()
            .find(|row| row.slot == slot)
            .expect("snapshot should contain every save/load slot")
    }
}
