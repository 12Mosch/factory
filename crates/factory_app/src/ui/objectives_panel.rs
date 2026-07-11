use bevy::prelude::*;
use factory_sim::EarlyGameProgress;

use crate::resources::SimResource;
use crate::ui::map_view::{MINIMAP_FRAME_SIZE, MINIMAP_RIGHT_OFFSET, MINIMAP_TOP_OFFSET};

const OBJECTIVE_COUNT: usize = 6;
const MINIMAP_PANEL_GAP: f32 = 12.0;
const OBJECTIVES_PANEL_RIGHT: f32 = MINIMAP_RIGHT_OFFSET + MINIMAP_FRAME_SIZE + MINIMAP_PANEL_GAP;

#[derive(Clone, Copy)]
struct ObjectiveDefinition {
    title: &'static str,
    hint: &'static str,
    target: u64,
}

const OBJECTIVES: [ObjectiveDefinition; OBJECTIVE_COUNT] = [
    ObjectiveDefinition {
        title: "Mine iron ore",
        hint: "Hold right mouse over an iron ore patch.",
        target: 10,
    },
    ObjectiveDefinition {
        title: "Place the stone furnace",
        hint: "Select the furnace in the hotbar, then left-click to place it.",
        target: 1,
    },
    ObjectiveDefinition {
        title: "Smelt iron plates",
        hint: "Open the furnace and add iron ore plus coal.",
        target: 10,
    },
    ObjectiveDefinition {
        title: "Place the burner mining drill",
        hint: "Place the drill over ore, then fuel it with coal.",
        target: 1,
    },
    ObjectiveDefinition {
        title: "Build an iron ore stockpile",
        hint: "Keep the fueled drill's output clear until 25 ore are produced in total.",
        target: 25,
    },
    ObjectiveDefinition {
        title: "Craft transport belts",
        hint: "Press C and craft 10 transport belts for your first production line.",
        target: 10,
    },
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ObjectiveProgress {
    current: u64,
    target: u64,
}

impl ObjectiveProgress {
    fn is_complete(self) -> bool {
        self.current >= self.target
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ObjectivesSnapshot {
    progress: [ObjectiveProgress; OBJECTIVE_COUNT],
}

impl Default for ObjectivesSnapshot {
    fn default() -> Self {
        Self {
            progress: std::array::from_fn(|index| ObjectiveProgress {
                current: 0,
                target: OBJECTIVES[index].target,
            }),
        }
    }
}

impl ObjectivesSnapshot {
    fn active_index(&self) -> Option<usize> {
        self.progress
            .iter()
            .position(|progress| !progress.is_complete())
    }
}

#[derive(Resource, Default)]
pub(crate) struct ObjectivesPanelState {
    snapshot: ObjectivesSnapshot,
    progress_revision: u64,
}

#[derive(Component)]
pub struct ObjectivesPanelRoot;

#[derive(Component)]
pub(crate) struct ObjectiveRow {
    index: usize,
}

#[derive(Component)]
pub(crate) struct ObjectiveRowText {
    index: usize,
}

#[derive(Component)]
pub(crate) struct ObjectiveHintText;

pub(crate) fn setup_objectives_panel(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut state: ResMut<ObjectivesPanelState>,
) {
    let sim = sim.read();
    let progress = sim.early_game_progress();
    state.snapshot = objectives_snapshot(progress);
    state.progress_revision = progress.revision;
    let snapshot = state.snapshot.clone();

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(MINIMAP_TOP_OFFSET),
                right: Val::Px(OBJECTIVES_PANEL_RIGHT),
                width: Val::Px(330.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(7.0),
                padding: UiRect::all(Val::Px(12.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.025, 0.030, 0.028, 0.92)),
            BorderColor::all(Color::srgba(0.34, 0.43, 0.34, 0.92)),
            GlobalZIndex(1100),
            panel_visibility(&snapshot),
            ObjectivesPanelRoot,
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("OBJECTIVES"),
                TextFont::from_font_size(16.0),
                TextColor(Color::srgb(0.92, 0.82, 0.45)),
            ));

            for index in 0..OBJECTIVE_COUNT {
                spawn_objective_row(panel, index, &snapshot);
            }

            panel
                .spawn((
                    Node {
                        margin: UiRect::top(Val::Px(3.0)),
                        padding: UiRect::top(Val::Px(8.0)),
                        border: UiRect::top(Val::Px(1.0)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(0.28, 0.34, 0.29, 0.8)),
                ))
                .with_child((
                    Text::new(hint_text(&snapshot)),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::srgb(0.74, 0.78, 0.70)),
                    ObjectiveHintText,
                ));
        });
}

