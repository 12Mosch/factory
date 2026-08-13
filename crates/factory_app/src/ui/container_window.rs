use bevy::prelude::*;
use factory_data::CraftingCategory;
use factory_sim::EntityId;

use crate::interaction::machine_kind::{OpenMachineKind, open_machine_kind};
use crate::placement::build::entity_display_name as prototype_display_name;
use crate::resources::SimResource;
use crate::ui::circuit::panel::{
    spawn_arithmetic_combinator_panel, spawn_circuit_control_panel,
    spawn_constant_combinator_panel, spawn_decider_combinator_panel,
};
use crate::ui::crafting_panel::{CraftingPanelSlots, CraftingPanelSpec, spawn_crafting_panel};
use crate::ui::formatting::{format_recipe_display_name, format_rocket_silo_launch_product_label};
use crate::ui::inventory_panel::{
    InventoryPanel, spawn_inventory_transfer_feedback, spawn_player_inventory_panel,
    spawn_slot_button,
};
use crate::ui::logistics_panel::spawn_logistic_chest_panel;
use crate::ui::machine_indicators::{
    spawn_boiler_panel, spawn_furnace_panel, spawn_heat_buffer_panel, spawn_inserter_panel,
    spawn_machine_guidance, spawn_mining_drill_panel, spawn_nuclear_reactor_panel,
    spawn_roboport_panel,
};
use crate::ui::module_panel::{module_slot_count, spawn_module_panel};
use crate::ui::resources::{InventoryTransferFeedback, OpenContainer};
use crate::ui::train_stop_panel::spawn_train_stop_panel;
use crate::ui::window_sync::{WindowRootQuery, WindowSync, sync_window};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ContainerWindowSnapshot {
    entity_id: EntityId,
    kind: OpenMachineKind,
}

pub(crate) fn sync_container_window(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut open_container: ResMut<OpenContainer>,
    mut feedback: ResMut<InventoryTransferFeedback>,
    mut roots: WindowRootQuery<ContainerWindowSnapshot>,
) {
    let open_kind = open_container
        .entity_id
        .and_then(|entity_id| open_machine_kind(&sim.read(), entity_id));
    if open_container.entity_id.is_some() && open_kind.is_none() {
        open_container.close();
    }
    let open = open_container.entity_id.zip(open_kind);

    let result = sync_window(
        &mut commands,
        &mut roots,
        open.is_some(),
        true,
        || {
            let (entity_id, kind) = open.expect("snapshot is only built while a container is open");
            ContainerWindowSnapshot { entity_id, kind }
        },
        container_window_root,
        |root, snapshot| spawn_container_window_contents(root, &sim.read(), snapshot),
    );
    // Transfer feedback belongs to the container it was produced in, so a
    // window that spawns or switches subject starts with none.
    //
    // Only those two, deliberately: `Closed` is reported on every frame no
    // window is open, and this window is closed for the whole time the
    // rolling-stock window beside it is showing something — clearing on it
    // would wipe that window's message the frame after it was set. Nothing
    // stale survives, because the text this feedback is drawn in is despawned
    // with the window that owned it and the next window to open clears it.
    if matches!(result, WindowSync::Spawned | WindowSync::Rebuilt) {
        feedback.message = None;
    }
}

fn container_window_root() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            top: Val::Px(12.0),
            padding: UiRect::all(Val::Px(10.0)),
            column_gap: Val::Px(10.0),
            align_items: AlignItems::FlexStart,
            ..default()
        },
        BackgroundColor(Color::srgba(0.03, 0.03, 0.035, 0.88)),
        GlobalZIndex(1100),
    )
}

