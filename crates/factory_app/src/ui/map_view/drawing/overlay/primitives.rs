use bevy::prelude::*;

use crate::map::resources::MapTextureBounds;
use crate::ui::map_view::layout::{MapUiRect, map_point_for_world_position};

#[derive(Bundle, Clone)]
pub(super) struct MapOverlayPrimitive {
    node: Node,
    background: BackgroundColor,
    border: BorderColor,
    transform: UiTransform,
    pub(super) z_index: ZIndex,
}

impl MapOverlayPrimitive {
    fn new(
        node: Node,
        background: BackgroundColor,
        border: BorderColor,
        transform: UiTransform,
    ) -> Self {
        Self {
            node,
            background,
            border,
            transform,
            z_index: ZIndex(0),
        }
    }
}

pub(super) fn spawn_point_overlay(
    overlays: &mut Vec<MapOverlayPrimitive>,
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

    overlays.push(MapOverlayPrimitive::new(
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
        UiTransform::default(),
    ));
}

pub(super) fn spawn_rect_overlay(
    overlays: &mut Vec<MapOverlayPrimitive>,
    rect: MapUiRect,
    border: Color,
    fill: Color,
    border_width: f32,
) {
    overlays.push(MapOverlayPrimitive::new(
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
        UiTransform::default(),
    ));
}

pub(super) fn spawn_world_line(
    overlays: &mut Vec<MapOverlayPrimitive>,
    bounds: MapTextureBounds,
    image_size: Vec2,
    start: Vec2,
    end: Vec2,
    width: f32,
    color: Color,
) {
    let Some((start, end)) = clip_line_to_bounds(bounds, start, end) else {
        return;
    };
    let Some(start) = map_point_for_world_position(bounds, image_size, start) else {
        return;
    };
    let Some(end) = map_point_for_world_position(bounds, image_size, end) else {
        return;
    };
    spawn_ui_line(overlays, start, end, width, color);
}

pub(super) fn spawn_ui_line(
    overlays: &mut Vec<MapOverlayPrimitive>,
    start: Vec2,
    end: Vec2,
    width: f32,
    color: Color,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= f32::EPSILON {
        return;
    }
    let midpoint = (start + end) * 0.5;
    overlays.push(MapOverlayPrimitive::new(
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(midpoint.x - length * 0.5),
            top: Val::Px(midpoint.y - width * 0.5),
            width: Val::Px(length),
            height: Val::Px(width),
            ..default()
        },
        BackgroundColor(color),
        BorderColor::DEFAULT,
        UiTransform::from_rotation(Rot2::radians(delta.y.atan2(delta.x))),
    ));
}

fn clip_line_to_bounds(bounds: MapTextureBounds, start: Vec2, end: Vec2) -> Option<(Vec2, Vec2)> {
    let min = Vec2::new(bounds.min_x as f32, bounds.min_y as f32);
    let max = Vec2::new(
        (bounds.min_x + i64::from(bounds.width)) as f32 - f32::EPSILON,
        (bounds.min_y + i64::from(bounds.height)) as f32 - f32::EPSILON,
    );
    let delta = end - start;
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    for (p, q) in [
        (-delta.x, start.x - min.x),
        (delta.x, max.x - start.x),
        (-delta.y, start.y - min.y),
        (delta.y, max.y - start.y),
    ] {
        if p.abs() <= f32::EPSILON {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            enter = enter.max(t);
        } else {
            exit = exit.min(t);
        }
        if enter > exit {
            return None;
        }
    }
    Some((start + delta * enter, start + delta * exit))
}
