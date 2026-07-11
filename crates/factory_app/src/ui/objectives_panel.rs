use bevy::prelude::*;
use factory_sim::OnboardingProgress;

use crate::resources::SimResource;
use crate::ui::map_view::{MINIMAP_FRAME_SIZE, MINIMAP_RIGHT_OFFSET, MINIMAP_TOP_OFFSET};

const OBJECTIVE_COUNT: usize = 17;
const VISIBLE_ROW_COUNT: usize = 5;
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
    ObjectiveDefinition {
        title: "Generate electricity",
        hint: "Connect an offshore pump, boiler, steam engine, and small electric pole; fuel the boiler.",
        target: 1,
    },
    ObjectiveDefinition {
        title: "Place a lab",
        hint: "Place a lab within the coverage of a small electric pole.",
        target: 1,
    },
    ObjectiveDefinition {
        title: "Produce automation science",
        hint: "Craft 10 red science packs and insert them into the powered lab.",
        target: 10,
    },
    ObjectiveDefinition {
        title: "Research Logistics",
        hint: "Press T to open technologies and research Logistics.",
        target: 1,
    },
    ObjectiveDefinition {
        title: "Research Automation",
        hint: "Research Automation to unlock assembling machines.",
        target: 1,
    },
    ObjectiveDefinition {
        title: "Automate an item",
        hint: "Power and supply an assembling machine, select a recipe, and let it finish one item.",
        target: 1,
    },
    ObjectiveDefinition {
        title: "Produce logistic science",
        hint: "Research electric power and logistic science packs, then automate 10 green science packs.",
        target: 10,
    },
    ObjectiveDefinition {
        title: "Research Oil Processing",
        hint: "Press T and queue the Logistics 2, Fluid Handling, and Oil Processing prerequisite chain.",
        target: 1,
    },
    ObjectiveDefinition {
        title: "Refine petroleum gas",
        hint: "Power a pumpjack over crude oil, pipe it to a refinery, and select Basic Oil Processing.",
        target: 45,
    },
    ObjectiveDefinition {
        title: "Research Turrets",
        hint: "Research Stone Walls followed by Turrets.",
        target: 1,
    },
    ObjectiveDefinition {
        title: "Deploy a loaded gun turret",
        hint: "Place a gun turret and load it with usable ammunition. Onboarding complete: expand and defend your factory!",
        target: 1,
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
            progress: std::array::from_fn(|i| ObjectiveProgress {
                current: 0,
                target: OBJECTIVES[i].target,
            }),
        }
    }
}
impl ObjectivesSnapshot {
    fn active_index(&self) -> Option<usize> {
        self.progress.iter().position(|p| !p.is_complete())
    }
    fn visible_indices(&self) -> [usize; VISIBLE_ROW_COUNT] {
        let active = self.active_index().unwrap_or(OBJECTIVE_COUNT - 1);
        let start = active
            .saturating_sub(2)
            .min(OBJECTIVE_COUNT - VISIBLE_ROW_COUNT);
        std::array::from_fn(|offset| start + offset)
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
    slot: usize,
}
#[derive(Component)]
pub(crate) struct ObjectiveRowText {
    slot: usize,
}
#[derive(Component)]
pub(crate) struct ObjectiveHintText;

pub(crate) fn setup_objectives_panel(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut state: ResMut<ObjectivesPanelState>,
) {
    let progress = sim.read().onboarding_progress();
    state.snapshot = objectives_snapshot(progress);
    state.progress_revision = progress.revision;
    let snapshot = state.snapshot.clone();
    let visible = snapshot.visible_indices();
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
            for (slot, index) in visible.into_iter().enumerate() {
                spawn_objective_row(panel, slot, index, &snapshot);
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
    slot: usize,
    index: usize,
    snapshot: &ObjectivesSnapshot,
) {
    let progress = snapshot.progress[index];
    let active = snapshot.active_index() == Some(index);
    panel
        .spawn((
            Node {
                min_height: Val::Px(31.0),
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                border: UiRect::left(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(row_background(progress, active)),
            BorderColor::all(row_accent(progress, active)),
            ObjectiveRow { slot },
        ))
        .with_child((
            Text::new(row_text(index, progress)),
            TextFont::from_font_size(13.0),
            TextColor(row_text_color(progress, active)),
            ObjectiveRowText { slot },
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
    let progress = sim.read().onboarding_progress();
    if progress.revision == state.progress_revision {
        return;
    }
    state.progress_revision = progress.revision;
    let next = objectives_snapshot(progress);
    if next == state.snapshot {
        return;
    }
    state.snapshot = next;
    let active = state.snapshot.active_index();
    let visible = state.snapshot.visible_indices();
    for mut visibility in &mut roots {
        *visibility = panel_visibility(&state.snapshot);
    }
    for (row, mut background, mut border) in &mut rows {
        let index = visible[row.slot];
        let p = state.snapshot.progress[index];
        background.0 = row_background(p, active == Some(index));
        border.set_all(row_accent(p, active == Some(index)));
    }
    for (label, mut text, mut color) in &mut labels {
        let index = visible[label.slot];
        let p = state.snapshot.progress[index];
        text.0 = row_text(index, p);
        color.0 = row_text_color(p, active == Some(index));
    }
    let hint = hint_text(&state.snapshot);
    for mut text in &mut hints {
        text.0 = hint.clone();
    }
}

fn objectives_snapshot(p: OnboardingProgress) -> ObjectivesSnapshot {
    let values = [
        p.iron_ore_manually_mined,
        p.stone_furnaces_placed,
        p.iron_plates_smelted,
        p.burner_mining_drills_placed,
        p.iron_ore_drill_mined,
        p.transport_belts_manually_crafted,
        u64::from(p.electricity_generated),
        p.labs_placed,
        p.automation_science_packs_produced,
        u64::from(p.logistics_researched),
        u64::from(p.automation_researched),
        p.assembler_items_produced,
        p.logistic_science_packs_produced,
        u64::from(p.oil_processing_researched),
        p.petroleum_gas_produced,
        u64::from(p.turrets_researched),
        p.loaded_gun_turrets,
    ];
    ObjectivesSnapshot {
        progress: std::array::from_fn(|i| ObjectiveProgress {
            current: values[i],
            target: OBJECTIVES[i].target,
        }),
    }
}
fn row_text(index: usize, p: ObjectiveProgress) -> String {
    if p.is_complete() {
        format!("[x] {}. {}", index + 1, OBJECTIVES[index].title)
    } else {
        format!(
            "[ ] {}. {}  {}/{}",
            index + 1,
            OBJECTIVES[index].title,
            p.current.min(p.target),
            p.target
        )
    }
}
fn hint_text(s: &ObjectivesSnapshot) -> String {
    s.active_index().map_or_else(
        || "Onboarding complete. Expand and defend your factory!".to_string(),
        |i| format!("NEXT: {}", OBJECTIVES[i].hint),
    )
}
fn panel_visibility(s: &ObjectivesSnapshot) -> Visibility {
    if s.active_index().is_some() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    }
}
fn row_background(p: ObjectiveProgress, active: bool) -> Color {
    if p.is_complete() {
        Color::srgba(0.08, 0.18, 0.11, 0.78)
    } else if active {
        Color::srgba(0.20, 0.16, 0.065, 0.90)
    } else {
        Color::srgba(0.07, 0.075, 0.072, 0.72)
    }
}
fn row_accent(p: ObjectiveProgress, active: bool) -> Color {
    if p.is_complete() {
        Color::srgb(0.31, 0.72, 0.40)
    } else if active {
        Color::srgb(0.92, 0.63, 0.18)
    } else {
        Color::srgb(0.25, 0.28, 0.25)
    }
}
fn row_text_color(p: ObjectiveProgress, active: bool) -> Color {
    if p.is_complete() {
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
    fn windows_track_active_objective() {
        let first = ObjectivesSnapshot::default();
        assert_eq!(first.visible_indices(), [0, 1, 2, 3, 4]);
        let middle = objectives_snapshot(OnboardingProgress {
            iron_ore_manually_mined: 10,
            stone_furnaces_placed: 1,
            iron_plates_smelted: 10,
            burner_mining_drills_placed: 1,
            iron_ore_drill_mined: 25,
            transport_belts_manually_crafted: 10,
            electricity_generated: true,
            labs_placed: 1,
            ..default()
        });
        assert_eq!(middle.active_index(), Some(8));
        assert_eq!(middle.visible_indices(), [6, 7, 8, 9, 10]);
    }
    #[test]
    fn end_window_is_thirteen_through_seventeen() {
        let mut s = ObjectivesSnapshot::default();
        for p in &mut s.progress[..16] {
            p.current = p.target;
        }
        assert_eq!(s.visible_indices(), [12, 13, 14, 15, 16]);
    }
    #[test]
    fn later_progress_does_not_skip_sequence() {
        let s = objectives_snapshot(OnboardingProgress {
            turrets_researched: true,
            loaded_gun_turrets: 1,
            ..default()
        });
        assert_eq!(s.active_index(), Some(0));
    }
    #[test]
    fn labels_use_absolute_numbers_and_cap_progress() {
        assert_eq!(
            row_text(
                8,
                ObjectiveProgress {
                    current: 99,
                    target: 10
                }
            ),
            "[x] 9. Produce automation science"
        );
        assert!(
            row_text(
                8,
                ObjectiveProgress {
                    current: 7,
                    target: 10
                }
            )
            .contains("7/10")
        );
    }
}