fn spawn_container_window_contents(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    snapshot: &ContainerWindowSnapshot,
) {
    let entity_id = snapshot.entity_id;
    spawn_player_inventory_panel(root);
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(machine_panel_width(snapshot.kind)),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|machine_panel| {
        match snapshot.kind {
            OpenMachineKind::Chest => spawn_container_inventory_panel(
                machine_panel,
                // Logistic chests share the chest window, so the prototype name
                // is what tells a requester apart from a passive provider.
                &entity_display_name(sim, entity_id).unwrap_or_else(|| "Chest".to_string()),
                container_slot_count(sim, entity_id),
            ),
            OpenMachineKind::Lab => spawn_container_inventory_panel(
                machine_panel,
                "Lab",
                container_slot_count(sim, entity_id),
            ),
            OpenMachineKind::Turret => spawn_container_inventory_panel(
                machine_panel,
                "Gun Turret",
                container_slot_count(sim, entity_id),
            ),
            OpenMachineKind::MiningDrill => spawn_mining_drill_panel(machine_panel, sim, entity_id),
            OpenMachineKind::Furnace => spawn_furnace_panel(machine_panel, sim, entity_id),
            OpenMachineKind::Boiler => spawn_boiler_panel(machine_panel),
            OpenMachineKind::NuclearReactor => {
                spawn_nuclear_reactor_panel(machine_panel, sim, entity_id);
            }
            OpenMachineKind::HeatBuffer => spawn_heat_buffer_panel(machine_panel, sim, entity_id),
            OpenMachineKind::Roboport => spawn_roboport_panel(machine_panel, sim, entity_id),
            OpenMachineKind::Inserter => spawn_inserter_panel(machine_panel),
            OpenMachineKind::Beacon => {
                machine_panel.spawn((
                    Text::new("Beacon"),
                    TextFont::from_font_size(14.0),
                    TextColor(Color::WHITE),
                ));
            }
            OpenMachineKind::ConstantCombinator => {
                let slot_count =
                    factory_sim::entity_access::constant_combinator_state(sim, entity_id)
                        .map_or(0, |state| state.slots.len());
                spawn_constant_combinator_panel(machine_panel, slot_count);
            }
            OpenMachineKind::ArithmeticCombinator => {
                spawn_arithmetic_combinator_panel(machine_panel);
            }
            OpenMachineKind::DeciderCombinator => spawn_decider_combinator_panel(machine_panel),
            OpenMachineKind::TrainStop => spawn_train_stop_panel(machine_panel),
            // Circuit-only entities get no other panel, so the heading below
            // carries the entity name.
            OpenMachineKind::Circuit => {
                if let Some(name) = entity_display_name(sim, entity_id) {
                    machine_panel.spawn((
                        Text::new(name),
                        TextFont::from_font_size(14.0),
                        TextColor(Color::WHITE),
                    ));
                }
            }
            OpenMachineKind::Assembler => {
                let prototype = sim
                    .entities()
                    .placed_entity(entity_id)
                    .and_then(|placed| sim.catalog().entity(placed.prototype_id));
                let machine_category = prototype
                    .and_then(|prototype| prototype.assembling_machine.as_ref())
                    .map(|assembling_machine| assembling_machine.crafting_category)
                    .unwrap_or(CraftingCategory::Crafting);
                let title = prototype
                    .map(|prototype| format_recipe_display_name(&prototype.name))
                    .unwrap_or_else(|| "Assembling Machine".to_string());
                spawn_crafting_panel(
                    machine_panel,
                    sim.catalog(),
                    CraftingPanelSpec {
                        title: &title,
                        input: panel_slots(sim, entity_id, InventoryPanel::AssemblerInput),
                        output: Some(panel_slots(sim, entity_id, InventoryPanel::AssemblerOutput)),
                        output_label: "Output",
                        additional_output: None,
                        selectable_category: Some(machine_category),
                    },
                );
            }
            // The same panel without a recipe picker, plus separate cargo and
            // launch-product groups.
            OpenMachineKind::RocketSilo => {
                let title = entity_display_name(sim, entity_id)
                    .unwrap_or_else(|| "Rocket Silo".to_string());
                let launch_product_label = format_rocket_silo_launch_product_label(sim, entity_id)
                    .unwrap_or_else(|| "Launch product".to_string());
                spawn_crafting_panel(
                    machine_panel,
                    sim.catalog(),
                    CraftingPanelSpec {
                        title: &title,
                        input: panel_slots(sim, entity_id, InventoryPanel::RocketSiloInput),
                        output: Some(panel_slots(sim, entity_id, InventoryPanel::RocketSiloCargo)),
                        output_label: "Cargo",
                        additional_output: Some((
                            &launch_product_label,
                            panel_slots(sim, entity_id, InventoryPanel::RocketSiloOutput),
                        )),
                        selectable_category: None,
                    },
                );
            }
        }
        if let Some(logistic_chest) = sim.logistic_chest_prototype(entity_id) {
            spawn_logistic_chest_panel(machine_panel, logistic_chest);
        }
        let module_slots = module_slot_count(sim, entity_id);
        if module_slots > 0 {
            spawn_module_panel(
                machine_panel,
                module_slots,
                snapshot.kind != OpenMachineKind::Beacon,
            );
        }
        // Combinators carry their own editor above; every other connectable
        // entity gets the shared contents/condition controls.
        if let Some(connector) = factory_sim::entity_access::circuit_connector(sim, entity_id)
            && !matches!(
                snapshot.kind,
                OpenMachineKind::ConstantCombinator
                    | OpenMachineKind::ArithmeticCombinator
                    | OpenMachineKind::DeciderCombinator
            )
        {
            spawn_circuit_control_panel(
                machine_panel,
                connector,
                sim.entity_reports_scalar(entity_id),
            );
        }
        if let Some(status) = sim.machine_status_for_entity(entity_id) {
            spawn_machine_guidance(machine_panel, status);
        }
        spawn_inventory_transfer_feedback(machine_panel);
    });
}

