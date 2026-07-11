use bevy::prelude::*;
use factory_sim::{CHUNK_SIZE, ChunkCoord, EntityFootprint, Simulation};

use crate::map::resources::{MapDisplaySettings, MapLayer, MapOverlayMarkers, MapTextureBounds};
use crate::rendering::entities::entity_prototype_render_style;

use super::components::{
    FullMapImage, FullMapLayerButton, FullMapOverlayRoot, FullMapRecenterButton, FullMapRoot,
    MinimapImage, MinimapOverlayRoot, MinimapRoot,
};
use super::layout::{
    MapTileRect, MapUiRect, map_point_for_world_position, map_rect_for_chunk,
    map_rect_for_footprint, map_rect_for_world_rect,
};

pub(crate) const MINIMAP_FRAME_SIZE: f32 = 184.0;
pub(crate) const MINIMAP_RIGHT_OFFSET: f32 = 14.0;
const MINIMAP_PADDING: f32 = 4.0;
pub(super) const MINIMAP_CONTENT_SIZE: f32 = MINIMAP_FRAME_SIZE - MINIMAP_PADDING * 2.0;
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

pub(super) struct MapOverlayContext<'a> {
    pub(super) crop_bounds: MapTextureBounds,
    pub(super) image_size: Vec2,
    pub(super) player_position: Vec2,
    pub(super) sim: &'a Simulation,
    pub(super) settings: &'a MapDisplaySettings,
    pub(super) layer: MapLayer,
    pub(super) camera_rect: Option<MapTileRect>,
    pub(super) chunk_cursor: Option<ChunkCoord>,
    pub(super) markers: &'a MapOverlayMarkers,
}

pub(super) fn rebuild_map_overlay(
    commands: &mut Commands,
    overlay_root: Entity,
    context: MapOverlayContext,
) {
    commands
        .entity(overlay_root)
        .despawn_related::<Children>()
        .with_children(|overlay| {
            if let Some(rect) = context.camera_rect.and_then(|rect| {
                map_rect_for_world_rect(context.crop_bounds, context.image_size, rect)
            }) {
                spawn_rect_overlay(
                    overlay,
                    rect,
                    Color::srgba(0.98, 0.92, 0.55, 0.96),
                    Color::srgba(0.98, 0.92, 0.55, 0.10),
                    2.0,
                );
            }

            if let Some(coord) = context.chunk_cursor
                && let Some(rect) =
                    map_rect_for_chunk(context.crop_bounds, context.image_size, coord)
            {
                spawn_rect_overlay(
                    overlay,
                    rect,
                    Color::srgba(0.42, 0.88, 1.0, 0.95),
                    Color::srgba(0.20, 0.66, 0.82, 0.16),
                    2.0,
                );
            }

            spawn_pollution_overlays(overlay, &context);
            spawn_entity_overlays(overlay, &context);

            spawn_point_overlay(
                overlay,
                context.crop_bounds,
                context.image_size,
                context.player_position,
                MAP_PLAYER_MARKER_SIZE,
                Color::srgba(0.98, 0.96, 0.74, 0.98),
                Color::srgba(0.02, 0.02, 0.018, 0.95),
            );

            for marker in &context.markers.pings {
                spawn_point_overlay(
                    overlay,
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
                    overlay,
                    context.crop_bounds,
                    context.image_size,
                    marker.position,
                    MAP_WAYPOINT_MARKER_SIZE,
                    marker.color,
                    Color::srgba(0.02, 0.02, 0.018, 0.92),
                );
            }
        });
}

