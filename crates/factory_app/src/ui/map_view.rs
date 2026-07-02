use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::constants::TILE_SIZE;
use crate::resources::{MapTextureBounds, MapTextureCache, MapViewState, SimResource};

const MINIMAP_FRAME_SIZE: f32 = 184.0;
const MINIMAP_PADDING: f32 = 4.0;
const MINIMAP_CONTENT_SIZE: f32 = MINIMAP_FRAME_SIZE - MINIMAP_PADDING * 2.0;
const MINIMAP_VIEW_TILES: u32 = 128;
const MINIMAP_PLAYER_MARKER_SIZE: f32 = 7.0;
const MINIMAP_MIN_VIEWPORT_SIZE: f32 = 2.0;

#[derive(Component)]
pub(crate) struct MinimapRoot;

#[derive(Component)]
pub(crate) struct MinimapImage;

#[derive(Component)]
pub(crate) struct MinimapPlayerMarker;

#[derive(Component)]
pub(crate) struct MinimapViewportMarker;

#[derive(Component)]
pub(crate) struct FullMapRoot;

#[derive(SystemParam)]
pub(crate) struct MinimapNodeQueries<'w, 's> {
    images: Query<'w, 's, &'static mut ImageNode, With<MinimapImage>>,
    player_markers: Query<'w, 's, &'static mut Node, With<MinimapPlayerMarker>>,
    viewport_markers: Query<
        'w,
        's,
        &'static mut Node,
        (With<MinimapViewportMarker>, Without<MinimapPlayerMarker>),
    >,
}

pub(crate) fn sync_minimap(
    mut commands: Commands,
    cache: Res<MapTextureCache>,
    sim: Res<SimResource>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    roots: Query<Entity, With<MinimapRoot>>,
    mut minimap_nodes: MinimapNodeQueries,
) {
    let Some(handle) = cache.handle.as_ref() else {
        return;
    };
    let Some(map_bounds) = cache.bounds else {
        return;
    };
    let mut roots_iter = roots.iter();
    let Some(_root) = roots_iter.next() else {
        spawn_minimap(&mut commands, handle.clone());
        return;
    };
    for duplicate in roots_iter {
        commands.entity(duplicate).despawn();
    }

    let player = sim.sim.player();
    let (player_x, player_y) = player.position_tiles();
    let crop_bounds = minimap_crop_bounds(map_bounds, Vec2::new(player_x, player_y));
    let texture_rect = texture_rect_for_world_bounds(map_bounds, crop_bounds);

    for mut image in &mut minimap_nodes.images {
        image.image = handle.clone();
        image.rect = Some(texture_rect);
    }

    update_player_marker(
        &mut minimap_nodes.player_markers,
        crop_bounds,
        Vec2::new(player_x, player_y),
    );

    let camera_rect = camera_world_rect(&cameras);
    update_viewport_marker(
        &mut minimap_nodes.viewport_markers,
        crop_bounds,
        camera_rect,
    );
}

fn spawn_minimap(commands: &mut Commands, handle: Handle<Image>) {
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
                        width: Val::Px(MINIMAP_PLAYER_MARKER_SIZE),
                        height: Val::Px(MINIMAP_PLAYER_MARKER_SIZE),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.96, 0.94, 0.78, 0.96)),
                    BorderColor::all(Color::srgba(0.02, 0.02, 0.018, 0.92)),
                    MinimapPlayerMarker,
                ));
                map.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                    BorderColor::all(Color::srgba(0.94, 0.96, 1.0, 0.9)),
                    MinimapViewportMarker,
                ));
            });
        });
}

