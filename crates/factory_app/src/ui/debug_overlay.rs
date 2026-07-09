use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use factory_sim::{PowerSummary, SimulationCounts};
use std::time::Duration;

use crate::rendering::resources::RenderSyncStats;
use crate::resources::{SimProfileStats, SimResource, UpsStats};

#[derive(Component)]
pub struct DebugOverlayText;

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
    diagnostics: Res<DiagnosticsStore>,
    sim_profile: Res<SimProfileStats>,
    render_sync: Res<RenderSyncStats>,
    mut overlay: Query<&mut Text, With<DebugOverlayText>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|diagnostic| diagnostic.smoothed());
    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|diagnostic| diagnostic.smoothed());
    let counts = sim.read().counts();
    let overlay_text = format_debug_overlay(DebugOverlaySnapshot {
        tick: sim.read().tick_count(),
        ups: stats.ups,
        fps,
        frame_ms,
        sim_profile: &sim_profile,
        render_sync: &render_sync,
        counts,
        power: sim.read().power_summary(),
    });

    for mut text in &mut overlay {
        text.0 = overlay_text.clone();
    }
}

pub struct DebugOverlaySnapshot<'a> {
    pub tick: u64,
    pub ups: f64,
    pub fps: Option<f64>,
    pub frame_ms: Option<f64>,
    pub sim_profile: &'a SimProfileStats,
    pub render_sync: &'a RenderSyncStats,
    pub counts: SimulationCounts,
    pub power: PowerSummary,
}

pub fn format_debug_overlay(snapshot: DebugOverlaySnapshot<'_>) -> String {
    format!(
        "\
Tick: {}
UPS: {:.1}
FPS: {}
Frame: {}
Sim tick: {:.3} ms
Entities: {}
Chunks: {}
Belts: {}
Belt items: {}
Machines: {}
Inserters: {}
Machines active/idle: {}/{}
Power: production {}, consumption {}, satisfaction {:.1}%
Phases: belts {}, fluids {}, power rebuild {}, machines {}, inserters {}, inventory transfers {}, chunk lookup {}, render sync total {} (player {}, world {}, resources {}, entities {}, belt dirs {}, belt items {})",
        snapshot.tick,
        snapshot.ups,
        format_optional(snapshot.fps, "", 1),
        format_optional(snapshot.frame_ms, " ms", 3),
        snapshot.sim_profile.rolling_average_sim_tick_ms,
        snapshot.counts.entity_count,
        snapshot.counts.chunk_count,
        snapshot.counts.belt_count,
        snapshot.counts.belt_item_count,
        snapshot.counts.machine_count,
        snapshot.counts.inserter_count,
        snapshot.counts.active_machines,
        snapshot.counts.idle_machines,
        format_watts(snapshot.power.production_watts),
        format_watts(snapshot.power.consumption_watts),
        f64::from(snapshot.power.satisfaction_permyriad) / 100.0,
        format_duration_ms(snapshot.sim_profile.last_tick.belts),
        format_duration_ms(snapshot.sim_profile.last_tick.fluids),
        format_duration_ms(snapshot.sim_profile.last_tick.power_rebuild),
        format_duration_ms(snapshot.sim_profile.last_tick.machines),
        format_duration_ms(snapshot.sim_profile.last_tick.inserters),
        format_duration_ms(snapshot.sim_profile.last_tick.inventory_transfers),
        format_duration_ms(snapshot.sim_profile.last_tick.chunk_lookup),
        format_duration_ms(snapshot.render_sync.total),
        format_duration_ms(snapshot.render_sync.player),
        format_duration_ms(snapshot.render_sync.world_tiles),
        format_duration_ms(snapshot.render_sync.resources),
        format_duration_ms(snapshot.render_sync.placed_entities),
        format_duration_ms(snapshot.render_sync.belt_directions),
        format_duration_ms(snapshot.render_sync.belt_items),
    )
}

fn format_duration_ms(duration: Duration) -> String {
    format!("{:.3} ms", duration.as_secs_f64() * 1000.0)
}

fn format_optional(value: Option<f64>, suffix: &str, decimals: usize) -> String {
    match value {
        Some(value) => format!("{value:.decimals$}{suffix}"),
        None => "n/a".to_string(),
    }
}

pub fn format_watts(watts: u64) -> String {
    if watts >= 1_000_000 {
        format!("{:.2} MW", watts as f64 / 1_000_000.0)
    } else if watts >= 1_000 {
        format!("{:.1} kW", watts as f64 / 1_000.0)
    } else {
        format!("{watts} W")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::SimulationTickProfile;

    #[test]
    fn debug_overlay_format_includes_required_profiling_labels() {
        let sim_profile = SimProfileStats {
            last_tick: SimulationTickProfile {
                belts: Duration::from_micros(100),
                fluids: Duration::from_micros(200),
                power_rebuild: Duration::from_micros(300),
                machines: Duration::from_micros(400),
                inserters: Duration::from_micros(500),
                inventory_transfers: Duration::from_micros(600),
                chunk_lookup: Duration::from_micros(700),
                ..default()
            },
            rolling_average_sim_tick_ms: 1.25,
            save_blocked_fixed_ticks: 0,
        };
        let mut render_sync = RenderSyncStats::default();
        render_sync.record_player(Duration::from_micros(10));
        render_sync.record_world_tiles(Duration::from_micros(20));
        render_sync.record_resources(Duration::from_micros(30));
        render_sync.record_placed_entities(Duration::from_micros(40));
        render_sync.record_belt_directions(Duration::from_micros(50));
        render_sync.record_belt_items(Duration::from_micros(450));
        let text = format_debug_overlay(DebugOverlaySnapshot {
            tick: 7,
            ups: 60.0,
            fps: Some(59.9),
            frame_ms: Some(16.667),
            sim_profile: &sim_profile,
            render_sync: &render_sync,
            counts: SimulationCounts {
                entity_count: 10,
                chunk_count: 25,
                belt_count: 3,
                belt_item_count: 4,
                machine_count: 5,
                inserter_count: 6,
                active_machines: 2,
                idle_machines: 3,
            },
            power: PowerSummary {
                production_watts: 900_000,
                available_production_watts: 900_000,
                consumption_watts: 75_000,
                satisfaction_permyriad: 10_000,
                network_count: 1,
            },
        });

        for label in [
            "UPS:",
            "FPS:",
            "Frame:",
            "Sim tick:",
            "Entities:",
            "Chunks:",
            "Belts:",
            "Belt items:",
            "Machines:",
            "Inserters:",
            "Machines active/idle:",
            "Power:",
            "belts",
            "fluids",
            "power rebuild",
            "machines",
            "inserters",
            "inventory transfers",
            "chunk lookup",
            "render sync total",
            "player",
            "world",
            "resources",
            "entities",
            "belt dirs",
            "belt items",
        ] {
            assert!(text.contains(label), "missing debug overlay label {label}");
        }
        assert!(!text.contains("Item:"));
        assert!(!text.contains("Count:"));
    }

    #[test]
    fn watts_format_uses_compact_units() {
        assert_eq!(format_watts(400), "400 W");
        assert_eq!(format_watts(15_100), "15.1 kW");
        assert_eq!(format_watts(1_800_000), "1.80 MW");
    }
}
