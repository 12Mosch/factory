use bevy::prelude::*;

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::ui::production_stats::components::{ProductionStatsSnapshot, ProductionStatsTabButton};
use crate::ui::production_stats::snapshot::production_stats_snapshot;
use crate::ui::production_stats::view::{production_stats_root, spawn_production_stats_contents};
use crate::ui::resources::ProductionStatsWindowState;
use crate::ui::window_sync::{WindowRootQuery, sync_window};

type StatsTabInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static ProductionStatsTabButton),
    (Changed<Interaction>, With<Button>),
>;

pub(crate) fn handle_production_stats_buttons(
    mut buttons: StatsTabInteractionQuery,
    mut state: ResMut<ProductionStatsWindowState>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    if !state.open {
        return;
    }

    for (interaction, button) in &mut buttons {
        if *interaction == Interaction::Pressed {
            sounds.write(SoundEvent::UiClick);
            state.selected_tab = button.tab;
        }
    }
}

pub(crate) fn sync_production_stats_window(
    mut commands: Commands,
    sim: Res<SimResource>,
    state: Res<ProductionStatsWindowState>,
    mut roots: WindowRootQuery<ProductionStatsSnapshot>,
) {
    sync_window(
        &mut commands,
        &mut roots,
        state.open,
        sim.is_changed() || state.is_changed(),
        || production_stats_snapshot(&sim.sim, state.selected_tab),
        production_stats_root,
        spawn_production_stats_contents,
    );
}
