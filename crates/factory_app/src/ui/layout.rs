use bevy::prelude::*;

pub(crate) const PANEL_MARGIN: f32 = 12.0;
pub(crate) const PANEL_SCROLLBAR_WIDTH: f32 = 10.0;

pub(crate) fn scroll_column() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        flex_grow: 1.0,
        flex_shrink: 1.0,
        min_height: Val::Px(0.0),
        overflow: Overflow::scroll_y(),
        scrollbar_width: PANEL_SCROLLBAR_WIDTH,
        ..default()
    }
}
