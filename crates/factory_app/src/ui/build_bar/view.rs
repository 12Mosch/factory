use bevy::prelude::*;
use bevy::scene::ScenePatch;

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

/// Static retained hierarchy for the build toolbar.
fn build_bar_scene() -> impl Scene {
    let slots = (0..HOTBAR_SLOT_COUNT)
        .map(build_slot_scene)
        .collect::<Vec<_>>();

    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Px(14.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
        }
        BackgroundColor(Color::NONE)
        GlobalZIndex(1050)
        BuildBarRoot
        Children [(
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(5.0),
                padding: UiRect::all(Val::Px(8.0)),
            }
            BackgroundColor(Color::srgba(0.035, 0.038, 0.040, 0.88))
            Children [
                (
                    Text("Ready")
                    TextFont { font_size: FontSize::Px(12.0) }
                    TextColor(Color::srgb(0.78, 0.80, 0.76))
                    BuildStatusText
                ),
                (
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                    }
                    BackgroundColor(Color::NONE)
                    Children [
                        {slots},
                        (
                            Node {
                                width: Val::Px(1.0),
                                height: Val::Px(SLOT_HEIGHT),
                                margin: UiRect::horizontal(Val::Px(3.0)),
                            }
                            BackgroundColor(Color::srgba(0.48, 0.46, 0.40, 0.48))
                        ),
                        action_button_scene(BuildMenuButton, action_label_scene("Buildings (B)")),
                        action_button_scene(BuildRotateButton, rotate_label_scene()),
                        action_button_scene(BuildCancelButton, action_label_scene("Cancel")),
                    ]
                ),
            ]
        )]
    }
}

pub(crate) fn setup_build_bar(
    mut commands: Commands,
    asset_server: Option<Res<AssetServer>>,
    scene_patches: Option<Res<Assets<ScenePatch>>>,
) {
    if asset_server.is_none() || scene_patches.is_none() {
        return;
    }

    commands.spawn_scene(build_bar_scene());
}

pub(crate) fn slot_key_label(slot_index: usize) -> String {
    ((slot_index + 1) % 10).to_string()
}

fn build_slot_scene(slot_index: usize) -> impl Scene {
    bsn! {
        Button
        Node {
            width: Val::Px(SLOT_WIDTH),
            height: Val::Px(SLOT_HEIGHT),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceBetween,
            padding: UiRect::all(Val::Px(5.0)),
            border: UiRect::all(Val::Px(1.0)),
        }
        BackgroundColor(Color::srgba(0.13, 0.13, 0.13, 0.95))
        BorderColor::all(Color::srgba(0.44, 0.43, 0.39, 0.70))
        BuildSlotButton { slot_index }
        Children [
            (
                Text(slot_key_label(slot_index))
                TextFont { font_size: FontSize::Px(10.0) }
                TextColor(Color::srgb(0.72, 0.72, 0.68))
            ),
            (
                Text("")
                TextFont { font_size: FontSize::Px(15.0) }
                TextColor(Color::WHITE)
                TextLayout::justify(Justify::Center)
                BuildSlotLabelText { slot_index }
            ),
            (
                Text("")
                TextFont { font_size: FontSize::Px(12.0) }
                TextColor(Color::srgb(0.91, 0.92, 0.86))
                BuildSlotCountText { slot_index }
            ),
        ]
    }
}

fn action_button_scene<T: Component + Clone + Default + Unpin>(
    marker: T,
    label: impl Scene,
) -> impl Scene {
    bsn! {
        Button
        Node {
            width: Val::Px(78.0),
            height: Val::Px(28.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            padding: UiRect::horizontal(Val::Px(8.0)),
            border: UiRect::all(Val::Px(1.0)),
        }
        BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.95))
        BorderColor::all(Color::srgba(0.44, 0.43, 0.39, 0.70))
        template_value(marker)
        Children [({label})]
    }
}

fn action_label_scene(label: &'static str) -> impl Scene {
    bsn! {
        Text(label)
        TextFont { font_size: FontSize::Px(12.0) }
        TextColor(Color::WHITE)
    }
}

fn rotate_label_scene() -> impl Scene {
    bsn! {
        Text("Rotate N")
        TextFont { font_size: FontSize::Px(12.0) }
        TextColor(Color::WHITE)
        BuildRotateButtonText
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{asset::AssetPlugin, scene::ScenePlugin};

    #[test]
    fn build_bar_scene_keeps_generated_slots_and_fixed_actions() {
        let mut app = App::new();
        app.add_plugins((AssetPlugin::default(), ScenePlugin))
            .add_systems(Update, setup_build_bar);
        app.update();

        let world = app.world_mut();
        let root = world
            .query_filtered::<Entity, With<BuildBarRoot>>()
            .single(world)
            .expect("setup should spawn one build bar root");
        assert_eq!(
            world
                .entity(root)
                .get::<Children>()
                .expect("build bar root should have a panel")
                .len(),
            1
        );

        let mut slots = world
            .query::<(&BuildSlotButton, &Children)>()
            .iter(world)
            .map(|(slot, children)| (slot.slot_index, children.len()))
            .collect::<Vec<_>>();
        slots.sort_unstable();
        assert_eq!(
            slots,
            (0..HOTBAR_SLOT_COUNT)
                .map(|slot_index| (slot_index, 3))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<BuildSlotLabelText>>()
                .iter(world)
                .count(),
            HOTBAR_SLOT_COUNT
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<BuildSlotCountText>>()
                .iter(world)
                .count(),
            HOTBAR_SLOT_COUNT
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<BuildStatusText>>()
                .iter(world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<BuildMenuButton>>()
                .iter(world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<BuildRotateButton>>()
                .iter(world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<BuildCancelButton>>()
                .iter(world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<BuildRotateButtonText>>()
                .iter(world)
                .count(),
            1
        );
    }
}
