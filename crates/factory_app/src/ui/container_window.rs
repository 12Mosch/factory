use bevy::prelude::*;
use factory_sim::EntityId;

use crate::interaction::machine_kind::{OpenMachineKind, open_machine_kind};
use crate::resources::{OpenContainer, SimResource};
use crate::ui::assembler_panel::spawn_assembler_panel;
use crate::ui::inventory_panel::{InventoryPanel, spawn_player_inventory_panel, spawn_slot_button};
use crate::ui::machine_indicators::{spawn_burner_drill_panel, spawn_furnace_panel};

#[derive(Component)]
pub(crate) struct ContainerWindowRoot {
    entity_id: EntityId,
    kind: OpenMachineKind,
}

pub(crate) fn sync_container_window(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut open_container: ResMut<OpenContainer>,
    roots: Query<(Entity, &ContainerWindowRoot)>,
) {
    let open_kind = open_container
        .entity_id
        .and_then(|entity_id| open_machine_kind(&sim.sim, entity_id));
    if open_container.entity_id.is_some() && open_kind.is_none() {
        open_container.entity_id = None;
    }

    if open_container.entity_id.is_none() {
        for (entity, _) in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let entity_id = open_container
        .entity_id
        .expect("open container should be set after validation");
    let kind = open_kind.expect("open machine kind should be known after validation");

    for (entity, root) in &roots {
        if root.entity_id != entity_id || root.kind != kind {
            commands.entity(entity).despawn();
        }
    }

    if roots
        .iter()
        .any(|(_, root)| root.entity_id == entity_id && root.kind == kind)
    {
        return;
    }

    commands
        .spawn((
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
            ContainerWindowRoot { entity_id, kind },
        ))
        .with_children(|root| {
            spawn_player_inventory_panel(root);
            match kind {
                OpenMachineKind::Chest => spawn_chest_panel(root),
                OpenMachineKind::BurnerDrill => spawn_burner_drill_panel(root),
                OpenMachineKind::Furnace => spawn_furnace_panel(root),
                OpenMachineKind::Assembler => {
                    let state = sim
                        .sim
                        .assembler_state(entity_id)
                        .expect("open assembler should expose state");
                    spawn_assembler_panel(root, sim.sim.catalog(), state)
                }
                OpenMachineKind::Lab => {
                    let slot_count = sim
                        .sim
                        .entity_inventory(entity_id)
                        .expect("open lab should expose inventory")
                        .slots
                        .len();
                    spawn_lab_panel(root, slot_count);
                }
            }
        });
}

pub(crate) fn spawn_chest_panel(root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
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
            Text::new("Chest"),
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
                for slot_index in 0..16 {
                    spawn_slot_button(grid, InventoryPanel::Container, slot_index);
                }
            });
    });
}

pub(crate) fn spawn_lab_panel(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
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
            Text::new("Lab"),
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
