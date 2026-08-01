//! Manual driving: F8 opens the throttle of the train under the cursor and F10
//! brakes it.
//!
//! Not a debug facility. A train is normally run by its schedule — clicking it
//! opens the editor that writes one — but there is always a train that has to
//! be shunted somewhere no station covers: off a siding it was built on, back
//! down a line it ran out of fuel at the end of, out of the way of track that is
//! being relaid. These two keys are that, and taking hold of a train by hand is
//! a real state rather than a debug back door: it drops whatever the train was
//! doing automatically, including the claim it held on a station, so a train
//! being driven by the player is not also holding a platform against everyone
//! else.
//!
//! F10 rather than the F9 beside the drive key, because F9 is quickload: a
//! player braking a train would have been reloading the world under it.
//!
//! Pressing F8 on a train already driving reverses it, so a train that reached
//! the end of a line can be driven back without being picked up. Any edit in
//! the schedule editor sends the whole schedule again, which is what puts a
//! hand-driven train back under automatic control.
//!
//! Deliberately keys of their own rather than something tied to holding an
//! item, and deliberately routed through the ordinary command queue so a drive
//! command lands on a tick boundary like every other input. The presses
//! themselves are collected in the frame schedule and consumed by the fixed one
//! ([`TrainManualInput`]), because a key edge belongs to a frame: read straight
//! from the keyboard in `FixedUpdate`, one press would fire once or twice
//! depending on how many fixed steps a frame happened to run. Each press
//! carries the tile it was aimed at, so the train that is driven is the one the
//! player pointed at rather than whatever the mouse finished up over.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{RollingStockId, SimCommand, Simulation, TrainId, TrainThrottle, WorldTileCoord};

use crate::input::panels::world_input_blocked;
use crate::input::resources::{AppInputState, TrainManualInput, TrainManualKey};
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::TechnologyWindowState;

const TRAIN_DRIVE_KEY: KeyCode = KeyCode::F8;
const TRAIN_BRAKE_KEY: KeyCode = KeyCode::F10;

/// Player-facing control copy derived from the keys the input system reads.
pub(crate) fn manual_train_controls_hint() -> String {
    format!("{TRAIN_DRIVE_KEY:?} drive / reverse  ·  {TRAIN_BRAKE_KEY:?} brake")
}

/// Collects the manual driving keys during the frame, each with the tile it was
/// aimed at, for the fixed step to consume.
///
/// The cursor is read here rather than where the presses are acted on because
/// where the player was pointing is part of what they pressed. Several frames
/// can pass before a fixed step runs, and by then the mouse has moved — reading
/// it late would aim a press at whatever the cursor found afterwards.
///
/// Presses already waiting are thrown away when the world becomes blocked
/// rather than kept: what the player aimed at is behind a panel now, and the
/// fixed step must not act on it there.
pub(crate) fn collect_train_manual_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    mut pending: ResMut<TrainManualInput>,
) {
    let Some(keyboard) = keyboard.as_deref() else {
        return;
    };
    if train_input_blocked(input_state.as_deref(), technology_window.as_deref()) {
        pending.clear();
        return;
    }
    let presses = [
        (TRAIN_DRIVE_KEY, TrainManualKey::Drive),
        (TRAIN_BRAKE_KEY, TrainManualKey::Brake),
    ]
    .into_iter()
    .filter(|(key, _)| keyboard.just_pressed(*key));
    for (_, key) in presses {
        // A press with nowhere to point is dropped rather than queued: it can
        // never be resolved, and the cursor is the half of it that says what it
        // meant.
        let Some(tile) = cursor_tile_from_window(&windows, &cameras) else {
            return;
        };
        pending.push(key, tile);
    }
}

