//! Debug driving: F8 opens the throttle of the train under the cursor, F9
//! brakes it, and F10 sends it somewhere.
//!
//! Trains have no schedules and no signals yet — nothing in normal play gives
//! one somewhere to be. These keys are how the motion model and the route
//! search are exercised in the running game: put a locomotive on a run of
//! track, hover it, and press F8 to watch it pull away and stop at the end of
//! the line. Pressing F8 on a train already driving reverses it, so a train
//! that ran out of track can be driven back without picking it up.
//!
//! F10 is two presses because it needs two things the cursor cannot name at
//! once: press it over a train to pick that train, then over a rail to send it
//! there. Pressing it over the picked train again puts it down. What the train
//! then does — which way round it sets off, where it turns round, where it
//! stops — is the route search's answer, not this file's.
//!
//! Deliberately keys of their own rather than something tied to holding an
//! item, and deliberately routed through the ordinary command queue so a drive
//! command lands on a tick boundary like every other input. The presses
//! themselves are collected in the frame schedule and consumed by the fixed one
//! ([`TrainDebugInput`]), because a key edge belongs to a frame: read straight
//! from the keyboard in `FixedUpdate`, one press would fire once or twice
//! depending on how many fixed steps a frame happened to run.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{
    EntityId, RollingStockId, SimCommand, Simulation, TrainId, TrainThrottle, WorldTileCoord,
};

use crate::input::panels::world_input_blocked;
use crate::input::resources::{
    AppInputState, TrainDebugInput, TrainDebugPress, TrainRoutingSelection,
};
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::TechnologyWindowState;

/// Collects the debug train keys during the frame, for the fixed step to
/// consume.
///
/// The technology window is checked alongside the general world-input block
/// because it does not set it, which is why the mining input checks both too.
/// Without it a train could be driven from behind an open full-screen panel,
/// with the cursor pointing at something the player cannot see. Presses already
/// waiting are thrown away when the world becomes blocked rather than kept:
/// what the player pressed at is behind a panel now, and the fixed step must
/// not act on it there.
pub(crate) fn collect_train_debug_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    mut pending: ResMut<TrainDebugInput>,
) {
    let Some(keyboard) = keyboard.as_deref() else {
        return;
    };
    if world_input_blocked(input_state.as_deref())
        || technology_window.as_deref().is_some_and(|state| state.open)
    {
        pending.clear();
        return;
    }
    for (key, press) in [
        (KeyCode::F8, TrainDebugPress::Drive),
        (KeyCode::F9, TrainDebugPress::Brake),
        (KeyCode::F10, TrainDebugPress::Route),
    ] {
        if keyboard.just_pressed(key) {
            pending.push(press);
        }
    }
}

/// Acts on the presses the frame collected, oldest first.
///
/// One system for all three keys rather than one each, because the order they
/// were pressed in is the whole point of collecting them and two systems
/// draining one queue could not preserve it.
pub(crate) fn apply_train_debug_input(
    mut pending: ResMut<TrainDebugInput>,
    sim: Res<SimResource>,
    mut selection: ResMut<TrainRoutingSelection>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    if pending.is_empty() {
        return;
    }
    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        // A press with nowhere to point is still a press that happened. Keeping
        // it would fire it later at a cursor somewhere else entirely.
        pending.clear();
        return;
    };

    let sim = sim.read();
    // What the last drive press asked of the train under the cursor. Commands
    // land on a tick boundary, so the simulation still reads the old throttle
    // while this drains; without carrying the answer forward, two presses in one
    // step would both read the same throttle and turn the train round once
    // instead of twice.
    let mut driving: Option<TrainThrottle> = None;
    for press in pending.drain() {
        match press {
            TrainDebugPress::Drive | TrainDebugPress::Brake => {
                let Some(train_id) = train_at_tile(&sim, x, y) else {
                    continue;
                };
                let throttle = match press {
                    TrainDebugPress::Brake => TrainThrottle::Brake,
                    _ => next_drive_throttle(
                        driving.or_else(|| sim.train(train_id).map(|train| train.throttle)),
                    ),
                };
                driving = Some(throttle);
                commands.write(SimCommandRequest(SimCommand::SetTrainThrottle {
                    train_id,
                    throttle,
                }));
            }
            TrainDebugPress::Route => match routing_action(&sim, selection.train, x, y) {
                Some(TrainRoutingAction::Pick(train_id)) => selection.train = Some(train_id),
                Some(TrainRoutingAction::Release) => selection.train = None,
                Some(TrainRoutingAction::SendTo { train_id, rail }) => {
                    selection.train = None;
                    commands.write(SimCommandRequest(SimCommand::SetTrainDestination {
                        train_id,
                        rail,
                    }));
                }
                None => {}
            },
        }
    }
}

