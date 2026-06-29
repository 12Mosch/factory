use bevy::prelude::*;
use factory_data::{EntityPrototypeId, ItemId};
use factory_sim::Direction;

use crate::input::build::select_build_slot;
use crate::input::panels::world_input_blocked;
use crate::placement::build::{buildable_prototypes, next_direction};
use crate::resources::{
    AppInputState, BuildPlacementState, BuildPlacementStatus, BuildSelection, SimResource,
    TechnologyWindowState,
};
use crate::utils::compact_item_name;

#[derive(Component)]
pub(crate) struct BuildBarRoot;

#[derive(Component)]
pub(crate) struct BuildSlotButton {
    pub(crate) slot_index: usize,
    pub(crate) prototype_id: EntityPrototypeId,
    pub(crate) item_id: ItemId,
}

#[derive(Component)]
pub(crate) struct BuildSlotCountText {
    pub(crate) item_id: ItemId,
}

#[derive(Component)]
pub(crate) struct BuildSlotLabelText {
    pub(crate) prototype_id: EntityPrototypeId,
    pub(crate) item_id: ItemId,
}

#[derive(Component)]
pub(crate) struct BuildRotateButton;

#[derive(Component)]
pub(crate) struct BuildRotateButtonText;

#[derive(Component)]
pub(crate) struct BuildCancelButton;

#[derive(Component)]
pub(crate) struct BuildStatusText;

const SLOT_WIDTH: f32 = 74.0;
const SLOT_HEIGHT: f32 = 58.0;

type BuildSlotInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static BuildSlotButton),
    (Changed<Interaction>, With<Button>),
>;
type BuildRotateInteractionQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<BuildRotateButton>,
        Without<BuildSlotButton>,
    ),
>;
type BuildCancelInteractionQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<BuildCancelButton>,
        Without<BuildSlotButton>,
        Without<BuildRotateButton>,
    ),
>;
type BuildSlotVisualQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static BuildSlotButton,
        &'static Interaction,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
>;
type BuildCountTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static BuildSlotCountText,
        &'static mut Text,
        &'static mut TextColor,
    ),
    Without<BuildSlotLabelText>,
>;
type BuildLabelTextQuery<'w, 's> = Query<
    'w,
    's,
    (&'static BuildSlotLabelText, &'static mut TextColor),
    Without<BuildSlotCountText>,
>;
type BuildRotateButtonVisualQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Interaction,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (
        With<BuildRotateButton>,
        Without<BuildCancelButton>,
        Without<BuildSlotButton>,
    ),
>;
type BuildCancelButtonVisualQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Interaction,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (
        With<BuildCancelButton>,
        Without<BuildRotateButton>,
        Without<BuildSlotButton>,
    ),
>;

pub(crate) fn setup_build_bar(mut commands: Commands, sim: Res<SimResource>) {
    let buildables = buildable_prototypes(sim.sim.catalog());

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
                        for buildable in buildables {
                            spawn_build_slot(toolbar, &buildable);
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
                        spawn_action_button(toolbar, "Rotate N", BuildRotateButton, true);
                        spawn_action_button(toolbar, "Cancel", BuildCancelButton, false);
                    });
            });
        });
}

pub(crate) fn handle_build_bar_button_clicks(
    mut slot_interactions: BuildSlotInteractionQuery,
    mut rotate_interactions: BuildRotateInteractionQuery,
    mut cancel_interactions: BuildCancelInteractionQuery,
    sim: Res<SimResource>,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    mut build_state: ResMut<BuildPlacementState>,
) {
    if world_input_blocked(input_state.as_deref())
        || technology_window
            .as_deref()
            .is_some_and(|window| window.open)
    {
        return;
    }

    for (interaction, button) in &mut slot_interactions {
        if *interaction == Interaction::Pressed {
            select_build_slot(
                &sim.sim,
                technology_window.as_deref(),
                &mut build_state,
                button.slot_index,
            );
        }
    }

    for interaction in &mut rotate_interactions {
        if *interaction == Interaction::Pressed && build_state.selected.is_some() {
            build_state.direction = next_direction(build_state.direction);
        }
    }

    for interaction in &mut cancel_interactions {
        if *interaction == Interaction::Pressed {
            build_state.selected = None;
            build_state.last_status = BuildPlacementStatus::Ready;
        }
    }
}

pub(crate) fn update_build_bar_visuals(
    sim: Res<SimResource>,
    build_state: Res<BuildPlacementState>,
    mut slot_buttons: BuildSlotVisualQuery,
    mut count_texts: BuildCountTextQuery,
    mut label_texts: BuildLabelTextQuery,
) {
    for (button, interaction, mut background, mut border) in &mut slot_buttons {
        let selected = build_state.selected
            == Some(BuildSelection {
                prototype_id: button.prototype_id,
                item_id: button.item_id,
            });
        let unlocked = sim.sim.is_entity_unlocked(button.prototype_id);
        let available = unlocked && sim.sim.player_inventory().count(button.item_id) > 0;
        *background = BackgroundColor(slot_background_color(*interaction, selected, available));
        *border = BorderColor::all(if selected {
            Color::srgb(0.94, 0.66, 0.20)
        } else {
            Color::srgba(0.44, 0.43, 0.39, 0.70)
        });
    }

    for (marker, mut text, mut color) in &mut count_texts {
        let count = sim.sim.player_inventory().count(marker.item_id);
        text.0 = count.to_string();
        *color = TextColor(if count == 0 {
            Color::srgb(0.62, 0.58, 0.52)
        } else {
            Color::srgb(0.91, 0.92, 0.86)
        });
    }

    for (marker, mut color) in &mut label_texts {
        let prototype_exists = sim
            .sim
            .catalog()
            .entities
            .get(marker.prototype_id.index())
            .is_some_and(|prototype| prototype.id == marker.prototype_id);
        let available = prototype_exists
            && sim.sim.is_entity_unlocked(marker.prototype_id)
            && sim.sim.player_inventory().count(marker.item_id) > 0;
        *color = TextColor(if available {
            Color::WHITE
        } else {
            Color::srgb(0.56, 0.55, 0.52)
        });
    }
}

