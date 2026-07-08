use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{CHUNK_SIZE, ChunkCoord, EntityFootprint, Simulation};

use crate::constants::TILE_SIZE;
use crate::rendering::entities::entity_prototype_render_style;
use crate::map::resources::{
    MapDisplaySettings, MapLayer, MapOverlayMarkers, MapTextureBounds, MapTextureCache,
    MapViewState,
};
use crate::resources::SimResource;

const MINIMAP_FRAME_SIZE: f32 = 184.0;
const MINIMAP_PADDING: f32 = 4.0;
const MINIMAP_CONTENT_SIZE: f32 = MINIMAP_FRAME_SIZE - MINIMAP_PADDING * 2.0;
const MINIMAP_VIEW_TILES: u32 = 128;
const MAP_PLAYER_MARKER_SIZE: f32 = 9.0;
const MAP_PING_MARKER_SIZE: f32 = 13.0;
const MAP_WAYPOINT_MARKER_SIZE: f32 = 9.0;
pub const FULL_MAP_BASELINE_VIEW_HEIGHT: f32 = 256.0;
pub const FULL_MAP_MIN_ZOOM: f32 = 0.25;
pub const FULL_MAP_MAX_ZOOM: f32 = 8.0;

#[derive(Component)]
pub(crate) struct MinimapRoot;

#[derive(Component)]
pub(crate) struct MinimapImage;

#[derive(Component)]
pub(crate) struct MinimapOverlayRoot;

#[derive(Component)]
pub(crate) struct FullMapRoot;

#[derive(Component)]
pub(crate) struct FullMapImage;

#[derive(Component)]
pub(crate) struct FullMapOverlayRoot;

#[derive(Component)]
pub(crate) struct FullMapLayerButton {
    pub(crate) layer: MapLayer,
}

#[derive(Component)]
pub(crate) struct FullMapRecenterButton;

type FullMapLayerInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static FullMapLayerButton),
    (
        Changed<Interaction>,
        With<Button>,
        Without<FullMapRecenterButton>,
    ),
>;

type FullMapRecenterInteractionQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<FullMapRecenterButton>,
    ),
>;

pub(crate) fn handle_full_map_buttons(
    mut layer_buttons: FullMapLayerInteractionQuery,
    mut recenter_buttons: FullMapRecenterInteractionQuery,
    sim: Res<SimResource>,
    mut state: ResMut<MapViewState>,
) {
    if !state.open {
        return;
    }

    for (interaction, button) in &mut layer_buttons {
        if *interaction == Interaction::Pressed {
            state.selected_layer = button.layer;
        }
    }

    for interaction in &mut recenter_buttons {
        if *interaction == Interaction::Pressed {
            let (x, y) = sim.sim.player().position_tiles();
            state.center_tile = Vec2::new(x, y);
            state.follow_player = true;
        }
    }
}

#[derive(SystemParam)]
pub(crate) struct MinimapSyncParams<'w, 's> {
    cache: Res<'w, MapTextureCache>,
    sim: Res<'w, SimResource>,
    settings: Res<'w, MapDisplaySettings>,
    markers: Res<'w, MapOverlayMarkers>,
    roots: Query<'w, 's, Entity, With<MinimapRoot>>,
    images: Query<'w, 's, &'static mut ImageNode, With<MinimapImage>>,
    overlay_roots: Query<'w, 's, Entity, With<MinimapOverlayRoot>>,
    cameras: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<Camera2d>>,
}