/// Acts on the presses the frame collected, oldest first and each at the tile it
/// was aimed at.
///
/// One system for both keys rather than one each, because the order they were
/// pressed in is the whole point of collecting them and two systems draining one
/// queue could not preserve it.
pub(crate) fn apply_train_manual_input(
    mut pending: ResMut<TrainManualInput>,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    sim: Res<SimResource>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    // Asked again here, not only where the presses were collected. A panel
    // opening this frame is seen by `PreUpdate`, which runs before the fixed
    // step — but the collector runs *after* it, so a press made a frame earlier
    // would otherwise be acted on behind a panel that is already up before the
    // collector ever gets to throw it away.
    if train_input_blocked(input_state.as_deref(), technology_window.as_deref()) {
        pending.clear();
        return;
    }
    if pending.is_empty() {
        return;
    }
    let sim = sim.read();
    // What each train the presses touched was last asked for. Commands land on a
    // tick boundary, so the simulation still reads the old throttle while this
    // drains; without carrying the answers forward, two presses in one step
    // would both read the same throttle and turn the train round once instead
    // of twice. Kept per train because a drain can hold presses aimed at several
    // of them, in any order.
    let mut driving: Vec<(TrainId, TrainThrottle)> = Vec::new();
    for press in pending.drain() {
        let (x, y) = press.tile;
        let Some(train_id) = train_at_tile(&sim, x, y) else {
            continue;
        };
        let throttle = match press.key {
            TrainManualKey::Brake => TrainThrottle::Brake,
            TrainManualKey::Drive => next_drive_throttle(
                driven_throttle(&driving, train_id)
                    .or_else(|| sim.train(train_id).map(|train| train.throttle)),
            ),
        };
        remember_driven(&mut driving, train_id, throttle);
        commands.write(SimCommandRequest(SimCommand::SetTrainThrottle {
            train_id,
            throttle,
        }));
    }
}

/// Whether the world should hear the manual driving keys at all.
///
/// The technology window is checked alongside the general world-input block
/// because it does not set it, which is why the mining input checks both too.
/// Without it a train could be driven from behind an open full-screen panel,
/// with the cursor pointing at something the player cannot see.
fn train_input_blocked(
    input_state: Option<&AppInputState>,
    technology_window: Option<&TechnologyWindowState>,
) -> bool {
    world_input_blocked(input_state) || technology_window.is_some_and(|state| state.open)
}

/// What a train has already been asked for during this drain, if anything.
///
/// A list rather than a map: a drain holds a handful of presses at most, so a
/// walk over what they touched is shorter than hashing one of them.
fn driven_throttle(
    driving: &[(TrainId, TrainThrottle)],
    train_id: TrainId,
) -> Option<TrainThrottle> {
    driving
        .iter()
        .find(|(driven, _)| *driven == train_id)
        .map(|(_, throttle)| *throttle)
}

fn remember_driven(
    driving: &mut Vec<(TrainId, TrainThrottle)>,
    train_id: TrainId,
    throttle: TrainThrottle,
) {
    match driving.iter_mut().find(|(driven, _)| *driven == train_id) {
        Some((_, driven)) => *driven = throttle,
        None => driving.push((train_id, throttle)),
    }
}

/// What the drive key should ask for next, given what the train is doing now:
/// pull away, or turn around if it is already driving that way. Reversing on a
/// repeat press is what lets a train that reached the end of a line be driven
/// back without being mined off it.
fn next_drive_throttle(throttle: Option<TrainThrottle>) -> TrainThrottle {
    match throttle {
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

    /// A drain can hold presses aimed at several trains in any order, so what
    /// each was last asked for is remembered per train. Carrying only the most
    /// recent answer would make the second press on the first train read the
    /// simulation's stale throttle and pull away again instead of turning round.
    #[test]
    fn each_train_remembers_what_the_last_press_asked_of_it() {
        let (first, second) = (TrainId::new(1), TrainId::new(2));
        let mut driving = Vec::new();
        assert_eq!(driven_throttle(&driving, first), None);

        remember_driven(&mut driving, first, TrainThrottle::Forward);
        remember_driven(&mut driving, second, TrainThrottle::Brake);
        assert_eq!(
            driven_throttle(&driving, first),
            Some(TrainThrottle::Forward)
        );
        assert_eq!(
            driven_throttle(&driving, second),
            Some(TrainThrottle::Brake)
        );

        remember_driven(&mut driving, first, TrainThrottle::Reverse);
        assert_eq!(
            driven_throttle(&driving, first),
            Some(TrainThrottle::Reverse)
        );
        assert_eq!(
            driving.len(),
            2,
            "a train already asked for is updated rather than listed twice"
        );
    }

    /// The drive key pulls away, and pressing it again on a driving train turns
    /// it round rather than doing nothing — including twice inside one fixed
    /// step, which is why the answer is asked of a throttle rather than of the
    /// simulation the commands have not reached yet.
    #[test]
    fn repeating_the_drive_key_reverses_a_train_that_is_already_driving() {
        assert_eq!(next_drive_throttle(None), TrainThrottle::Forward);
        assert_eq!(
            next_drive_throttle(Some(TrainThrottle::Coast)),
            TrainThrottle::Forward
        );
        assert_eq!(
            next_drive_throttle(Some(TrainThrottle::Forward)),
            TrainThrottle::Reverse
        );
        assert_eq!(
            next_drive_throttle(Some(TrainThrottle::Reverse)),
            TrainThrottle::Forward
        );
    }
}
