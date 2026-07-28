//! Debug driving: F8 opens the throttle of the train under the cursor, F9
//! brakes it.
//!
//! Trains have no schedules, no signals, and nowhere to be — nothing in normal
//! play makes one move. These keys are how the motion model is exercised in the
//! running game: put a locomotive on a run of track, hover it, and press F8 to
//! watch it pull away and stop at the end of the line. Pressing F8 on a train
//! already driving reverses it, so a train that ran out of track can be driven
//! back without picking it up.
//!
//! Deliberately keys of their own rather than something tied to holding an
//! item, and deliberately routed through the ordinary command queue so a drive
//! command lands on a tick boundary like every other input.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{RollingStockId, SimCommand, Simulation, TrainId, TrainThrottle, WorldTileCoord};

use crate::input::panels::world_input_blocked;
use crate::input::resources::AppInputState;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::TechnologyWindowState;

pub(crate) fn drive_train_from_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    sim: Res<SimResource>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    let Some(keyboard) = keyboard.as_deref() else {
        return;
    };
    let drive = keyboard.just_pressed(KeyCode::F8);
    let brake = keyboard.just_pressed(KeyCode::F9);
    // The technology window is checked alongside the general world-input block
    // because it does not set it, which is why the mining input checks both
    // too. Without it a train could be driven from behind an open full-screen
    // panel, with the cursor pointing at something the player cannot see.
    if (!drive && !brake)
        || world_input_blocked(input_state.as_deref())
        || technology_window.as_deref().is_some_and(|state| state.open)
    {
        return;
    }
    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        return;
    };

    let sim = sim.read();
    let Some(train_id) = train_at_tile(&sim, x, y) else {
        return;
    };
    let throttle = if brake {
        TrainThrottle::Brake
    } else {
        next_drive_throttle(&sim, train_id)
    };
    commands.write(SimCommandRequest(SimCommand::SetTrainThrottle {
        train_id,
        throttle,
    }));
}

/// What F8 should ask for next: pull away, or turn around if the train is
/// already driving that way. Reversing on a repeat press is what lets a train
/// that reached the end of a line be driven back without being mined off it.
fn next_drive_throttle(sim: &Simulation, train_id: TrainId) -> TrainThrottle {
    match sim.train(train_id).map(|train| train.throttle) {
        Some(TrainThrottle::Forward) => TrainThrottle::Reverse,
        _ => TrainThrottle::Forward,
    }
}

/// The train whose stock covers `(x, y)`, choosing the lowest stock id when
/// two pieces share a tile so the answer never depends on iteration order.
pub(crate) fn train_at_tile(
    sim: &Simulation,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> Option<TrainId> {
    stock_at_tile(sim, x, y)
        .and_then(|stock_id| sim.rolling_stock_piece(stock_id).map(|stock| stock.train))
}

/// The piece of rolling stock standing on `(x, y)`.
///
/// Rolling stock is not in the occupancy grid — it sits between tiles — so
/// there is no index to ask. The scan is over the world's stock, which numbers
/// in the tens, and only runs on a key press or a click rather than per frame.
pub(crate) fn stock_at_tile(
    sim: &Simulation,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> Option<RollingStockId> {
    sim.rolling_stock()
        .filter(|stock| sim.rolling_stock_covers_tile(stock.id, x, y))
        .map(|stock| stock.id)
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursor_finds_the_train_standing_on_a_tile() {
        let sim = Simulation::new_rolling_stock_fixture(1);
        let stock = sim
            .rolling_stock()
            .next()
            .expect("the fixture placed stock");
        let (x, y) = sim
            .rolling_stock_tile(stock.id)
            .expect("placed stock has a world tile");

        assert_eq!(train_at_tile(&sim, x, y), Some(stock.train));
        assert_eq!(
            train_at_tile(&sim, x + 64, y + 64),
            None,
            "a tile far from any track holds no train"
        );
    }

    /// F8 pulls away, and pressing it again on a driving train turns it round
    /// rather than doing nothing.
    #[test]
    fn repeating_the_drive_key_reverses_a_train_that_is_already_driving() {
        let mut sim = Simulation::new_rolling_stock_fixture(1);
        let train_id = sim.trains().next().expect("the fixture placed a train").id;

        sim.set_train_throttle(train_id, TrainThrottle::Coast)
            .expect("the train takes a throttle command");
        assert_eq!(next_drive_throttle(&sim, train_id), TrainThrottle::Forward);

        sim.set_train_throttle(train_id, TrainThrottle::Forward)
            .expect("the train takes a throttle command");
        assert_eq!(next_drive_throttle(&sim, train_id), TrainThrottle::Reverse);
    }
}
