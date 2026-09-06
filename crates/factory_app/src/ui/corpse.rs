use crate::resources::SimResource;
use crate::simulation::{AppPauseState, SimCommandRequest, SimCommandResult};
use crate::ui::formatting::format_item_display_name;
use bevy::prelude::*;
use factory_sim::{CorpseRecoveryError, SimCommand, SimCommandError, Simulation};

#[derive(Resource, Default)]
pub(crate) struct CorpseSelection {
    selected: Option<u64>,
    feedback: Option<(u64, &'static str)>,
}
#[derive(Component)]
pub(crate) struct NextCorpseButton;
#[derive(Component)]
pub(crate) struct CorpsePanel;
#[derive(Component)]
pub(crate) struct CorpseText;
#[derive(Component)]
pub(crate) struct RecoverButton(pub u64);

pub(crate) fn setup(mut commands: Commands) {
    commands
        .spawn((
            CorpsePanel,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(14.0),
                top: Val::Px(200.0),
                width: Val::Px(280.0),
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.10, 0.04, 0.09, 0.95)),
            GlobalZIndex(1850),
        ))
        .with_children(|parent| {
            parent.spawn((
                CorpseText,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
            ));
            parent
                .spawn((
                    NextCorpseButton,
                    Button,
                    Node {
                        padding: UiRect::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.2, 0.1, 0.2)),
                ))
                .with_children(|parent| {
                    parent.spawn(Text::new("Next corpse"));
                });
            parent
                .spawn((
                    RecoverButton(0),
                    Button,
                    Node {
                        padding: UiRect::all(Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.35, 0.12, 0.28)),
                ))
                .with_children(|parent| {
                    parent.spawn(Text::new("Recover items"));
                });
        });
}

fn nearest_corpse(sim: &Simulation) -> Option<u64> {
    let (px, py) = sim.player().tile_position();
    sim.corpses()
        .min_by_key(|corpse| {
            let (x, y) = corpse.tile_position();
            let dx = i128::from(x) - i128::from(px);
            let dy = i128::from(y) - i128::from(py);
            (dx * dx + dy * dy, corpse.id())
        })
        .map(|corpse| corpse.id())
}

pub(crate) fn sync(
    sim: Res<SimResource>,
    pause: Res<AppPauseState>,
    mut selection: ResMut<CorpseSelection>,
    mut panels: Query<&mut Node, With<CorpsePanel>>,
    mut texts: Query<&mut Text, With<CorpseText>>,
    mut buttons: Query<&mut RecoverButton>,
    mut results: MessageReader<SimCommandResult>,
) {
    for result in results.read() {
        if let SimCommand::RecoverCorpse { corpse_id } = result.command {
            selection.feedback = match result.result {
                Err(SimCommandError::CorpseRecovery(CorpseRecoveryError::NoCapacity)) => Some((
                    corpse_id,
                    "Inventory full or opened consumable already held. Free space/use that consumable, then recover again.",
                )),
                Err(SimCommandError::CorpseRecovery(CorpseRecoveryError::OutOfReach)) => {
                    Some((corpse_id, "Walk closer to the pink corpse marker."))
                }
                _ => None,
            };
        }
    }
    let simulation = sim.read();
    let id = selection
        .selected
        .filter(|id| simulation.corpse(*id).is_some())
        .or_else(|| nearest_corpse(&simulation));
    for mut node in &mut panels {
        node.display = if id.is_some() && !simulation.player().is_dead() && !pause.is_paused() {
            Display::Flex
        } else {
            Display::None
        };
    }
    let Some(id) = id else {
        return;
    };
    let corpse = simulation
        .corpse(id)
        .expect("nearest corpse came from this snapshot");
    for mut button in &mut buttons {
        button.0 = id;
    }
    let (x, y) = corpse.tile_position();
    let mut amounts = std::collections::BTreeMap::new();
    for amount in corpse.items() {
        *amounts.entry(amount.item_id()).or_insert(0_u128) += u128::from(amount.count());
    }
    let mut message = format!("CORPSE #{id} · ({x}, {y})\nPink marker on world and map\n");
    for (item, count) in amounts.iter().take(6) {
        message.push_str(&format!(
            "\n{count} × {}",
            format_item_display_name(simulation.catalog(), *item)
        ));
    }
    if amounts.len() > 6 {
        message.push_str(&format!("\n… and {} more item types", amounts.len() - 6));
    }
    if corpse.remaining_ammunition() > 0 {
        message.push_str(&format!(
            "\n{} opened-magazine shots",
            corpse.remaining_ammunition()
        ));
    }
    if corpse.remaining_repair_health() > 0 {
        message.push_str("\nPartially used repair pack");
    }
    message.push_str(if simulation.can_reach_corpse(id) {
        "\n\nRecover what fits. Leftovers stay here. Re-equip recovered armor/modules."
    } else {
        "\n\nReturn within mining reach to recover. Corpses never expire."
    });
    if let Some((feedback_id, text)) = selection.feedback
        && feedback_id == id
    {
        message.push_str(&format!("\n\n{text}"));
    }
    for mut text in &mut texts {
        if **text != message {
            **text = message.clone();
        }
    }
}

