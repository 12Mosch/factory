use bevy::prelude::*;
use factory_data::{DamageType, ItemId};
use factory_sim::{InstalledEquipment, PlayerEquipmentError, SimCommand, SimCommandError};

use crate::audio::SoundEvent;
use crate::constants::{SLOT_BUTTON_HEIGHT, SLOT_BUTTON_WIDTH};
use crate::resources::SimResource;
use crate::simulation::{SimCommandRequest, SimCommandResult};
use crate::ui::formatting::{format_item_display_name, format_item_stack};
use crate::ui::resources::EquipmentWindowState;
use crate::ui::window_sync::{WindowRootQuery, sync_window};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EquipmentWindowSnapshot {
    armor: Option<(ItemId, String, String, u8, u8)>,
    installed: Vec<EquipmentTopology>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EquipmentTopology {
    installed: InstalledEquipment,
    name: String,
    width: u8,
    height: u8,
}

#[derive(Component)]
pub struct EquipmentInventoryButton {
    pub slot_index: usize,
}

#[derive(Component)]
pub(crate) struct EquipmentInventoryText {
    slot_index: usize,
}

#[derive(Component)]
pub struct EquipmentArmorSlotButton;

#[derive(Component)]
pub(crate) struct EquipmentArmorSlotText;

#[derive(Component)]
pub struct EquipmentGridCellButton {
    pub x: u8,
    pub y: u8,
}

#[derive(Component)]
pub(crate) struct EquipmentEnergyText;

#[derive(Component)]
pub(crate) struct EquipmentShieldText;

#[derive(Component)]
pub(crate) struct EquipmentFeedbackText;

type InventoryInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static EquipmentInventoryButton),
    (Changed<Interaction>, With<Button>),
>;
type GridInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static EquipmentGridCellButton),
    (Changed<Interaction>, With<Button>),
>;
type ArmorInteractionQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<EquipmentArmorSlotButton>,
    ),
>;
type ArmorTextQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<EquipmentArmorSlotText>,
        Without<EquipmentInventoryText>,
        Without<EquipmentEnergyText>,
        Without<EquipmentShieldText>,
        Without<EquipmentFeedbackText>,
    ),
>;
type EnergyTextQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<EquipmentEnergyText>,
        Without<EquipmentArmorSlotText>,
        Without<EquipmentShieldText>,
        Without<EquipmentFeedbackText>,
    ),
>;
type ShieldTextQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<EquipmentShieldText>,
        Without<EquipmentArmorSlotText>,
        Without<EquipmentEnergyText>,
        Without<EquipmentFeedbackText>,
    ),
>;
type FeedbackTextQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<EquipmentFeedbackText>,
        Without<EquipmentArmorSlotText>,
        Without<EquipmentEnergyText>,
        Without<EquipmentShieldText>,
    ),
>;
type InventoryTextQuery<'w, 's> = Query<
    'w,
    's,
    (&'static EquipmentInventoryText, &'static mut Text),
    (
        Without<EquipmentArmorSlotText>,
        Without<EquipmentEnergyText>,
        Without<EquipmentShieldText>,
        Without<EquipmentFeedbackText>,
    ),
>;

pub(crate) fn sync_equipment_window(
    mut commands: Commands,
    window: Res<EquipmentWindowState>,
    sim: Res<SimResource>,
    mut roots: WindowRootQuery<EquipmentWindowSnapshot>,
) {
    sync_window(
        &mut commands,
        &mut roots,
        window.open,
        true,
        || topology_snapshot(&sim.read()),
        equipment_window_root,
        spawn_equipment_contents,
    );
}

