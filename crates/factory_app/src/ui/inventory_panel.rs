use bevy::prelude::*;
use factory_data::{ItemId, PrototypeCatalog};
use factory_sim::{
    AssemblerError, BOILER_FUEL_SLOT_INDEX, BURNER_MINING_DRILL_FUEL_SLOT_INDEX,
    BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX, BoilerError, BurnerDrillError, ContainerError,
    FURNACE_FUEL_SLOT_INDEX, FURNACE_INPUT_SLOT_INDEX, FURNACE_OUTPUT_SLOT_INDEX, FurnaceError,
    SimCommand, SlotTransferError,
};

use crate::constants::{SLOT_BUTTON_HEIGHT, SLOT_BUTTON_WIDTH};
use crate::resources::{InventoryTransferFeedback, OpenContainer, SimResource};
use crate::simulation::SimCommandRequest;
use crate::ui::formatting::{format_item_display_name, format_item_stack};

pub use factory_sim::InventoryPanel;

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

#[derive(Component)]
pub(crate) struct InventoryTransferFeedbackText;

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

pub(crate) fn spawn_inventory_transfer_feedback(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
) {
    parent.spawn((
        Text::new(""),
        TextFont::from_font_size(12.0),
        TextColor(Color::srgb(0.98, 0.72, 0.28)),
        Node {
            width: Val::Px(190.0),
            ..default()
        },
        InventoryTransferFeedbackText,
    ));
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
    open_container: Res<OpenContainer>,
    mut feedback: ResMut<InventoryTransferFeedback>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    for (interaction, button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let Some(entity_id) = open_container.entity_id else {
            feedback.message = Some("No open container".to_string());
            continue;
        };

        commands.write(SimCommandRequest(SimCommand::TransferSlot {
            entity_id,
            panel: button.panel,
            slot_index: button.slot_index,
        }));
    }
}

pub(crate) fn update_inventory_transfer_feedback_text(
    feedback: Res<InventoryTransferFeedback>,
    mut texts: Query<&mut Text, With<InventoryTransferFeedbackText>>,
) {
    if !feedback.is_changed() {
        return;
    }

    let message = feedback.message.as_deref().unwrap_or_default();
    for mut text in &mut texts {
        text.0 = message.to_string();
    }
}