/// Red haze over polluted revealed chunks; opacity scales with the chunk's
/// pollution level.
fn spawn_pollution_overlays(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands<'_>,
    context: &MapOverlayContext,
) {
    // Below this level the haze would be invisible anyway; skip the rect.
    const MIN_VISIBLE_POLLUTION_MICRO: u64 = 100_000;
    // Pollution level rendered at full haze opacity (10 pollution units).
    const FULL_HAZE_POLLUTION_MICRO: u64 = 10_000_000;

    if context.layer == MapLayer::Resources {
        return;
    }

    for (coord, amount_micro) in context.sim.pollution().polluted_chunks() {
        if amount_micro < MIN_VISIBLE_POLLUTION_MICRO {
            continue;
        }
        if !context.settings.debug_reveal_all && !context.sim.is_chunk_revealed(coord) {
            continue;
        }
        let Some(rect) = map_rect_for_chunk(context.crop_bounds, context.image_size, coord) else {
            continue;
        };

        let strength = (amount_micro as f32 / FULL_HAZE_POLLUTION_MICRO as f32).clamp(0.06, 1.0);
        spawn_rect_overlay(
            parent,
            rect,
            Color::NONE,
            Color::srgba(0.82, 0.20, 0.16, 0.30 * strength),
            0.0,
        );
    }
}

fn spawn_entity_overlays(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands<'_>,
    context: &MapOverlayContext,
) {
    if context.layer == MapLayer::Resources
        || context.crop_bounds.width == 0
        || context.crop_bounds.height == 0
    {
        return;
    }

    let max_x = context.crop_bounds.min_x + i64::from(context.crop_bounds.width) - 1;
    let max_y = context.crop_bounds.min_y + i64::from(context.crop_bounds.height) - 1;
    for entity_id in context.sim.entities().occupancy().entity_ids_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    ) {
        let Some(placed) = context.sim.entities().placed_entity(entity_id) else {
            continue;
        };
        if !entity_footprint_is_visible(context.sim, context.settings, placed.footprint) {
            continue;
        }
        let Some((color, _)) = entity_prototype_render_style(
            context.sim.catalog(),
            placed.prototype_id,
            placed.direction,
        ) else {
            continue;
        };
        let Some(rect) =
            map_rect_for_footprint(context.crop_bounds, context.image_size, placed.footprint)
        else {
            continue;
        };

        spawn_rect_overlay(
            parent,
            rect,
            map_color_with_alpha(color, 0.96),
            map_color_with_alpha(color, 0.38),
            1.0,
        );
    }
}

fn entity_footprint_is_visible(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    footprint: EntityFootprint,
) -> bool {
    settings.debug_reveal_all
        || footprint.tiles().into_iter().any(|(x, y)| {
            ChunkCoord::from_tile(x, y).is_some_and(|coord| sim.is_chunk_revealed(coord))
        })
}

fn map_color_with_alpha(color: Color, alpha: f32) -> Color {
    let srgba = color.to_srgba();
    Color::srgba(srgba.red, srgba.green, srgba.blue, alpha)
}

fn spawn_point_overlay(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands<'_>,
    crop_bounds: MapTextureBounds,
    image_size: Vec2,
    position: Vec2,
    size: f32,
    fill: Color,
    border: Color,
) {
    let Some(position) = map_point_for_world_position(crop_bounds, image_size, position) else {
        return;
    };

    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(position.x - size * 0.5),
            top: Val::Px(position.y - size * 0.5),
            width: Val::Px(size),
            height: Val::Px(size),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(border),
    ));
}

fn spawn_rect_overlay(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands<'_>,
    rect: MapUiRect,
    border: Color,
    fill: Color,
    border_width: f32,
) {
    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(rect.left),
            top: Val::Px(rect.top),
            width: Val::Px(rect.width.max(border_width)),
            height: Val::Px(rect.height.max(border_width)),
            border: UiRect::all(Val::Px(border_width)),
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(border),
    ));
}

pub(super) fn spawn_minimap(commands: &mut Commands, handle: Handle<Image>, texture_rect: Rect) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(MINIMAP_RIGHT_OFFSET),
                top: Val::Px(14.0),
                width: Val::Px(MINIMAP_FRAME_SIZE),
                height: Val::Px(MINIMAP_FRAME_SIZE),
                padding: UiRect::all(Val::Px(MINIMAP_PADDING)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.025, 0.027, 0.88)),
            BorderColor::all(Color::srgba(0.36, 0.38, 0.34, 0.82)),
            GlobalZIndex(1800),
            MinimapRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    position_type: PositionType::Relative,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::BLACK),
            ))
            .with_children(|map| {
                map.spawn((
                    ImageNode {
                        image: handle,
                        rect: Some(texture_rect),
                        image_mode: NodeImageMode::Stretch,
                        ..default()
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    MinimapImage,
                ));
                spawn_overlay_root(map, MinimapOverlayRoot);
            });
        });
}

