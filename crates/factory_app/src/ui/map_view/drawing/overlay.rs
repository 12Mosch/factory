mod bounds;
mod construction;
mod entities;
mod pollution;
mod power;
mod primitives;
mod production;
mod threats;

use bevy::prelude::*;
use factory_sim::{ChunkCoord, Simulation};

use self::construction::spawn_construction_overlays;
use self::entities::spawn_entity_overlays;
use self::pollution::spawn_pollution_overlays;
use self::power::spawn_power_overlays;
use self::primitives::{MapOverlayPrimitive, spawn_point_overlay, spawn_rect_overlay};
use self::production::spawn_production_problem_overlays;
use self::threats::spawn_threat_overlays;
use crate::map::resources::{
    MapDetailCache, MapDisplaySettings, MapOverlayLayer, MapOverlayMarkers, MapTextureBounds,
};
use crate::ui::map_view::layout::{MapTileRect, map_rect_for_chunk, map_rect_for_world_rect};

const MAP_PLAYER_MARKER_SIZE: f32 = 9.0;
const MAP_PING_MARKER_SIZE: f32 = 13.0;
const MAP_WAYPOINT_MARKER_SIZE: f32 = 9.0;

pub(super) fn spawn_overlay_root(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands<'_>,
    marker: impl Bundle,
) {
    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
        marker,
    ));
}

pub(in crate::ui::map_view) struct MapOverlayContext<'a> {
    pub(in crate::ui::map_view) crop_bounds: MapTextureBounds,
    pub(in crate::ui::map_view) image_size: Vec2,
    pub(in crate::ui::map_view) player_position: Vec2,
    pub(in crate::ui::map_view) sim: &'a Simulation,
    pub(in crate::ui::map_view) settings: &'a MapDisplaySettings,
    pub(in crate::ui::map_view) camera_rect: Option<MapTileRect>,
    pub(in crate::ui::map_view) chunk_cursor: Option<ChunkCoord>,
    pub(in crate::ui::map_view) markers: &'a MapOverlayMarkers,
}

pub(in crate::ui::map_view) fn reconcile_map_overlay(
    commands: &mut Commands,
    overlay_root: Entity,
    details: &mut MapDetailCache,
    changed_layers: [bool; MapOverlayLayer::ALL.len()],
    context: MapOverlayContext,
) {
    for (layer_index, layer) in MapOverlayLayer::ALL.into_iter().enumerate() {
        if !changed_layers[layer_index] {
            continue;
        }

        let mut desired = Vec::new();
        match layer {
            MapOverlayLayer::Navigation => spawn_navigation_overlays(&mut desired, &context),
            MapOverlayLayer::Pollution => spawn_pollution_overlays(&mut desired, &context),
            MapOverlayLayer::Entities => spawn_entity_overlays(&mut desired, &context),
            MapOverlayLayer::Power => spawn_power_overlays(&mut desired, &context),
            MapOverlayLayer::ProductionProblems => {
                spawn_production_problem_overlays(&mut desired, &context);
            }
            MapOverlayLayer::Enemies => spawn_threat_overlays(&mut desired, &context),
            MapOverlayLayer::Construction => spawn_construction_overlays(&mut desired, &context),
            MapOverlayLayer::Player => spawn_player_overlay(&mut desired, &context),
            MapOverlayLayer::Markers => spawn_marker_overlays(&mut desired, &context),
        }

        for primitive in &mut desired {
            primitive.z_index = ZIndex(layer_index as i32);
        }
        reconcile_overlay_layer(
            commands,
            overlay_root,
            details.layer_entities_mut(overlay_root, layer),
            desired,
        );
    }
}

fn spawn_navigation_overlays(overlays: &mut Vec<MapOverlayPrimitive>, context: &MapOverlayContext) {
    if let Some(rect) = context
        .camera_rect
        .and_then(|rect| map_rect_for_world_rect(context.crop_bounds, context.image_size, rect))
    {
        spawn_rect_overlay(
            overlays,
            rect,
            Color::srgba(0.98, 0.92, 0.55, 0.96),
            Color::srgba(0.98, 0.92, 0.55, 0.10),
            2.0,
        );
    }

    if let Some(coord) = context.chunk_cursor
        && let Some(rect) = map_rect_for_chunk(context.crop_bounds, context.image_size, coord)
    {
        spawn_rect_overlay(
            overlays,
            rect,
            Color::srgba(0.42, 0.88, 1.0, 0.95),
            Color::srgba(0.20, 0.66, 0.82, 0.16),
            2.0,
        );
    }
}

fn spawn_player_overlay(overlays: &mut Vec<MapOverlayPrimitive>, context: &MapOverlayContext) {
    spawn_point_overlay(
        overlays,
        context.crop_bounds,
        context.image_size,
        context.player_position,
        MAP_PLAYER_MARKER_SIZE,
        Color::srgba(0.98, 0.96, 0.74, 0.98),
        Color::srgba(0.02, 0.02, 0.018, 0.95),
    );
}

fn spawn_marker_overlays(overlays: &mut Vec<MapOverlayPrimitive>, context: &MapOverlayContext) {
    for marker in &context.markers.pings {
        spawn_point_overlay(
            overlays,
            context.crop_bounds,
            context.image_size,
            marker.position,
            MAP_PING_MARKER_SIZE,
            Color::NONE,
            marker.color,
        );
    }

    for marker in &context.markers.waypoints {
        spawn_point_overlay(
            overlays,
            context.crop_bounds,
            context.image_size,
            marker.position,
            MAP_WAYPOINT_MARKER_SIZE,
            marker.color,
            Color::srgba(0.02, 0.02, 0.018, 0.92),
        );
    }
}

fn reconcile_overlay_layer(
    commands: &mut Commands,
    overlay_root: Entity,
    entities: &mut Vec<Entity>,
    desired: Vec<MapOverlayPrimitive>,
) {
    let desired_len = desired.len();
    let retained = entities.len().min(desired.len());
    for (entity, primitive) in entities.iter().copied().zip(desired.iter()).take(retained) {
        commands.entity(entity).insert(primitive.clone());
    }
    for primitive in desired.into_iter().skip(retained) {
        let entity = commands.spawn(primitive).id();
        commands.entity(overlay_root).add_child(entity);
        entities.push(entity);
    }
    for entity in entities.drain(desired_len..) {
        commands.entity(entity).despawn();
    }
}
