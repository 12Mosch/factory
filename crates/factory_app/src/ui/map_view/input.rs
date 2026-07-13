use bevy::prelude::*;

use crate::map::resources::{MapDisplaySettings, MapViewState};
use crate::resources::SimResource;

use super::components::{FullMapOverlayButton, FullMapRecenterButton};

type FullMapOverlayInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static FullMapOverlayButton),
    (
        Changed<Interaction>,
        With<Button>,
        Without<FullMapRecenterButton>,
    ),
>;

type FullMapRecenterInteractionQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<FullMapRecenterButton>,
    ),
>;

pub(crate) fn handle_full_map_buttons(
    mut overlay_buttons: FullMapOverlayInteractionQuery,
    mut recenter_buttons: FullMapRecenterInteractionQuery,
    sim: Res<SimResource>,
    mut state: ResMut<MapViewState>,
    mut settings: ResMut<MapDisplaySettings>,
) {
    if !state.open {
        return;
    }

    for (interaction, button) in &mut overlay_buttons {
        if *interaction == Interaction::Pressed {
            settings.overlays.toggle(button.overlay);
        }
    }

    for interaction in &mut recenter_buttons {
        if *interaction == Interaction::Pressed {
            let (x, y) = sim.read().player().position_tiles();
            state.center_tile = Vec2::new(x, y);
            state.follow_player = true;
        }
    }
}
