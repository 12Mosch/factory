use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::save_load::{
    DeferredNamedSave, LoadState, PendingCatalogScan, PendingLoadJobs, PendingSaveConfirmation,
    PendingSaveJobs, SaveCatalog, SaveEntry, SaveId, SaveKind, SaveLoadConfig, SaveLoadStatus,
    SaveLoadStatusKind, SaveLoadTab, SaveLoadWindowState, delete_save_with_loads,
    format_world_seed, load_save, local_datetime_from_unix_ms, request_named_save_guarded,
    request_overwrite,
};
use crate::ui::layout::scroll_column;
use crate::ui::pause_menu::PauseMenuState;
use crate::ui::text_input::{
    EditableTextSanitizer, can_submit, editor_value, is_non_control, set_editor_value,
    single_line_editor,
};
use crate::ui::window_sync::{WindowRoot, WindowRootQuery, WindowSync, sync_contents, sync_window};

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
/// Defers a create-button press until the current editor value has been
/// synchronized in `PostUpdate`.
#[derive(Message)]
pub(crate) struct SaveCreateRequested;
#[derive(Component)]
pub struct SaveConfirmationButton(pub bool);
#[derive(Component)]
pub struct SaveLoadModal;
#[derive(Component)]
pub struct SaveLoadSlotList;
#[derive(Component)]
pub struct SaveLoadBackButton;
#[derive(Component)]
pub(crate) struct SaveNameInput;
/// Marker for the in-game current world-seed readout.
#[derive(Component)]
pub struct CurrentWorldSeedText;
/// Copy button that places the current world seed on the system clipboard.
#[derive(Component)]
pub struct CopyWorldSeedButton;

/// Returns the active world seed when a world has been started or loaded.
pub fn current_world_seed(sim: &SimResource) -> Option<u64> {
    sim.is_initialized().then(|| sim.read().seed())
}

/// Formats the current-seed row so the displayed decimal round-trips the `u64`.
pub fn format_current_seed_text(seed: Option<u64>) -> String {
    match seed {
        Some(seed) => format!("World seed: {}", format_world_seed(seed)),
        None => "World seed: unavailable".to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaveEntryAction {
    Overwrite,
    Load,
    Delete,
}

#[derive(Clone, Debug)]
pub(crate) struct SaveLoadSnapshot {
    window: SaveLoadWindowState,
    status: SaveLoadStatus,
    entries: Vec<SaveEntry>,
    pending: Vec<SaveId>,
    loading: Vec<SaveId>,
    confirmation: PendingSaveConfirmation,
    current_seed: Option<u64>,
}

impl PartialEq for SaveLoadSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.window.open == other.window.open
            && self.window.tab == other.window.tab
            && self.status == other.status
            && self.entries == other.entries
            && self.pending == other.pending
            && self.loading == other.loading
            && self.confirmation == other.confirmation
            && self.current_seed == other.current_seed
    }
}

impl Eq for SaveLoadSnapshot {}

#[derive(Clone, Debug)]
pub(crate) struct SaveLoadShellSnapshot {
    tab: SaveLoadTab,
    name_buffer: String,
}

impl PartialEq for SaveLoadShellSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.tab == other.tab
    }
}

impl Eq for SaveLoadShellSnapshot {}

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
type CopySeedButtons<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<CopyWorldSeedButton>,
    ),
>;

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_save_load_buttons(
    mut tabs: TabButtons,
    mut entries: EntryButtons,
    mut create: Query<&Interaction, (Changed<Interaction>, With<SaveCreateButton>)>,
    mut backs: Query<&Interaction, (Changed<Interaction>, With<SaveLoadBackButton>)>,
    mut confirms: Query<(&Interaction, &SaveConfirmationButton), Changed<Interaction>>,
    config: Res<SaveLoadConfig>,
    mut catalog: ResMut<SaveCatalog>,
    mut pending: ResMut<PendingSaveJobs>,
    mut pending_loads: ResMut<PendingLoadJobs>,
    mut confirmation: ResMut<PendingSaveConfirmation>,
    mut status: ResMut<SaveLoadStatus>,
    mut load_state: LoadState,
    mut pause: ResMut<PauseMenuState>,
    mut sounds: MessageWriter<SoundEvent>,
    mut create_requests: MessageWriter<SaveCreateRequested>,
) {
    if !load_state.window.open {
        return;
    }
    for interaction in &mut backs {
        if *interaction == Interaction::Pressed {
            sounds.write(SoundEvent::UiClick);
            load_state.window.open = false;
            pause.open = true;
            *confirmation = PendingSaveConfirmation::None;
        }
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
            create_requests.write(SaveCreateRequested);
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
                if load_save(
                    &button.id,
                    &catalog,
                    &mut pending_loads,
                    &mut status,
                    &load_state,
                ) && load_state.app_pause.is_paused()
                {
                    pause.open = true;
                }
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
                delete_save_with_loads(
                    &id,
                    &config,
                    &mut catalog,
                    &pending,
                    Some(&pending_loads),
                    &mut status,
                );
            }
            PendingSaveConfirmation::None => {}
        }
    }
}

