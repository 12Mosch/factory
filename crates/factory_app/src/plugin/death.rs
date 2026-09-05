use bevy::prelude::*;
use factory_sim::SimCommand;

use super::{AppSet, InGameSet};
use crate::input::resources::{AppInputState, TrainManualInput, WeaponInput};
use crate::resources::SimResource;
use crate::simulation::{AppPauseState, SimCommandRequest};

pub(super) struct PlayerDeathPlugin;

impl Plugin for PlayerDeathPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(OnExit(crate::world_setup::AppMode::InGame), hide)
            .add_systems(
                PreUpdate,
                block_dead_input.after(AppSet::PanelInput).in_set(InGameSet),
            )
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
        "YOU DIED · Deaths: {}\n\nInventory, armor and ammunition retained. Stored armor energy lost. Crafting and personal robots pause until respawn.\n\nRespawn restores full health at the nearest free starting-area tile.{}",
        simulation.player_deaths(),
        if pause.is_paused() {
            "\nResume the game to complete respawn."
        } else {
            " If no tile is free, recovery waits for one."
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
    use crate::simulation::{SimCommandBacklog, collect_sim_commands};
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
            .add_message::<SimCommandRequest>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (block_dead_input, sync, respawn, collect_sim_commands).chain(),
            );
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
        assert_eq!(
            app.world().resource::<SimCommandBacklog>().0,
            vec![SimCommand::RespawnPlayer]
        );
        {
            let mut resource = app.world_mut().resource_mut::<SimResource>();
            let mut simulation = resource.write_for_tests();
            simulation
                .apply_command(&SimCommand::RespawnPlayer)
                .unwrap();
            simulation.tick();
        }
        app.update();
        let world = app.world_mut();
        let mut panels = world.query_filtered::<&Node, With<DeathPanel>>();
        assert_eq!(panels.single(world).unwrap().display, Display::None);
    }
}
