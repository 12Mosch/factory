use bevy::prelude::*;
use factory_sim::{CHUNK_SIZE, ThreatEvent, ThreatEventKind, ThreatLocation};
use std::collections::VecDeque;

use crate::map::resources::{MapLayer, MapViewState};
use crate::resources::SimResource;
use crate::save_load::PresentationReloadToken;

#[derive(Component)]
pub struct ThreatPanelText;
#[derive(Component)]
pub struct ThreatAlertRoot;
#[derive(Component, Clone, Copy)]
pub struct ThreatAlertCard {
    pub location: ThreatLocation,
}

#[derive(Resource, Default)]
pub struct ThreatUiState {
    cursor: u64,
    reload_token: u64,
    initialized: bool,
    cards: VecDeque<ThreatEvent>,
}

pub fn setup_threat_ui(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(14.0),
                top: Val::Px(204.0),
                width: Val::Px(184.0),
                min_height: Val::Px(92.0),
                padding: UiRect::all(Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.025, 0.027, 0.9)),
            BorderColor::all(Color::srgba(0.36, 0.38, 0.34, 0.82)),
            GlobalZIndex(1800),
        ))
        .with_child((
            Text::new("THREAT: LOW"),
            TextFont::from_font_size(11.0),
            TextColor(Color::srgb(0.76, 0.86, 0.7)),
            ThreatPanelText,
        ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Px(18.0),
            width: Val::Px(360.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(7.0),
            ..default()
        },
        GlobalZIndex(3200),
        ThreatAlertRoot,
    ));
}

pub fn sync_threat_ui(
    mut commands: Commands,
    sim: Res<SimResource>,
    reload: Option<Res<PresentationReloadToken>>,
    mut state: ResMut<ThreatUiState>,
    mut panel: Query<&mut Text, With<ThreatPanelText>>,
    roots: Query<Entity, With<ThreatAlertRoot>>,
) {
    let reload_token = reload.as_deref().map_or(0, |token| token.value);
    let simulation = sim.read();
    let snapshot = simulation.threat_snapshot();
    for mut text in &mut panel {
        **text = format!("THREAT: {:?}\nEvolution {}% · Pollution {:.1}\n{} active bases · {} staged ({}s)\n{} inbound · {} expansions", snapshot.tier, snapshot.evolution_percent, snapshot.total_pollution_micro as f64 / 1_000_000.0, snapshot.pollution_active_colonies, snapshot.staged_units, snapshot.maximum_launch_countdown_ticks / 60, snapshot.inbound_raids, snapshot.spotted_expansions).to_uppercase();
    }
    let events = simulation.threat_events_after(0);
    if !state.initialized || state.reload_token != reload_token {
        state.initialized = true;
        state.reload_token = reload_token;
        state.cursor = events.last().map_or(0, |event| event.sequence);
        state.cards.clear();
    } else {
        let cursor = state.cursor;
        for event in events.iter().filter(|event| event.sequence > cursor) {
            state.cards.push_back(*event);
        }
        if let Some(event) = events.last() {
            state.cursor = event.sequence;
        }
    }
    while state
        .cards
        .front()
        .is_some_and(|event| simulation.tick_count().saturating_sub(event.tick) > 600)
    {
        state.cards.pop_front();
    }
    while state.cards.len() > 3 {
        state.cards.pop_front();
    }
    for root in &roots {
        commands
            .entity(root)
            .despawn_related::<Children>()
            .with_children(|parent| {
                for event in state.cards.iter().rev() {
                    let (label, color) = event_style(event.kind);
                    parent
                        .spawn((
                            Button,
                            Node {
                                width: Val::Px(360.0),
                                min_height: Val::Px(48.0),
                                padding: UiRect::all(Val::Px(10.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.08, 0.06, 0.055, 0.96)),
                            BorderColor::all(color),
                            ThreatAlertCard {
                                location: event.location,
                            },
                        ))
                        .with_child((
                            Text::new(label),
                            TextFont::from_font_size(14.0),
                            TextColor(color),
                        ));
                }
            });
    }
}

type AlertClicks<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static ThreatAlertCard),
    (Changed<Interaction>, With<Button>),
>;
pub fn handle_threat_alert_clicks(mut clicks: AlertClicks, mut map: ResMut<MapViewState>) {
    for (interaction, card) in &mut clicks {
        if *interaction != Interaction::Pressed {
            continue;
        }
        map.open = true;
        map.selected_layer = MapLayer::Threat;
        map.follow_player = false;
        map.center_tile = match card.location {
            ThreatLocation::Exact { x, y } => Vec2::new(x as f32, y as f32),
            ThreatLocation::Sector(coord) => {
                let (x, y) = coord.min_tile();
                Vec2::new(
                    (x + i64::from(CHUNK_SIZE) / 2) as f32,
                    (y + i64::from(CHUNK_SIZE) / 2) as f32,
                )
            }
        };
    }
}

fn event_style(kind: ThreatEventKind) -> (&'static str, Color) {
    match kind {
        ThreatEventKind::PollutionContact => (
            "Pollution reached an enemy colony",
            Color::srgb(0.95, 0.64, 0.2),
        ),
        ThreatEventKind::RaidPreparing => ("Enemy raid is preparing", Color::srgb(1.0, 0.52, 0.15)),
        ThreatEventKind::RaidLaunched => ("Enemy raid launched", Color::srgb(1.0, 0.25, 0.12)),
        ThreatEventKind::StructureUnderAttack => {
            ("Structure under attack", Color::srgb(1.0, 0.12, 0.08))
        }
        ThreatEventKind::ExpansionSpotted => {
            ("Enemy expansion spotted", Color::srgb(0.95, 0.48, 0.15))
        }
        ThreatEventKind::BaseDestroyed => ("Enemy colony destroyed", Color::srgb(0.5, 0.9, 0.45)),
    }
}
