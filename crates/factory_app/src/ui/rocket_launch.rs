use bevy::prelude::*;
use bevy::scene::ScenePatch;
use bevy::ui::FocusPolicy;

use crate::constants::SIM_TICKS_PER_SECOND;
use crate::resources::SimResource;
use crate::save_load::PresentationReloadToken;

const NOTIFICATION_LIFETIME_TICKS: u64 = 12 * SIM_TICKS_PER_SECOND as u64;

/// Marker for the non-interactive first-launch notification overlay.
#[derive(Component, Default, Clone)]
pub struct RocketLaunchNotificationRoot;

/// Static retained hierarchy for the first-launch notification.
fn rocket_launch_notification_scene() -> impl Scene {
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(72.0),
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            justify_content: JustifyContent::Center,
        }
        GlobalZIndex(3300)
        Visibility::Hidden
        Pickable::IGNORE
        FocusPolicy
        RocketLaunchNotificationRoot
        Children [(
            Node {
                width: Val::Px(430.0),
                min_height: Val::Px(92.0),
                padding: UiRect::all(Val::Px(16.0)),
                border: UiRect::all(Val::Px(2.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
            }
            BackgroundColor(Color::srgba(0.035, 0.055, 0.07, 0.97))
            BorderColor::all(Color::srgb(0.45, 0.82, 1.0))
            Pickable::IGNORE
            FocusPolicy
            Children [(
                Text("ROCKET LAUNCHED\nA satellite has reached orbit. Your factory keeps running.")
                TextFont { font_size: FontSize::Px(16.0) }
                TextColor(Color::srgb(0.82, 0.94, 1.0))
                TextLayout::justify(Justify::Center)
                Pickable::IGNORE
                FocusPolicy
            )]
        )]
    }
}

/// Presentation-only state used to detect and time the world's first launch.
#[derive(Resource, Default)]
pub(crate) struct RocketLaunchUiState {
    observed_launches: u64,
    reload_token: u64,
    expires_at_tick: Option<u64>,
}

impl RocketLaunchUiState {
    /// Observes the durable launch count and reports whether the banner should be visible.
    fn observe(&mut self, launches: u64, tick: u64, reload_token: u64) -> bool {
        if reload_token != self.reload_token {
            self.reload_token = reload_token;
            self.observed_launches = launches;
            self.expires_at_tick = None;
        } else {
            if self.observed_launches == 0 && launches > 0 {
                self.expires_at_tick = Some(tick.saturating_add(NOTIFICATION_LIFETIME_TICKS));
            }
            self.observed_launches = launches;
        }

        if self
            .expires_at_tick
            .is_some_and(|expires_at| tick < expires_at)
        {
            true
        } else {
            self.expires_at_tick = None;
            false
        }
    }
}

/// Initializes the first-launch observer and hidden notification hierarchy.
pub(crate) fn setup_rocket_launch_ui(
    mut commands: Commands,
    sim: Res<SimResource>,
    reload: Option<Res<PresentationReloadToken>>,
    asset_server: Option<Res<AssetServer>>,
    scene_patches: Option<Res<Assets<ScenePatch>>>,
    mut state: ResMut<RocketLaunchUiState>,
    existing: Query<(), With<RocketLaunchNotificationRoot>>,
) {
    state.observed_launches = sim.read().rockets_launched();
    state.reload_token = reload.as_deref().map_or(0, |token| token.value);
    state.expires_at_tick = None;

    if !existing.is_empty() || asset_server.is_none() || scene_patches.is_none() {
        return;
    }

    commands.spawn_scene(rocket_launch_notification_scene());
}

/// Shows the banner for the configured lifetime after the world's first launch.
pub(crate) fn sync_rocket_launch_ui(
    sim: Res<SimResource>,
    reload: Option<Res<PresentationReloadToken>>,
    mut state: ResMut<RocketLaunchUiState>,
    mut roots: Query<&mut Visibility, With<RocketLaunchNotificationRoot>>,
) {
    let simulation = sim.read();
    let launches = simulation.rockets_launched();
    let tick = simulation.tick_count();
    let reload_token = reload.as_deref().map_or(0, |token| token.value);

    let visibility = if state.observe(launches, tick, reload_token) {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut root in &mut roots {
        *root = visibility;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{asset::AssetPlugin, scene::ScenePlugin};
    use factory_sim::Simulation;

    fn scene_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((AssetPlugin::default(), ScenePlugin));
        app
    }

    #[test]
    fn notification_scene_has_the_expected_hidden_hierarchy() {
        let mut app = scene_test_app();
        app.insert_resource(SimResource::new(Simulation::new_test_world(0)))
            .init_resource::<RocketLaunchUiState>()
            .add_systems(Update, setup_rocket_launch_ui);
        app.update();

        let root = {
            let world = app.world_mut();
            let mut roots = world.query_filtered::<Entity, With<RocketLaunchNotificationRoot>>();
            roots
                .single(world)
                .expect("setup should spawn one rocket launch notification root")
        };

        let world = app.world();
        assert!(
            world
                .entity(root)
                .contains::<RocketLaunchNotificationRoot>()
        );
        assert_eq!(
            world.entity(root).get::<Visibility>(),
            Some(&Visibility::Hidden)
        );

        let root_children = world
            .entity(root)
            .get::<Children>()
            .expect("notification root should have a panel child");
        assert_eq!(root_children.len(), 1);
        let panel = root_children[0];
        assert!(world.entity(panel).contains::<Node>());
        assert!(world.entity(panel).contains::<BackgroundColor>());
        assert!(world.entity(panel).contains::<BorderColor>());

        let panel_children = world
            .entity(panel)
            .get::<Children>()
            .expect("notification panel should have a text child");
        assert_eq!(panel_children.len(), 1);
        let text = panel_children[0];
        assert!(world.entity(text).contains::<Text>());
        assert!(world.entity(text).contains::<TextFont>());
        assert!(world.entity(text).contains::<TextColor>());
    }

    #[test]
    fn sync_updates_notification_visibility() {
        let mut app = scene_test_app();
        let root = app
            .world_mut()
            .spawn_scene(rocket_launch_notification_scene())
            .expect("rocket launch notification scene should spawn")
            .id();
        app.insert_resource(SimResource::new(Simulation::new_test_world(0)))
            .insert_resource(RocketLaunchUiState {
                expires_at_tick: Some(u64::MAX),
                ..default()
            })
            .add_systems(Update, sync_rocket_launch_ui);

        app.update();
        assert_eq!(
            app.world().entity(root).get::<Visibility>(),
            Some(&Visibility::Visible)
        );

        app.world_mut()
            .resource_mut::<RocketLaunchUiState>()
            .expires_at_tick = Some(0);
        app.update();
        assert_eq!(
            app.world().entity(root).get::<Visibility>(),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn only_the_first_launch_opens_the_notification() {
        let mut state = RocketLaunchUiState::default();

        assert!(state.observe(1, 10, 0));
        assert!(state.observe(1, 10 + NOTIFICATION_LIFETIME_TICKS - 1, 0));
        assert!(!state.observe(1, 10 + NOTIFICATION_LIFETIME_TICKS, 0));
        assert!(!state.observe(2, 20 + NOTIFICATION_LIFETIME_TICKS, 0));
    }

    #[test]
    fn loading_a_world_with_launches_does_not_replay_the_notification() {
        let mut state = RocketLaunchUiState::default();
        assert!(!state.observe(4, 100, 1));
    }
}
