use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::system::SystemParam;
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use factory_data::{ItemId, PrototypeCatalog};
use factory_sim::{Blueprint, Inventory, SimCommand};
use std::collections::BTreeMap;

use crate::audio::SoundEvent;
use crate::build::resources::{
    BlueprintLibraryWindowState, BuildPlacementState, PlannerState, PlannerTool,
};
use crate::input::planner::activate_planner_tool;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::formatting::format_item_display_name;
use crate::ui::resources::OpenContainer;
use crate::ui::text_input::{
    EditableTextSanitizer, can_submit, editor_value, is_non_control, set_editor_value,
    single_line_editor,
};
use crate::ui::window_sync::{WindowRootQuery, sync_window};

#[derive(Component)]
pub(crate) struct BlueprintLibraryButton {
    pub action: BlueprintLibraryAction,
}

#[derive(Component)]
pub(crate) struct BlueprintRenameInput;

/// Defers a Save-button rename until queued editor changes have been applied.
#[derive(Message)]
pub(crate) struct BlueprintRenameCommitRequested;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlueprintLibraryAction {
    Close,
    /// Close the window and start a drag-select blueprint capture.
    CaptureNew,
    /// Load a library blueprint into the clipboard and enter paste mode.
    Paste {
        index: usize,
    },
    Delete {
        index: usize,
    },
    /// Start editing a blueprint's name.
    Rename {
        index: usize,
    },
    /// Commit the in-progress rename.
    ConfirmRename,
    /// Discard the in-progress rename.
    CancelRename,
}

#[derive(Clone, Debug)]
pub(crate) struct BlueprintLibrarySnapshot {
    rows: Vec<BlueprintRowSnapshot>,
    clipboard: Option<(String, usize)>,
    /// (index, current buffer) of the blueprint being renamed, if any.
    editing: Option<(usize, String)>,
}

impl PartialEq for BlueprintLibrarySnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
            && self.clipboard == other.clipboard
            && self.editing.as_ref().map(|(index, _)| index)
                == other.editing.as_ref().map(|(index, _)| index)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct BlueprintRowSnapshot {
    name: String,
    entity_count: usize,
    materials: String,
}

type BlueprintLibraryButtonQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static BlueprintLibraryButton),
    (Changed<Interaction>, With<Button>),
>;

#[derive(SystemParam)]
pub(crate) struct BlueprintLibraryButtonState<'w> {
    sim: Res<'w, SimResource>,
    window: ResMut<'w, BlueprintLibraryWindowState>,
    planner: ResMut<'w, PlannerState>,
    build_state: ResMut<'w, BuildPlacementState>,
    open_container: ResMut<'w, OpenContainer>,
    commands: MessageWriter<'w, SimCommandRequest>,
    rename_requests: MessageWriter<'w, BlueprintRenameCommitRequested>,
    sounds: MessageWriter<'w, SoundEvent>,
}

pub(crate) fn handle_blueprint_library_buttons(
    mut buttons: BlueprintLibraryButtonQuery,
    mut state: BlueprintLibraryButtonState,
) {
    if !state.window.open {
        return;
    }

    for (interaction, button) in &mut buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }

        state.sounds.write(SoundEvent::UiClick);
        match button.action {
            BlueprintLibraryAction::Close => {
                state.window.close();
            }
            BlueprintLibraryAction::CaptureNew => {
                state.window.close();
                activate_planner_tool(
                    &mut state.planner,
                    &mut state.build_state,
                    &mut state.open_container,
                    PlannerTool::CaptureBlueprint,
                );
            }
            BlueprintLibraryAction::Paste { index } => {
                let sim = state.sim.read();
                let Some(blueprint) = sim.construction().blueprints().get(index) else {
                    continue;
                };
                state.planner.clipboard = Some(blueprint.clone());
                state.window.close();
                activate_planner_tool(
                    &mut state.planner,
                    &mut state.build_state,
                    &mut state.open_container,
                    PlannerTool::Paste,
                );
            }
            BlueprintLibraryAction::Delete { index } => {
                if state
                    .sim
                    .read()
                    .construction()
                    .blueprints()
                    .get(index)
                    .is_none()
                {
                    continue;
                }
                state.window.cancel_rename();
                state
                    .commands
                    .write(SimCommandRequest(SimCommand::DeleteBlueprint { index }));
            }
            BlueprintLibraryAction::Rename { index } => {
                let sim = state.sim.read();
                let Some(blueprint) = sim.construction().blueprints().get(index) else {
                    continue;
                };
                state.window.editing_index = Some(index);
                state.window.rename_buffer = blueprint.name.clone();
            }
            BlueprintLibraryAction::ConfirmRename => {
                state.rename_requests.write(BlueprintRenameCommitRequested);
            }
            BlueprintLibraryAction::CancelRename => {
                state.window.cancel_rename();
            }
        }
    }
}