pub(crate) fn handle_equipment_buttons(
    mut inventory_buttons: InventoryInteractionQuery,
    mut armor_buttons: ArmorInteractionQuery,
    mut grid_buttons: GridInteractionQuery,
    mut window: ResMut<EquipmentWindowState>,
    sim: Res<SimResource>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    if !window.open {
        return;
    }
    for (interaction, button) in &mut inventory_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let sim = sim.read();
        let Some(stack) = sim.player_inventory().slot(button.slot_index) else {
            window.feedback = Some("Empty inventory slot".into());
            continue;
        };
        let Some(item) = sim.catalog().item(stack.item_id()) else {
            window.feedback = Some("Unknown item".into());
            continue;
        };
        if item.armor.is_some() {
            commands.write(SimCommandRequest(SimCommand::EquipArmor {
                inventory_slot: button.slot_index,
            }));
            sounds.write(SoundEvent::UiClick);
        } else if item.equipment.is_some() {
            window.selected_inventory_slot = Some(button.slot_index);
            window.feedback = Some(format!(
                "Selected {} — choose a grid cell",
                format_item_display_name(sim.catalog(), item.id)
            ));
            sounds.write(SoundEvent::UiClick);
        } else {
            window.feedback = Some("Select armor or powered equipment".into());
        }
    }
    for interaction in &mut armor_buttons {
        if *interaction == Interaction::Pressed {
            commands.write(SimCommandRequest(SimCommand::UnequipArmor));
            sounds.write(SoundEvent::UiClick);
        }
    }
    for (interaction, cell) in &mut grid_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let sim = sim.read();
        if installed_at_cell(&sim, cell.x, cell.y).is_some() {
            commands.write(SimCommandRequest(SimCommand::RemoveEquipment {
                x: cell.x,
                y: cell.y,
            }));
            sounds.write(SoundEvent::UiClick);
        } else if let Some(inventory_slot) = window.selected_inventory_slot {
            commands.write(SimCommandRequest(SimCommand::InstallEquipment {
                inventory_slot,
                x: cell.x,
                y: cell.y,
            }));
            sounds.write(SoundEvent::UiClick);
        } else {
            window.feedback = Some("Select equipment from the inventory first".into());
        }
    }
}

pub(crate) fn handle_equipment_command_results(
    mut results: MessageReader<SimCommandResult>,
    mut window: ResMut<EquipmentWindowState>,
) {
    for outcome in results.read() {
        let is_equipment_command = matches!(
            outcome.command,
            SimCommand::EquipArmor { .. }
                | SimCommand::UnequipArmor
                | SimCommand::InstallEquipment { .. }
                | SimCommand::RemoveEquipment { .. }
        );
        if !is_equipment_command {
            continue;
        }
        match outcome.result {
            Ok(_) => {
                if matches!(outcome.command, SimCommand::InstallEquipment { .. }) {
                    window.selected_inventory_slot = None;
                }
                window.last_error = None;
                window.feedback = Some("Equipment updated".into());
            }
            Err(SimCommandError::Equipment(error)) => {
                window.last_error = Some(error);
                window.feedback = Some(equipment_error_message(error).into());
            }
            Err(_) => {}
        }
    }
}

pub(crate) fn update_equipment_window_text(
    sim: Res<SimResource>,
    window: Res<EquipmentWindowState>,
    mut inventory_texts: InventoryTextQuery,
    mut armor_texts: ArmorTextQuery,
    mut energy_texts: EnergyTextQuery,
    mut shield_texts: ShieldTextQuery,
    mut feedback_texts: FeedbackTextQuery,
) {
    if !window.open {
        return;
    }
    let sim = sim.read();
    for (marker, mut text) in &mut inventory_texts {
        text.0 = sim
            .player_inventory()
            .slot(marker.slot_index)
            .map(|stack| format_item_stack(stack, sim.catalog()))
            .unwrap_or_default();
    }
    let armor_name = sim
        .equipped_armor()
        .and_then(|item_id| sim.catalog().item(item_id))
        .map(|item| format_item_display_name(sim.catalog(), item.id))
        .unwrap_or_else(|| "Empty armor slot".into());
    for mut text in &mut armor_texts {
        text.0 = armor_name.clone();
    }
    let (energy, capacity) = sim.personal_stored_energy();
    for mut text in &mut energy_texts {
        text.0 = format!("Stored energy: {energy} / {capacity} J");
    }
    let (shield, shield_capacity) = sim.personal_shield_points();
    for mut text in &mut shield_texts {
        text.0 = format!("Shield: {shield} / {shield_capacity} points");
    }
    let feedback = window.feedback.as_deref().unwrap_or_default();
    for mut text in &mut feedback_texts {
        text.0 = feedback.into();
    }
}

pub(crate) fn update_equipment_selection_colors(
    window: Res<EquipmentWindowState>,
    mut buttons: Query<(&EquipmentInventoryButton, &mut BackgroundColor)>,
) {
    if !window.is_changed() {
        return;
    }
    for (button, mut color) in &mut buttons {
        color.0 = if window.selected_inventory_slot == Some(button.slot_index) {
            Color::srgba(0.10, 0.46, 0.62, 0.98)
        } else {
            Color::srgba(0.12, 0.14, 0.17, 0.96)
        };
    }
}

