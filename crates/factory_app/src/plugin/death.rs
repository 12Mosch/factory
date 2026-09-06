use bevy::prelude::*;
use factory_sim::SimCommand;

use super::{AppSet, InGameSet};
use crate::input::panels::WorldBlockingWindows;
use crate::input::resources::{AppInputState, TrainManualInput, WeaponInput};
use crate::resources::SimResource;
use crate::simulation::{AppPauseState, SimCommandRequest};

pub(super) struct PlayerDeathPlugin;

impl Plugin for PlayerDeathPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::ui::corpse::CorpseSelection>()
            .add_systems(Startup, (setup, crate::ui::corpse::setup))
            .add_systems(
                Update,
                (
                    crate::ui::corpse::select_next,
                    crate::ui::corpse::recover,
                    crate::ui::corpse::sync,
                )
                    .chain()
                    .in_set(AppSet::UiInteraction),
            )
            .add_systems(
                Update,
                crate::rendering::corpses::sync_corpses.in_set(AppSet::RenderSync),
            )
            .add_systems(
                OnExit(crate::world_setup::AppMode::InGame),
                (
                    crate::ui::corpse::hide,
                    crate::rendering::corpses::clear_corpses,
                ),
            )
            .add_systems(OnExit(crate::world_setup::AppMode::InGame), hide)
            .add_systems(
                PreUpdate,
                block_dead_input.after(AppSet::PanelInput).in_set(InGameSet),
            )
            .add_systems(FixedUpdate, refresh_world_blocked.in_set(AppSet::PostTick))
            .add_systems(Update, (sync, respawn).in_set(InGameSet));
    }
}

#[derive(Component)]
struct DeathPanel;
#[derive(Component)]
struct RespawnButton;
#[derive(Component)]
struct DeathText;

