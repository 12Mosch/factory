use bevy::prelude::*;

use super::components::{
    BuildBarRoot, BuildCancelButton, BuildMenuButton, BuildRotateButton, BuildRotateButtonText,
    BuildSlotButton, BuildSlotCountText, BuildSlotLabelText, BuildStatusText,
};
use crate::build::resources::HOTBAR_SLOT_COUNT;

const SLOT_WIDTH: f32 = 74.0;
const SLOT_HEIGHT: f32 = 58.0;

const _: () = assert!(
    HOTBAR_SLOT_COUNT == 10,
    "slot_key_label assumes 10 hotbar slots mapped to keys 1-9, 0"
);

pub(crate) fn setup_build_bar(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(14.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::NONE),
            GlobalZIndex(1050),
            BuildBarRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(5.0),
                    padding: UiRect::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.035, 0.038, 0.040, 0.88)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Ready"),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::srgb(0.78, 0.80, 0.76)),
                    BuildStatusText,
                ));
                panel
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(6.0),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ))
                    .with_children(|toolbar| {
                        for slot_index in 0..HOTBAR_SLOT_COUNT {
                            spawn_build_slot(toolbar, slot_index);
                        }

                        toolbar.spawn((
                            Node {
                                width: Val::Px(1.0),
                                height: Val::Px(SLOT_HEIGHT),
                                margin: UiRect::horizontal(Val::Px(3.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.48, 0.46, 0.40, 0.48)),
                        ));
                        spawn_action_button(toolbar, "Buildings (B)", BuildMenuButton, false);
                        spawn_action_button(toolbar, "Rotate N", BuildRotateButton, true);
                        spawn_action_button(toolbar, "Cancel", BuildCancelButton, false);
                    });
            });
        });
}

pub(crate) fn slot_key_label(slot_index: usize) -> String {
    ((slot_index + 1) % 10).to_string()
}

fn spawn_build_slot(toolbar: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, slot_index: usize) {
    toolbar
        .spawn((
            Button,
            Node {
                width: Val::Px(SLOT_WIDTH),
                height: Val::Px(SLOT_HEIGHT),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.13, 0.13, 0.13, 0.95)),
            BorderColor::all(Color::srgba(0.44, 0.43, 0.39, 0.70)),
            BuildSlotButton { slot_index },
        ))
        .with_children(|slot| {
            slot.spawn((
                Text::new(slot_key_label(slot_index)),
                TextFont::from_font_size(10.0),
                TextColor(Color::srgb(0.72, 0.72, 0.68)),
            ));
            slot.spawn((
                Text::new(""),
                TextFont::from_font_size(15.0),
                TextColor(Color::WHITE),
                TextLayout::justify(Justify::Center),
                BuildSlotLabelText { slot_index },
            ));
            slot.spawn((
                Text::new(""),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.91, 0.92, 0.86)),
                BuildSlotCountText { slot_index },
            ));
        });
}

fn spawn_action_button<T: Component>(
    toolbar: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    marker: T,
    rotate_text: bool,
) {
    toolbar
        .spawn((
            Button,
            Node {
                width: Val::Px(78.0),
                height: Val::Px(28.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.95)),
            BorderColor::all(Color::srgba(0.44, 0.43, 0.39, 0.70)),
            marker,
        ))
        .with_children(|button| {
            if rotate_text {
                button.spawn((
                    Text::new(label),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::WHITE),
                    BuildRotateButtonText,
                ));
            } else {
                button.spawn((
                    Text::new(label),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::WHITE),
                ));
            }
        });
}
