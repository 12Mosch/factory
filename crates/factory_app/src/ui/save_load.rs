use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use chrono::{DateTime, Local};

use crate::audio::SoundEvent;
use crate::save_load::{
    LoadState, PendingSaveConfirmation, PendingSaveJobs, SaveCatalog, SaveEntry, SaveId, SaveKind,
    SaveLoadConfig, SaveLoadStatus, SaveLoadStatusKind, SaveLoadTab, SaveLoadWindowState,
    delete_save, load_save, request_named_save, request_overwrite,
};
use crate::ui::layout::scroll_column;
use crate::ui::window_sync::{WindowRootQuery, sync_window};
use crate::utils::remove_previous_word;
use crate::world_setup::{AppMode, WorldSetupState};

#[derive(Component)]
pub struct SaveLoadTabButton {
    pub tab: SaveLoadTab,
}
#[derive(Component)]
pub struct SaveEntryButton {
    pub id: SaveId,
    pub action: SaveEntryAction,
}
#[derive(Component)]
pub struct SaveCreateButton;
#[derive(Component)]
pub struct SaveConfirmationButton(pub bool);
#[derive(Component)]
pub struct SaveLoadModal;
#[derive(Component)]
pub struct SaveLoadSlotList;
#[derive(Component)]
pub struct NewWorldButton;
#[derive(Resource, Default)]
pub struct NewWorldConfirmation {
    pub awaiting_confirmation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaveEntryAction {
    Overwrite,
    Load,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SaveLoadSnapshot {
    window: SaveLoadWindowState,
    status: SaveLoadStatus,
    entries: Vec<SaveEntry>,
    pending: Vec<SaveId>,
    confirmation: PendingSaveConfirmation,
    new_world_confirmation: bool,
}

type TabButtons<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static SaveLoadTabButton),
    (Changed<Interaction>, With<Button>),
>;
type EntryButtons<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static SaveEntryButton),
    (Changed<Interaction>, With<Button>),