pub(crate) fn update_build_bar_action_visuals(
    build_state: Res<BuildPlacementState>,
    mut rotate_buttons: BuildRotateButtonVisualQuery,
    mut rotate_texts: Query<&mut Text, With<BuildRotateButtonText>>,
    mut cancel_buttons: BuildCancelButtonVisualQuery,
) {
    for (interaction, mut background, mut border) in &mut rotate_buttons {
        let active = build_state.selected.is_some();
        *background = BackgroundColor(action_background_color(*interaction, active));
        *border = BorderColor::all(action_border_color(active));
    }
    for mut text in &mut rotate_texts {
        text.0 = format!("Rotate {}", direction_abbreviation(build_state.direction));
    }

    for (interaction, mut background, mut border) in &mut cancel_buttons {
        let active = build_state.selected.is_some();
        *background = BackgroundColor(action_background_color(*interaction, active));
        *border = BorderColor::all(action_border_color(active));
    }
}

pub(crate) fn update_build_status_text(
    build_state: Res<BuildPlacementState>,
    mut texts: Query<(&mut Text, &mut TextColor), With<BuildStatusText>>,
) {
    let (message, color) = match &build_state.last_status {
        BuildPlacementStatus::Ready => ("Ready".to_string(), Color::srgb(0.78, 0.80, 0.76)),
        BuildPlacementStatus::Placed(message) => (message.clone(), Color::srgb(0.56, 0.92, 0.55)),
        BuildPlacementStatus::CannotPlace(message) => {
            (message.clone(), Color::srgb(0.96, 0.33, 0.27))
        }
        BuildPlacementStatus::MissingInventory(message) => {
            (message.clone(), Color::srgb(0.98, 0.72, 0.28))
        }
        BuildPlacementStatus::Locked(message) => (message.clone(), Color::srgb(0.98, 0.72, 0.28)),
    };

    for (mut text, mut text_color) in &mut texts {
        text.0 = message.clone();
        *text_color = TextColor(color);
    }
}

fn spawn_build_slot(
    toolbar: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    buildable: &crate::placement::build::BuildablePrototype,
) {
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
            BuildSlotButton {
                slot_index: buildable.slot_index,
                prototype_id: buildable.prototype_id,
                item_id: buildable.item_id,
            },
        ))
        .with_children(|slot| {
            slot.spawn((
                Text::new((buildable.slot_index + 1).to_string()),
                TextFont::from_font_size(10.0),
                TextColor(Color::srgb(0.72, 0.72, 0.68)),
            ));
            slot.spawn((
                Text::new(compact_item_name(
                    &buildable.display_name.to_lowercase().replace(' ', "_"),
                )),
                TextFont::from_font_size(15.0),
                TextColor(Color::WHITE),
                TextLayout::justify(Justify::Center),
                BuildSlotLabelText {
                    prototype_id: buildable.prototype_id,
                    item_id: buildable.item_id,
                },
            ));
            slot.spawn((
                Text::new("0"),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.91, 0.92, 0.86)),
                BuildSlotCountText {
                    item_id: buildable.item_id,
                },
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

fn slot_background_color(interaction: Interaction, selected: bool, available: bool) -> Color {
    if selected {
        Color::srgba(0.34, 0.26, 0.12, 0.98)
    } else if !available {
        Color::srgba(0.075, 0.075, 0.075, 0.86)
    } else {
        match interaction {
            Interaction::Pressed => Color::srgba(0.24, 0.22, 0.18, 0.98),
            Interaction::Hovered => Color::srgba(0.19, 0.18, 0.16, 0.98),
            Interaction::None => Color::srgba(0.13, 0.13, 0.13, 0.95),
        }
    }
}

fn action_background_color(interaction: Interaction, active: bool) -> Color {
    if !active {
        return Color::srgba(0.09, 0.09, 0.09, 0.82);
    }

    match interaction {
        Interaction::Pressed => Color::srgba(0.27, 0.23, 0.16, 0.98),
        Interaction::Hovered => Color::srgba(0.21, 0.19, 0.16, 0.98),
        Interaction::None => Color::srgba(0.15, 0.15, 0.15, 0.95),
    }
}

fn action_border_color(active: bool) -> Color {
    if active {
        Color::srgba(0.66, 0.55, 0.35, 0.85)
    } else {
        Color::srgba(0.33, 0.32, 0.30, 0.60)
    }
}

fn direction_abbreviation(direction: Direction) -> &'static str {
    match direction {
        Direction::North => "N",
        Direction::East => "E",
        Direction::South => "S",
        Direction::West => "W",
    }
}
