use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_sim::SimCommand;

use crate::audio::SoundEvent;
use crate::build::resources::{
    BlueprintLibraryWindowState, BuildPlacementState, PlannerState, PlannerTool,
};
use crate::input::planner::activate_planner_tool;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::OpenContainer;
use crate::ui::window_sync::{WindowRootQuery, sync_window};

#[derive(Component)]
pub(crate) struct BlueprintLibraryButton {
    pub action: BlueprintLibraryAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlueprintLibraryAction {
    /// Close the window and start a drag-select blueprint capture.
    CaptureNew,
    /// Load a library blueprint into the clipboard and enter paste mode.
    Paste {
        index: usize,
    },
    Delete {
        index: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BlueprintLibrarySnapshot {
    rows: Vec<BlueprintRowSnapshot>,
    clipboard: Option<(String, usize)>,
}

#[derive(Clone, Debug, PartialEq)]
struct BlueprintRowSnapshot {
    name: String,
    entity_count: usize,
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
            BlueprintLibraryAction::CaptureNew => {
                state.window.open = false;
                activate_planner_tool(
                    &mut state.planner,
                    &mut state.build_state,
                    &mut state.open_container,
                    PlannerTool::CaptureBlueprint,
                );
            }
            BlueprintLibraryAction::Paste { index } => {
                let Some(blueprint) = state.sim.sim.construction().blueprints().get(index) else {
                    continue;
                };
                state.planner.clipboard = Some(blueprint.clone());
                state.window.open = false;
                activate_planner_tool(
                    &mut state.planner,
                    &mut state.build_state,
                    &mut state.open_container,
                    PlannerTool::Paste,
                );
            }
            BlueprintLibraryAction::Delete { index } => {
                state
                    .commands
                    .write(SimCommandRequest(SimCommand::DeleteBlueprint { index }));
            }
        }
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
        || blueprint_library_snapshot(&sim, &planner),
        blueprint_library_root,
        spawn_blueprint_library_modal,
    );
}

fn blueprint_library_snapshot(
    sim: &SimResource,
    planner: &PlannerState,
) -> BlueprintLibrarySnapshot {
    BlueprintLibrarySnapshot {
        rows: sim
            .sim
            .construction()
            .blueprints()
            .iter()
            .map(|blueprint| BlueprintRowSnapshot {
                name: blueprint.name.clone(),
                entity_count: blueprint.entities.len(),
            })
            .collect(),
        clipboard: planner
            .clipboard
            .as_ref()
            .map(|blueprint| (blueprint.name.clone(), blueprint.entities.len())),
    }
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
        modal.spawn((
            Text::new("Blueprint Library"),
            TextFont::from_font_size(20.0),
            TextColor(Color::srgb(0.94, 0.95, 0.90)),
        ));
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
            modal
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::SpaceBetween,
                        column_gap: Val::Px(8.0),
                        padding: UiRect::all(Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.08, 0.08, 0.08, 0.9)),
                    BorderColor::all(Color::srgba(0.30, 0.30, 0.28, 0.60)),
                ))
                .with_children(|row_node| {
                    row_node.spawn((
                        Text::new(format!("{} ({} entities)", row.name, row.entity_count)),
                        TextFont::from_font_size(13.0),
                        TextColor(Color::srgb(0.91, 0.92, 0.86)),
                    ));
                    row_node
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(6.0),
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                        ))
                        .with_children(|actions| {
                            spawn_library_button(
                                actions,
                                "Paste",
                                BlueprintLibraryAction::Paste { index },
                                64.0,
                            );
                            spawn_library_button(
                                actions,
                                "Delete",
                                BlueprintLibraryAction::Delete { index },
                                64.0,
                            );
                        });
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