/// Commits Save-button requests after editor sanitization and state sync.
pub(crate) fn submit_blueprint_rename_button(
    mut requests: MessageReader<BlueprintRenameCommitRequested>,
    mut input_focus: Option<ResMut<InputFocus>>,
    inputs: Query<(&EditableText, &EditableTextSanitizer), With<BlueprintRenameInput>>,
    mut window: ResMut<BlueprintLibraryWindowState>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    if requests.read().count() == 0 || window.editing_index.is_none() {
        return;
    }
    let Ok((input, sanitizer)) = inputs.single() else {
        return;
    };
    if !can_submit(input, sanitizer) {
        return;
    }
    commit_blueprint_rename(&mut window, editor_value(input), &mut commands);
    if let Some(focus) = input_focus.as_deref_mut() {
        focus.clear();
    }
}

pub(crate) fn sync_blueprint_library_window(
    mut commands: Commands,
    window: Res<BlueprintLibraryWindowState>,
    sim: Res<SimResource>,
    planner: Res<PlannerState>,
    mut roots: WindowRootQuery<BlueprintLibrarySnapshot>,
) {
    sync_window(
        &mut commands,
        &mut roots,
        window.open,
        true,
        || blueprint_library_snapshot(&sim, &planner, &window),
        blueprint_library_root,
        spawn_blueprint_library_modal,
    );
}

pub(crate) fn sync_blueprint_rename_from_state(
    window: Res<BlueprintLibraryWindowState>,
    mut inputs: Query<&mut EditableText, With<BlueprintRenameInput>>,
) {
    if !window.is_changed() {
        return;
    }
    for mut input in &mut inputs {
        set_editor_value(&mut input, &window.rename_buffer);
    }
}

pub(crate) fn sync_blueprint_rename_to_state(
    inputs: Query<&EditableText, (With<BlueprintRenameInput>, Changed<EditableText>)>,
    mut window: ResMut<BlueprintLibraryWindowState>,
) {
    for input in &inputs {
        let value = editor_value(input);
        if window.rename_buffer != value {
            window.rename_buffer = value;
        }
    }
}

pub(crate) fn submit_blueprint_rename(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_focus: Option<Res<InputFocus>>,
    inputs: Query<(&EditableText, &EditableTextSanitizer), With<BlueprintRenameInput>>,
    mut window: ResMut<BlueprintLibraryWindowState>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    let Some(index) = window.editing_index else {
        return;
    };
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
    debug_assert_eq!(window.editing_index, Some(index));
    commit_blueprint_rename(&mut window, editor_value(input), &mut commands);
}

fn commit_blueprint_rename(
    window: &mut BlueprintLibraryWindowState,
    value: String,
    commands: &mut MessageWriter<SimCommandRequest>,
) {
    let Some(index) = window.editing_index else {
        return;
    };
    let name = value.trim().to_string();
    if !name.is_empty() {
        commands.write(SimCommandRequest(SimCommand::RenameBlueprint {
            index,
            name,
        }));
    }
    window.cancel_rename();
}

