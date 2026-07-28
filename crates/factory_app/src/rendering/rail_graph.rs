//! Rail endpoint markers: the connection preview, and the connectivity overlay.
//!
//! Track is the one thing in the game whose connectivity a player cannot read
//! off the sprites. Two rails that end a thousandth of a tile apart, or that end
//! at the same point but face the same way, look exactly like two rails that
//! join — and until trains exist there is nothing running on the track to reveal
//! the difference.
//!
//! An endpoint marker is drawn **filled** where the end joins other track and
//! **hollow** (a dark centre inside the same colour) where it is a dead end.
//! Two things use that marker, for two different reasons:
//!
//! * **Always, while a rail is held on the build cursor**: the ends of the piece
//!   that is about to be placed, so the join can be checked before committing to
//!   it. This is placement feedback, not a diagnostic, so it is not behind the
//!   toggle.
//! * **Behind F7**: every end of every visible piece, coloured by the network it
//!   belongs to, so a run that looks continuous but is secretly two networks
//!   shows up as two colours meeting. That is what makes placeable track
//!   shippable before there is anything running on it.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_data::EntityPrototypeId;
use factory_sim::{Direction, POSITION_SCALE, RailEndpoint, WorldTileCoord};

use crate::build::resources::{BuildPlacementPreviewState, BuildPlacementState};
use crate::constants::TILE_SIZE;
use crate::input::panels::world_input_blocked;
use crate::input::resources::AppInputState;
use crate::map::resources::VisibleChunks;
use crate::rendering::colors::{rail_endpoint_dead_end_color, rail_network_color};
use crate::rendering::resources::VisibleEntityIds;
use crate::resources::SimResource;

/// Above entity sprites: the markers are a diagnostic and have to be legible
/// over the track they describe.
const MARKER_Z: f32 = 6.0;
const MARKER_SIZE: f32 = TILE_SIZE * 0.34;
const MARKER_CENTER_SIZE: f32 = TILE_SIZE * 0.18;

#[derive(Component)]
pub(crate) struct RailGraphMarker;

/// A prospective placement: what the player has in hand and where it would go.
/// Not necessarily track — `Simulation::rail_placement_preview` is what decides
/// whether there is anything rail-shaped to preview.
#[derive(Clone, Copy, PartialEq, Eq)]
struct HeldPlacement {
    prototype_id: EntityPrototypeId,
    x: WorldTileCoord,
    y: WorldTileCoord,
    direction: Direction,
}

/// Everything the drawn markers are a function of.
///
/// Endpoints move only when track is placed or destroyed (which bumps the
/// topology revision), when the visible set changes, or when the held preview
/// moves. Nothing else in a frame can change a marker.
#[derive(Clone, Copy, PartialEq, Eq)]
struct RailOverlayKey {
    visible: bool,
    visible_revision: u64,
    entity_topology_revision: u64,
    held: Option<HeldPlacement>,
}

#[derive(Resource, Default)]
pub(crate) struct RailGraphOverlay {
    pub(crate) visible: bool,
    synced: Option<RailOverlayKey>,
}

/// The build selection and its preview, which are only ever read together here.
#[derive(SystemParam)]
pub(crate) struct HeldBuildSelection<'w> {
    build_state: Res<'w, BuildPlacementState>,
    preview_state: Res<'w, BuildPlacementPreviewState>,
}

impl HeldBuildSelection<'_> {
    /// The entity the player is about to place, or `None` when they hold
    /// nothing placeable.
    fn held_placement(&self) -> Option<HeldPlacement> {
        let (x, y) = self.preview_state.cursor_tile?;
        Some(HeldPlacement {
            prototype_id: self.build_state.selected?.entity_prototype_id()?,
            x,
            y,
            direction: self.build_state.direction,
        })
    }
}

pub(crate) fn toggle_rail_graph_overlay(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_state: Option<Res<AppInputState>>,
    mut overlay: ResMut<RailGraphOverlay>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };
    if keyboard.just_pressed(KeyCode::F7) && !world_input_blocked(input_state.as_deref()) {
        overlay.visible = !overlay.visible;
    }
}

pub(crate) fn sync_rail_graph_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible_chunks: Res<VisibleChunks>,
    visible_entity_ids: Res<VisibleEntityIds>,
    held: HeldBuildSelection,
    mut overlay: ResMut<RailGraphOverlay>,
    existing: Query<Entity, With<RailGraphMarker>>,
) {
    let sim = sim.read();
    let key = RailOverlayKey {
        visible: overlay.visible,
        visible_revision: visible_chunks.revision,
        entity_topology_revision: sim.entity_topology_revision(),
        held: held.held_placement(),
    };
    if overlay.synced == Some(key) {
        return;
    }
    overlay.synced = Some(key);

    for entity in &existing {
        commands.entity(entity).despawn();
    }

    if let Some(held) = key.held
        && let Some(preview) =
            sim.rail_placement_preview(held.prototype_id, held.x, held.y, held.direction)
    {
        for connection in preview.endpoints {
            spawn_marker(
                &mut commands,
                connection.endpoint,
                rail_network_color(None),
                !connection.connected.is_empty(),
            );
        }
    }

    if !key.visible {
        return;
    }
    for &entity_id in &visible_entity_ids.ids {
        let Some(connections) = sim.rail_endpoint_connections(entity_id) else {
            continue;
        };
        let color = rail_network_color(sim.rail_network_id_for_entity(entity_id));
        for connection in connections {
            spawn_marker(
                &mut commands,
                connection.endpoint,
                color,
                !connection.connected.is_empty(),
            );
        }
    }
}

fn spawn_marker(commands: &mut Commands, endpoint: RailEndpoint, color: Color, connected: bool) {
    let scale = POSITION_SCALE as f32;
    let center = Vec2::new(
        endpoint.position.x as f32 / scale * TILE_SIZE,
        endpoint.position.y as f32 / scale * TILE_SIZE,
    );

    commands.spawn((
        Sprite::from_color(color, Vec2::splat(MARKER_SIZE)),
        Transform::from_translation(center.extend(MARKER_Z)),
        RailGraphMarker,
    ));
    // A dead end is hollowed out rather than recoloured, so the network colour
    // stays readable on both kinds of marker.
    if !connected {
        commands.spawn((
            Sprite::from_color(
                rail_endpoint_dead_end_color(),
                Vec2::splat(MARKER_CENTER_SIZE),
            ),
            Transform::from_translation(center.extend(MARKER_Z + 0.1)),
            RailGraphMarker,
        ));
    }
}
