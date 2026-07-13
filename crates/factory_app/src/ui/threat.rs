use bevy::prelude::*;
use factory_sim::{CHUNK_SIZE, ThreatEvent, ThreatEventKind, ThreatLocation, ThreatSnapshot};
use std::collections::VecDeque;

use crate::constants::SIM_TICKS_PER_SECOND;
use crate::map::resources::{MapDisplaySettings, MapOverlay, MapViewState};
use crate::resources::SimResource;
use crate::save_load::PresentationReloadToken;
use crate::threat_events::ThreatEventCursor;

const TICKS_PER_SECOND: u64 = SIM_TICKS_PER_SECOND as u64;
/// How long an alert card stays on screen before it expires.
const ALERT_LIFETIME_TICKS: u64 = 10 * TICKS_PER_SECOND;

#[derive(Component)]
pub struct ThreatPanelText;
#[derive(Component)]
pub struct ThreatAlertRoot;
#[derive(Component, Clone, Copy)]
pub struct ThreatAlertCard {
    pub location: ThreatLocation,
    pub kind: ThreatEventKind,
}

#[derive(Resource, Default)]
pub struct ThreatUiState {
    cursor: ThreatEventCursor,
    cards: VecDeque<ThreatEvent>,
    /// The cards the alert root's children were last built from; the root is
    /// only rebuilt when `cards` diverges from this.
    rendered_cards: VecDeque<ThreatEvent>,
    /// The snapshot the HUD panel text was last formatted from.
    rendered_panel: Option<ThreatSnapshot>,
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
    if state.rendered_panel != Some(snapshot) {
        state.rendered_panel = Some(snapshot);
        for mut text in &mut panel {
            **text = format!("THREAT: {:?}\nEvolution {}% · Pollution {:.1}\n{} active bases · {} staged ({}s)\n{} inbound · {} expansions", snapshot.tier, snapshot.evolution_percent, snapshot.total_pollution_micro as f64 / 1_000_000.0, snapshot.pollution_active_colonies, snapshot.staged_units, snapshot.maximum_launch_countdown_ticks / TICKS_PER_SECOND, snapshot.inbound_raids, snapshot.spotted_expansions).to_uppercase();
        }
    }

    let poll = state.cursor.poll_new(&simulation, reload_token);
    if poll.reset {
        state.cards.clear();
    }
    state.cards.extend(poll.events);
    while state.cards.front().is_some_and(|event| {
        simulation.tick_count().saturating_sub(event.tick) > ALERT_LIFETIME_TICKS
    }) {
        state.cards.pop_front();
    }
    while state.cards.len() > 3 {
        state.cards.pop_front();
    }
    if state.cards == state.rendered_cards {
        return;
    }
    state.rendered_cards = state.cards.clone();
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
                                kind: event.kind,
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
pub fn handle_threat_alert_clicks(
    mut clicks: AlertClicks,
    mut map: ResMut<MapViewState>,
    mut settings: ResMut<MapDisplaySettings>,
) {
    for (interaction, card) in &mut clicks {
        if *interaction != Interaction::Pressed {
            continue;
        }
        map.open = true;
        settings.overlays.set_enabled(MapOverlay::Enemies, true);
        if card.kind == ThreatEventKind::PollutionContact {
            settings.overlays.set_enabled(MapOverlay::Pollution, true);
        }
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
