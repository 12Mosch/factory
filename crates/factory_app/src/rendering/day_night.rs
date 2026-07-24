use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use crate::resources::SimResource;

const NIGHT_TINT_RED: f32 = 0.015;
const NIGHT_TINT_GREEN: f32 = 0.03;
const NIGHT_TINT_BLUE: f32 = 0.08;
const NIGHT_TINT_MAX_ALPHA: f32 = 0.65;
const DAY_NIGHT_TINT_Z_INDEX: i32 = -1000;

#[derive(Component)]
pub(crate) struct DayNightTint;

pub(crate) fn tint_color(daylight: f32) -> Color {
    let alpha = (1.0 - daylight).clamp(0.0, 1.0) * NIGHT_TINT_MAX_ALPHA;
    Color::srgba(NIGHT_TINT_RED, NIGHT_TINT_GREEN, NIGHT_TINT_BLUE, alpha)
}

pub(crate) fn spawn_day_night_tint(mut commands: Commands) {
    commands.spawn((
        DayNightTint,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(tint_color(1.0)),
        GlobalZIndex(DAY_NIGHT_TINT_Z_INDEX),
        Pickable::IGNORE,
        FocusPolicy::Pass,
    ));
}

pub(crate) fn sync_day_night_tint(
    sim: Res<SimResource>,
    mut tint: Single<&mut BackgroundColor, With<DayNightTint>>,
) {
    let next = BackgroundColor(tint_color(sim.read().daylight()));
    if **tint != next {
        **tint = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_data::{DayNightCycleConfig, PrototypeCatalog};
    use factory_sim::Simulation;

    fn assert_color(daylight: f32, expected_alpha: f32) {
        let color = tint_color(daylight).to_srgba();
        assert_eq!(color.red, NIGHT_TINT_RED);
        assert_eq!(color.green, NIGHT_TINT_GREEN);
        assert_eq!(color.blue, NIGHT_TINT_BLUE);
        assert!((color.alpha - expected_alpha).abs() < f32::EPSILON);
    }

    fn short_cycle_sim(enabled: bool) -> Simulation {
        let mut catalog = PrototypeCatalog::load_base().expect("base catalog should load");
        catalog.day_night_cycle = enabled.then_some(DayNightCycleConfig {
            cycle_length_ticks: 20,
            dawn_dusk_ticks: 4,
        });
        Simulation::new(5, catalog)
    }

    fn tint_test_app(sim: Simulation) -> App {
        let mut app = App::new();
        app.insert_resource(SimResource::new(sim))
            .add_systems(Startup, spawn_day_night_tint)
            .add_systems(Update, sync_day_night_tint);
        app.update();
        app
    }

    #[test]
    fn tint_mapping_matches_day_half_light_and_night() {
        assert_color(1.0, 0.0);
        assert_color(0.5, 0.325);
        assert_color(0.0, 0.65);
    }

    #[test]
    fn tint_node_is_fullscreen_behind_ui_and_ignores_input() {
        let mut app = tint_test_app(short_cycle_sim(true));
        let world = app.world_mut();
        let mut query = world
            .query_filtered::<(&Node, &GlobalZIndex, &Pickable, &FocusPolicy), With<DayNightTint>>(
            );
        let (node, z_index, pickable, focus) = query.single(world).expect("tint should exist");

        assert_eq!(node.position_type, PositionType::Absolute);
        assert_eq!(node.left, Val::Px(0.0));
        assert_eq!(node.right, Val::Px(0.0));
        assert_eq!(node.top, Val::Px(0.0));
        assert_eq!(node.bottom, Val::Px(0.0));
        assert_eq!(node.width, Val::Percent(100.0));
        assert_eq!(node.height, Val::Percent(100.0));
        assert_eq!(*z_index, GlobalZIndex(DAY_NIGHT_TINT_Z_INDEX));
        assert_eq!(*pickable, Pickable::IGNORE);
        assert_eq!(*focus, FocusPolicy::Pass);
    }

    #[test]
    fn sync_tracks_fixed_simulation_ticks() {
        let mut app = tint_test_app(short_cycle_sim(true));

        {
            let world = app.world_mut();
            let mut query = world.query_filtered::<&BackgroundColor, With<DayNightTint>>();
            let tint = query.single(world).expect("tint should exist");
            assert_eq!(tint.0.to_srgba().alpha, 0.0);
        }

        {
            let mut sim = app.world_mut().resource_mut::<SimResource>();
            let mut sim = sim.write_for_tests();
            for _ in 0..14 {
                sim.tick();
            }
            assert_eq!(sim.daylight(), 0.0);
        }
        app.update();

        let world = app.world_mut();
        let mut query = world.query_filtered::<&BackgroundColor, With<DayNightTint>>();
        let tint = query.single(world).expect("tint should exist");
        assert_eq!(tint.0.to_srgba().alpha, NIGHT_TINT_MAX_ALPHA);
    }

    #[test]
    fn disabled_cycle_leaves_tint_transparent() {
        let mut app = tint_test_app(short_cycle_sim(false));
        {
            let mut sim = app.world_mut().resource_mut::<SimResource>();
            let mut sim = sim.write_for_tests();
            for _ in 0..30 {
                sim.tick();
            }
        }
        app.update();

        let world = app.world_mut();
        let mut query = world.query_filtered::<&BackgroundColor, With<DayNightTint>>();
        let tint = query.single(world).expect("tint should exist");
        assert_eq!(tint.0.to_srgba().alpha, 0.0);
    }
}
