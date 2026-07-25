use bevy::prelude::*;

use super::overlay::spawn_overlay_root;
use crate::ui::map_view::components::{
    MinimapImage, MinimapOverlayRoot, MinimapResourceImage, MinimapRoot,
};

pub(crate) const MINIMAP_FRAME_SIZE: f32 = 184.0;
pub(crate) const MINIMAP_RIGHT_OFFSET: f32 = 14.0;
pub(crate) const MINIMAP_TOP_OFFSET: f32 = 14.0;
const MINIMAP_PADDING: f32 = 4.0;
const MINIMAP_BORDER_WIDTH: f32 = 1.0;
pub(in crate::ui::map_view) const MINIMAP_CONTENT_SIZE: f32 =
    MINIMAP_FRAME_SIZE - (MINIMAP_PADDING + MINIMAP_BORDER_WIDTH) * 2.0;

pub(in crate::ui::map_view) fn spawn_minimap(
    commands: &mut Commands,
    surface: Handle<Image>,
    resources: Option<Handle<Image>>,
    texture_rect: Rect,
) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(MINIMAP_RIGHT_OFFSET),
                top: Val::Px(MINIMAP_TOP_OFFSET),
                width: Val::Px(MINIMAP_FRAME_SIZE),
                height: Val::Px(MINIMAP_FRAME_SIZE),
                padding: UiRect::all(Val::Px(MINIMAP_PADDING)),
                border: UiRect::all(Val::Px(MINIMAP_BORDER_WIDTH)),
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
                        image: surface,
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
                if let Some(image) = resources {
                    map.spawn((
                        ImageNode {
                            image,
                            rect: Some(texture_rect),
                            image_mode: NodeImageMode::Stretch,
                            ..default()
                        },
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        MinimapResourceImage,
                    ));
                }
                spawn_overlay_root(map, MinimapOverlayRoot);
            });
        });
}