/// Processes create-button requests after editor-to-state synchronization so
/// a click uses text entered during the same frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn submit_save_create_requests(
    mut requests: MessageReader<SaveCreateRequested>,
    config: Res<SaveLoadConfig>,
    catalog: Res<SaveCatalog>,
    pending_scan: Res<PendingCatalogScan>,
    mut deferred: ResMut<DeferredNamedSave>,
    mut pending: ResMut<PendingSaveJobs>,
    mut confirmation: ResMut<PendingSaveConfirmation>,
    mut status: ResMut<SaveLoadStatus>,
    state: Res<SaveLoadWindowState>,
    sim: Res<crate::resources::SimResource>,
    mut metrics: ResMut<crate::save_load::SaveLoadMetrics>,
) {
    if requests.read().count() == 0 {
        return;
    }
    request_named_save_guarded(
        &state.name_buffer,
        &sim,
        &config,
        &catalog,
        &pending_scan,
        &mut deferred,
        &mut pending,
        &mut confirmation,
        &mut status,
        &mut metrics,
    );
}

/// Submits a named-save request parked while a catalog refresh was
/// outstanding. PostUpdate runs after the scan poll, so the re-admission
/// uniqueness check observes the landed catalog; a still-outstanding (or
/// renewed) scan keeps the request parked for a later frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn submit_deferred_named_save(
    config: Res<SaveLoadConfig>,
    catalog: Res<SaveCatalog>,
    pending_scan: Res<PendingCatalogScan>,
    mut deferred: ResMut<DeferredNamedSave>,
    mut pending: ResMut<PendingSaveJobs>,
    mut confirmation: ResMut<PendingSaveConfirmation>,
    mut status: ResMut<SaveLoadStatus>,
    sim: Res<crate::resources::SimResource>,
    mut metrics: ResMut<crate::save_load::SaveLoadMetrics>,
) {
    let Some(name) = deferred.name.take() else {
        return;
    };
    if !pending_scan.is_empty() {
        deferred.name = Some(name);
        return;
    }
    request_named_save_guarded(
        &name,
        &sim,
        &config,
        &catalog,
        &pending_scan,
        &mut deferred,
        &mut pending,
        &mut confirmation,
        &mut status,
        &mut metrics,
    );
}

pub(crate) fn sync_save_name_from_state(
    state: Res<SaveLoadWindowState>,
    mut inputs: Query<&mut EditableText, With<SaveNameInput>>,
) {
    if !state.is_changed() {
        return;
    }
    for mut input in &mut inputs {
        set_editor_value(&mut input, &state.name_buffer);
    }
}

