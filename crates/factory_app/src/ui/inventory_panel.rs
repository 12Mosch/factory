use bevy::prelude::*;
use factory_data::{ItemId, PrototypeCatalog};
use factory_sim::{
    AssemblerError, BoilerError, ContainerError, FurnaceError, InserterError, MiningDrillError,
    ModuleError, NuclearReactorError, RoboportError, RollingStockTransferError, SimCommand,
    SlotTransferError,
};

use crate::constants::{SLOT_BUTTON_HEIGHT, SLOT_BUTTON_WIDTH};
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::formatting::{format_item_display_name, format_item_stack};
use crate::ui::resources::{InventoryTransferFeedback, OpenContainer};

pub use factory_sim::InventoryPanel;

#[derive(Component)]
pub struct ContainerSlotButton {
    panel: InventoryPanel,
    slot_index: usize,
}

impl ContainerSlotButton {
    /// Which panel this button belongs to, for tests that check how a slot is
    /// drawn rather than what clicking it does.
    pub fn panel(&self) -> InventoryPanel {
        self.panel
    }

    pub fn slot_index(&self) -> usize {
        self.slot_index
    }
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

/// An ordinary slot.
const SLOT_BACKGROUND: Color = Color::srgba(0.14, 0.14, 0.15, 0.96);

/// A wagon cargo slot a player has reserved for one item.
///
/// A reservation has to be visible while the slot is *full*, because filtering
/// a slot to what it already holds is the only reservation the gesture can
/// make: without this, shift-clicking would look like it had done nothing at
/// all. The name of the reserved item shows in the slot once it empties, where
/// there is room for it; until then this is what says the slot is spoken for.
const SLOT_RESERVED_BACKGROUND: Color = Color::srgba(0.26, 0.21, 0.09, 0.96);

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
        BackgroundColor(SLOT_BACKGROUND),
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
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut feedback: ResMut<InventoryTransferFeedback>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    let shift_held = keyboard.as_deref().is_some_and(|keyboard| {
        keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight)
    });

    for (interaction, button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        // The two windows share this grid, so which command a click becomes
        // follows from what is open rather than from the button.
        if let Some(stock_id) = open_container.rolling_stock {
            if shift_held && button.panel == InventoryPanel::RollingStockCargo {
                commands.write(SimCommandRequest(rolling_stock_filter_command(
                    &sim.read(),
                    stock_id,
                    button.slot_index,
                )));
                continue;
            }
            commands.write(SimCommandRequest(SimCommand::TransferRollingStockSlot {
                stock_id,
                panel: button.panel,
                slot_index: button.slot_index,
            }));
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

/// What shift-clicking a wagon cargo slot means: lock the slot to what it is
/// holding, or release a slot that is already locked.
///
/// One gesture for both directions because they are the same intent — "this
/// slot is mine for iron plate" and "it is not any more" — and because the slot
/// itself says which one it is: a filtered slot can only be unfiltered, and an
/// occupied unfiltered slot can only be filtered to what it holds. That leaves
/// exactly one thing an empty unfiltered slot could mean, and the simulation
/// refuses a filter that contradicts a slot's contents anyway, so nothing here
/// has to ask the player to choose an item from a list.
fn rolling_stock_filter_command(
    sim: &factory_sim::Simulation,
    stock_id: factory_sim::RollingStockId,
    slot_index: usize,
) -> SimCommand {
    let filter = if factory_sim::entity_access::rolling_stock_slot_filter(sim, stock_id, slot_index)
        .is_some()
    {
        None
    } else {
        factory_sim::entity_access::rolling_stock_panel_slot(
            sim,
            Some(stock_id),
            InventoryPanel::RollingStockCargo,
            slot_index,
        )
        .map(|stack| stack.item_id())
    };
    SimCommand::SetRollingStockSlotFilter {
        stock_id,
        slot_index,
        filter,
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
    let sim = sim.read();

    for (marker, mut text) in &mut texts {
        let stack = if open_container.rolling_stock.is_some() {
            factory_sim::entity_access::rolling_stock_panel_slot(
                &sim,
                open_container.rolling_stock,
                marker.panel,
                marker.slot_index,
            )
        } else {
            factory_sim::entity_access::inventory_panel_slot(
                &sim,
                open_container.entity_id,
                marker.panel,
                marker.slot_index,
            )
        };
        text.0 = match stack {
            Some(stack) => format_item_stack(stack, sim.catalog()),
            // An emptied slot that a player locked still belongs to its item, so
            // it keeps saying so in parentheses rather than going blank and
            // looking like any other free slot.
            None => {
                rolling_stock_slot_filter_label(&sim, &open_container, marker).unwrap_or_default()
            }
        };
    }
}

/// Tints the wagon cargo slots a player has reserved.
///
/// Kept apart from the slot text because the two live on different entities —
/// the text on the button's child, the colour on the button itself — and
/// because the colour is the half of the answer that survives the slot being
/// full.
pub(crate) fn update_container_slot_reservation_tint(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut buttons: Query<(&ContainerSlotButton, &mut BackgroundColor)>,
) {
    let sim = sim.read();

    for (button, mut background) in &mut buttons {
        let reserved = button.panel == InventoryPanel::RollingStockCargo
            && open_container.rolling_stock.is_some_and(|stock_id| {
                factory_sim::entity_access::rolling_stock_slot_filter(
                    &sim,
                    stock_id,
                    button.slot_index,
                )
                .is_some()
            });
        let wanted = if reserved {
            SLOT_RESERVED_BACKGROUND
        } else {
            SLOT_BACKGROUND
        };
        if background.0 != wanted {
            background.0 = wanted;
        }
    }
}

/// The parenthesised item name an empty but filtered wagon cargo slot shows.
fn rolling_stock_slot_filter_label(
    sim: &factory_sim::Simulation,
    open_container: &OpenContainer,
    marker: &ContainerSlotText,
) -> Option<String> {
    if marker.panel != InventoryPanel::RollingStockCargo {
        return None;
    }
    let filter = factory_sim::entity_access::rolling_stock_slot_filter(
        sim,
        open_container.rolling_stock?,
        marker.slot_index,
    )?;
    Some(format!(
        "({})",
        format_item_display_name(sim.catalog(), filter)
    ))
}

pub fn slot_transfer_error_message(catalog: &PrototypeCatalog, error: SlotTransferError) -> String {
    match error {
        SlotTransferError::RollingStock(error) => rolling_stock_error_message(catalog, error),
        SlotTransferError::Transfer(error) => container_error_message(catalog, error),
        SlotTransferError::MiningDrill(error) => mining_drill_error_message(catalog, error),
        SlotTransferError::Furnace(error) => furnace_error_message(catalog, error),
        SlotTransferError::Boiler(error) => boiler_error_message(catalog, error),
        SlotTransferError::NuclearReactor(error) => nuclear_reactor_error_message(catalog, error),
        SlotTransferError::Roboport(error) => roboport_error_message(catalog, error),
        SlotTransferError::Assembler(error) => assembler_error_message(catalog, error),
        SlotTransferError::Inserter(error) => inserter_error_message(catalog, error),
        SlotTransferError::Module(error) => module_error_message(catalog, error),
    }
}

fn rolling_stock_error_message(
    catalog: &PrototypeCatalog,
    error: RollingStockTransferError,
) -> String {
    match error {
        RollingStockTransferError::MissingStock(_) => "Wagon unavailable".to_string(),
        RollingStockTransferError::NoInventory(_) => "No cargo space".to_string(),
        RollingStockTransferError::NoFuelSlot(_) => "No fuel slot".to_string(),
        RollingStockTransferError::InvalidItem(item_id) => wrong_item_message(catalog, item_id),
        RollingStockTransferError::InvalidSlot { .. } => "Invalid slot".to_string(),
        RollingStockTransferError::EmptySlot { .. } => "Empty slot".to_string(),
        RollingStockTransferError::SlotNotEmpty { .. } => "Empty the slot first".to_string(),
        RollingStockTransferError::InsufficientSpace => "No space".to_string(),
        RollingStockTransferError::UnknownItem => "Unknown item".to_string(),
        RollingStockTransferError::UnsupportedPanel => "Nothing to transfer".to_string(),
    }
}

fn module_error_message(catalog: &PrototypeCatalog, error: ModuleError) -> String {
    match error {
        ModuleError::MissingEntity(_) => "Machine unavailable".to_string(),
        ModuleError::UnsupportedMachine(_) => "Machine has no module slots".to_string(),
        ModuleError::InvalidModule(item_id) => wrong_item_message(catalog, item_id),
        ModuleError::InvalidSlot { .. } => "Invalid slot".to_string(),
        ModuleError::EmptySlot { .. } => "Empty slot".to_string(),
        ModuleError::InsufficientSpace => "Module slots full".to_string(),
    }
}

fn inserter_error_message(catalog: &PrototypeCatalog, error: InserterError) -> String {
    match error {
        InserterError::MissingEntity(_) | InserterError::NotInserter(_) => {
            "Machine unavailable".to_string()
        }
        InserterError::InvalidFuel(item_id) => wrong_item_message(catalog, item_id),
        InserterError::InvalidSlot { .. } => "Invalid slot".to_string(),
        InserterError::EmptySlot { .. } => "Empty slot".to_string(),
        InserterError::InsufficientSpace => "No space".to_string(),
        InserterError::NoFuelSlot => "Electric machine: no fuel slot".to_string(),
        InserterError::UnknownItem => "Unknown item".to_string(),
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

fn mining_drill_error_message(catalog: &PrototypeCatalog, error: MiningDrillError) -> String {
    match error {
        MiningDrillError::MissingEntity(_) | MiningDrillError::NotMiningDrill(_) => {
            "Machine unavailable".to_string()
        }
        MiningDrillError::InvalidFuel(item_id) => wrong_item_message(catalog, item_id),
        MiningDrillError::InvalidSlot { .. } => "Invalid slot".to_string(),
        MiningDrillError::EmptySlot { .. } => "Empty slot".to_string(),
        MiningDrillError::InsufficientSpace => "No space".to_string(),
        MiningDrillError::NoFuelSlot => "Electric machine: no fuel slot".to_string(),
        MiningDrillError::UnknownItem => "Unknown item".to_string(),
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
        FurnaceError::NoFuelSlot => "Electric machine: no fuel slot".to_string(),
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

fn nuclear_reactor_error_message(catalog: &PrototypeCatalog, error: NuclearReactorError) -> String {
    match error {
        NuclearReactorError::MissingEntity(_) | NuclearReactorError::NotNuclearReactor(_) => {
            "Machine unavailable".to_string()
        }
        NuclearReactorError::InvalidFuel(item_id) | NuclearReactorError::InvalidOutput(item_id) => {
            wrong_item_message(catalog, item_id)
        }
        NuclearReactorError::InvalidSlot { .. } => "Invalid slot".to_string(),
        NuclearReactorError::EmptySlot { .. } => "Empty slot".to_string(),
        NuclearReactorError::InsufficientSpace => "No space".to_string(),
        NuclearReactorError::UnknownItem => "Unknown item".to_string(),
    }
}

fn roboport_error_message(catalog: &PrototypeCatalog, error: RoboportError) -> String {
    match error {
        RoboportError::MissingEntity(_) | RoboportError::NotRoboport(_) => {
            "Machine unavailable".to_string()
        }
        // Both robot kinds share this error path when the player tries to
        // store an item that the selected roboport slot does not accept.
        RoboportError::InvalidRobot(item_id) | RoboportError::InvalidMaterial(item_id) => {
            wrong_item_message(catalog, item_id)
        }
        RoboportError::InvalidSlot { .. } => "Invalid slot".to_string(),
        RoboportError::EmptySlot { .. } => "Empty slot".to_string(),
        RoboportError::InsufficientSpace => "No space".to_string(),
        RoboportError::UnknownItem => "Unknown item".to_string(),
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