pub(super) fn spawn_full_map(
    commands: &mut Commands,
    handle: Handle<Image>,
    texture_rect: Rect,
    display_size: Vec2,
    selected_layer: MapLayer,
) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(Val::Px(28.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.017, 0.018, 0.96)),
            GlobalZIndex(2200),
            FullMapRoot,
        ))
        .with_children(|root| {
            root.spawn((
                ImageNode {
                    image: handle,
                    rect: Some(texture_rect),
                    image_mode: NodeImageMode::Stretch,
                    ..default()
                },
                full_map_image_node(display_size),
                BorderColor::all(Color::srgba(0.42, 0.43, 0.39, 0.9)),
                FullMapImage,
            ))
            .with_children(|image| {
                spawn_overlay_root(image, FullMapOverlayRoot);
            });
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(28.0),
                    top: Val::Px(24.0),
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|bar| {
                spawn_layer_button(bar, MapLayer::Surface, "Surface", selected_layer);
                spawn_layer_button(bar, MapLayer::Resources, "Resources", selected_layer);
                spawn_layer_button(bar, MapLayer::Entities, "Entities", selected_layer);
                spawn_recenter_button(bar);
            });
        });
}

pub(super) fn set_full_map_image_node_size(node: &mut Node, display_size: Vec2) {
    node.width = Val::Px(display_size.x);
    node.height = Val::Px(display_size.y);
}

pub(super) fn full_map_image_node(display_size: Vec2) -> Node {
    Node {
        width: Val::Px(display_size.x),
        height: Val::Px(display_size.y),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    }
}

pub(super) fn spawn_layer_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands<'_>,
    layer: MapLayer,
    label: &'static str,
    selected_layer: MapLayer,
) {
    let selected = layer == selected_layer;
    parent
        .spawn((
            Button,
            Node {
                height: Val::Px(34.0),
                padding: UiRect::axes(Val::Px(13.0), Val::Px(0.0)),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(layer_button_color(Interaction::None, selected)),
            BorderColor::all(layer_button_border_color(selected)),
            FullMapLayerButton { layer },
        ))
        .with_child((
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(13.0),
                ..default()
            },
            TextColor(Color::srgba(0.90, 0.88, 0.80, 0.96)),
        ));
}

pub(super) fn spawn_recenter_button(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands<'_>) {
    parent
        .spawn((
            Button,
            Node {
                height: Val::Px(34.0),
                padding: UiRect::axes(Val::Px(13.0), Val::Px(0.0)),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.13, 0.14, 0.13, 0.95)),
            BorderColor::all(Color::srgba(0.38, 0.42, 0.36, 0.85)),
            FullMapRecenterButton,
        ))
        .with_child((
            Text::new("Center"),
            TextFont {
                font_size: FontSize::Px(13.0),
                ..default()
            },
            TextColor(Color::srgba(0.90, 0.88, 0.80, 0.96)),
        ));
}

pub(super) fn layer_button_color(interaction: Interaction, selected: bool) -> Color {
    if selected {
        return Color::srgba(0.30, 0.28, 0.20, 0.98);
    }

    match interaction {
        Interaction::Pressed => Color::srgba(0.22, 0.20, 0.16, 0.98),
        Interaction::Hovered => Color::srgba(0.17, 0.17, 0.15, 0.98),
        Interaction::None => Color::srgba(0.10, 0.11, 0.11, 0.95),
    }
}

pub(super) fn layer_button_border_color(selected: bool) -> Color {
    if selected {
        Color::srgba(0.72, 0.60, 0.36, 0.95)
    } else {
        Color::srgba(0.38, 0.42, 0.36, 0.85)
    }
}
