use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::resources::{MapLayer, MapTextureBounds, MapTextureCache, MapViewState, SimResource};

const MINIMAP_FRAME_SIZE: f32 = 184.0;
const MINIMAP_PADDING: f32 = 4.0;
const MINIMAP_CONTENT_SIZE: f32 = MINIMAP_FRAME_SIZE - MINIMAP_PADDING * 2.0;
const MINIMAP_VIEW_TILES: u32 = 128;
const MINIMAP_PLAYER_MARKER_SIZE: f32 = 7.0;
pub const FULL_MAP_BASELINE_VIEW_HEIGHT: f32 = 256.0;
pub const FULL_MAP_MIN_ZOOM: f32 = 0.25;
pub const FULL_MAP_MAX_ZOOM: f32 = 8.0;

#[derive(Component)]
pub(crate) struct MinimapRoot;

#[derive(Component)]
pub(crate) struct MinimapImage;

#[derive(Component)]
pub(crate) struct MinimapPlayerMarker;

#[derive(Component)]
pub(crate) struct FullMapRoot;

#[derive(Component)]
pub(crate) struct FullMapImage;

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

pub(crate) fn sync_minimap(
    mut commands: Commands,
    cache: Res<MapTextureCache>,
    sim: Res<SimResource>,
    roots: Query<Entity, With<MinimapRoot>>,
    mut images: Query<&mut ImageNode, With<MinimapImage>>,
    mut player_markers: Query<&mut Node, With<MinimapPlayerMarker>>,
) {
    let Some(handle) = cache.handle.as_ref() else {
        return;
    };
    let Some(map_bounds) = cache.bounds else {
        return;
    };
    let player = sim.sim.player();
    let (player_x, player_y) = player.position_tiles();
    let crop_bounds = minimap_crop_bounds(map_bounds, Vec2::new(player_x, player_y));
    let texture_rect = texture_rect_for_world_bounds(map_bounds, crop_bounds);

    let mut roots_iter = roots.iter();
    let Some(_root) = roots_iter.next() else {
        spawn_minimap(&mut commands, handle.clone(), texture_rect);
        return;
    };
    for duplicate in roots_iter {
        commands.entity(duplicate).despawn();
    }

    for mut image in &mut images {
        image.image = handle.clone();
        image.rect = Some(texture_rect);
    }

    update_player_marker(
        &mut player_markers,
        crop_bounds,
        Vec2::new(player_x, player_y),
    );
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
                map.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        display: Display::None,
                        width: Val::Px(MINIMAP_PLAYER_MARKER_SIZE),
                        height: Val::Px(MINIMAP_PLAYER_MARKER_SIZE),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.96, 0.94, 0.78, 0.96)),
                    BorderColor::all(Color::srgba(0.02, 0.02, 0.018, 0.92)),
                    MinimapPlayerMarker,
                ));
            });
        });
}

pub(crate) fn sync_full_map_view(
    mut commands: Commands,
    cache: Res<MapTextureCache>,
    state: Res<MapViewState>,
    roots: Query<Entity, With<FullMapRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut images: Query<&mut ImageNode, With<FullMapImage>>,
    mut layer_buttons: Query<
        (
            &FullMapLayerButton,
            &Interaction,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<Button>,
    >,
) {
    if !state.open {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Some(layer_cache) = cache.layer_caches.get(&state.selected_layer) else {
        return;
    };
    let Some(handle) = layer_cache.handle.as_ref() else {
        return;
    };
    let Some(map_bounds) = layer_cache.bounds else {
        return;
    };
    let image_size = fullscreen_map_image_size(windows.iter().next());
    let crop_bounds = fullscreen_crop_bounds(
        map_bounds,
        state.center_tile,
        state.zoom.clamp(FULL_MAP_MIN_ZOOM, FULL_MAP_MAX_ZOOM),
        image_size,
    );
    let texture_rect = texture_rect_for_world_bounds(map_bounds, crop_bounds);

    let mut roots_iter = roots.iter();
    let Some(_root) = roots_iter.next() else {
        spawn_full_map(
            &mut commands,
            handle.clone(),
            texture_rect,
            state.selected_layer,
        );
        return;
    };
    for duplicate in roots_iter {
        commands.entity(duplicate).despawn();
    }

    for mut image in &mut images {
        image.image = handle.clone();
        image.rect = Some(texture_rect);
    }
    for (button, interaction, mut background, mut border) in &mut layer_buttons {
        let selected = button.layer == state.selected_layer;
        *background = BackgroundColor(layer_button_color(*interaction, selected));
        *border = BorderColor::all(layer_button_border_color(selected));
    }
}

fn spawn_full_map(
    commands: &mut Commands,
    handle: Handle<Image>,
    texture_rect: Rect,
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
                Node {
                    width: Val::Percent(84.0),
                    height: Val::Percent(84.0),
                    max_width: Val::Px(980.0),
                    max_height: Val::Px(980.0),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.42, 0.43, 0.39, 0.9)),
                FullMapImage,
            ));
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

fn update_player_marker(
    markers: &mut Query<&mut Node, With<MinimapPlayerMarker>>,
    crop_bounds: MapTextureBounds,
    player_position: Vec2,
) {
    let Some(position) = minimap_point_for_world_position(crop_bounds, player_position) else {
        for mut marker in markers {
            marker.display = Display::None;
        }
        return;
    };

    for mut marker in markers {
        marker.display = Display::Flex;
        marker.left = Val::Px(position.x - MINIMAP_PLAYER_MARKER_SIZE * 0.5);
        marker.top = Val::Px(position.y - MINIMAP_PLAYER_MARKER_SIZE * 0.5);
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

fn minimap_point_for_world_position(crop_bounds: MapTextureBounds, position: Vec2) -> Option<Vec2> {
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
        (position.x - crop_min_x) / crop_bounds.width as f32 * MINIMAP_CONTENT_SIZE,
        (crop_max_y - position.y) / crop_bounds.height as f32 * MINIMAP_CONTENT_SIZE,
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
}