fn blueprint_library_snapshot(
    sim: &SimResource,
    planner: &PlannerState,
    window: &BlueprintLibraryWindowState,
) -> BlueprintLibrarySnapshot {
    let sim = sim.read();
    let catalog = sim.catalog();
    let inventory = sim.player_inventory();
    BlueprintLibrarySnapshot {
        rows: sim
            .construction()
            .blueprints()
            .iter()
            .map(|blueprint| BlueprintRowSnapshot {
                name: blueprint.name.clone(),
                entity_count: blueprint.entities.len(),
                materials: blueprint_materials_line(catalog, inventory, blueprint),
            })
            .collect(),
        clipboard: planner
            .clipboard
            .as_ref()
            .map(|blueprint| (blueprint.name.clone(), blueprint.entities.len())),
        editing: window
            .editing_index
            .map(|index| (index, window.rename_buffer.clone())),
    }
}

/// Aggregates the flat `build_item` shopping list needed to build every
/// entity in `blueprint`, formatted as `"item have/need"` pairs against the
/// player's current inventory.
fn blueprint_materials_line(
    catalog: &PrototypeCatalog,
    inventory: &Inventory,
    blueprint: &Blueprint,
) -> String {
    let mut needed: BTreeMap<ItemId, u32> = BTreeMap::new();
    for entity in &blueprint.entities {
        if let Some(item_id) = catalog
            .entity(entity.prototype_id)
            .and_then(|prototype| prototype.build_item)
        {
            *needed.entry(item_id).or_insert(0) += 1;
        }
    }
    if needed.is_empty() {
        return "Materials: <none>".to_string();
    }
    format!(
        "Materials: {}",
        needed
            .iter()
            .map(|(item_id, count)| format!(
                "{} {}/{}",
                format_item_display_name(catalog, *item_id),
                inventory.count(*item_id),
                count
            ))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn blueprint_library_root() -> impl Bundle {
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
        GlobalZIndex(2600),
    )
}

fn spawn_blueprint_library_modal(
    root: &mut ChildSpawnerCommands,
    snapshot: &BlueprintLibrarySnapshot,
) {
    root.spawn((
        Node {
            width: Val::Vw(88.0),
            max_width: Val::Px(480.0),
            max_height: Val::Vh(70.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            padding: UiRect::all(Val::Px(16.0)),
            border: UiRect::all(Val::Px(1.0)),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.030, 0.032, 0.034, 0.98)),
        BorderColor::all(Color::srgba(0.36, 0.39, 0.34, 0.95)),
    ))
    .with_children(|modal| {
        modal
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(12.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|header| {
                header.spawn((
                    Text::new("Blueprint Library"),
                    TextFont::from_font_size(20.0),
                    TextColor(Color::srgb(0.94, 0.95, 0.90)),
                ));
                spawn_library_button(header, "Close", BlueprintLibraryAction::Close, 72.0);
            });
        let clipboard_line = match &snapshot.clipboard {
            Some((name, count)) => format!("Clipboard: {name} ({count} entities) - Ctrl+V pastes"),
            None => "Clipboard empty - Ctrl+C copies an area".to_string(),
        };
        modal.spawn((
            Text::new(clipboard_line),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.72, 0.75, 0.70)),
        ));
        spawn_library_button(
            modal,
            "Capture New Blueprint (drag select)",
            BlueprintLibraryAction::CaptureNew,
            280.0,
        );

        if snapshot.rows.is_empty() {
            modal.spawn((
                Text::new("No saved blueprints yet."),
                TextFont::from_font_size(13.0),
                TextColor(Color::srgb(0.62, 0.63, 0.60)),
            ));
        }
        for (index, row) in snapshot.rows.iter().enumerate() {
            let editing_buffer = snapshot
                .editing
                .as_ref()
                .filter(|(editing_index, _)| *editing_index == index)
                .map(|(_, buffer)| buffer.clone());
            modal
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        padding: UiRect::all(Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.08, 0.08, 0.08, 0.9)),
                    BorderColor::all(Color::srgba(0.30, 0.30, 0.28, 0.60)),
                ))
                .with_children(|row_node| {
                    row_node
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::SpaceBetween,
                                column_gap: Val::Px(8.0),
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                        ))
                        .with_children(|header_row| {
                            if let Some(buffer) = &editing_buffer {
                                header_row.spawn((
                                    Node {
                                        flex_grow: 1.0,
                                        height: Val::Px(26.0),
                                        align_items: AlignItems::Center,
                                        padding: UiRect::horizontal(Val::Px(8.0)),
                                        border: UiRect::all(Val::Px(1.0)),
                                        overflow: Overflow::clip_x(),
                                        ..default()
                                    },
                                    single_line_editor(buffer, None, is_non_control),
                                    TextLayout::no_wrap(),
                                    TextCursorStyle::default(),
                                    TextFont::from_font_size(13.0),
                                    TextColor(Color::srgb(0.91, 0.92, 0.86)),
                                    BackgroundColor(Color::srgba(0.07, 0.08, 0.08, 0.96)),
                                    BorderColor::all(Color::srgba(0.48, 0.55, 0.48, 0.8)),
                                    AutoFocus,
                                    BlueprintRenameInput,
                                ));
                            } else {
                                header_row.spawn((
                                    Text::new(format!(
                                        "{} ({} entities)",
                                        row.name, row.entity_count
                                    )),
                                    TextFont::from_font_size(13.0),
                                    TextColor(Color::srgb(0.91, 0.92, 0.86)),
                                ));
                            }
                            header_row
                                .spawn((
                                    Node {
                                        flex_direction: FlexDirection::Row,
                                        column_gap: Val::Px(6.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::NONE),
                                ))
                                .with_children(|actions| {
                                    if editing_buffer.is_some() {
                                        spawn_library_button(
                                            actions,
                                            "Save",
                                            BlueprintLibraryAction::ConfirmRename,
                                            56.0,
                                        );
                                        spawn_library_button(
                                            actions,
                                            "Cancel",
                                            BlueprintLibraryAction::CancelRename,
                                            56.0,
                                        );
                                    } else {
                                        spawn_library_button(
                                            actions,
                                            "Paste",
                                            BlueprintLibraryAction::Paste { index },
                                            56.0,
                                        );
                                        spawn_library_button(
                                            actions,
                                            "Rename",
                                            BlueprintLibraryAction::Rename { index },
                                            56.0,
                                        );
                                        spawn_library_button(
                                            actions,
                                            "Delete",
                                            BlueprintLibraryAction::Delete { index },
                                            56.0,
                                        );
                                    }
                                });
                        });
                    row_node.spawn((
                        Text::new(row.materials.clone()),
                        TextFont::from_font_size(11.0),
                        TextColor(Color::srgb(0.68, 0.70, 0.66)),
                    ));
                });
        }
    });
}