pub(crate) fn sync_minimap(mut commands: Commands, mut params: MinimapSyncParams) {
    let Some(surface_cache) = params.cache.surface() else {
        return;
    };
    let Some(handle) = surface_cache.handle.as_ref() else {
        return;
    };
    let Some(map_bounds) = surface_cache.bounds else {
        return;
    };
    let player = params.sim.sim.player();
    let (player_x, player_y) = player.position_tiles();
    let crop_bounds = minimap_crop_bounds(map_bounds, Vec2::new(player_x, player_y));
    let texture_rect = texture_rect_for_world_bounds(map_bounds, crop_bounds);

    let mut roots_iter = params.roots.iter();
    let Some(_root) = roots_iter.next() else {
        spawn_minimap(&mut commands, handle.clone(), texture_rect);
        return;
    };
    for duplicate in roots_iter {
        commands.entity(duplicate).despawn();
    }

    for mut image in &mut params.images {
        image.image = handle.clone();
        image.rect = Some(texture_rect);
    }

    let camera_rect = camera_tile_rect(&params.cameras);
    for overlay_root in &params.overlay_roots {
        rebuild_map_overlay(
            &mut commands,
            overlay_root,
            MapOverlayContext {
                crop_bounds,
                image_size: Vec2::splat(MINIMAP_CONTENT_SIZE),
                player_position: Vec2::new(player_x, player_y),
                sim: &params.sim.sim,
                settings: &params.settings,
                layer: MapLayer::Surface,
                camera_rect,
                chunk_cursor: None,
                markers: &params.markers,
            },
        );
    }
}

fn spawn_overlay_root(
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

#[derive(Clone, Copy)]
struct MapTileRect {
    min: Vec2,
    max: Vec2,
}

#[derive(Clone, Copy)]
struct MapUiRect {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

struct MapOverlayContext<'a> {
    crop_bounds: MapTextureBounds,
    image_size: Vec2,
    player_position: Vec2,
    sim: &'a Simulation,
    settings: &'a MapDisplaySettings,
    layer: MapLayer,
    camera_rect: Option<MapTileRect>,
    chunk_cursor: Option<ChunkCoord>,
    markers: &'a MapOverlayMarkers,
}

fn rebuild_map_overlay(commands: &mut Commands, overlay_root: Entity, context: MapOverlayContext) {
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

    let max_x = context.crop_bounds.min_x + context.crop_bounds.width as i32 - 1;
    let max_y = context.crop_bounds.min_y + context.crop_bounds.height as i32 - 1;
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
            sim.is_chunk_revealed(ChunkCoord {
                x: x.div_euclid(CHUNK_SIZE),
                y: y.div_euclid(CHUNK_SIZE),
            })
        })
}