pub(crate) fn update_container_slot_text(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut texts: Query<(&ContainerSlotText, &mut Text)>,
) {
    let container_inventory = open_container
        .entity_id
        .and_then(|entity_id| factory_sim::entity_access::inventory(&sim.sim, entity_id).ok());
    let burner_drill_state = open_container.entity_id.and_then(|entity_id| {
        factory_sim::entity_access::burner_drill_state(&sim.sim, entity_id).ok()
    });
    let furnace_state = open_container
        .entity_id
        .and_then(|entity_id| factory_sim::entity_access::furnace_state(&sim.sim, entity_id).ok());
    let boiler_state = open_container
        .entity_id
        .and_then(|entity_id| factory_sim::entity_access::boiler_state(&sim.sim, entity_id).ok());
    let assembler_state = open_container.entity_id.and_then(|entity_id| {
        factory_sim::entity_access::assembler_state(&sim.sim, entity_id).ok()
    });

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
            InventoryPanel::BoilerFuel => boiler_state.and_then(|state| {
                (marker.slot_index == BOILER_FUEL_SLOT_INDEX)
                    .then_some(state.energy.fuel_slot)
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

pub fn slot_transfer_error_message(catalog: &PrototypeCatalog, error: SlotTransferError) -> String {
    match error {
        SlotTransferError::Transfer(error) => container_error_message(catalog, error),
        SlotTransferError::BurnerDrill(error) => burner_drill_error_message(catalog, error),
        SlotTransferError::Furnace(error) => furnace_error_message(catalog, error),
        SlotTransferError::Boiler(error) => boiler_error_message(catalog, error),
        SlotTransferError::Assembler(error) => assembler_error_message(catalog, error),
    }
}

fn wrong_item_message(catalog: &PrototypeCatalog, item_id: ItemId) -> String {
    format!("Wrong item: {}", format_item_display_name(catalog, item_id))
}

fn container_error_message(catalog: &PrototypeCatalog, error: ContainerError) -> String {
    match error {
        ContainerError::MissingEntity(_) | ContainerError::NotContainer(_) => {
            "Container unavailable".to_string()
        }
        ContainerError::InvalidItem(item_id) => wrong_item_message(catalog, item_id),
        ContainerError::InvalidSlot { .. } => "Invalid slot".to_string(),
        ContainerError::EmptySlot { .. } => "Empty slot".to_string(),
        ContainerError::InsufficientSpace => "No space".to_string(),
        ContainerError::UnknownItem => "Unknown item".to_string(),
    }
}

fn burner_drill_error_message(catalog: &PrototypeCatalog, error: BurnerDrillError) -> String {
    match error {
        BurnerDrillError::MissingEntity(_) | BurnerDrillError::NotBurnerDrill(_) => {
            "Machine unavailable".to_string()
        }
        BurnerDrillError::InvalidFuel(item_id) => wrong_item_message(catalog, item_id),
        BurnerDrillError::InvalidSlot { .. } => "Invalid slot".to_string(),
        BurnerDrillError::EmptySlot { .. } => "Empty slot".to_string(),
        BurnerDrillError::InsufficientSpace => "No space".to_string(),
        BurnerDrillError::UnknownItem => "Unknown item".to_string(),
    }
}

fn furnace_error_message(catalog: &PrototypeCatalog, error: FurnaceError) -> String {
    match error {
        FurnaceError::MissingEntity(_) | FurnaceError::NotFurnace(_) => {
            "Machine unavailable".to_string()
        }
        FurnaceError::InvalidInput(item_id) | FurnaceError::InvalidFuel(item_id) => {
            wrong_item_message(catalog, item_id)
        }
        FurnaceError::InvalidSlot { .. } => "Invalid slot".to_string(),
        FurnaceError::EmptySlot { .. } => "Empty slot".to_string(),
        FurnaceError::InsufficientSpace => "No space".to_string(),
        FurnaceError::UnknownItem => "Unknown item".to_string(),
    }
}

fn boiler_error_message(catalog: &PrototypeCatalog, error: BoilerError) -> String {
    match error {
        BoilerError::MissingEntity(_) | BoilerError::NotBoiler(_) => {
            "Machine unavailable".to_string()
        }
        BoilerError::InvalidFuel(item_id) => wrong_item_message(catalog, item_id),
        BoilerError::InvalidSlot { .. } => "Invalid slot".to_string(),
        BoilerError::EmptySlot { .. } => "Empty slot".to_string(),
        BoilerError::InsufficientSpace => "No space".to_string(),
        BoilerError::UnknownItem => "Unknown item".to_string(),
    }
}

fn assembler_error_message(catalog: &PrototypeCatalog, error: AssemblerError) -> String {
    match error {
        AssemblerError::MissingEntity(_) | AssemblerError::NotAssembler(_) => {
            "Machine unavailable".to_string()
        }
        AssemblerError::MissingRecipe(_)
        | AssemblerError::InvalidRecipe(_)
        | AssemblerError::RecipeLocked(_) => "Recipe unavailable".to_string(),
        AssemblerError::RecipeChangeRequiresEmpty { .. } => "Empty assembler first".to_string(),
        AssemblerError::InvalidInput(item_id) => wrong_item_message(catalog, item_id),
        AssemblerError::InvalidSlot { .. } => "Invalid slot".to_string(),
        AssemblerError::EmptySlot { .. } => "Empty slot".to_string(),
        AssemblerError::InsufficientSpace => "No space".to_string(),
        AssemblerError::UnknownItem => "Unknown item".to_string(),
    }
}