fn spawn_library_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: BlueprintLibraryAction,
    width: f32,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(width),
                height: Val::Px(30.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.95)),
            BorderColor::all(Color::srgba(0.44, 0.43, 0.39, 0.70)),
            BlueprintLibraryButton { action },
        ))
        .with_child((
            Text::new(label),
            TextFont::from_font_size(13.0),
            TextColor(Color::WHITE),
        ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;

    #[test]
    fn save_button_commit_uses_the_synchronized_editor_value() {
        let mut app = App::new();
        app.add_message::<BlueprintRenameCommitRequested>()
            .add_message::<SimCommandRequest>()
            .insert_resource(BlueprintLibraryWindowState {
                open: true,
                editing_index: Some(3),
                rename_buffer: "Previous Name".into(),
            })
            .add_systems(
                Update,
                (
                    sync_blueprint_rename_to_state,
                    submit_blueprint_rename_button,
                )
                    .chain(),
            );
        app.world_mut().spawn((
            EditableText::new("Current Name"),
            EditableTextSanitizer::new("Current Name", is_non_control, None),
            BlueprintRenameInput,
        ));
        app.world_mut()
            .write_message(BlueprintRenameCommitRequested);

        app.update();

        let requests = app
            .world_mut()
            .resource_mut::<Messages<SimCommandRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert!(matches!(
            requests.as_slice(),
            [SimCommandRequest(SimCommand::RenameBlueprint { index: 3, name })]
                if name == "Current Name"
        ));
    }
}
