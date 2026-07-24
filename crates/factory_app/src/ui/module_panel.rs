use bevy::prelude::*;
use factory_sim::{EntityId, InventoryPanel, ResolvedModuleEffects};

use crate::constants::{MACHINE_BAR_HEIGHT, MACHINE_BAR_WIDTH};
use crate::resources::SimResource;
use crate::ui::inventory_panel::spawn_slot_button;
use crate::ui::resources::OpenContainer;

#[derive(Component)]
pub(crate) struct ModuleEffectText;

#[derive(Component)]
pub(crate) struct ProductivityProgressFill;

pub(crate) fn spawn_module_panel(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    slot_count: usize,
    productive: bool,
) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(5.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new("Modules"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.78, 0.86, 0.96)),
        ));
        panel
            .spawn((
                Node {
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(4.0),
                    row_gap: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|slots| {
                for slot_index in 0..slot_count {
                    spawn_slot_button(slots, InventoryPanel::Modules, slot_index);
                }
            });
        panel.spawn((
            Text::new(format_module_effects(ResolvedModuleEffects::default())),
            TextFont::from_font_size(10.0),
            TextColor(Color::srgb(0.82, 0.84, 0.86)),
            ModuleEffectText,
        ));
        if productive {
            panel
                .spawn((
                    Node {
                        width: Val::Px(MACHINE_BAR_WIDTH),
                        height: Val::Px(MACHINE_BAR_HEIGHT),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.10, 0.08, 0.13, 0.96)),
                ))
                .with_child((
                    Node {
                        width: Val::Px(0.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.68, 0.34, 0.88)),
                    ProductivityProgressFill,
                ));
        }
    });
}

pub(crate) fn update_module_panel(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut effect_texts: Query<&mut Text, With<ModuleEffectText>>,
    mut progress_fills: Query<&mut Node, With<ProductivityProgressFill>>,
) {
    let sim = sim.read();
    let entity_id = open_container.entity_id;
    let effects = entity_id
        .and_then(|id| factory_sim::entity_access::resolved_module_effects(&sim, id).ok())
        .unwrap_or_default();
    for mut text in &mut effect_texts {
        text.0 = format_module_effects(effects);
    }

    let progress = entity_id
        .and_then(|id| factory_sim::entity_access::productivity_progress_permyriad(&sim, id).ok())
        .unwrap_or(0);
    for mut node in &mut progress_fills {
        node.width = Val::Px(MACHINE_BAR_WIDTH * progress as f32 / 10_000.0);
    }
}

pub(crate) fn format_module_effects(effects: ResolvedModuleEffects) -> String {
    format!(
        "Speed: {}%\nProductivity: +{}%\nActive energy: {}%\nPollution: {}%",
        effects.speed_multiplier_permyriad() as f64 / 100.0,
        effects.productivity_permyriad() as f64 / 100.0,
        effects.energy_multiplier_permyriad() as f64 / 100.0,
        effects.pollution_multiplier_permyriad() as f64 / 100.0,
    )
}

pub(crate) fn module_slot_count(sim: &factory_sim::Simulation, entity_id: EntityId) -> usize {
    factory_sim::entity_access::inventory_panel_slot_count(
        sim,
        Some(entity_id),
        InventoryPanel::Modules,
    )
}
