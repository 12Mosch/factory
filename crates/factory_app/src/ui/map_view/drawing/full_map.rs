use bevy::prelude::*;

use super::overlay::spawn_overlay_root;
use crate::map::resources::{MapOverlay, MapOverlaySettings};
use crate::ui::map_view::components::{
    FullMapImage, FullMapOverlayButton, FullMapOverlayRoot, FullMapRecenterButton,
    FullMapResourceImage, FullMapRoot,
};

pub(in crate::ui::map_view) fn spawn_full_map(
    commands: &mut Commands,
    surface: Handle<Image>,
    resources: Option<Handle<Image>>,
    texture_rect: Rect,
    display_size: Vec2,
    overlays: MapOverlaySettings,
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
                    image: surface,
                    rect: Some(texture_rect),
                    image_mode: NodeImageMode::Stretch,
                    ..default()
                },
                full_map_image_node(display_size),
                BorderColor::all(Color::srgba(0.42, 0.43, 0.39, 0.9)),
                FullMapImage,
            ))
            .with_children(|image| {
                if let Some(resource_image) = resources {
                    image.spawn((
                        ImageNode {
                            image: resource_image,
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
                        FullMapResourceImage,
                    ));
                }
                spawn_overlay_root(image, FullMapOverlayRoot);
            });
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(28.0),
                    top: Val::Px(24.0),
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(8.0),
                    flex_wrap: FlexWrap::Wrap,
                    max_width: Val::Percent(90.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|bar| {
                spawn_overlay_button(bar, MapOverlay::Pollution, "1 Pollution", overlays);
                spawn_overlay_button(bar, MapOverlay::Resources, "2 Resources", overlays);
                spawn_overlay_button(bar, MapOverlay::PowerNetworks, "3 Power", overlays);
                spawn_overlay_button(bar, MapOverlay::ProductionProblems, "4 Problems", overlays);
                spawn_overlay_button(bar, MapOverlay::Enemies, "5 Enemies", overlays);
                spawn_overlay_button(bar, MapOverlay::ConstructionPlans, "6 Plans", overlays);
                spawn_recenter_button(bar);
            });
        });
}

pub(in crate::ui::map_view) fn set_full_map_image_node_size(node: &mut Node, display_size: Vec2) {
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

fn spawn_overlay_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands<'_>,
    overlay: MapOverlay,
    label: &'static str,
    overlays: MapOverlaySettings,
) {
    let selected = overlays.is_enabled(overlay);
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
            FullMapOverlayButton { overlay },
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

pub(in crate::ui::map_view) fn layer_button_color(
    interaction: Interaction,
    selected: bool,
) -> Color {
    if selected {
        return Color::srgba(0.30, 0.28, 0.20, 0.98);
    }

    match interaction {
        Interaction::Pressed => Color::srgba(0.22, 0.20, 0.16, 0.98),
        Interaction::Hovered => Color::srgba(0.17, 0.17, 0.15, 0.98),
        Interaction::None => Color::srgba(0.10, 0.11, 0.11, 0.95),
    }
}

pub(in crate::ui::map_view) fn layer_button_border_color(selected: bool) -> Color {
    if selected {
        Color::srgba(0.72, 0.60, 0.36, 0.95)
    } else {
        Color::srgba(0.38, 0.42, 0.36, 0.85)
    }
}
