use bevy::prelude::*;

use crate::input::debug_inventory::selected_inventory_item_state;
use crate::resources::{DebugInventorySelection, SimResource, UpsStats};

#[derive(Component)]
pub(crate) struct DebugOverlayText;

pub(crate) fn setup_debug_overlay(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Px(12.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.02, 0.72)),
            GlobalZIndex(1000),
        ))
        .with_child((
            Text::new("Tick: 0\nUPS: 0.0"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
            DebugOverlayText,
        ));
}

pub(crate) fn update_ups_stats(time: Res<Time<Real>>, mut stats: ResMut<UpsStats>) {
    let delta = time.delta_secs_f64();
    if delta <= 0.0 {
        return;
    }

    stats.elapsed += delta;
    if stats.elapsed >= 1.0 {
        stats.ups = f64::from(stats.fixed_ticks) / stats.elapsed;
        stats.elapsed = 0.0;
        stats.fixed_ticks = 0;
    }
}

pub(crate) fn update_debug_overlay(
    sim: Res<SimResource>,
    stats: Res<UpsStats>,
    inventory_selection: Res<DebugInventorySelection>,
    mut overlay: Query<&mut Text, With<DebugOverlayText>>,
) {
    let catalog = sim.sim.catalog();
    let (selected_name, selected_count) =
        selected_inventory_item_state(&sim.sim, &inventory_selection, catalog);

    for mut text in &mut overlay {
        text.0 = format!(
            "Tick: {}\nUPS: {:.1}\nItem: {}\nCount: {}",
            sim.sim.tick_count(),
            stats.ups,
            selected_name,
            selected_count
        );
    }
}