fn topology_snapshot(sim: &factory_sim::Simulation) -> EquipmentWindowSnapshot {
    let armor = sim.equipped_armor().and_then(|item_id| {
        let item = sim.catalog().item(item_id)?;
        let armor = item.armor.as_ref()?;
        let resistance = if armor.resistances.is_empty() {
            "No resistance".into()
        } else {
            armor
                .resistances
                .iter()
                .map(|resistance| {
                    format!(
                        "{}: -{} then -{}%",
                        damage_type_name(resistance.damage_type),
                        resistance.flat_reduction,
                        resistance.percent_reduction_permyriad / 100
                    )
                })
                .collect::<Vec<_>>()
                .join(" · ")
        };
        Some((
            item_id,
            format_item_display_name(sim.catalog(), item_id),
            resistance,
            armor.grid_width,
            armor.grid_height,
        ))
    });
    let installed = sim
        .installed_equipment()
        .iter()
        .filter_map(|installed| {
            let item = sim.catalog().item(installed.item_id)?;
            let equipment = item.equipment?;
            Some(EquipmentTopology {
                installed: *installed,
                name: format_item_display_name(sim.catalog(), installed.item_id),
                width: equipment.width,
                height: equipment.height,
            })
        })
        .collect();
    EquipmentWindowSnapshot { armor, installed }
}

fn equipment_window_root() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.02, 0.04, 0.64)),
        GlobalZIndex(2750),
    )
}

fn spawn_equipment_contents(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &EquipmentWindowSnapshot,
) {
    root.spawn((
        Node {
            width: Val::Vw(92.0),
            max_width: Val::Px(980.0),
            max_height: Val::Vh(92.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            padding: UiRect::all(Val::Px(16.0)),
            border: UiRect::all(Val::Px(1.0)),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.025, 0.045, 0.060, 0.99)),
        BorderColor::all(Color::srgba(0.18, 0.68, 0.82, 0.82)),
    ))
    .with_children(|modal| {
        modal.spawn((
            Text::new("Powered Combat Equipment  [E]"),
            TextFont::from_font_size(20.0),
            TextColor(Color::srgb(0.72, 0.94, 1.0)),
        ));
        modal
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(18.0),
                    row_gap: Val::Px(14.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|body| {
                spawn_equipment_inventory(body);
                spawn_armor_and_grid(body, snapshot);
            });
        modal.spawn((
            Text::new(""),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.96, 0.72, 0.32)),
            EquipmentFeedbackText,
        ));
    });
}

fn spawn_equipment_inventory(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    parent
        .spawn((
            Node {
                width: Val::Px(500.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("Player inventory — click armor to equip, equipment to select"),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.76, 0.84, 0.86)),
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
                            BackgroundColor(Color::srgba(0.12, 0.14, 0.17, 0.96)),
                            EquipmentInventoryButton { slot_index },
                        ))
                        .with_child((
                            Text::new(""),
                            TextFont::from_font_size(9.0),
                            TextColor(Color::WHITE),
                            TextLayout::justify(Justify::Center),
                            EquipmentInventoryText { slot_index },
                        ));
                    }
                });
        });
}