>;

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_save_load_buttons(
    mut tabs: TabButtons,
    mut entries: EntryButtons,
    mut create: Query<&Interaction, (Changed<Interaction>, With<SaveCreateButton>)>,
    mut confirms: Query<(&Interaction, &SaveConfirmationButton), Changed<Interaction>>,
    config: Res<SaveLoadConfig>,
    mut catalog: ResMut<SaveCatalog>,
    mut pending: ResMut<PendingSaveJobs>,
    mut confirmation: ResMut<PendingSaveConfirmation>,
    mut status: ResMut<SaveLoadStatus>,
    mut load_state: LoadState,
    mut sounds: MessageWriter<SoundEvent>,
) {
    if !load_state.window.open {
        return;
    }
    for (interaction, button) in &mut tabs {
        if *interaction == Interaction::Pressed {
            sounds.write(SoundEvent::UiClick);
            load_state.window.tab = button.tab;
        }
    }
    for interaction in &mut create {
        if *interaction == Interaction::Pressed {
            sounds.write(SoundEvent::UiClick);
            request_named_save(
                &load_state.window.name_buffer,
                &load_state.sim,
                &config,
                &catalog,
                &mut pending,
                &mut confirmation,
                &mut status,
                &mut load_state.metrics,
            );
        }
    }
    for (interaction, button) in &mut entries {
        if *interaction != Interaction::Pressed {
            continue;
        }
        sounds.write(SoundEvent::UiClick);
        match button.action {
            SaveEntryAction::Overwrite => {
                *confirmation = PendingSaveConfirmation::Overwrite(button.id.clone())
            }
            SaveEntryAction::Delete => {
                *confirmation = PendingSaveConfirmation::Delete(button.id.clone())
            }
            SaveEntryAction::Load => {
                load_save(&button.id, &catalog, &pending, &mut status, &mut load_state);
            }
        }
    }
    for (interaction, button) in &mut confirms {
        if *interaction != Interaction::Pressed {
            continue;
        }
        sounds.write(SoundEvent::UiClick);
        let pending_confirmation = std::mem::take(&mut *confirmation);
        if !button.0 {
            continue;
        }
        match pending_confirmation {
            PendingSaveConfirmation::Overwrite(id) => {
                request_overwrite(
                    &id,
                    &load_state.sim,
                    &catalog,
                    &mut pending,
                    &mut status,
                    &mut load_state.metrics,
                );
            }
            PendingSaveConfirmation::Delete(id) => {
                delete_save(&id, &config, &mut catalog, &pending, &mut status);
            }
            PendingSaveConfirmation::None => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_save_name_input(
    mut inputs: MessageReader<KeyboardInput>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    config: Res<SaveLoadConfig>,
    catalog: Res<SaveCatalog>,
    mut pending: ResMut<PendingSaveJobs>,
    mut confirmation: ResMut<PendingSaveConfirmation>,
    mut status: ResMut<SaveLoadStatus>,
    mut state: ResMut<SaveLoadWindowState>,
    sim: Res<crate::resources::SimResource>,
    mut metrics: ResMut<crate::save_load::SaveLoadMetrics>,
) {
    if !state.open
        || state.tab != SaveLoadTab::Save
        || *confirmation != PendingSaveConfirmation::None
    {
        return;
    }
    let control = keyboard.as_deref().is_some_and(|keys| {
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
    });
    for input in inputs.read() {
        if input.state != ButtonState::Pressed || input.key_code == KeyCode::Escape {
            continue;
        }
        match input.key_code {
            KeyCode::Enter | KeyCode::NumpadEnter => {
                request_named_save(
                    &state.name_buffer,
                    &sim,
                    &config,
                    &catalog,
                    &mut pending,
                    &mut confirmation,
                    &mut status,
                    &mut metrics,
                );
            }
            KeyCode::Backspace if control => remove_previous_word(&mut state.name_buffer),
            KeyCode::Backspace => {
                state.name_buffer.pop();
            }
            _ if !control => {
                if let Some(text) = &input.text {
                    let remaining = 65usize.saturating_sub(state.name_buffer.chars().count());
                    state
                        .name_buffer
                        .extend(text.chars().filter(|c| !c.is_control()).take(remaining));
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_save_load_window(
    mut commands: Commands,
    state: Res<SaveLoadWindowState>,
    catalog: Res<SaveCatalog>,
    pending: Res<PendingSaveJobs>,
    status: Res<SaveLoadStatus>,
    confirmation: Res<PendingSaveConfirmation>,
    mut new_world: ResMut<NewWorldConfirmation>,
    mut roots: WindowRootQuery<SaveLoadSnapshot>,
) {
    if !state.open {
        new_world.awaiting_confirmation = false;
    }
    sync_window(
        &mut commands,
        &mut roots,
        state.open,
        true,
        || SaveLoadSnapshot {
            window: state.clone(),
            status: status.clone(),
            entries: catalog.entries().to_vec(),
            pending: pending.pending_ids(),
            confirmation: confirmation.clone(),
            new_world_confirmation: new_world.awaiting_confirmation,
        },
        save_load_root,
        spawn_save_load_modal,
    );
}

fn save_load_root() -> impl Bundle {
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
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.62)),
        GlobalZIndex(2600),
    )
}

fn spawn_save_load_modal(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &SaveLoadSnapshot,
) {
    root.spawn((
        Node {
            width: Val::Vw(94.0),
            max_width: Val::Px(820.0),
            height: Val::Vh(88.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            padding: UiRect::all(Val::Px(18.0)),
            border: UiRect::all(Val::Px(1.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.025, 0.030, 0.029, 0.99)),
        BorderColor::all(Color::srgb(0.40, 0.48, 0.36)),
        SaveLoadModal,
    ))
    .with_children(|modal| {
        modal.spawn((
            Text::new("FACTORY ARCHIVE"),
            TextFont::from_font_size(22.0),
            TextColor(Color::srgb(0.88, 0.94, 0.78)),
        ));
        spawn_tabs(modal, snapshot.window.tab);
        if snapshot.window.tab == SaveLoadTab::Save {
            spawn_name_input(modal, &snapshot.window.name_buffer);
        }
        spawn_catalog(modal, snapshot);
        if let Some(id) = confirmation_id(&snapshot.confirmation)
            && let Some(entry) = snapshot.entries.iter().find(|entry| &entry.id == id)
        {
            spawn_confirmation(
                modal,
                entry,
                matches!(snapshot.confirmation, PendingSaveConfirmation::Delete(_)),
            );
        }
        spawn_status(modal, &snapshot.status);
        modal
            .spawn((
                Button,
                Node {
                    height: Val::Px(32.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.17, 0.07, 0.05)),
                BorderColor::all(Color::srgb(0.72, 0.28, 0.18)),
                NewWorldButton,
            ))
            .with_child((
                Text::new(if snapshot.new_world_confirmation {
                    "Confirm New World"
                } else {
                    "New World"
                }),
                TextFont::from_font_size(12.0),
            ));
    });
}

fn spawn_tabs(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, selected: SaveLoadTab) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|row| {
            for (tab, label) in [(SaveLoadTab::Save, "SAVE"), (SaveLoadTab::Load, "LOAD")] {
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(100.0),
                        height: Val::Px(30.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(if tab == selected {
                        Color::srgb(0.22, 0.29, 0.20)
                    } else {
                        Color::srgb(0.08, 0.10, 0.09)
                    }),
                    BorderColor::all(Color::srgb(0.38, 0.45, 0.34)),
                    SaveLoadTabButton { tab },
                ))
                .with_child((Text::new(label), TextFont::from_font_size(12.0)));
            }
        });
}

fn spawn_name_input(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, value: &str) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(8.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Node {
                    height: Val::Px(34.0),
                    flex_grow: 1.0,
                    padding: UiRect::horizontal(Val::Px(10.0)),
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.045, 0.055, 0.050)),
                BorderColor::all(Color::srgb(0.34, 0.42, 0.31)),
            ))
            .with_child((
                Text::new(if value.is_empty() {
                    "Name your save…".to_string()
                } else {
                    value.to_string()
                }),
                TextFont::from_font_size(13.0),
                TextColor(if value.is_empty() {
                    Color::srgb(0.48, 0.52, 0.46)
                } else {
                    Color::WHITE
                }),
            ));
            spawn_plain_button(row, "Create Save", Some(SaveCreateButton));
        });
}