pub(crate) fn sync_full_map_view(
    mut commands: Commands,
    cache: Res<MapTextureCache>,
    state: Res<MapViewState>,
    roots: Query<Entity, With<FullMapRoot>>,
) {
    if !state.open {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Some(handle) = cache.handle.as_ref() else {
        return;
    };
    let mut roots_iter = roots.iter();
    let Some(_root) = roots_iter.next() else {
        spawn_full_map(&mut commands, handle.clone());
        return;
    };
    for duplicate in roots_iter {
        commands.entity(duplicate).despawn();
    }
}

fn spawn_full_map(commands: &mut Commands, handle: Handle<Image>) {
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
        .with_child((
            ImageNode::new(handle),
            Node {
                width: Val::Percent(84.0),
                height: Val::Percent(84.0),
                max_width: Val::Px(980.0),
                max_height: Val::Px(980.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgba(0.42, 0.43, 0.39, 0.9)),
        ));
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

fn update_viewport_marker(
    markers: &mut Query<&mut Node, (With<MinimapViewportMarker>, Without<MinimapPlayerMarker>)>,
    crop_bounds: MapTextureBounds,
    camera_rect: Option<WorldRect>,
) {
    let marker_rect = camera_rect.and_then(|rect| minimap_rect_for_world_rect(crop_bounds, rect));
    let Some(marker_rect) = marker_rect else {
        for mut marker in markers {
            marker.display = Display::None;
        }
        return;
    };

    for mut marker in markers {
        marker.display = Display::Flex;
        marker.left = Val::Px(marker_rect.left);
        marker.top = Val::Px(marker_rect.top);
        marker.width = Val::Px(marker_rect.width.max(MINIMAP_MIN_VIEWPORT_SIZE));
        marker.height = Val::Px(marker_rect.height.max(MINIMAP_MIN_VIEWPORT_SIZE));
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

fn texture_rect_for_world_bounds(
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

fn minimap_rect_for_world_rect(
    crop_bounds: MapTextureBounds,
    rect: WorldRect,
) -> Option<UiPixelRect> {
    let crop_min_x = crop_bounds.min_x as f32;
    let crop_min_y = crop_bounds.min_y as f32;
    let crop_max_x = crop_min_x + crop_bounds.width as f32;
    let crop_max_y = crop_min_y + crop_bounds.height as f32;

    let min_x = rect.min_x.max(crop_min_x);
    let max_x = rect.max_x.min(crop_max_x);
    let min_y = rect.min_y.max(crop_min_y);
    let max_y = rect.max_y.min(crop_max_y);
    if min_x >= max_x || min_y >= max_y {
        return None;
    }

    let left = (min_x - crop_min_x) / crop_bounds.width as f32 * MINIMAP_CONTENT_SIZE;
    let right = (max_x - crop_min_x) / crop_bounds.width as f32 * MINIMAP_CONTENT_SIZE;
    let top = (crop_max_y - max_y) / crop_bounds.height as f32 * MINIMAP_CONTENT_SIZE;
    let bottom = (crop_max_y - min_y) / crop_bounds.height as f32 * MINIMAP_CONTENT_SIZE;

    Some(UiPixelRect {
        left,
        top,
        width: right - left,
        height: bottom - top,
    })
}

fn camera_world_rect(
    cameras: &Query<(&Camera, &GlobalTransform), With<Camera2d>>,
) -> Option<WorldRect> {
    let (camera, transform) = cameras.iter().next()?;
    let viewport_size = camera.logical_viewport_size()?;
    let first = camera.viewport_to_world_2d(transform, Vec2::ZERO).ok()? / TILE_SIZE;
    let second = camera.viewport_to_world_2d(transform, viewport_size).ok()? / TILE_SIZE;

    Some(WorldRect {
        min_x: first.x.min(second.x),
        max_x: first.x.max(second.x),
        min_y: first.y.min(second.y),
        max_y: first.y.max(second.y),
    })
}

#[derive(Clone, Copy, Debug)]
struct WorldRect {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

#[derive(Clone, Copy, Debug)]
struct UiPixelRect {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
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
    fn minimap_viewport_overlay_is_clipped_to_crop() {
        let crop = MapTextureBounds {
            min_x: 0,
            min_y: 0,
            width: 128,
            height: 128,
        };

        let rect = minimap_rect_for_world_rect(
            crop,
            WorldRect {
                min_x: -8.0,
                max_x: 40.0,
                min_y: 24.0,
                max_y: 72.0,
            },
        )
        .expect("viewport should intersect minimap crop");

        assert_eq!(rect.left, 0.0);
        assert_eq!(rect.top, 77.0);
        assert_eq!(rect.width, 55.0);
        assert_eq!(rect.height, 66.0);
    }
}
