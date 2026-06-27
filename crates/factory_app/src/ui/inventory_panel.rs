use bevy::prelude::*;
use factory_sim::{
    BURNER_MINING_DRILL_FUEL_SLOT_INDEX, BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX,
    FURNACE_FUEL_SLOT_INDEX, FURNACE_INPUT_SLOT_INDEX, FURNACE_OUTPUT_SLOT_INDEX,
};

use crate::constants::{SLOT_BUTTON_HEIGHT, SLOT_BUTTON_WIDTH};
use crate::interaction::slot_transfer::transfer_open_container_slot;
use crate::resources::{OpenContainer, SimResource};
use crate::ui::formatting::format_item_stack;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryPanel {
    Player,
    Container,
    BurnerFuel,
    BurnerOutput,
    FurnaceInput,
    FurnaceFuel,
    FurnaceOutput,
    AssemblerInput,
    AssemblerOutput,
}

#[derive(Component)]
pub(crate) struct ContainerSlotButton {
    panel: InventoryPanel,
    slot_index: usize,
}

#[derive(Component)]
pub(crate) struct ContainerSlotText {
    panel: InventoryPanel,
    slot_index: usize,
}

pub(crate) type ContainerSlotInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static ContainerSlotButton),
    (Changed<Interaction>, With<Button>),
>;

pub(crate) fn spawn_player_inventory_panel(root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new("Player"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        panel
            .spawn((
                Node {
                    width: Val::Px(500.0),
                    flex_wrap: FlexWrap::Wrap,
                    row_gap: Val::Px(4.0),
                    column_gap: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|grid| {
                for slot_index in 0..factory_sim::PLAYER_INVENTORY_SLOT_COUNT {
                    spawn_slot_button(grid, InventoryPanel::Player, slot_index);
                }
            });
    });
}

pub(crate) fn spawn_labeled_slot(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    panel: InventoryPanel,
    slot_index: usize,
) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|slot| {
            slot.spawn((
                Text::new(label),
                TextFont::from_font_size(11.0),
                TextColor(Color::srgb(0.78, 0.80, 0.78)),
            ));
            spawn_slot_button(slot, panel, slot_index);
        });
}

pub(crate) fn spawn_slot_button(
    grid: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    panel: InventoryPanel,
    slot_index: usize,
) {
    grid.spawn((
        Button,
        Node {
            width: Val::Px(SLOT_BUTTON_WIDTH),
            height: Val::Px(SLOT_BUTTON_HEIGHT),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            padding: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.14, 0.14, 0.15, 0.96)),
        ContainerSlotButton { panel, slot_index },
    ))
    .with_child((
        Text::new(""),
        TextFont::from_font_size(9.0),
        TextColor(Color::WHITE),
        TextLayout::justify(Justify::Center),
        ContainerSlotText { panel, slot_index },
    ));
}

pub(crate) fn handle_container_slot_clicks(
    mut interactions: ContainerSlotInteractionQuery,
    mut sim: ResMut<SimResource>,
    open_container: Res<OpenContainer>,
) {
    for (interaction, button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let _ = transfer_open_container_slot(
            &mut sim.sim,
            open_container.entity_id,
            button.panel,
            button.slot_index,
        );
    }
}

pub(crate) fn update_container_slot_text(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut texts: Query<(&ContainerSlotText, &mut Text)>,
) {
    let container_inventory = open_container
        .entity_id
        .and_then(|entity_id| sim.sim.entity_inventory(entity_id).ok());
    let burner_drill_state = open_container
        .entity_id
        .and_then(|entity_id| sim.sim.burner_drill_state(entity_id).ok());
    let furnace_state = open_container
        .entity_id
        .and_then(|entity_id| sim.sim.furnace_state(entity_id).ok());
    let assembler_state = open_container
        .entity_id
        .and_then(|entity_id| sim.sim.assembler_state(entity_id).ok());

    for (marker, mut text) in &mut texts {
        let stack = match marker.panel {
            InventoryPanel::Player => sim
                .sim
                .player_inventory()
                .slots
                .get(marker.slot_index)
                .and_then(|slot| *slot),
            InventoryPanel::Container => container_inventory
                .and_then(|inventory| inventory.slots.get(marker.slot_index))
                .and_then(|slot| *slot),
            InventoryPanel::BurnerFuel => burner_drill_state.and_then(|state| {
                (marker.slot_index == BURNER_MINING_DRILL_FUEL_SLOT_INDEX)
                    .then_some(state.energy.fuel_slot)
                    .flatten()
            }),
            InventoryPanel::BurnerOutput => burner_drill_state.and_then(|state| {
                (marker.slot_index == BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX)
                    .then_some(state.output_slot)
                    .flatten()
            }),
            InventoryPanel::FurnaceInput => furnace_state.and_then(|state| {
                (marker.slot_index == FURNACE_INPUT_SLOT_INDEX)
                    .then_some(state.input_slot)
                    .flatten()
            }),
            InventoryPanel::FurnaceFuel => furnace_state.and_then(|state| {
                (marker.slot_index == FURNACE_FUEL_SLOT_INDEX)
                    .then_some(state.energy.fuel_slot)
                    .flatten()
            }),
            InventoryPanel::FurnaceOutput => furnace_state.and_then(|state| {
                (marker.slot_index == FURNACE_OUTPUT_SLOT_INDEX)
                    .then_some(state.output_slot)
                    .flatten()
            }),
            InventoryPanel::AssemblerInput => assembler_state
                .and_then(|state| state.input_inventory.slots.get(marker.slot_index))
                .and_then(|slot| *slot),
            InventoryPanel::AssemblerOutput => assembler_state
                .and_then(|state| state.output_inventory.slots.get(marker.slot_index))
                .and_then(|slot| *slot),
        };
        text.0 = stack
            .map(|stack| format_item_stack(stack, sim.sim.catalog()))
            .unwrap_or_default();
    }
}
