use bevy::prelude::*;
use factory_data::ItemId;
use factory_sim::{BELT_SUBTILES_PER_TILE, BeltItemId, Direction};

use crate::constants::TILE_SIZE;

use super::components::VisibleBeltItemRenderState;

pub(super) fn transport_item_render_state_from_parts(
    key: BeltItemId,
    lane_index: usize,
    dir: Direction,
    center: Vec3,
    item_id: ItemId,
    position_subtile: u16,
    color: Color,
) -> VisibleBeltItemRenderState {
    let along = direction_render_vector(dir);
    let perpendicular = Vec2::new(-along.y, along.x);
    let progress = f32::from(position_subtile) / f32::from(BELT_SUBTILES_PER_TILE) - 0.5;
    let lane_offset = if lane_index == 0 { -0.18 } else { 0.18 };
    let offset = (along * progress + perpendicular * lane_offset) * TILE_SIZE;
    let translation = Vec3::new(center.x + offset.x, center.y + offset.y, 4.0);

    VisibleBeltItemRenderState {
        key,
        item_id,
        translation,
        color,
    }
}

pub(super) fn splitter_port_tiles_for_render(
    footprint: &factory_sim::EntityFootprint,
) -> Option<[(factory_sim::WorldTileCoord, factory_sim::WorldTileCoord); 2]> {
    let mut tiles = footprint.tiles();
    if tiles.len() != 2 {
        return None;
    }

    tiles.sort_unstable();
    Some([tiles[0], tiles[1]])
}

pub(super) fn direction_render_vector(direction: Direction) -> Vec2 {
    match direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => Vec2::NEG_Y,
        Direction::West => Vec2::NEG_X,
    }
}
