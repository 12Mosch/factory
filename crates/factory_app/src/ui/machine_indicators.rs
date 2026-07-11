use bevy::prelude::*;
use factory_sim::{
    BOILER_FUEL_SLOT_INDEX, BURNER_MINING_DRILL_FUEL_SLOT_INDEX,
    BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX, FURNACE_FUEL_SLOT_INDEX, FURNACE_INPUT_SLOT_INDEX,
    FURNACE_OUTPUT_SLOT_INDEX, MachineStatus,
};

use crate::constants::{MACHINE_BAR_HEIGHT, MACHINE_BAR_WIDTH};
use crate::interaction::machine_kind::{OpenMachineKind, open_machine_kind};
use crate::resources::SimResource;
use crate::ui::inventory_panel::{InventoryPanel, spawn_labeled_slot};
use crate::ui::resources::OpenContainer;

#[derive(Component)]
pub(crate) struct BurnerEnergyText;

#[derive(Component)]
pub(crate) struct BurnerProgressFill;

#[derive(Component)]
pub(crate) struct MachineGuidanceText;

pub(crate) fn spawn_machine_guidance(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    status: MachineStatus,
) {
    root.spawn((
        Text::new(format_machine_guidance(status)),
        TextFont::from_font_size(11.0),
        TextColor(machine_guidance_color(status)),
        TextLayout::justify(Justify::Left),
        Node {
            width: Val::Percent(100.0),
            ..default()
        },
        MachineGuidanceText,
    ));
}

pub(crate) fn format_machine_guidance(status: MachineStatus) -> &'static str {
    match status {
        MachineStatus::Working => "Working — machine is operating normally.",
        MachineStatus::Idle => "Idle — give this machine work to do.",
        MachineStatus::NoRecipe => "Missing recipe — select a recipe above to begin crafting.",
        MachineStatus::NoResearch => {
            "No research — select research or unlock the required technology."
        }
        MachineStatus::NoFuel => "Needs fuel — add a burnable item to the Fuel slot.",
        MachineStatus::NoPower => "No power — connect the machine to a powered electric network.",
        MachineStatus::NoInput => "Missing input — add the required ingredients or resources.",
        MachineStatus::NoFluid => "Missing fluid — connect a pipe carrying the required fluid.",
        MachineStatus::OutputFull => {
            "Output blocked — clear the output or connect space for products."
        }
    }
}

fn machine_guidance_color(status: MachineStatus) -> Color {
    match status {
        MachineStatus::Working => Color::srgb(0.42, 0.84, 0.55),
        MachineStatus::Idle => Color::srgb(0.72, 0.74, 0.72),
        MachineStatus::NoRecipe => Color::srgb(1.0, 0.72, 0.30),
        MachineStatus::NoResearch => Color::srgb(1.0, 0.72, 0.30),
        MachineStatus::NoFuel => Color::srgb(1.0, 0.72, 0.30),
        MachineStatus::NoPower => Color::srgb(1.0, 0.52, 0.30),
        MachineStatus::NoInput => Color::srgb(1.0, 0.72, 0.30),
        MachineStatus::NoFluid => Color::srgb(1.0, 0.72, 0.30),
        MachineStatus::OutputFull => Color::srgb(1.0, 0.72, 0.30),
    }
}

pub(crate) fn update_machine_guidance(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut guidance: Query<(&mut Text, &mut TextColor), With<MachineGuidanceText>>,
) {
    let status = open_container
        .entity_id
        .and_then(|entity_id| sim.read().machine_status_for_entity(entity_id));
    let Some(status) = status else {
        return;
    };

    for (mut text, mut color) in &mut guidance {
        text.0 = format_machine_guidance(status).to_string();
        color.0 = machine_guidance_color(status);
    }
}

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
    let sim = sim.read();
    let indicator =
        open_container
            .entity_id
            .and_then(|entity_id| match open_machine_kind(&sim, entity_id)? {
                OpenMachineKind::BurnerDrill => {
                    let state =
                        factory_sim::entity_access::burner_drill_state(&sim, entity_id).ok()?;
                    Some((
                        state.energy.energy_remaining_joules,
                        state.mining_progress_ticks,
                        state.mining_required_ticks,
                    ))
                }
                OpenMachineKind::Furnace => {
                    let state = factory_sim::entity_access::furnace_state(&sim, entity_id).ok()?;
                    Some((
                        state.energy.energy_remaining_joules,
                        state.crafting_progress_ticks,
                        state.crafting_required_ticks,
                    ))
                }
                OpenMachineKind::Boiler => {
                    let state = factory_sim::entity_access::boiler_state(&sim, entity_id).ok()?;
                    Some((state.energy.energy_remaining_joules, 0, 1))
                }
                OpenMachineKind::Assembler => {
                    let state =
                        factory_sim::entity_access::assembler_state(&sim, entity_id).ok()?;
                    Some((
                        0.0,
                        state.crafting_progress_ticks,
                        state.crafting_required_ticks,
                    ))
                }
                OpenMachineKind::Chest | OpenMachineKind::Lab | OpenMachineKind::Turret => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_guidance_explains_common_blockers_and_resolution() {
        assert_eq!(
            format_machine_guidance(MachineStatus::Working),
            "Working — machine is operating normally."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::Idle),
            "Idle — give this machine work to do."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::NoFuel),
            "Needs fuel — add a burnable item to the Fuel slot."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::NoPower),
            "No power — connect the machine to a powered electric network."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::OutputFull),
            "Output blocked — clear the output or connect space for products."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::NoRecipe),
            "Missing recipe — select a recipe above to begin crafting."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::NoResearch),
            "No research — select research or unlock the required technology."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::NoInput),
            "Missing input — add the required ingredients or resources."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::NoFluid),
            "Missing fluid — connect a pipe carrying the required fluid."
        );
    }
}