fn spawn_armor_and_grid(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &EquipmentWindowSnapshot,
) {
    parent
        .spawn((
            Node {
                width: Val::Px(370.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("Armor"),
                TextFont::from_font_size(14.0),
                TextColor(Color::WHITE),
            ));
            panel
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(240.0),
                        height: Val::Px(40.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.08, 0.22, 0.29, 0.96)),
                    BorderColor::all(Color::srgba(0.20, 0.72, 0.86, 0.72)),
                    EquipmentArmorSlotButton,
                ))
                .with_child((
                    Text::new(""),
                    TextFont::from_font_size(11.0),
                    TextColor(Color::WHITE),
                    EquipmentArmorSlotText,
                ));
            if let Some((_, _, resistance, width, height)) = &snapshot.armor {
                panel.spawn((
                    Text::new(resistance.clone()),
                    TextFont::from_font_size(11.0),
                    TextColor(Color::srgb(0.70, 0.82, 0.84)),
                ));
                panel
                    .spawn((
                        Node {
                            width: Val::Px(f32::from(*width) * 62.0),
                            flex_wrap: FlexWrap::Wrap,
                            row_gap: Val::Px(4.0),
                            column_gap: Val::Px(4.0),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ))
                    .with_children(|grid| {
                        for y in 0..*height {
                            for x in 0..*width {
                                let topology = topology_at_cell(snapshot, x, y);
                                let label = topology.map_or_else(String::new, |topology| {
                                    if topology.installed.x == x && topology.installed.y == y {
                                        format!(
                                            "{}\n{}×{}",
                                            topology.name, topology.width, topology.height
                                        )
                                    } else {
                                        "■".into()
                                    }
                                });
                                grid.spawn((
                                    Button,
                                    Node {
                                        width: Val::Px(58.0),
                                        height: Val::Px(58.0),
                                        align_items: AlignItems::Center,
                                        justify_content: JustifyContent::Center,
                                        border: UiRect::all(Val::Px(1.0)),
                                        padding: UiRect::all(Val::Px(2.0)),
                                        ..default()
                                    },
                                    BackgroundColor(if topology.is_some() {
                                        Color::srgba(0.08, 0.48, 0.64, 0.96)
                                    } else {
                                        Color::srgba(0.06, 0.10, 0.13, 0.96)
                                    }),
                                    BorderColor::all(Color::srgba(0.18, 0.54, 0.64, 0.72)),
                                    EquipmentGridCellButton { x, y },
                                ))
                                .with_child((
                                    Text::new(label),
                                    TextFont::from_font_size(8.0),
                                    TextColor(Color::WHITE),
                                    TextLayout::justify(Justify::Center),
                                ));
                            }
                        }
                    });
            } else {
                panel.spawn((
                    Text::new("Equip modular armor to enable its equipment grid."),
                    TextFont::from_font_size(11.0),
                    TextColor(Color::srgb(0.65, 0.68, 0.69)),
                ));
            }
            panel.spawn((
                Text::new(""),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.72, 0.92, 0.98)),
                EquipmentEnergyText,
            ));
            panel.spawn((
                Text::new(""),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.42, 0.84, 1.0)),
                EquipmentShieldText,
            ));
        });
}

fn topology_at_cell(
    snapshot: &EquipmentWindowSnapshot,
    x: u8,
    y: u8,
) -> Option<&EquipmentTopology> {
    snapshot.installed.iter().find(|topology| {
        x >= topology.installed.x
            && x < topology.installed.x.saturating_add(topology.width)
            && y >= topology.installed.y
            && y < topology.installed.y.saturating_add(topology.height)
    })
}

fn installed_at_cell(sim: &factory_sim::Simulation, x: u8, y: u8) -> Option<InstalledEquipment> {
    sim.installed_equipment().iter().copied().find(|installed| {
        sim.catalog()
            .item(installed.item_id)
            .and_then(|item| item.equipment)
            .is_some_and(|equipment| {
                x >= installed.x
                    && x < installed.x.saturating_add(equipment.width)
                    && y >= installed.y
                    && y < installed.y.saturating_add(equipment.height)
            })
    })
}

fn damage_type_name(damage_type: DamageType) -> &'static str {
    match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Explosion => "Explosion",
        DamageType::Acid => "Acid",
        DamageType::Laser => "Laser",
    }
}

fn equipment_error_message(error: PlayerEquipmentError) -> &'static str {
    match error {
        PlayerEquipmentError::InvalidInventorySlot { .. } => "Invalid inventory slot",
        PlayerEquipmentError::EmptyInventorySlot { .. } => "Inventory slot is empty",
        PlayerEquipmentError::NotArmor(_) => "That item is not armor",
        PlayerEquipmentError::NotEquipment(_) => "That item is not equipment",
        PlayerEquipmentError::NoArmorEquipped => "No armor equipped",
        PlayerEquipmentError::ArmorGridNotEmpty => "Remove installed equipment first",
        PlayerEquipmentError::PlacementOutOfBounds => "Equipment does not fit there",
        PlayerEquipmentError::PlacementOverlaps => "Equipment overlaps another module",
        PlayerEquipmentError::NoEquipmentAtCell { .. } => "No equipment in that cell",
        PlayerEquipmentError::InventoryFull => "Player inventory is full",
    }
}