pub(crate) fn sync_save_name_to_state(
    inputs: Query<&EditableText, (With<SaveNameInput>, Changed<EditableText>)>,
    mut state: ResMut<SaveLoadWindowState>,
) {
    for input in &inputs {
        let value = editor_value(input);
        if state.name_buffer != value {
            state.name_buffer = value;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn submit_save_name_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_focus: Option<Res<InputFocus>>,
    inputs: Query<(&EditableText, &EditableTextSanitizer), With<SaveNameInput>>,
    config: Res<SaveLoadConfig>,
    catalog: Res<SaveCatalog>,
    pending_scan: Res<PendingCatalogScan>,
    mut deferred: ResMut<DeferredNamedSave>,
    mut pending: ResMut<PendingSaveJobs>,
    mut confirmation: ResMut<PendingSaveConfirmation>,
    mut status: ResMut<SaveLoadStatus>,
    state: Res<SaveLoadWindowState>,
    sim: Res<crate::resources::SimResource>,
    mut metrics: ResMut<crate::save_load::SaveLoadMetrics>,
) {
    if !state.open
        || state.tab != SaveLoadTab::Save
        || *confirmation != PendingSaveConfirmation::None
    {
        return;
    }
    let Some(keyboard) = keyboard else {
        return;
    };
    if !keyboard.just_pressed(KeyCode::Enter) && !keyboard.just_pressed(KeyCode::NumpadEnter) {
        return;
    }
    let Some(focused) = input_focus.as_deref().and_then(InputFocus::get) else {
        return;
    };
    let Ok((input, sanitizer)) = inputs.get(focused) else {
        return;
    };
    if !can_submit(input, sanitizer) {
        return;
    }
    request_named_save_guarded(
        &editor_value(input),
        &sim,
        &config,
        &catalog,
        &pending_scan,
        &mut deferred,
        &mut pending,
        &mut confirmation,
        &mut status,
        &mut metrics,
    );
}

/// Copies the formatted seed through the provided writer so the clipboard
/// boundary stays testable without an OS clipboard. Returns the success
/// message, or a failure message that still includes the seed.
pub(crate) fn copy_world_seed_text(
    seed: u64,
    write: impl FnOnce(String) -> Result<(), String>,
) -> Result<String, String> {
    let text = format_world_seed(seed);
    match write(text.clone()) {
        Ok(()) => Ok(format!("World seed {text} copied to clipboard.")),
        Err(error) => Err(format!("Could not copy world seed {text}: {error}")),
    }
}

pub(crate) fn handle_copy_world_seed_button(
    mut buttons: CopySeedButtons,
    sim: Res<SimResource>,
    mut clipboard: Option<ResMut<bevy::clipboard::Clipboard>>,
    mut status: ResMut<SaveLoadStatus>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    for interaction in &mut buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        sounds.write(SoundEvent::UiClick);
        let Some(seed) = current_world_seed(&sim) else {
            status.message = Some("No world seed is available.".into());
            status.kind = SaveLoadStatusKind::Error;
            continue;
        };
        let Some(clipboard) = clipboard.as_deref_mut() else {
            status.message = Some("System clipboard is unavailable.".into());
            status.kind = SaveLoadStatusKind::Error;
            continue;
        };
        match copy_world_seed_text(seed, |text| {
            clipboard.set_text(text).map_err(|error| error.to_string())
        }) {
            Ok(message) => {
                status.message = Some(message);
                status.kind = SaveLoadStatusKind::Success;
            }
            Err(message) => {
                status.message = Some(message);
                status.kind = SaveLoadStatusKind::Error;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_save_load_window(
    mut commands: Commands,
    state: Res<SaveLoadWindowState>,
    catalog: Res<SaveCatalog>,
    pending: Res<PendingSaveJobs>,
    pending_loads: Res<PendingLoadJobs>,
    status: Res<SaveLoadStatus>,
    confirmation: Res<PendingSaveConfirmation>,
    sim: Res<SimResource>,
    mut last_replacement: Local<Option<u64>>,
    mut shell_roots: WindowRootQuery<SaveLoadShellSnapshot>,
    mut contents_roots: WindowRootQuery<SaveLoadSnapshot>,
) {
    // The seed only changes when the active world is replaced. Key the
    // refresh off the replacement revision (a cheap field read) rather than
    // `is_changed`, which `tick_sim` sets after every fixed tick, and only
    // lock the simulation for the seed when a (re)build actually runs.
    let replacement = sim.replacement_revision();
    let replacement_changed = (*last_replacement).replace(replacement) != Some(replacement);
    let contents_changed = catalog.is_changed()
        || pending.is_changed()
        || pending_loads.is_changed()
        || status.is_changed()
        || confirmation.is_changed()
        || replacement_changed;
    let mut snapshot = None;
    let shell_sync = sync_window(
        &mut commands,
        &mut shell_roots,
        state.open,
        state.is_changed(),
        || SaveLoadShellSnapshot {
            tab: state.tab,
            name_buffer: state.name_buffer.clone(),
        },
        save_load_root,
        |root, shell| {
            let snapshot = snapshot.get_or_insert_with(|| {
                save_load_snapshot(
                    &state,
                    &catalog,
                    &pending,
                    &pending_loads,
                    &status,
                    &confirmation,
                    current_world_seed(&sim),
                )
            });
            spawn_save_load_modal(root, shell, snapshot);
        },
    );
    if state.open && contents_changed && shell_sync == WindowSync::Unchanged {
        let snapshot = snapshot.unwrap_or_else(|| {
            save_load_snapshot(
                &state,
                &catalog,
                &pending,
                &pending_loads,
                &status,
                &confirmation,
                current_world_seed(&sim),
            )
        });
        sync_contents(
            &mut commands,
            &mut contents_roots,
            snapshot,
            spawn_save_load_dynamic_contents,
        );
    }
}

fn save_load_snapshot(
    state: &SaveLoadWindowState,
    catalog: &SaveCatalog,
    pending: &PendingSaveJobs,
    pending_loads: &PendingLoadJobs,
    status: &SaveLoadStatus,
    confirmation: &PendingSaveConfirmation,
    current_seed: Option<u64>,
) -> SaveLoadSnapshot {
    SaveLoadSnapshot {
        window: state.clone(),
        status: status.clone(),
        entries: catalog.entries().to_vec(),
        pending: pending.pending_ids(),
        loading: pending_loads.pending_ids(),
        confirmation: confirmation.clone(),
        current_seed,
    }
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
    shell: &SaveLoadShellSnapshot,
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
        spawn_tabs(modal, shell.tab);
        if shell.tab == SaveLoadTab::Save {
            spawn_name_input(modal, &shell.name_buffer);
        }
        modal
            .spawn((
                Node {
                    flex_grow: 1.0,
                    min_height: Val::ZERO,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(10.0),
                    ..default()
                },
                WindowRoot::new(snapshot.clone()),
            ))
            .with_children(|contents| spawn_save_load_dynamic_contents(contents, snapshot));
    });
}

fn spawn_save_load_dynamic_contents(
    modal: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &SaveLoadSnapshot,
) {
    spawn_current_seed_row(modal, snapshot.current_seed);
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
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::FlexEnd,
            ..default()
        })
        .with_children(|row| spawn_plain_button(row, "Back", Some(SaveLoadBackButton)));
}

fn spawn_current_seed_row(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    seed: Option<u64>,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            row_gap: Val::Px(4.0),
            padding: UiRect::all(Val::Px(8.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
                Text::new(format_current_seed_text(seed)),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.88, 0.90, 0.78)),
                CurrentWorldSeedText,
            ));
            if seed.is_some() {
                row.spawn((
                    Button,
                    Node {
                        height: Val::Px(28.0),
                        min_width: Val::Px(82.0),
                        padding: UiRect::horizontal(Val::Px(10.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.11, 0.15, 0.10)),
                    BorderColor::all(Color::srgb(0.40, 0.49, 0.35)),
                    CopyWorldSeedButton,
                ))
                .with_child((Text::new("Copy"), TextFont::from_font_size(11.0)));
            }
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
                    overflow: Overflow::clip_x(),
                    ..default()
                },
                single_line_editor(value, Some(65), is_non_control),
                TextLayout::no_wrap(),
                TextCursorStyle::default(),
                TextFont::from_font_size(13.0),
                TextColor(Color::WHITE),
                BackgroundColor(Color::srgb(0.045, 0.055, 0.050)),
                BorderColor::all(Color::srgb(0.34, 0.42, 0.31)),
                AutoFocus,
                SaveNameInput,
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
    let loading = snapshot.loading.contains(&entry.id);
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
                "{}  ·  {}  ·  {}",
                format_timestamp(entry.metadata.completed_at_unix_ms),
                entry.compatibility.short_label(),
                entry.metadata.world_seed_label()
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
            if loading {
                // A second Load press is still accepted (newest wins), so
                // the row keeps its buttons alongside the badge.
                spawn_badge(row, "LOADING");
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
    local_datetime_from_unix_ms(unix_ms).map_or_else(
        || "Invalid timestamp".to_owned(),
        |timestamp| timestamp.format("%Y-%m-%d %H:%M:%S %:z").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_seed_follows_initialized_simulation() {
        let empty = SimResource::empty();
        assert_eq!(current_world_seed(&empty), None);

        let seeded = SimResource::new(factory_sim::Simulation::new_test_world(24680));
        assert_eq!(current_world_seed(&seeded), Some(24680));
    }

    #[test]
    fn seed_copy_hands_the_exact_decimal_to_the_writer() {
        for seed in [0, 123, u64::MAX] {
            let mut written = None;
            let result = copy_world_seed_text(seed, |text| {
                written = Some(text);
                Ok(())
            });
            assert_eq!(written, Some(seed.to_string()), "seed {seed}");
            assert!(
                result
                    .expect("copy should succeed")
                    .contains(&seed.to_string())
            );
        }
    }

    #[test]
    fn seed_copy_failure_still_reports_the_seed() {
        let result = copy_world_seed_text(424242, |_| Err("locked".to_string()));
        let message = result.expect_err("copy should fail");
        assert!(message.contains("424242"));
        assert!(message.contains("locked"));
    }
}
