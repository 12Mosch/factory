use bevy::prelude::*;

use crate::map::resources::MapViewState;
use crate::resources::SimResource;

use super::components::{FullMapLayerButton, FullMapRecenterButton};

type FullMapLayerInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static FullMapLayerButton),
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
    mut layer_buttons: FullMapLayerInteractionQuery,
    mut recenter_buttons: FullMapRecenterInteractionQuery,
    sim: Res<SimResource>,
    mut state: ResMut<MapViewState>,
) {
    if !state.open {
        return;
    }

    for (interaction, button) in &mut layer_buttons {
        if *interaction == Interaction::Pressed {
            state.selected_layer = button.layer;
        }
    }

    for interaction in &mut recenter_buttons {
        if *interaction == Interaction::Pressed {
            let (x, y) = sim.sim.player().position_tiles();
            state.center_tile = Vec2::new(x, y);
            state.follow_player = true;
        }
    }
}
