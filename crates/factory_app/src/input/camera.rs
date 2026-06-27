use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;

use crate::constants::{MAX_CAMERA_SCALE, MIN_CAMERA_SCALE};

pub(crate) fn zoom_camera(
    mouse_scroll: Option<Res<AccumulatedMouseScroll>>,
    mut camera: Query<&mut Projection, With<Camera2d>>,
) {
    let Some(mouse_scroll) = mouse_scroll else {
        return;
    };

    let scroll = mouse_scroll.delta.y;
    if scroll == 0.0 {
        return;
    }

    for mut projection in &mut camera {
        let Projection::Orthographic(orthographic) = &mut *projection else {
            continue;
        };

        let zoom_factor = (1.0 - scroll * 0.12).clamp(0.5, 1.5);
        orthographic.scale =
            (orthographic.scale * zoom_factor).clamp(MIN_CAMERA_SCALE, MAX_CAMERA_SCALE);
    }
}
