use bevy::prelude::*;
use factory_sim::{CHUNK_SIZE, ChunkCoord};
use std::collections::BTreeSet;

use crate::constants::{INITIAL_CAMERA_SCALE, TILE_SIZE};
use crate::map::resources::{MapTextureBounds, VisibleChunks};
use crate::rendering::player::PlayerSprite;
use crate::rendering::resources::RenderDetail;
use crate::rendering::transforms::player_translation;
use crate::resources::SimResource;

pub(crate) const RENDER_CHUNK_MARGIN: i32 = 1;
pub(crate) const FALLBACK_VISIBLE_CHUNK_RADIUS: i32 = 2;
pub(crate) const RESOURCE_AMOUNT_LABEL_MAX_SCALE: f32 = 2.75;
pub(crate) const BELT_ITEM_LABEL_MAX_SCALE: f32 = 2.25;
pub(crate) const BELT_DETAIL_MAX_SCALE: f32 = 5.0;

pub(crate) fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: INITIAL_CAMERA_SCALE,
            ..OrthographicProjection::default_2d()
        }),
    ));
}

pub(crate) fn follow_player_camera(
    sim: Res<SimResource>,
    mut cameras: Query<&mut Transform, (With<Camera2d>, Without<PlayerSprite>)>,
) {
    let player = player_translation(sim.read().player(), 0.0);

    for mut transform in &mut cameras {
        transform.translation.x = player.x;
        transform.translation.y = player.y;
    }
}

pub(crate) fn update_visible_chunks(
    sim: Res<SimResource>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut visible: ResMut<VisibleChunks>,
) {
    let sim = sim.read();
    let candidate_chunks = cameras
        .iter()
        .next()
        .and_then(|(camera, transform)| visible_chunks_from_camera(camera, transform))
        .unwrap_or_else(|| {
            let (tile_x, tile_y) = sim.player().tile_position();
            ChunkCoord::from_tile(tile_x, tile_y)
                .map(|coord| visible_chunks_around(coord, FALLBACK_VISIBLE_CHUNK_RADIUS))
                .unwrap_or_default()
        });
    let chunks = candidate_chunks
        .into_iter()
        .filter(|coord| sim.world().chunks.contains_key(coord))
        .collect::<BTreeSet<_>>();
    let tile_bounds = tile_bounds_for_chunks(&chunks);

    if visible.chunks != chunks || visible.tile_bounds != tile_bounds {
        visible.chunks = chunks;
        visible.tile_bounds = tile_bounds;
        visible.revision = visible.revision.wrapping_add(1);
    }
}

pub(crate) fn update_render_detail(
    cameras: Query<&Projection, With<Camera2d>>,
    mut detail: ResMut<RenderDetail>,
) {
    let Some(scale) = cameras.iter().find_map(orthographic_camera_scale) else {
        return;
    };
    let next = render_detail_for_camera_scale(scale);

    if *detail != next {
        *detail = next;
    }
}

fn visible_chunks_from_camera(
    camera: &Camera,
    transform: &GlobalTransform,
) -> Option<BTreeSet<ChunkCoord>> {
    let viewport_size = camera.logical_viewport_size()?;
    let first = camera.viewport_to_world_2d(transform, Vec2::ZERO).ok()?;
    let second = camera
        .viewport_to_world_2d(transform, Vec2::new(viewport_size.x, viewport_size.y))
        .ok()?;
    Some(visible_chunks_for_world_rect(
        first,
        second,
        RENDER_CHUNK_MARGIN,
    ))
}

