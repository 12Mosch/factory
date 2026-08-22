use bevy::prelude::*;
use bevy::scene::ScenePatch;
use factory_sim::{OnboardingProgress, RocketProgramProgress, Simulation};

use crate::resources::SimResource;
use crate::ui::map_view::{MINIMAP_FRAME_SIZE, MINIMAP_RIGHT_OFFSET, MINIMAP_TOP_OFFSET};

const VISIBLE_ROW_COUNT: usize = 5;
const MINIMAP_PANEL_GAP: f32 = 12.0;
const OBJECTIVES_PANEL_RIGHT: f32 = MINIMAP_RIGHT_OFFSET + MINIMAP_FRAME_SIZE + MINIMAP_PANEL_GAP;

#[derive(Clone, Copy)]
struct ObjectiveDefinition {
    title: &'static str,
    hint: &'static str,
    progress: fn(ObjectiveFacts) -> ObjectiveProgress,
}

const OBJECTIVES: &[ObjectiveDefinition] = &[
    ObjectiveDefinition {
        title: "Mine iron ore",
        hint: "Hold right mouse over an iron ore patch.",
        progress: |facts| ObjectiveProgress::new(facts.onboarding.iron_ore_manually_mined, 10),
    },
    ObjectiveDefinition {
        title: "Place the stone furnace",
        hint: "Select the furnace in the hotbar, then left-click to place it.",
        progress: |facts| ObjectiveProgress::new(facts.onboarding.stone_furnaces_placed, 1),
    },
    ObjectiveDefinition {
        title: "Smelt iron plates",
        hint: "Open the furnace and add iron ore plus coal.",
        progress: |facts| ObjectiveProgress::new(facts.onboarding.iron_plates_smelted, 10),
    },
    ObjectiveDefinition {
        title: "Place the burner mining drill",
        hint: "Place the drill over ore, then fuel it with coal.",
        progress: |facts| ObjectiveProgress::new(facts.onboarding.burner_mining_drills_placed, 1),
    },
    ObjectiveDefinition {
        title: "Build an iron ore stockpile",
        hint: "Keep the fueled drill's output clear until 25 ore are produced in total.",
        progress: |facts| ObjectiveProgress::new(facts.onboarding.iron_ore_drill_mined, 25),
    },
    ObjectiveDefinition {
        title: "Craft transport belts",
        hint: "Press C and craft 10 transport belts for your first production line.",
        progress: |facts| {
            ObjectiveProgress::new(facts.onboarding.transport_belts_manually_crafted, 10)
        },
    },
    ObjectiveDefinition {
        title: "Generate electricity",
        hint: "Connect an offshore pump, boiler, steam engine, and small electric pole; fuel the boiler.",
        progress: |facts| {
            ObjectiveProgress::new(u64::from(facts.onboarding.electricity_generated), 1)
        },
    },
    ObjectiveDefinition {
        title: "Place a lab",
        hint: "Place a lab within the coverage of a small electric pole.",
        progress: |facts| ObjectiveProgress::new(facts.onboarding.labs_placed, 1),
    },
    ObjectiveDefinition {
        title: "Produce automation science",
        hint: "Craft 10 red science packs and insert them into the powered lab.",
        progress: |facts| {
            ObjectiveProgress::new(facts.onboarding.automation_science_packs_produced, 10)
        },
    },
    ObjectiveDefinition {
        title: "Research Logistics",
        hint: "Press T to open technologies and research Logistics.",
        progress: |facts| {
            ObjectiveProgress::new(u64::from(facts.onboarding.logistics_researched), 1)
        },
    },
    ObjectiveDefinition {
        title: "Research Automation",
        hint: "Research Automation to unlock assembling machines.",
        progress: |facts| {
            ObjectiveProgress::new(u64::from(facts.onboarding.automation_researched), 1)
        },
    },
    ObjectiveDefinition {
        title: "Automate an item",
        hint: "Power and supply an assembling machine, select a recipe, and let it finish one item.",
        progress: |facts| ObjectiveProgress::new(facts.onboarding.assembler_items_produced, 1),
    },
    ObjectiveDefinition {
        title: "Produce logistic science",
        hint: "Research electric power and logistic science packs, then automate 10 green science packs.",
        progress: |facts| {
            ObjectiveProgress::new(facts.onboarding.logistic_science_packs_produced, 10)
        },
    },
    ObjectiveDefinition {
        title: "Research Oil Processing",
        hint: "Press T and queue the Logistics 2, Fluid Handling, and Oil Processing prerequisite chain.",
        progress: |facts| {
            ObjectiveProgress::new(u64::from(facts.onboarding.oil_processing_researched), 1)
        },
    },
    ObjectiveDefinition {
        title: "Refine petroleum gas",
        hint: "Power a pumpjack over crude oil, pipe it to a refinery, and select Basic Oil Processing.",
        progress: |facts| ObjectiveProgress::new(facts.onboarding.petroleum_gas_produced, 45),
    },
    ObjectiveDefinition {
        title: "Research Turrets",
        hint: "Research Stone Walls followed by Turrets.",
        progress: |facts| ObjectiveProgress::new(u64::from(facts.onboarding.turrets_researched), 1),
    },
    ObjectiveDefinition {
        title: "Deploy a loaded gun turret",
        hint: "Place a gun turret and load it with usable ammunition.",
        progress: |facts| ObjectiveProgress::new(facts.onboarding.loaded_gun_turrets, 1),
    },
    ObjectiveDefinition {
        title: "Produce production science",
        hint: "Research Production Science Pack, then automate 10 purple science packs.",
        progress: |facts| {
            ObjectiveProgress::new(facts.rocket.production_science_packs_produced, 10)
        },
    },
    ObjectiveDefinition {
        title: "Produce utility science",
        hint: "Research Utility Science Pack, then automate 10 yellow science packs.",
        progress: |facts| ObjectiveProgress::new(facts.rocket.utility_science_packs_produced, 10),
    },
    ObjectiveDefinition {
        title: "Research Rocket Silo",
        hint: "Research Rocket Silo after producing both late-game science packs.",
        progress: |facts| ObjectiveProgress::new(u64::from(facts.rocket.rocket_silo_researched), 1),
    },
    ObjectiveDefinition {
        title: "Place and power a rocket silo",
        hint: "Build a rocket silo inside a powered electric network.",
        progress: |facts| ObjectiveProgress::new(u64::from(facts.rocket.powered_rocket_silo), 1),
    },
    ObjectiveDefinition {
        title: "Complete the rocket's parts",
        hint: "Supply low density structures, rocket fuel, and processing units until all rocket parts are built.",
        progress: |facts| {
            ObjectiveProgress::new(
                u64::from(facts.rocket.rocket_parts_completed),
                u64::from(facts.rocket.rocket_parts_required.max(1)),
            )
        },
    },
    ObjectiveDefinition {
        title: "Craft or load a satellite",
        hint: "Research Space Science Pack, craft a satellite, and load it into the completed rocket.",
        progress: |facts| ObjectiveProgress::new(u64::from(facts.rocket.satellite_prepared), 1),
    },
    ObjectiveDefinition {
        title: "Launch the first rocket",
        hint: "Keep the silo's launch output clear; a completed rocket with a satellite launches automatically.",
        progress: |facts| ObjectiveProgress::new(facts.rocket.rockets_launched, 1),
    },
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ObjectiveFacts {
    onboarding: OnboardingProgress,
    rocket: RocketProgramProgress,
}

impl ObjectiveFacts {
    fn from_simulation(simulation: &Simulation) -> Self {
        Self {
            onboarding: simulation.onboarding_progress(),
            rocket: simulation.rocket_program_progress(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ObjectiveProgress {
    current: u64,
    target: u64,
}
impl ObjectiveProgress {
    const fn new(current: u64, target: u64) -> Self {
        Self { current, target }
    }

    fn is_complete(self) -> bool {
        self.current >= self.target
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ObjectivesSnapshot {
    progress: [ObjectiveProgress; OBJECTIVES.len()],
}
impl Default for ObjectivesSnapshot {
    fn default() -> Self {
        Self::from_facts(ObjectiveFacts::default())
    }
}
impl ObjectivesSnapshot {
    fn from_facts(facts: ObjectiveFacts) -> Self {
        Self {
            progress: std::array::from_fn(|index| (OBJECTIVES[index].progress)(facts)),
        }
    }

    fn active_index(&self) -> Option<usize> {
        self.progress.iter().position(|p| !p.is_complete())
    }
    fn visible_indices(&self) -> [usize; VISIBLE_ROW_COUNT] {
        let active = self.active_index().unwrap_or(OBJECTIVES.len() - 1);
        let start = active
            .saturating_sub(2)
            .min(OBJECTIVES.len() - VISIBLE_ROW_COUNT);
        std::array::from_fn(|offset| start + offset)
    }
}

#[derive(Resource, Default)]
pub(crate) struct ObjectivesPanelState {
    snapshot: ObjectivesSnapshot,
}
#[derive(Component, Default, Clone)]
pub struct ObjectivesPanelRoot;
#[derive(Component, Default, Clone)]
pub(crate) struct ObjectiveRow {
    slot: usize,
}
#[derive(Component, Default, Clone)]
pub(crate) struct ObjectiveRowText {
    slot: usize,
}
#[derive(Component, Default, Clone)]
pub(crate) struct ObjectiveHintText;

/// Retained objectives panel shell with snapshot-backed initial presentation state.
fn objectives_panel_scene(snapshot: &ObjectivesSnapshot) -> impl Scene {
    let rows = snapshot
        .visible_indices()
        .into_iter()
        .enumerate()
        .map(|(slot, index)| objective_row_scene(slot, index, snapshot))
        .collect::<Vec<_>>();
    let visibility = panel_visibility(snapshot);
    let hint = hint_text(snapshot);

    bsn! {
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(MINIMAP_TOP_OFFSET),
            right: Val::Px(OBJECTIVES_PANEL_RIGHT),
            width: Val::Px(330.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(7.0),
            padding: UiRect::all(Val::Px(12.0)),
            border: UiRect::all(Val::Px(1.0)),
        }
        BackgroundColor(Color::srgba(0.025, 0.030, 0.028, 0.92))
        BorderColor::all(Color::srgba(0.34, 0.43, 0.34, 0.92))
        GlobalZIndex(1100)
        template_value(visibility)
        ObjectivesPanelRoot
        Children [
            (
                Text("OBJECTIVES")
                TextFont { font_size: FontSize::Px(16.0) }
                TextColor(Color::srgb(0.92, 0.82, 0.45))
            ),
            {rows},
            (
                Node {
                    margin: UiRect::top(Val::Px(3.0)),
                    padding: UiRect::top(Val::Px(8.0)),
                    border: UiRect::top(Val::Px(1.0)),
                }
                BorderColor::all(Color::srgba(0.28, 0.34, 0.29, 0.8))
                Children [(
                    Text(hint)
                    TextFont { font_size: FontSize::Px(12.0) }
                    TextColor(Color::srgb(0.74, 0.78, 0.70))
                    ObjectiveHintText
                )]
            ),
        ]
    }
}

fn objective_row_scene(slot: usize, index: usize, snapshot: &ObjectivesSnapshot) -> impl Scene {
    let progress = snapshot.progress[index];
    let active = snapshot.active_index() == Some(index);
    let background = row_background(progress, active);
    let accent = row_accent(progress, active);
    let text = row_text(index, progress);
    let text_color = row_text_color(progress, active);

    bsn! {
        Node {
            min_height: Val::Px(31.0),
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
            border: UiRect::left(Val::Px(3.0)),
        }
        BackgroundColor(background)
        BorderColor::all(accent)
        ObjectiveRow { slot }
        Children [(
            Text(text)
            TextFont { font_size: FontSize::Px(13.0) }
            TextColor(text_color)
            ObjectiveRowText { slot }
        )]
    }
}

/// Creates the objectives hierarchy for the first world while retaining it across world swaps.
pub(crate) fn setup_objectives_panel(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut state: ResMut<ObjectivesPanelState>,
    existing: Query<(), With<ObjectivesPanelRoot>>,
    asset_server: Option<Res<AssetServer>>,
    scene_patches: Option<Res<Assets<ScenePatch>>>,
) {
    // Retain the previous world's cached snapshot when the hierarchy already
    // exists. The regular sync system will then observe the new simulation as
    // a change and refresh every retained row during this frame's Update.
    if !existing.is_empty() {
        return;
    }
    let simulation = sim.read();
    state.snapshot = objectives_snapshot(&simulation);
    drop(simulation);
    let snapshot = state.snapshot.clone();

    if asset_server.is_none() || scene_patches.is_none() {
        return;
    }

    commands.spawn_scene(objectives_panel_scene(&snapshot));
}

pub(crate) fn sync_objectives_panel(
    sim: Res<SimResource>,
    mut state: ResMut<ObjectivesPanelState>,
    mut rows: Query<(&ObjectiveRow, &mut BackgroundColor, &mut BorderColor)>,
    mut labels: Query<(&ObjectiveRowText, &mut Text, &mut TextColor)>,
    mut hints: Query<&mut Text, (With<ObjectiveHintText>, Without<ObjectiveRowText>)>,
    mut roots: Query<&mut Visibility, With<ObjectivesPanelRoot>>,
) {
    let simulation = sim.read();
    let next = objectives_snapshot(&simulation);
    drop(simulation);
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

fn objectives_snapshot(simulation: &Simulation) -> ObjectivesSnapshot {
    ObjectivesSnapshot::from_facts(ObjectiveFacts::from_simulation(simulation))
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
        || "Rocket launched. Keep expanding: the factory must grow!".to_string(),
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
    use bevy::{asset::AssetPlugin, scene::ScenePlugin};

    fn completed_onboarding() -> OnboardingProgress {
        OnboardingProgress {
            iron_ore_manually_mined: 10,
            stone_furnaces_placed: 1,
            iron_plates_smelted: 10,
            burner_mining_drills_placed: 1,
            iron_ore_drill_mined: 25,
            transport_belts_manually_crafted: 10,
            electricity_generated: true,
            labs_placed: 1,
            automation_science_packs_produced: 10,
            logistics_researched: true,
            automation_researched: true,
            assembler_items_produced: 1,
            logistic_science_packs_produced: 10,
            oil_processing_researched: true,
            petroleum_gas_produced: 45,
            turrets_researched: true,
            loaded_gun_turrets: 1,
            ..default()
        }
    }

    #[test]
    fn objectives_scene_keeps_the_retained_rows_and_hint_hierarchy() {
        let mut app = App::new();
        app.add_plugins((AssetPlugin::default(), ScenePlugin))
            .insert_resource(SimResource::new(Simulation::new_test_world(0)))
            .init_resource::<ObjectivesPanelState>()
            .add_systems(Update, setup_objectives_panel);
        app.update();

        let world = app.world_mut();
        let root = world
            .query_filtered::<Entity, With<ObjectivesPanelRoot>>()
            .single(world)
            .expect("setup should spawn one objectives panel root");
        assert_eq!(
            world.entity(root).get::<Visibility>(),
            Some(&Visibility::Visible)
        );
        assert_eq!(
            world
                .entity(root)
                .get::<Children>()
                .expect("objectives panel should have retained children")
                .len(),
            VISIBLE_ROW_COUNT + 2
        );

        let mut rows = world
            .query::<(&ObjectiveRow, &Children)>()
            .iter(world)
            .map(|(row, children)| (row.slot, children.len()))
            .collect::<Vec<_>>();
        rows.sort_unstable();
        assert_eq!(
            rows,
            (0..VISIBLE_ROW_COUNT)
                .map(|slot| (slot, 1))
                .collect::<Vec<_>>()
        );

        let mut labels = world
            .query::<&ObjectiveRowText>()
            .iter(world)
            .map(|label| label.slot)
            .collect::<Vec<_>>();
        labels.sort_unstable();
        assert_eq!(labels, (0..VISIBLE_ROW_COUNT).collect::<Vec<_>>());
        assert_eq!(
            world
                .query_filtered::<Entity, With<ObjectiveHintText>>()
                .iter(world)
                .count(),
            1
        );
    }

    #[test]
    fn reentering_game_refreshes_retained_objective_rows() {
        let previous_snapshot = ObjectivesSnapshot::from_facts(ObjectiveFacts {
            onboarding: OnboardingProgress {
                iron_ore_manually_mined: 10,
                ..default()
            },
            ..default()
        });
        let simulation = Simulation::new_test_world(123);
        let expected = objectives_snapshot(&simulation);
        let mut app = App::new();
        app.insert_resource(SimResource::new(simulation))
            .insert_resource(ObjectivesPanelState {
                snapshot: previous_snapshot,
            })
            .add_systems(
                Update,
                (setup_objectives_panel, sync_objectives_panel).chain(),
            );
        app.world_mut()
            .spawn((ObjectivesPanelRoot, Visibility::Visible));
        app.world_mut().spawn((
            ObjectiveRow { slot: 0 },
            BackgroundColor(Color::BLACK),
            BorderColor::all(Color::BLACK),
        ));
        app.world_mut().spawn((
            ObjectiveRowText { slot: 0 },
            Text::new("stale objective"),
            TextColor(Color::BLACK),
        ));
        app.world_mut()
            .spawn((ObjectiveHintText, Text::new("stale hint")));

        app.update();

        assert_eq!(
            app.world().resource::<ObjectivesPanelState>().snapshot,
            expected
        );
        let label = app
            .world_mut()
            .query_filtered::<&Text, With<ObjectiveRowText>>()
            .single(app.world())
            .expect("the retained row should still exist");
        assert_eq!(label.0, row_text(0, expected.progress[0]));
        let hint = app
            .world_mut()
            .query_filtered::<&Text, (With<ObjectiveHintText>, Without<ObjectiveRowText>)>()
            .single(app.world())
            .expect("the retained hint should still exist");
        assert_eq!(hint.0, hint_text(&expected));
    }

    #[test]
    fn windows_track_active_objective() {
        let first = ObjectivesSnapshot::default();
        assert_eq!(first.visible_indices(), [0, 1, 2, 3, 4]);
        let middle = ObjectivesSnapshot::from_facts(ObjectiveFacts {
            onboarding: OnboardingProgress {
                iron_ore_manually_mined: 10,
                stone_furnaces_placed: 1,
                iron_plates_smelted: 10,
                burner_mining_drills_placed: 1,
                iron_ore_drill_mined: 25,
                transport_belts_manually_crafted: 10,
                electricity_generated: true,
                labs_placed: 1,
                ..default()
            },
            ..default()
        });
        assert_eq!(middle.active_index(), Some(8));
        assert_eq!(middle.visible_indices(), [6, 7, 8, 9, 10]);
    }

    #[test]
    fn visible_window_follows_each_late_game_milestone() {
        let mut s = ObjectivesSnapshot::default();
        for p in &mut s.progress[..17] {
            p.current = p.target;
        }
        assert_eq!(s.active_index(), Some(17));
        assert_eq!(s.visible_indices(), [15, 16, 17, 18, 19]);

        for p in &mut s.progress[17..20] {
            p.current = p.target;
        }
        assert_eq!(s.active_index(), Some(20));
        assert_eq!(s.visible_indices(), [18, 19, 20, 21, 22]);

        for p in &mut s.progress[20..23] {
            p.current = p.target;
        }
        assert_eq!(s.active_index(), Some(23));
        assert_eq!(s.visible_indices(), [19, 20, 21, 22, 23]);
    }

    #[test]
    fn later_progress_does_not_skip_sequence() {
        let s = ObjectivesSnapshot::from_facts(ObjectiveFacts {
            rocket: RocketProgramProgress {
                production_science_packs_produced: 10,
                utility_science_packs_produced: 10,
                rocket_silo_researched: true,
                powered_rocket_silo: true,
                rocket_parts_completed: 100,
                rocket_parts_required: 100,
                satellite_prepared: true,
                rockets_launched: 1,
            },
            ..default()
        });
        assert_eq!(s.active_index(), Some(0));
    }

    #[test]
    fn completed_launch_finishes_the_guided_path() {
        let s = ObjectivesSnapshot::from_facts(ObjectiveFacts {
            onboarding: completed_onboarding(),
            rocket: RocketProgramProgress {
                production_science_packs_produced: 10,
                utility_science_packs_produced: 10,
                rocket_silo_researched: true,
                powered_rocket_silo: true,
                rocket_parts_completed: 100,
                rocket_parts_required: 100,
                satellite_prepared: true,
                rockets_launched: 1,
            },
        });

        assert_eq!(s.active_index(), None);
        assert_eq!(panel_visibility(&s), Visibility::Hidden);
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