fn spawn_catalog(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &SaveLoadSnapshot,
) {
    let mut node = scroll_column();
    node.row_gap = Val::Px(5.0);
    node.flex_grow = 1.0;
    parent
        .spawn((node, SaveLoadSlotList))
        .with_children(|list| {
            for (kind, heading) in [(0, "NAMED SAVES"), (1, "QUICKSAVE"), (2, "AUTOSAVES")] {
                list.spawn((
                    Text::new(heading),
                    TextFont::from_font_size(11.0),
                    TextColor(Color::srgb(0.68, 0.76, 0.59)),
                    Node {
                        margin: UiRect::top(Val::Px(6.0)),
                        ..default()
                    },
                ));
                let entries = snapshot
                    .entries
                    .iter()
                    .filter(|entry| group(&entry.metadata.kind) == kind)
                    .collect::<Vec<_>>();
                if entries.is_empty() {
                    list.spawn((
                        Text::new("No saves"),
                        TextFont::from_font_size(12.0),
                        TextColor(Color::srgb(0.45, 0.48, 0.43)),
                    ));
                }
                for entry in entries {
                    spawn_entry_row(list, entry, snapshot);
                }
            }
        });
}

fn spawn_entry_row(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    entry: &SaveEntry,
    snapshot: &SaveLoadSnapshot,
) {
    let pending = snapshot.pending.contains(&entry.id);
    parent
        .spawn((
            Node {
                min_height: Val::Px(48.0),
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                row_gap: Val::Px(3.0),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.050, 0.060, 0.055)),
        ))
        .with_children(|row| {
            let name = match entry.metadata.kind {
                SaveKind::Autosave { generation } => {
                    format!("{}  ·  #{}", entry.metadata.display_name, generation)
                }
                _ => entry.metadata.display_name.clone(),
            };
            row.spawn((
                Node {
                    width: Val::Px(190.0),
                    ..default()
                },
                Text::new(name),
                TextFont::from_font_size(13.0),
                TextColor(Color::srgb(0.90, 0.92, 0.84)),
            ));
            let mut metadata = format!(
                "{}  ·  {}",
                format_timestamp(entry.metadata.completed_at_unix_ms),
                entry.compatibility.short_label()
            );
            if !entry.metadata_available {
                metadata.push_str("  ·  metadata unavailable");
            }
            if let Some(reason) = entry.compatibility.reason() {
                metadata.push_str(&format!("\n{reason}"));
            }
            row.spawn((
                Node {
                    flex_grow: 1.0,
                    flex_basis: Val::Px(280.0),
                    ..default()
                },
                Text::new(metadata),
                TextFont::from_font_size(11.0),
                TextColor(if entry.compatibility.can_load() {
                    Color::srgb(0.68, 0.78, 0.64)
                } else {
                    Color::srgb(0.92, 0.48, 0.38)
                }),
            ));
            if pending {
                spawn_badge(row, "SAVING");
                return;
            }
            match snapshot.window.tab {
                SaveLoadTab::Save if entry.metadata.kind == SaveKind::Named => {
                    spawn_entry_button(row, entry, SaveEntryAction::Overwrite, "Overwrite", false)
                }
                SaveLoadTab::Load if entry.compatibility.can_load() => {
                    spawn_entry_button(row, entry, SaveEntryAction::Load, "Load", false)
                }
                _ => {}
            }
            spawn_entry_button(row, entry, SaveEntryAction::Delete, "Delete", true);
        });
}