fn machine_panel_width(kind: OpenMachineKind) -> f32 {
    match kind {
        OpenMachineKind::Assembler | OpenMachineKind::RocketSilo => 420.0,
        // The combinator editors lay their operands out in one row, so they
        // need more width than an inventory grid.
        OpenMachineKind::ConstantCombinator
        | OpenMachineKind::ArithmeticCombinator
        | OpenMachineKind::DeciderCombinator => 340.0,
        // A logistic chest's request rows carry an item button, an amount, and
        // a stepper, which is wider than a bare inventory grid.
        OpenMachineKind::Chest => 300.0,
        OpenMachineKind::Lab | OpenMachineKind::Turret | OpenMachineKind::Beacon => 260.0,
        // The roboport shows two slot grids side by side under its readouts.
        OpenMachineKind::Roboport => 280.0,
        OpenMachineKind::MiningDrill
        | OpenMachineKind::Furnace
        | OpenMachineKind::Boiler
        | OpenMachineKind::NuclearReactor
        | OpenMachineKind::HeatBuffer
        | OpenMachineKind::Inserter
        // The stop's rows carry a caption, a stepper, and a signal button.
        | OpenMachineKind::TrainStop
        | OpenMachineKind::Circuit => 260.0,
    }
}

fn panel_slots(
    sim: &factory_sim::Simulation,
    entity_id: EntityId,
    panel: InventoryPanel,
) -> CraftingPanelSlots {
    CraftingPanelSlots {
        panel,
        count: factory_sim::entity_access::inventory_panel_slot_count(sim, Some(entity_id), panel),
    }
}

fn entity_display_name(sim: &factory_sim::Simulation, entity_id: EntityId) -> Option<String> {
    let placed = sim.entities().placed_entity(entity_id)?;
    prototype_display_name(sim.catalog(), placed.prototype_id)
}

fn container_slot_count(sim: &factory_sim::Simulation, entity_id: EntityId) -> usize {
    factory_sim::entity_access::inventory_panel_slot_count(
        sim,
        Some(entity_id),
        InventoryPanel::Container,
    )
}

pub(crate) fn spawn_container_inventory_panel(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    title: &str,
    slot_count: usize,
) {
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
            Text::new(title.to_string()),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        panel
            .spawn((
                Node {
                    width: Val::Px(244.0),
                    flex_wrap: FlexWrap::Wrap,
                    row_gap: Val::Px(4.0),
                    column_gap: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|grid| {
                for slot_index in 0..slot_count {
                    spawn_slot_button(grid, InventoryPanel::Container, slot_index);
                }
            });
    });
}
