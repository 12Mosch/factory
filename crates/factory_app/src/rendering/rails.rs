//! Rail connectivity overlay and the build cursor's connection markers.
//!
//! Track that looks joined and track that *is* joined are two different claims,
//! and only the simulation can settle the second one. Both drawings here read
//! the rail graph rather than the sprites: the overlay colours every rail end by
//! the network its piece belongs to, and the build cursor marks the ends a held
//! piece would join before it is placed.

use bevy::prelude::*;
use factory_sim::{EntityId, POSITION_SCALE, RailPoint, Simulation};

use crate::build::resources::{BuildPlacementPreviewState, BuildPlacementState, BuildTarget};
use crate::constants::TILE_SIZE;
use crate::input::resources::RailGraphOverlay;
use crate::map::resources::VisibleChunks;
use crate::rendering::colors::{rail_connection_preview_color, rail_network_color};
use crate::resources::SimResource;

/// Above entity sprites, below the build preview.
const RAIL_OVERLAY_Z: f32 = 9.5;
const PREVIEW_MARKER_Z: f32 = 20.5;
/// A joined end is drawn at full size; an open one is drawn small, so a run's
/// dead ends stand out from the joins along it.
const JOINED_MARKER_SIZE: f32 = TILE_SIZE * 0.42;
const OPEN_MARKER_SIZE: f32 = TILE_SIZE * 0.22;

#[derive(Component)]
pub(crate) struct RailOverlaySprite;

#[derive(Component)]
pub(crate) struct RailConnectionPreviewSprite;

/// Everything the overlay is a function of: what is on screen, what has been
/// built, and whether the overlay is on at all.
#[derive(Resource, Default)]
pub(crate) struct RailOverlayRenderState {
    synced: Option<(bool, u64, u64)>,
}

pub(crate) fn sync_rail_graph_overlay(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible: Res<VisibleChunks>,
    overlay: Res<RailGraphOverlay>,
    mut state: ResMut<RailOverlayRenderState>,
    existing: Query<Entity, With<RailOverlaySprite>>,
) {
    let sim = sim.read();
    let key = (
        overlay.enabled,
        sim.entity_topology_revision(),
        visible.revision,
    );
    if state.synced == Some(key) {
        return;
    }
    state.synced = Some(key);

    // Rail ends are few next to entities and only change when track does, so a
    // full respawn per change costs less than per-marker sprite identity would.
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    if !overlay.enabled {
        return;
    }

    let Some(bounds) = visible.tile_bounds else {
        return;
    };
    let max_x = bounds.min_x + i64::from(bounds.width) - 1;
    let max_y = bounds.min_y + i64::from(bounds.height) - 1;
    for entity_id in
        sim.entities()
            .occupancy()
            .entity_ids_in_tile_rect(bounds.min_x, max_x, bounds.min_y, max_y)
    {
        spawn_rail_markers(&mut commands, &sim, entity_id);
    }
}

fn spawn_rail_markers(commands: &mut Commands, sim: &Simulation, entity_id: EntityId) {
    let Some(geometry) = sim.rail_piece_geometry(entity_id) else {
        return;
    };
    let Some(network_id) = sim.rail_network_id_for_entity(entity_id) else {
        return;
    };
    let connections = sim.rail_piece_connections(entity_id);

    for (end, joined) in geometry.ends().iter().zip(connections) {
        let size = if joined.is_some() {
            JOINED_MARKER_SIZE
        } else {
            OPEN_MARKER_SIZE
        };
        commands.spawn((
            Sprite::from_color(rail_network_color(network_id), Vec2::splat(size)),
            Transform::from_translation(world_position(end.position).extend(RAIL_OVERLAY_Z)),
            RailOverlaySprite,
        ));
    }
}

/// Marks what a held rail would join, so a player can see the connection before
/// committing to the placement.
pub(crate) fn sync_rail_connection_preview(
    mut commands: Commands,
    sim: Res<SimResource>,
    build_state: Res<BuildPlacementState>,
    preview_state: Res<BuildPlacementPreviewState>,
    existing: Query<Entity, With<RailConnectionPreviewSprite>>,
) {
    // The cursor moves every frame, so the markers are respawned every frame
    // rather than diffed; there are at most two of them.
    for entity in &existing {
        commands.entity(entity).despawn();
    }

    let Some(selection) = build_state.selected else {
        return;
    };
    let BuildTarget::Entity(prototype_id) = selection.target else {
        return;
    };
    let Some((x, y)) = preview_state.cursor_tile else {
        return;
    };

    let connections = factory_sim::placement::rail_connection_preview(
        &sim.read(),
        factory_sim::placement::EntityPlacementRequest {
            prototype_id,
            x,
            y,
            direction: build_state.direction,
        },
    );
    for connection in connections {
        commands.spawn((
            Sprite::from_color(
                rail_connection_preview_color(connection.joins.is_some()),
                Vec2::splat(JOINED_MARKER_SIZE),
            ),
            Transform::from_translation(
                world_position(connection.position).extend(PREVIEW_MARKER_Z),
            ),
            RailConnectionPreviewSprite,
        ));
    }
}

/// Sub-tile world geometry in render coordinates. Tile `(0, 0)` starts at the
/// world origin, so a fixed-point position is a straight division by the
/// simulation's units-per-tile.
fn world_position(point: RailPoint) -> Vec2 {
    let scale = POSITION_SCALE as f32;
    Vec2::new(
        point.x as f32 / scale * TILE_SIZE,
        point.y as f32 / scale * TILE_SIZE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The overlay has to land on the same tiles the world renderer draws, so a
    /// rail end on a tile boundary sits exactly on that boundary in pixels.
    #[test]
    fn rail_positions_map_onto_the_tile_grid() {
        assert_eq!(world_position(RailPoint::new(0, 0)), Vec2::ZERO);
        assert_eq!(
            world_position(RailPoint::new(POSITION_SCALE, POSITION_SCALE * 2)),
            Vec2::new(TILE_SIZE, TILE_SIZE * 2.0)
        );
        assert_eq!(
            world_position(RailPoint::new(POSITION_SCALE / 2, -POSITION_SCALE)),
            Vec2::new(TILE_SIZE * 0.5, -TILE_SIZE)
        );
    }
}
