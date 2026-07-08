use bevy::prelude::*;
use factory_sim::{
    BOILER_FUEL_SLOT_INDEX, BURNER_MINING_DRILL_FUEL_SLOT_INDEX,
    BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX, FURNACE_FUEL_SLOT_INDEX, FURNACE_INPUT_SLOT_INDEX,
    FURNACE_OUTPUT_SLOT_INDEX,
};

use crate::constants::{MACHINE_BAR_HEIGHT, MACHINE_BAR_WIDTH};
use crate::interaction::machine_kind::{OpenMachineKind, open_machine_kind};
use crate::resources::SimResource;
use crate::ui::resources::OpenContainer;
use crate::ui::inventory_panel::{InventoryPanel, spawn_labeled_slot};

#[derive(Component)]
pub(crate) struct BurnerEnergyText;

#[derive(Component)]
pub(crate) struct BurnerProgressFill;

pub(crate) fn spawn_burner_drill_panel(root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(220.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new("Burner Drill"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        panel.spawn((
            Text::new("Energy: 0 J"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            BurnerEnergyText,
        ));
        panel
            .spawn((
                Node {
                    width: Val::Px(MACHINE_BAR_WIDTH),
                    height: Val::Px(MACHINE_BAR_HEIGHT),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.10, 0.10, 0.11, 0.96)),
            ))
            .with_child((
                Node {
                    width: Val::Px(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.33, 0.74, 0.48)),
                BurnerProgressFill,
            ));
        panel
            .spawn((
                Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|slots| {
                spawn_labeled_slot(
                    slots,
                    "Fuel",
                    InventoryPanel::BurnerFuel,
                    BURNER_MINING_DRILL_FUEL_SLOT_INDEX,
                );
                spawn_labeled_slot(
                    slots,
                    "Output",
                    InventoryPanel::BurnerOutput,
                    BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX,
                );
            });
    });
}

pub(crate) fn spawn_furnace_panel(root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(220.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new("Stone Furnace"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        panel.spawn((
            Text::new("Energy: 0 J"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            BurnerEnergyText,
        ));
        panel
            .spawn((
                Node {
                    width: Val::Px(MACHINE_BAR_WIDTH),
                    height: Val::Px(MACHINE_BAR_HEIGHT),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.10, 0.10, 0.11, 0.96)),
            ))
            .with_child((
                Node {
                    width: Val::Px(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.82, 0.48, 0.24)),
                BurnerProgressFill,
            ));
        panel
            .spawn((
                Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|slots| {
                spawn_labeled_slot(
                    slots,
                    "Input",
                    InventoryPanel::FurnaceInput,
                    FURNACE_INPUT_SLOT_INDEX,
                );
                spawn_labeled_slot(
                    slots,
                    "Fuel",
                    InventoryPanel::FurnaceFuel,
                    FURNACE_FUEL_SLOT_INDEX,
                );
                spawn_labeled_slot(
                    slots,
                    "Output",
                    InventoryPanel::FurnaceOutput,
                    FURNACE_OUTPUT_SLOT_INDEX,
                );
            });
    });
}

pub(crate) fn spawn_boiler_panel(root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(220.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new("Boiler"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        panel.spawn((
            Text::new("Energy: 0 J"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            BurnerEnergyText,
        ));
        panel
            .spawn((
                Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|slots| {
                spawn_labeled_slot(
                    slots,
                    "Fuel",
                    InventoryPanel::BoilerFuel,
                    BOILER_FUEL_SLOT_INDEX,
                );
            });
    });
}

pub(crate) fn update_burner_drill_indicators(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut energy_texts: Query<&mut Text, With<BurnerEnergyText>>,
    mut progress_fills: Query<&mut Node, With<BurnerProgressFill>>,
) {
    let indicator = open_container.entity_id.and_then(|entity_id| {
        match open_machine_kind(&sim.sim, entity_id)? {
            OpenMachineKind::BurnerDrill => {
                let state =
                    factory_sim::entity_access::burner_drill_state(&sim.sim, entity_id).ok()?;
                Some((
                    state.energy.energy_remaining_joules,
                    state.mining_progress_ticks,
                    state.mining_required_ticks,
                ))
            }
            OpenMachineKind::Furnace => {
                let state = factory_sim::entity_access::furnace_state(&sim.sim, entity_id).ok()?;
                Some((
                    state.energy.energy_remaining_joules,
                    state.crafting_progress_ticks,
                    state.crafting_required_ticks,
                ))
            }
            OpenMachineKind::Boiler => {
                let state = factory_sim::entity_access::boiler_state(&sim.sim, entity_id).ok()?;
                Some((state.energy.energy_remaining_joules, 0, 1))
            }
            OpenMachineKind::Assembler => {
                let state =
                    factory_sim::entity_access::assembler_state(&sim.sim, entity_id).ok()?;
                Some((
                    0.0,
                    state.crafting_progress_ticks,
                    state.crafting_required_ticks,
                ))
            }
            OpenMachineKind::Chest | OpenMachineKind::Lab => None,
        }
    });

    for mut text in &mut energy_texts {
        text.0 = indicator
            .map(|(energy_remaining_joules, _, _)| {
                format!(
                    "Energy: {} J",
                    energy_remaining_joules.max(0.0).round() as u64
                )
            })
            .unwrap_or_else(|| "Energy: 0 J".to_string());
    }

    for mut node in &mut progress_fills {
        let progress = indicator
            .map(|(_, progress_ticks, required_ticks)| {
                progress_ticks as f32 / required_ticks.max(1) as f32
            })
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        node.width = Val::Px(MACHINE_BAR_WIDTH * progress);
    }
}
