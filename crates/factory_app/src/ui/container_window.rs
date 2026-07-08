use bevy::prelude::*;
use factory_sim::EntityId;

use crate::interaction::machine_kind::{OpenMachineKind, open_machine_kind};
use crate::resources::SimResource;
use crate::ui::assembler_panel::spawn_assembler_panel;
use crate::ui::inventory_panel::{
    InventoryPanel, spawn_inventory_transfer_feedback, spawn_player_inventory_panel,
    spawn_slot_button,
};
use crate::ui::machine_indicators::{
    spawn_boiler_panel, spawn_burner_drill_panel, spawn_furnace_panel,
};
use crate::ui::resources::{InventoryTransferFeedback, OpenContainer};
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
        .and_then(|entity_id| open_machine_kind(&sim.sim, entity_id));
    if open_container.entity_id.is_some() && open_kind.is_none() {
        open_container.entity_id = None;
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
        |root, snapshot| spawn_container_window_contents(root, &sim.sim, snapshot),
    );
    // Transfer feedback belongs to the container it was produced in; drop it
    // whenever the window closed or switched to another container.
    if result != WindowSync::Unchanged {
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
    match snapshot.kind {
        OpenMachineKind::Chest => {
            spawn_container_inventory_panel(root, "Chest", container_slot_count(sim, entity_id))
        }
        OpenMachineKind::Lab => {
            spawn_container_inventory_panel(root, "Lab", container_slot_count(sim, entity_id))
        }
        OpenMachineKind::BurnerDrill => spawn_burner_drill_panel(root),
        OpenMachineKind::Furnace => spawn_furnace_panel(root),
        OpenMachineKind::Boiler => spawn_boiler_panel(root),
        OpenMachineKind::Assembler => {
            let state = factory_sim::entity_access::assembler_state(sim, entity_id)
                .expect("open assembler should expose state");
            spawn_assembler_panel(root, sim.catalog(), state)
        }
    }
    spawn_inventory_transfer_feedback(root);
}

fn container_slot_count(sim: &factory_sim::Simulation, entity_id: EntityId) -> usize {
    factory_sim::entity_access::inventory(sim, entity_id)
        .expect("open container should expose inventory")
        .slots
        .len()
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