fn map_rect_for_footprint(
    crop_bounds: MapTextureBounds,
    image_size: Vec2,
    footprint: EntityFootprint,
) -> Option<MapUiRect> {
    map_rect_for_world_rect(
        crop_bounds,
        image_size,
        MapTileRect {
            min: Vec2::new(footprint.x as f32, footprint.y as f32),
            max: Vec2::new(
                (footprint.x + footprint.width) as f32,
                (footprint.y + footprint.height) as f32,
            ),
        },
    )
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

fn map_rect_for_chunk(
    crop_bounds: MapTextureBounds,
    image_size: Vec2,
    coord: ChunkCoord,
) -> Option<MapUiRect> {
    let min = Vec2::new((coord.x * CHUNK_SIZE) as f32, (coord.y * CHUNK_SIZE) as f32);
    map_rect_for_world_rect(
        crop_bounds,
        image_size,
        MapTileRect {
            min,
            max: min + Vec2::splat(CHUNK_SIZE as f32),
        },
    )
}

fn map_rect_for_world_rect(
    crop_bounds: MapTextureBounds,
    image_size: Vec2,
    rect: MapTileRect,
) -> Option<MapUiRect> {
    if crop_bounds.width == 0 || crop_bounds.height == 0 {
        return None;
    }

    let crop_min = Vec2::new(crop_bounds.min_x as f32, crop_bounds.min_y as f32);
    let crop_max = crop_min + Vec2::new(crop_bounds.width as f32, crop_bounds.height as f32);
    let min_x = rect.min.x.max(crop_min.x);
    let max_x = rect.max.x.min(crop_max.x);
    let min_y = rect.min.y.max(crop_min.y);
    let max_y = rect.max.y.min(crop_max.y);
    if min_x >= max_x || min_y >= max_y {
        return None;
    }

    let left = (min_x - crop_min.x) / crop_bounds.width as f32 * image_size.x;
    let right = (max_x - crop_min.x) / crop_bounds.width as f32 * image_size.x;
    let top = (crop_max.y - max_y) / crop_bounds.height as f32 * image_size.y;
    let bottom = (crop_max.y - min_y) / crop_bounds.height as f32 * image_size.y;

    Some(MapUiRect {
        left,
        top,
        width: right - left,
        height: bottom - top,
    })
}

fn spawn_minimap(commands: &mut Commands, handle: Handle<Image>, texture_rect: Rect) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(14.0),
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

#[derive(SystemParam)]
pub(crate) struct FullMapSyncParams<'w, 's> {
    cache: Res<'w, MapTextureCache>,
    state: Res<'w, MapViewState>,
    sim: Res<'w, SimResource>,
    settings: Res<'w, MapDisplaySettings>,
    markers: Res<'w, MapOverlayMarkers>,
    roots: Query<'w, 's, Entity, With<FullMapRoot>>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    images: Query<'w, 's, &'static mut ImageNode, With<FullMapImage>>,
    image_nodes: Query<'w, 's, &'static mut Node, With<FullMapImage>>,
    image_layout:
        Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<FullMapImage>>,
    overlay_roots: Query<'w, 's, Entity, With<FullMapOverlayRoot>>,
    cameras: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<Camera2d>>,
    layer_buttons: Query<
        'w,
        's,
        (
            &'static FullMapLayerButton,
            &'static Interaction,
            &'static mut BackgroundColor,
            &'static mut BorderColor,
        ),
        With<Button>,
    >,
}

pub(crate) fn sync_full_map_view(mut commands: Commands, mut params: FullMapSyncParams) {
    if !params.state.open {
        for entity in &params.roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Some(layer_cache) = params.cache.layer(params.state.selected_layer) else {
        return;
    };
    let Some(handle) = layer_cache.handle.as_ref() else {
        return;
    };
    let Some(map_bounds) = layer_cache.bounds else {
        return;
    };
    let image_size = fullscreen_map_image_size(params.windows.iter().next());
    let crop_bounds = fullscreen_crop_bounds(
        map_bounds,
        params.state.center_tile,
        params
            .state
            .zoom
            .clamp(FULL_MAP_MIN_ZOOM, FULL_MAP_MAX_ZOOM),
        image_size,
    );
    let display_size = fullscreen_map_display_size(image_size, crop_bounds);
    let texture_rect = texture_rect_for_world_bounds(map_bounds, crop_bounds);

    let mut roots_iter = params.roots.iter();
    let Some(_root) = roots_iter.next() else {
        spawn_full_map(
            &mut commands,
            handle.clone(),
            texture_rect,
            display_size,
            params.state.selected_layer,
        );
        return;
    };
    for duplicate in roots_iter {
        commands.entity(duplicate).despawn();
    }

    for mut image in &mut params.images {
        image.image = handle.clone();
        image.rect = Some(texture_rect);
    }
    for mut node in &mut params.image_nodes {
        set_full_map_image_node_size(&mut node, display_size);
    }
    for (button, interaction, mut background, mut border) in &mut params.layer_buttons {
        let selected = button.layer == params.state.selected_layer;
        *background = BackgroundColor(layer_button_color(*interaction, selected));
        *border = BorderColor::all(layer_button_border_color(selected));
    }

    let (player_x, player_y) = params.sim.sim.player().position_tiles();
    let camera_rect = camera_tile_rect(&params.cameras);
    let chunk_cursor = fullscreen_cursor_chunk(crop_bounds, &params.windows, &params.image_layout);
    for overlay_root in &params.overlay_roots {
        rebuild_map_overlay(
            &mut commands,
            overlay_root,
            MapOverlayContext {
                crop_bounds,
                image_size: display_size,
                player_position: Vec2::new(player_x, player_y),
                sim: &params.sim.sim,
                settings: &params.settings,
                layer: params.state.selected_layer,
                camera_rect,
                chunk_cursor,
                markers: &params.markers,
            },
        );
    }
}

fn spawn_full_map(
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

fn set_full_map_image_node_size(node: &mut Node, display_size: Vec2) {
    node.width = Val::Px(display_size.x);
    node.height = Val::Px(display_size.y);
}

fn full_map_image_node(display_size: Vec2) -> Node {
    Node {
        width: Val::Px(display_size.x),
        height: Val::Px(display_size.y),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    }
}

fn spawn_layer_button(
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

fn spawn_recenter_button(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands<'_>) {
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

fn layer_button_color(interaction: Interaction, selected: bool) -> Color {
    if selected {
        return Color::srgba(0.30, 0.28, 0.20, 0.98);
    }

    match interaction {
        Interaction::Pressed => Color::srgba(0.22, 0.20, 0.16, 0.98),
        Interaction::Hovered => Color::srgba(0.17, 0.17, 0.15, 0.98),
        Interaction::None => Color::srgba(0.10, 0.11, 0.11, 0.95),
    }
}

fn layer_button_border_color(selected: bool) -> Color {
    if selected {
        Color::srgba(0.72, 0.60, 0.36, 0.95)
    } else {
        Color::srgba(0.38, 0.42, 0.36, 0.85)
    }
}

fn minimap_crop_bounds(map_bounds: MapTextureBounds, center: Vec2) -> MapTextureBounds {
    let width = map_bounds.width.min(MINIMAP_VIEW_TILES);
    let height = map_bounds.height.min(MINIMAP_VIEW_TILES);

    MapTextureBounds {
        min_x: clamped_window_min(center.x, width, map_bounds.min_x, map_bounds.width),
        min_y: clamped_window_min(center.y, height, map_bounds.min_y, map_bounds.height),
        width,
        height,
    }
}

fn clamped_window_min(center: f32, window_size: u32, bounds_min: i32, bounds_size: u32) -> i32 {
    if bounds_size <= window_size {
        return bounds_min;
    }

    let desired = (center - window_size as f32 * 0.5).floor() as i32;
    let max_min = bounds_min + bounds_size as i32 - window_size as i32;
    desired.clamp(bounds_min, max_min)
}

pub fn fullscreen_crop_bounds(
    map_bounds: MapTextureBounds,
    center_tile: Vec2,
    zoom: f32,
    image_size: Vec2,
) -> MapTextureBounds {
    if map_bounds.width == 0 || map_bounds.height == 0 {
        return map_bounds;
    }

    let (width, height) = fullscreen_view_tile_size(map_bounds, zoom, image_size);
    let center = clamp_map_center(map_bounds, center_tile, zoom, image_size);

    MapTextureBounds {
        min_x: clamped_window_min(center.x, width, map_bounds.min_x, map_bounds.width),
        min_y: clamped_window_min(center.y, height, map_bounds.min_y, map_bounds.height),
        width,
        height,
    }
}

pub fn clamp_map_center(
    map_bounds: MapTextureBounds,
    center_tile: Vec2,
    zoom: f32,
    image_size: Vec2,
) -> Vec2 {
    if map_bounds.width == 0 || map_bounds.height == 0 {
        return Vec2::ZERO;
    }

    let (view_width, view_height) = fullscreen_view_tile_size(map_bounds, zoom, image_size);
    Vec2::new(
        clamp_center_axis(
            center_tile.x,
            map_bounds.min_x,
            map_bounds.width,
            view_width,
        ),
        clamp_center_axis(
            center_tile.y,
            map_bounds.min_y,
            map_bounds.height,
            view_height,
        ),
    )
}

fn fullscreen_view_tile_size(
    map_bounds: MapTextureBounds,
    zoom: f32,
    image_size: Vec2,
) -> (u32, u32) {
    let zoom = zoom.clamp(FULL_MAP_MIN_ZOOM, FULL_MAP_MAX_ZOOM);
    let aspect = if image_size.y > 0.0 {
        (image_size.x / image_size.y).max(0.1)
    } else {
        1.0
    };
    let height =
        ((FULL_MAP_BASELINE_VIEW_HEIGHT / zoom).ceil() as u32).clamp(1, map_bounds.height.max(1));
    let width = ((height as f32 * aspect).ceil() as u32).clamp(1, map_bounds.width.max(1));
    (width, height)
}

fn clamp_center_axis(center: f32, bounds_min: i32, bounds_size: u32, view_size: u32) -> f32 {
    let bounds_min = bounds_min as f32;
    let bounds_max = bounds_min + bounds_size as f32;
    if bounds_size <= view_size {
        return bounds_min + bounds_size as f32 * 0.5;
    }

    let half_view = view_size as f32 * 0.5;
    center.clamp(bounds_min + half_view, bounds_max - half_view)
}

pub fn texture_rect_for_world_bounds(
    map_bounds: MapTextureBounds,
    crop_bounds: MapTextureBounds,
) -> Rect {
    let local_x = crop_bounds.min_x - map_bounds.min_x;
    let local_y = crop_bounds.min_y - map_bounds.min_y;
    let top_y = map_bounds.height as i32 - local_y - crop_bounds.height as i32;

    Rect::new(
        local_x as f32,
        top_y as f32,
        (local_x + crop_bounds.width as i32) as f32,
        (top_y + crop_bounds.height as i32) as f32,
    )
}

pub(crate) fn fullscreen_map_image_size(window: Option<&Window>) -> Vec2 {
    let Some(window) = window else {
        return Vec2::splat(980.0);
    };
    let content_size = (window.resolution.size() - Vec2::splat(56.0)).max(Vec2::splat(1.0));
    let size = content_size * 0.84;
    Vec2::new(size.x.clamp(1.0, 980.0), size.y.clamp(1.0, 980.0))
}

pub(crate) fn fullscreen_map_display_size(
    available_size: Vec2,
    crop_bounds: MapTextureBounds,
) -> Vec2 {
    if crop_bounds.width == 0 || crop_bounds.height == 0 {
        return available_size.max(Vec2::splat(1.0));
    }

    let available_size = available_size.max(Vec2::splat(1.0));
    let crop_aspect = (crop_bounds.width as f32 / crop_bounds.height as f32).max(0.1);
    let available_aspect = available_size.x / available_size.y;
    if available_aspect > crop_aspect {
        Vec2::new(available_size.y * crop_aspect, available_size.y)
    } else {
        Vec2::new(available_size.x, available_size.x / crop_aspect)
    }
}

fn camera_tile_rect(
    cameras: &Query<(&Camera, &GlobalTransform), With<Camera2d>>,
) -> Option<MapTileRect> {
    let (camera, transform) = cameras.iter().next()?;
    let viewport_size = camera.logical_viewport_size()?;
    let first = camera.viewport_to_world_2d(transform, Vec2::ZERO).ok()?;
    let second = camera.viewport_to_world_2d(transform, viewport_size).ok()?;
    Some(MapTileRect {
        min: Vec2::new(first.x.min(second.x), first.y.min(second.y)) / TILE_SIZE,
        max: Vec2::new(first.x.max(second.x), first.y.max(second.y)) / TILE_SIZE,
    })
}

fn fullscreen_cursor_chunk(
    crop_bounds: MapTextureBounds,
    windows: &Query<&Window, With<PrimaryWindow>>,
    image_layout: &Query<(&ComputedNode, &UiGlobalTransform), With<FullMapImage>>,
) -> Option<ChunkCoord> {
    let cursor_position = windows.single().ok()?.physical_cursor_position()?;
    let crop_max_y = crop_bounds.min_y as f32 + crop_bounds.height as f32;

    for (node, transform) in image_layout {
        let size = node.size();
        if size.x <= 0.0 || size.y <= 0.0 {
            continue;
        }
        let local_position = transform.try_inverse()?.transform_point2(cursor_position);
        let half_size = size * 0.5;
        if local_position.x < -half_size.x
            || local_position.x > half_size.x
            || local_position.y < -half_size.y
            || local_position.y > half_size.y
        {
            continue;
        }

        let normalized = (local_position + half_size) / size;
        let tile_x = crop_bounds.min_x as f32 + normalized.x * crop_bounds.width as f32;
        let tile_y = crop_max_y - normalized.y * crop_bounds.height as f32;
        return Some(ChunkCoord {
            x: (tile_x.floor() as i32).div_euclid(CHUNK_SIZE),
            y: (tile_y.floor() as i32).div_euclid(CHUNK_SIZE),
        });
    }

    None
}

fn map_point_for_world_position(
    crop_bounds: MapTextureBounds,
    image_size: Vec2,
    position: Vec2,
) -> Option<Vec2> {
    if crop_bounds.width == 0 || crop_bounds.height == 0 {
        return None;
    }

    let crop_min_x = crop_bounds.min_x as f32;
    let crop_min_y = crop_bounds.min_y as f32;
    let crop_max_x = crop_min_x + crop_bounds.width as f32;
    let crop_max_y = crop_min_y + crop_bounds.height as f32;
    if position.x < crop_min_x
        || position.x > crop_max_x
        || position.y < crop_min_y
        || position.y > crop_max_y
    {
        return None;
    }

    Some(Vec2::new(
        (position.x - crop_min_x) / crop_bounds.width as f32 * image_size.x,
        (crop_max_y - position.y) / crop_bounds.height as f32 * image_size.y,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimap_crop_stays_local_when_world_is_large() {
        let map = MapTextureBounds {
            min_x: -256,
            min_y: -256,
            width: 512,
            height: 512,
        };

        let crop = minimap_crop_bounds(map, Vec2::new(0.5, 0.5));

        assert_eq!(crop.width, MINIMAP_VIEW_TILES);
        assert_eq!(crop.height, MINIMAP_VIEW_TILES);
        assert_eq!(crop.min_x, -64);
        assert_eq!(crop.min_y, -64);
    }

    #[test]
    fn minimap_texture_rect_flips_world_y_to_image_space() {
        let map = MapTextureBounds {
            min_x: -64,
            min_y: -64,
            width: 256,
            height: 256,
        };
        let crop = MapTextureBounds {
            min_x: -32,
            min_y: 16,
            width: 128,
            height: 128,
        };

        let rect = texture_rect_for_world_bounds(map, crop);

        assert_eq!(rect.min, Vec2::new(32.0, 48.0));
        assert_eq!(rect.max, Vec2::new(160.0, 176.0));
    }

    #[test]
    fn map_overlay_point_flips_world_y_to_ui_space() {
        let crop = MapTextureBounds {
            min_x: -32,
            min_y: 16,
            width: 128,
            height: 64,
        };

        let point =
            map_point_for_world_position(crop, Vec2::new(256.0, 128.0), Vec2::new(32.0, 64.0))
                .expect("point should be inside crop");

        assert_eq!(point, Vec2::new(128.0, 32.0));
    }

    #[test]
    fn fullscreen_map_display_size_keeps_square_crop_square() {
        let crop = MapTextureBounds {
            min_x: -256,
            min_y: -256,
            width: 512,
            height: 512,
        };

        let display_size = fullscreen_map_display_size(Vec2::new(980.0, 860.0), crop);

        assert_eq!(display_size, Vec2::splat(860.0));
    }

    #[test]
    fn fullscreen_map_display_size_fits_wide_crop_inside_available_area() {
        let crop = MapTextureBounds {
            min_x: 0,
            min_y: 0,
            width: 300,
            height: 100,
        };

        let display_size = fullscreen_map_display_size(Vec2::new(980.0, 860.0), crop);

        assert_eq!(display_size, Vec2::new(980.0, 980.0 / 3.0));
    }
}
