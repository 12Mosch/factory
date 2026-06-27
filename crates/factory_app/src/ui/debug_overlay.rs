use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use factory_sim::SimulationCounts;
use std::time::Duration;

use crate::resources::{RenderSyncStats, SimProfileStats, SimResource, UpsStats};

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
    let counts = sim.sim.counts();
    let overlay_text = format_debug_overlay(DebugOverlaySnapshot {
        tick: sim.sim.tick_count(),
        ups: stats.ups,
        fps,
        frame_ms,
        sim_profile: &sim_profile,
        render_sync: &render_sync,
        counts,
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
Phases: belts {}, machines {}, inserters {}, inventory transfers {}, chunk lookup {}, render sync {}",
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
        format_duration_ms(snapshot.sim_profile.last_tick.belts),
        format_duration_ms(snapshot.sim_profile.last_tick.machines),
        format_duration_ms(snapshot.sim_profile.last_tick.inserters),
        format_duration_ms(snapshot.sim_profile.last_tick.inventory_transfers),
        format_duration_ms(snapshot.sim_profile.last_tick.chunk_lookup),
        format_duration_ms(snapshot.render_sync.total),
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

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::SimulationTickProfile;

    #[test]
    fn debug_overlay_format_includes_required_profiling_labels() {
        let sim_profile = SimProfileStats {
            last_tick: SimulationTickProfile {
                belts: Duration::from_micros(100),
                machines: Duration::from_micros(200),
                inserters: Duration::from_micros(300),
                inventory_transfers: Duration::from_micros(400),
                chunk_lookup: Duration::from_micros(500),
                ..default()
            },
            rolling_average_sim_tick_ms: 1.25,
        };
        let render_sync = RenderSyncStats {
            total: Duration::from_micros(600),
            ..default()
        };
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
            "belts",
            "machines",
            "inserters",
            "inventory transfers",
            "chunk lookup",
            "render sync",
        ] {
            assert!(text.contains(label), "missing debug overlay label {label}");
        }
        assert!(!text.contains("Item:"));
        assert!(!text.contains("Count:"));
    }
}