pub(crate) fn recover(
    buttons: Query<(&Interaction, &RecoverButton), Changed<Interaction>>,
    sim: Res<SimResource>,
    pause: Res<AppPauseState>,
    mut requests: MessageWriter<SimCommandRequest>,
) {
    if pause.is_paused() || sim.read().player().is_dead() {
        return;
    }
    for (interaction, button) in &buttons {
        if *interaction == Interaction::Pressed {
            requests.write(SimCommandRequest(SimCommand::RecoverCorpse {
                corpse_id: button.0,
            }));
        }
    }
}

pub(crate) fn hide(mut panels: Query<&mut Node, With<CorpsePanel>>) {
    for mut node in &mut panels {
        node.display = Display::None;
    }
}

pub(crate) fn select_next(
    sim: Res<SimResource>,
    buttons: Query<&Interaction, (Changed<Interaction>, With<NextCorpseButton>)>,
    mut selection: ResMut<CorpseSelection>,
) {
    if !buttons
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
    {
        return;
    }
    let simulation = sim.read();
    let current = selection
        .selected
        .filter(|id| simulation.corpse(*id).is_some())
        .or_else(|| nearest_corpse(&simulation));
    selection.selected = simulation
        .corpses()
        .find(|corpse| current.is_none_or(|id| corpse.id() > id))
        .or_else(|| simulation.corpses().next())
        .map(|corpse| corpse.id());
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::{
        CombatCommand, CombatCommandBuffer, CombatSource, CombatantId, Damage, EnemyId, Faction,
    };

    fn kill_and_respawn(sim: &mut Simulation) {
        let mut damage = CombatCommandBuffer::default();
        damage.push(CombatCommand {
            source: CombatSource::new(CombatantId::Enemy(EnemyId::new(999)), Faction::Enemy),
            target: CombatantId::Player,
            damage: Damage::physical(u32::MAX),
        });
        sim.resolve_combat_commands(damage);
        sim.apply_command(&SimCommand::RespawnPlayer).unwrap();
        sim.tick();
    }

    #[test]
    fn recovery_ui_selects_overlapping_corpses_and_queues_the_displayed_identity() {
        let mut simulation = Simulation::new_test_world(123);
        let item = simulation
            .player_inventory()
            .slots()
            .iter()
            .find_map(|slot| slot.stack())
            .unwrap()
            .item_id();
        kill_and_respawn(&mut simulation);
        let catalog = simulation.catalog().clone();
        simulation
            .player_inventory_mut()
            .insert(&catalog, item, 1)
            .unwrap();
        kill_and_respawn(&mut simulation);
        assert_eq!(simulation.corpses().count(), 2);
        let mut app = App::new();
        app.insert_resource(SimResource::new(simulation))
            .init_resource::<AppPauseState>()
            .init_resource::<CorpseSelection>()
            .add_message::<SimCommandRequest>()
            .add_message::<SimCommandResult>()
            .add_systems(Startup, setup)
            .add_systems(Update, (select_next, recover, sync).chain());
        app.update();
        {
            let world = app.world_mut();
            let mut buttons = world.query::<&RecoverButton>();
            assert_eq!(buttons.single(world).unwrap().0, 1);
            let mut next = world.query_filtered::<&mut Interaction, With<NextCorpseButton>>();
            *next.single_mut(world).unwrap() = Interaction::Pressed;
        }
        app.update();
        {
            let world = app.world_mut();
            let mut buttons = world.query::<(&RecoverButton, &mut Interaction)>();
            let (button, mut interaction) = buttons.single_mut(world).unwrap();
            assert_eq!(button.0, 2);
            *interaction = Interaction::Pressed;
        }
        app.update();
        let requests = app
            .world_mut()
            .resource_mut::<Messages<SimCommandRequest>>()
            .drain()
            .map(|request| request.0)
            .collect::<Vec<_>>();
        assert_eq!(requests, vec![SimCommand::RecoverCorpse { corpse_id: 2 }]);
    }
}