fn setup(mut commands: Commands) {
    commands
        .spawn((
            DeathPanel,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(30.0),
                top: Val::Percent(30.0),
                width: Val::Percent(40.0),
                padding: UiRect::all(Val::Px(24.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(18.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgb(0.10, 0.04, 0.04)),
            GlobalZIndex(9000),
        ))
        .with_children(|parent| {
            parent.spawn((
                DeathText,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
            ));
            parent
                .spawn((
                    RespawnButton,
                    Button,
                    Node {
                        padding: UiRect::all(Val::Px(14.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.3, 0.2, 0.12)),
                ))
                .with_children(|parent| {
                    parent.spawn(Text::new("Respawn"));
                });
        });
}

fn block_dead_input(
    sim: Res<SimResource>,
    mut input: ResMut<AppInputState>,
    mut train: ResMut<TrainManualInput>,
    mut weapon: ResMut<WeaponInput>,
) {
    if sim.read().player().is_dead() {
        input.world_blocked = true;
        train.clear();
        weapon.clear();
    }
}

fn refresh_world_blocked(
    sim: Res<SimResource>,
    windows: WorldBlockingWindows,
    mut input: ResMut<AppInputState>,
) {
    input.world_blocked = sim.read().player().is_dead() || windows.any_open();
}

fn sync(
    sim: Res<SimResource>,
    pause: Res<AppPauseState>,
    mut panels: Query<&mut Node, With<DeathPanel>>,
    mut texts: Query<&mut Text, With<DeathText>>,
) {
    let simulation = sim.read();
    let player = simulation.player();
    for mut node in &mut panels {
        node.display = if player.is_dead() && !pause.is_paused() {
            Display::Flex
        } else {
            Display::None
        };
    }
    if !player.is_dead() {
        return;
    }
    let message = format!(
        "YOU DIED · Deaths: {}\n\nYour items are in a corpse at the death location. Craft ingredients and personal robots/cargo are included. Stored equipment energy is lost.\n\nRespawn empty-handed with full health, then return to the pink corpse marker and click Recover items.{}",
        simulation.player_deaths(),
        if pause.is_paused() {
            "\nResume the game to complete respawn."
        } else {
            " If no tile is free, respawn waits for one."
        }
    );
    for mut text in &mut texts {
        if **text != message {
            **text = message.clone();
        }
    }
}

fn respawn(
    sim: Res<SimResource>,
    buttons: Query<&Interaction, (Changed<Interaction>, With<RespawnButton>)>,
    mut requests: MessageWriter<SimCommandRequest>,
) {
    if sim.read().player().is_dead()
        && buttons
            .iter()
            .any(|interaction| *interaction == Interaction::Pressed)
    {
        requests.write(SimCommandRequest(SimCommand::RespawnPlayer));
    }
}

fn hide(mut panels: Query<&mut Node, With<DeathPanel>>) {
    for mut node in &mut panels {
        node.display = Display::None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::{SimCommandBacklog, SimCommandResult, collect_sim_commands, tick_sim};
    use factory_sim::{
        CombatCommand, CombatCommandBuffer, CombatSource, CombatantId, Damage, EnemyId, Faction,
        Simulation,
    };

    #[test]
    fn death_panel_blocks_input_and_queues_only_recovery() {
        let mut simulation = Simulation::new_test_world(123);
        let mut damage = CombatCommandBuffer::default();
        damage.push(CombatCommand {
            source: CombatSource::new(CombatantId::Enemy(EnemyId::new(999)), Faction::Enemy),
            target: CombatantId::Player,
            damage: Damage::physical(u32::MAX),
        });
        simulation.resolve_combat_commands(damage);
        let mut app = App::new();
        app.insert_resource(SimResource::new(simulation))
            .init_resource::<AppPauseState>()
            .init_resource::<AppInputState>()
            .init_resource::<TrainManualInput>()
            .init_resource::<WeaponInput>()
            .init_resource::<SimCommandBacklog>()
            .init_resource::<crate::map::resources::MapViewState>()
            .init_resource::<crate::ui::resources::ProductionStatsWindowState>()
            .init_resource::<crate::ui::resources::CraftingWindowState>()
            .init_resource::<crate::ui::settings::SettingsWindowState>()
            .init_resource::<crate::ui::pause_menu::PauseMenuState>()
            .init_resource::<crate::save_load::SaveLoadWindowState>()
            .init_resource::<crate::build::resources::BuildMenuState>()
            .init_resource::<crate::build::resources::BlueprintLibraryWindowState>()
            .init_resource::<crate::ui::resources::EquipmentWindowState>()
            .init_resource::<crate::resources::UpsStats>()
            .init_resource::<crate::resources::SimProfileStats>()
            .init_resource::<crate::resources::FixedStepCatchUpStats>()
            .add_message::<SimCommandResult>()
            .add_message::<SimCommandRequest>()
            .add_systems(Startup, setup)
            .add_systems(PreUpdate, block_dead_input)
            .configure_sets(FixedUpdate, (AppSet::SimTick, AppSet::PostTick).chain())
            .add_systems(
                FixedUpdate,
                (collect_sim_commands, tick_sim)
                    .chain()
                    .in_set(AppSet::SimTick),
            )
            .add_systems(FixedUpdate, refresh_world_blocked.in_set(AppSet::PostTick))
            .add_systems(Update, (sync, respawn).chain());
        app.update();
        assert!(app.world().resource::<AppInputState>().world_blocked);
        let world = app.world_mut();
        let mut panels = world.query_filtered::<&Node, With<DeathPanel>>();
        assert_eq!(panels.single(world).unwrap().display, Display::Flex);
        let mut texts = world.query_filtered::<&Text, With<DeathText>>();
        assert!(texts.single(world).unwrap().contains("YOU DIED"));
        let mut buttons = world.query_filtered::<&mut Interaction, With<RespawnButton>>();
        *buttons.single_mut(world).unwrap() = Interaction::Pressed;
        world.write_message(SimCommandRequest(SimCommand::CyclePlayerWeapon));
        app.update();
        app.world_mut()
            .resource_mut::<AppInputState>()
            .escape_consumed = true;
        app.world_mut().run_schedule(FixedUpdate);
        assert!(!app.world().resource::<AppInputState>().world_blocked);
        assert!(app.world().resource::<AppInputState>().escape_consumed);
        assert!(app.world().resource::<SimCommandBacklog>().0.is_empty());
        let results = app.world().resource::<Messages<SimCommandResult>>();
        let mut cursor = results.get_cursor();
        let commands: Vec<_> = cursor.read(results).map(|result| &result.command).collect();
        assert_eq!(commands, vec![&SimCommand::RespawnPlayer]);
        app.update();
        let world = app.world_mut();
        let mut panels = world.query_filtered::<&Node, With<DeathPanel>>();
        assert_eq!(panels.single(world).unwrap().display, Display::None);
        assert!(!world.resource::<AppInputState>().world_blocked);
        world
            .resource_mut::<crate::map::resources::MapViewState>()
            .open = true;
        world.run_schedule(FixedUpdate);
        assert!(world.resource::<AppInputState>().world_blocked);
    }
}