/// What pressing the routing key over `(x, y)` should do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrainRoutingAction {
    Pick(TrainId),
    /// The picked train was pressed again: it is put back down rather than
    /// being sent to the rail it happens to be standing on, which is a journey
    /// nobody asks for.
    Release,
    SendTo {
        train_id: TrainId,
        rail: EntityId,
    },
}

/// The train under the cursor wins over the rail under it, because stock always
/// stands on track and the other reading would make picking a train impossible.
fn routing_action(
    sim: &Simulation,
    picked: Option<TrainId>,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> Option<TrainRoutingAction> {
    if let Some(train_id) = train_at_tile(sim, x, y) {
        return Some(if picked == Some(train_id) {
            TrainRoutingAction::Release
        } else {
            TrainRoutingAction::Pick(train_id)
        });
    }
    let train_id = picked?;
    let rail = sim
        .entities()
        .occupancy()
        .entity_at(x, y)
        .filter(|entity_id| sim.rail_piece_geometry(*entity_id).is_some())?;
    Some(TrainRoutingAction::SendTo { train_id, rail })
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

    /// The two presses the routing key needs: one on a train to pick it, one on
    /// a rail to send it there.
    #[test]
    fn the_routing_key_picks_a_train_and_then_sends_it_to_a_rail() {
        let sim = Simulation::new_rolling_stock_fixture(1);
        let stock = sim
            .rolling_stock()
            .next()
            .expect("the fixture placed stock");
        let (train_x, train_y) = sim
            .rolling_stock_tile(stock.id)
            .expect("placed stock has a world tile");
        let rail = stock.position.edge;

        assert_eq!(
            routing_action(&sim, None, train_x, train_y),
            Some(TrainRoutingAction::Pick(stock.train))
        );
        // Pressed again on the same train, it is put down rather than sent to
        // the rail underneath itself.
        assert_eq!(
            routing_action(&sim, Some(stock.train), train_x, train_y),
            Some(TrainRoutingAction::Release)
        );

        // A rail well clear of the train, so the cursor names track rather than
        // stock. The fixture lays one long run, so walking up it finds one.
        let (rail_x, rail_y) = far_rail_tile(&sim, rail);
        assert_eq!(
            routing_action(&sim, Some(stock.train), rail_x, rail_y),
            Some(TrainRoutingAction::SendTo {
                train_id: stock.train,
                rail: sim
                    .entities()
                    .occupancy()
                    .entity_at(rail_x, rail_y)
                    .expect("the tile holds a rail"),
            })
        );
        // With no train picked, a rail is not an instruction.
        assert_eq!(routing_action(&sim, None, rail_x, rail_y), None);
    }

    /// A tile holding track that no stock is standing on.
    fn far_rail_tile(
        sim: &Simulation,
        near: factory_sim::EntityId,
    ) -> (WorldTileCoord, WorldTileCoord) {
        let footprint = sim
            .entities()
            .placed_entity(near)
            .expect("the rail under the train is placed")
            .footprint;
        (0..64)
            .map(|offset| (footprint.x, footprint.y + offset))
            .find(|(x, y)| {
                sim.entities()
                    .occupancy()
                    .entity_at(*x, *y)
                    .is_some_and(|entity_id| sim.rail_piece_geometry(entity_id).is_some())
                    && stock_at_tile(sim, *x, *y).is_none()
            })
            .expect("the fixture lays a run longer than the train standing on it")
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