fn spawn_entry_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    entry: &SaveEntry,
    action: SaveEntryAction,
    label: &str,
    destructive: bool,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(72.0),
                height: Val::Px(27.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(if destructive {
                Color::srgb(0.22, 0.07, 0.05)
            } else {
                Color::srgb(0.11, 0.15, 0.10)
            }),
            BorderColor::all(if destructive {
                Color::srgb(0.72, 0.28, 0.20)
            } else {
                Color::srgb(0.40, 0.49, 0.35)
            }),
            SaveEntryButton {
                id: entry.id.clone(),
                action,
            },
        ))
        .with_child((Text::new(label.to_string()), TextFont::from_font_size(11.0)));
}

fn spawn_confirmation(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    entry: &SaveEntry,
    destructive: bool,
) {
    let verb = if destructive { "Delete" } else { "Overwrite" };
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(10.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.19, 0.055, 0.035)),
            BorderColor::all(Color::srgb(0.82, 0.28, 0.16)),
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
                Text::new(format!(
                    "{verb} “{}” · {}?",
                    entry.metadata.display_name,
                    format_timestamp(entry.metadata.completed_at_unix_ms)
                )),
                TextFont::from_font_size(12.0),
            ));
            spawn_confirmation_button(row, "Cancel", false, false);
            spawn_confirmation_button(row, &format!("Confirm {verb}"), true, destructive);
        });
}

fn spawn_confirmation_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    confirm: bool,
    destructive: bool,
) {
    parent
        .spawn((
            Button,
            Node {
                height: Val::Px(28.0),
                min_width: Val::Px(82.0),
                padding: UiRect::horizontal(Val::Px(8.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(if destructive {
                Color::srgb(0.50, 0.09, 0.05)
            } else {
                Color::srgb(0.12, 0.14, 0.12)
            }),
            BorderColor::all(Color::srgb(0.72, 0.34, 0.24)),
            SaveConfirmationButton(confirm),
        ))
        .with_child((Text::new(label.to_string()), TextFont::from_font_size(11.0)));
}

fn spawn_plain_button<T: Component>(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    marker: Option<T>,
) {
    let mut entity = parent.spawn((
        Button,
        Node {
            height: Val::Px(34.0),
            min_width: Val::Px(108.0),
            padding: UiRect::horizontal(Val::Px(10.0)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.16, 0.22, 0.14)),
        BorderColor::all(Color::srgb(0.45, 0.55, 0.38)),
    ));
    if let Some(marker) = marker {
        entity.insert(marker);
    }
    entity.with_child((Text::new(label.to_string()), TextFont::from_font_size(12.0)));
}

fn spawn_badge(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, label: &str) {
    parent.spawn((
        Text::new(label),
        TextFont::from_font_size(10.0),
        TextColor(Color::srgb(0.94, 0.72, 0.30)),
    ));
}
fn spawn_status(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, status: &SaveLoadStatus) {
    parent.spawn((
        Text::new(status.message.clone().unwrap_or_default()),
        TextFont::from_font_size(11.0),
        TextColor(match status.kind {
            SaveLoadStatusKind::Info => Color::srgb(0.82, 0.78, 0.60),
            SaveLoadStatusKind::Success => Color::srgb(0.48, 0.84, 0.48),
            SaveLoadStatusKind::Error => Color::srgb(0.96, 0.36, 0.28),
        }),
    ));
}
fn group(kind: &SaveKind) -> u8 {
    match kind {
        SaveKind::Named => 0,
        SaveKind::Quicksave => 1,
        SaveKind::Autosave { .. } => 2,
    }
}
fn confirmation_id(confirmation: &PendingSaveConfirmation) -> Option<&SaveId> {
    match confirmation {
        PendingSaveConfirmation::Overwrite(id) | PendingSaveConfirmation::Delete(id) => Some(id),
        PendingSaveConfirmation::None => None,
    }
}
pub fn format_timestamp(unix_ms: u64) -> String {
    DateTime::<Local>::from(std::time::UNIX_EPOCH + std::time::Duration::from_millis(unix_ms))
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string()
}

type NewWorldQuery<'w, 's> =
    Query<'w, 's, &'static Interaction, (Changed<Interaction>, With<Button>, With<NewWorldButton>)>;
pub(crate) fn handle_new_world_button(
    mut buttons: NewWorldQuery,
    mut confirmation: ResMut<NewWorldConfirmation>,
    mut window: ResMut<SaveLoadWindowState>,
    mut setup: ResMut<WorldSetupState>,
    mut next: ResMut<NextState<AppMode>>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    for interaction in &mut buttons {
        if *interaction == Interaction::Pressed {
            sounds.write(SoundEvent::UiClick);
            if confirmation.awaiting_confirmation {
                window.open = false;
                confirmation.awaiting_confirmation = false;
                setup.allow_cancel = true;
                next.set(AppMode::WorldSetup);
            } else {
                confirmation.awaiting_confirmation = true;
            }
        }
    }
}