pub(crate) fn visible_chunks_for_world_rect(
    first: Vec2,
    second: Vec2,
    margin_chunks: i32,
) -> BTreeSet<ChunkCoord> {
    let min_world_x = first.x.min(second.x);
    let max_world_x = first.x.max(second.x);
    let min_world_y = first.y.min(second.y);
    let max_world_y = first.y.max(second.y);
    let min_tile_x = world_pixel_to_tile(min_world_x);
    let max_tile_x = world_pixel_to_tile(max_world_x);
    let min_tile_y = world_pixel_to_tile(min_world_y);
    let max_tile_y = world_pixel_to_tile(max_world_y);
    let min_chunk_x = min_tile_x.div_euclid(i64::from(CHUNK_SIZE)) - i64::from(margin_chunks);
    let max_chunk_x = max_tile_x.div_euclid(i64::from(CHUNK_SIZE)) + i64::from(margin_chunks);
    let min_chunk_y = min_tile_y.div_euclid(i64::from(CHUNK_SIZE)) - i64::from(margin_chunks);
    let max_chunk_y = max_tile_y.div_euclid(i64::from(CHUNK_SIZE)) + i64::from(margin_chunks);

    let mut chunks = BTreeSet::new();
    for y in min_chunk_y..=max_chunk_y {
        for x in min_chunk_x..=max_chunk_x {
            if let (Ok(x), Ok(y)) = (i32::try_from(x), i32::try_from(y)) {
                chunks.insert(ChunkCoord { x, y });
            }
        }
    }
    chunks
}

fn visible_chunks_around(center: ChunkCoord, radius: i32) -> BTreeSet<ChunkCoord> {
    let radius = radius.max(0);
    let mut chunks = BTreeSet::new();
    for y in center.y - radius..=center.y + radius {
        for x in center.x - radius..=center.x + radius {
            chunks.insert(ChunkCoord { x, y });
        }
    }
    chunks
}

fn world_pixel_to_tile(value: f32) -> i64 {
    (value / TILE_SIZE).floor() as i64
}

fn orthographic_camera_scale(projection: &Projection) -> Option<f32> {
    let Projection::Orthographic(orthographic) = projection else {
        return None;
    };
    Some(orthographic.scale)
}

pub(crate) fn render_detail_for_camera_scale(scale: f32) -> RenderDetail {
    RenderDetail {
        show_resource_amount_labels: scale <= RESOURCE_AMOUNT_LABEL_MAX_SCALE,
        show_belt_directions: scale <= BELT_DETAIL_MAX_SCALE,
        show_belt_items: scale <= BELT_DETAIL_MAX_SCALE,
        show_belt_item_labels: scale <= BELT_ITEM_LABEL_MAX_SCALE,
    }
}

fn tile_bounds_for_chunks(chunks: &BTreeSet<ChunkCoord>) -> Option<MapTextureBounds> {
    let min_chunk_x = chunks.iter().map(|coord| coord.x).min()?;
    let max_chunk_x = chunks.iter().map(|coord| coord.x).max()?;
    let min_chunk_y = chunks.iter().map(|coord| coord.y).min()?;
    let max_chunk_y = chunks.iter().map(|coord| coord.y).max()?;

    Some(MapTextureBounds {
        min_x: ChunkCoord {
            x: min_chunk_x,
            y: min_chunk_y,
        }
        .min_tile()
        .0,
        min_y: ChunkCoord {
            x: min_chunk_x,
            y: min_chunk_y,
        }
        .min_tile()
        .1,
        width: u32::try_from(
            (i64::from(max_chunk_x) - i64::from(min_chunk_x) + 1) * i64::from(CHUNK_SIZE),
        )
        .unwrap_or(u32::MAX)
        .min(crate::map::resources::MAX_MAP_TEXTURE_SIDE_TILES),
        height: u32::try_from(
            (i64::from(max_chunk_y) - i64::from(min_chunk_y) + 1) * i64::from(CHUNK_SIZE),
        )
        .unwrap_or(u32::MAX)
        .min(crate::map::resources::MAX_MAP_TEXTURE_SIDE_TILES),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_chunks_for_world_rect_handles_negative_coordinates() {
        let chunks =
            visible_chunks_for_world_rect(Vec2::new(-33.0 * TILE_SIZE, -1.0), Vec2::ZERO, 0);

        assert!(chunks.contains(&ChunkCoord { x: -2, y: -1 }));
        assert!(chunks.contains(&ChunkCoord { x: 0, y: 0 }));
    }

    #[test]
    fn render_detail_hides_unreadable_overlays_when_zoomed_out() {
        assert!(render_detail_for_camera_scale(2.0).show_belt_item_labels);

        let detail = render_detail_for_camera_scale(6.0);

        assert!(!detail.show_resource_amount_labels);
        assert!(!detail.show_belt_directions);
        assert!(!detail.show_belt_items);
        assert!(!detail.show_belt_item_labels);
    }
}