fn spawn_objective_row(
    panel: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    index: usize,
    snapshot: &ObjectivesSnapshot,
) {
    let progress = snapshot.progress[index];
    panel
        .spawn((
            Node {
                min_height: Val::Px(31.0),
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                border: UiRect::left(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(row_background(
                progress,
                snapshot.active_index() == Some(index),
            )),
            BorderColor::all(row_accent(progress, snapshot.active_index() == Some(index))),
            ObjectiveRow { index },
        ))
        .with_child((
            Text::new(row_text(index, progress)),
            TextFont::from_font_size(13.0),
            TextColor(row_text_color(
                progress,
                snapshot.active_index() == Some(index),
            )),
            ObjectiveRowText { index },
        ));
}

pub(crate) fn sync_objectives_panel(
    sim: Res<SimResource>,
    mut state: ResMut<ObjectivesPanelState>,
    mut rows: Query<(&ObjectiveRow, &mut BackgroundColor, &mut BorderColor)>,
    mut labels: Query<(&ObjectiveRowText, &mut Text, &mut TextColor)>,
    mut hints: Query<&mut Text, (With<ObjectiveHintText>, Without<ObjectiveRowText>)>,
    mut roots: Query<&mut Visibility, With<ObjectivesPanelRoot>>,
) {
    let progress = sim.read().early_game_progress();
    if progress.revision == state.progress_revision {
        return;
    }
    state.progress_revision = progress.revision;
    let next = objectives_snapshot(progress);
    if next == state.snapshot {
        return;
    }
    state.snapshot = next;
    let active_index = state.snapshot.active_index();

    for mut visibility in &mut roots {
        *visibility = panel_visibility(&state.snapshot);
    }

    for (row, mut background, mut border) in &mut rows {
        let progress = state.snapshot.progress[row.index];
        let active = active_index == Some(row.index);
        background.0 = row_background(progress, active);
        border.set_all(row_accent(progress, active));
    }

    for (label, mut text, mut color) in &mut labels {
        let progress = state.snapshot.progress[label.index];
        text.0 = row_text(label.index, progress);
        color.0 = row_text_color(progress, active_index == Some(label.index));
    }

    let hint = hint_text(&state.snapshot);
    for mut text in &mut hints {
        text.0 = hint.clone();
    }
}

fn objectives_snapshot(progress: EarlyGameProgress) -> ObjectivesSnapshot {
    ObjectivesSnapshot {
        progress: [
            ObjectiveProgress {
                current: progress.iron_ore_manually_mined,
                target: OBJECTIVES[0].target,
            },
            ObjectiveProgress {
                current: progress.stone_furnaces_placed,
                target: OBJECTIVES[1].target,
            },
            ObjectiveProgress {
                current: progress.iron_plates_smelted,
                target: OBJECTIVES[2].target,
            },
            ObjectiveProgress {
                current: progress.burner_mining_drills_placed,
                target: OBJECTIVES[3].target,
            },
            ObjectiveProgress {
                current: progress.iron_ore_drill_mined,
                target: OBJECTIVES[4].target,
            },
            ObjectiveProgress {
                current: progress.transport_belts_manually_crafted,
                target: OBJECTIVES[5].target,
            },
        ],
    }
}

fn row_text(index: usize, progress: ObjectiveProgress) -> String {
    if progress.is_complete() {
        format!("[x] {}", OBJECTIVES[index].title)
    } else {
        format!(
            "[ ] {}  {}/{}",
            OBJECTIVES[index].title,
            progress.current.min(progress.target),
            progress.target
        )
    }
}

fn hint_text(snapshot: &ObjectivesSnapshot) -> String {
    snapshot.active_index().map_or_else(
        || "Early objectives complete. Grow the factory!".to_string(),
        |index| format!("NEXT: {}", OBJECTIVES[index].hint),
    )
}

fn panel_visibility(snapshot: &ObjectivesSnapshot) -> Visibility {
    if snapshot.active_index().is_some() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    }
}

fn row_background(progress: ObjectiveProgress, active: bool) -> Color {
    if progress.is_complete() {
        Color::srgba(0.08, 0.18, 0.11, 0.78)
    } else if active {
        Color::srgba(0.20, 0.16, 0.065, 0.90)
    } else {
        Color::srgba(0.07, 0.075, 0.072, 0.72)
    }
}

fn row_accent(progress: ObjectiveProgress, active: bool) -> Color {
    if progress.is_complete() {
        Color::srgb(0.31, 0.72, 0.40)
    } else if active {
        Color::srgb(0.92, 0.63, 0.18)
    } else {
        Color::srgb(0.25, 0.28, 0.25)
    }
}

fn row_text_color(progress: ObjectiveProgress, active: bool) -> Color {
    if progress.is_complete() {
        Color::srgb(0.62, 0.82, 0.64)
    } else if active {
        Color::srgb(1.0, 0.91, 0.66)
    } else {
        Color::srgb(0.68, 0.70, 0.66)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objectives_advance_in_early_game_order() {
        let snapshot = objectives_snapshot(EarlyGameProgress {
            iron_ore_manually_mined: 12,
            stone_furnaces_placed: 1,
            iron_plates_smelted: 4,
            ..default()
        });

        assert!(snapshot.progress[0].is_complete());
        assert!(snapshot.progress[1].is_complete());
        assert!(!snapshot.progress[2].is_complete());
        assert_eq!(snapshot.active_index(), Some(2));
        assert_eq!(
            row_text(2, snapshot.progress[2]),
            "[ ] Smelt iron plates  4/10"
        );
    }

    #[test]
    fn production_does_not_infer_unrelated_objectives() {
        let snapshot = objectives_snapshot(EarlyGameProgress {
            iron_plates_smelted: 10,
            iron_ore_drill_mined: 25,
            ..default()
        });

        assert!(!snapshot.progress[0].is_complete());
        assert!(!snapshot.progress[1].is_complete());
        assert!(!snapshot.progress[3].is_complete());
        assert!(snapshot.progress[4].is_complete());
    }

    #[test]
    fn completed_panel_replaces_next_step_with_growth_message() {
        let snapshot = objectives_snapshot(EarlyGameProgress {
            iron_ore_manually_mined: 10,
            stone_furnaces_placed: 1,
            iron_plates_smelted: 10,
            burner_mining_drills_placed: 1,
            iron_ore_drill_mined: 25,
            transport_belts_manually_crafted: 10,
            ..default()
        });

        assert_eq!(snapshot.active_index(), None);
        assert_eq!(
            hint_text(&snapshot),
            "Early objectives complete. Grow the factory!"
        );
        assert_eq!(panel_visibility(&snapshot), Visibility::Hidden);
    }

    #[test]
    fn unfinished_panel_remains_visible() {
        assert_eq!(
            panel_visibility(&ObjectivesSnapshot::default()),
            Visibility::Visible
        );
    }
}
