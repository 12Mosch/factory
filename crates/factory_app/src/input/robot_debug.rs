//! Debug errand: F6 sends a stationed robot to the cursor tile and back.
//!
//! Robots have no jobs yet — no construction, no logistics — so nothing in
//! normal play makes one fly. This key is how the flight layer is exercised in
//! the running game: pick a tile inside a network's construction area, press
//! F6, and watch a robot leave its roboport, cross to the tile, and dock again.
//! It is deliberately the only way to spend a robot, and it goes through the
//! ordinary command queue so it lands on a tick boundary like every other input.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{EntityId, SimCommand, Simulation, WorldTileCoord};

use crate::input::panels::world_input_blocked;
use crate::input::resources::AppInputState;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;

pub(crate) fn dispatch_debug_robot_from_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_state: Option<Res<AppInputState>>,
    sim: Res<SimResource>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    let Some(keyboard) = keyboard.as_deref() else {
        return;
    };
    if !keyboard.just_pressed(KeyCode::F6) || world_input_blocked(input_state.as_deref()) {
        return;
    }
    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        return;
    };

    let sim = sim.read();
    let Some(roboport) = dispatching_roboport(&sim, x, y) else {
        return;
    };
    commands.write(SimCommandRequest(SimCommand::DispatchRobot {
        roboport,
        x,
        y,
    }));
}

/// Roboport that should serve `(x, y)`: the lowest-id member of the network
/// covering the tile that actually has a robot to send.
///
/// Choosing here rather than letting the simulation pick keeps the command
/// explicit about which roboport it spends — the same property construction
/// jobs will need — and stops the key from silently failing on a network whose
/// first roboport happens to be empty.
fn dispatching_roboport(
    sim: &Simulation,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> Option<EntityId> {
    let network_id = sim.construction_network_covering_tile(x, y)?;
    let network = sim
        .robot_networks()
        .iter()
        .find(|network| network.network_id == network_id)?;
    network
        .roboports
        .iter()
        .map(|roboport| roboport.entity_id)
        .find(|entity_id| roboport_has_a_robot(sim, *entity_id))
}

fn roboport_has_a_robot(sim: &Simulation, entity_id: EntityId) -> bool {
    factory_sim::entity_access::roboport_state(sim, entity_id).is_ok_and(|state| {
        state.robots.slots().iter().any(|slot| {
            slot.stack().is_some_and(|stack| {
                sim.catalog()
                    .item(stack.item_id())
                    .is_some_and(|item| item.robot.is_some())
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::{CHUNK_SIZE, Direction, Simulation};

    /// Places a roboport on the first tile of the test world that will take
    /// one, and optionally stations robots in it.
    fn world_with_roboport(stationed_robots: u16) -> (Simulation, EntityId) {
        let mut sim = Simulation::new_test_world(123);
        let prototype_id = factory_data::entity_prototype_id_by_name(sim.catalog(), "roboport");
        let robot = factory_data::item_id_by_name(sim.catalog(), "construction_robot");
        let candidates = sim
            .world()
            .chunks
            .values()
            .flat_map(|chunk| {
                (0..CHUNK_SIZE)
                    .flat_map(move |local_y| (0..CHUNK_SIZE).map(move |local_x| (local_x, local_y)))
                    .map(|(local_x, local_y)| chunk.coord.tile_at(local_x, local_y))
            })
            .collect::<Vec<_>>();
        let entity_id = candidates
            .into_iter()
            .find_map(|(x, y)| {
                factory_sim::placement::place(
                    &mut sim,
                    factory_sim::placement::EntityPlacementRequest {
                        prototype_id,
                        x,
                        y,
                        direction: Direction::North,
                    },
                )
                .ok()
            })
            .expect("the test world should have room for a roboport");
        sim.tick();

        if stationed_robots > 0 {
            let catalog = sim.catalog().clone();
            sim.player_inventory_mut()
                .insert(&catalog, robot, stationed_robots)
                .expect("the player inventory should accept robots");
            let slot_index = sim
                .player_inventory()
                .slots()
                .iter()
                .position(|slot| slot.stack().is_some_and(|stack| stack.item_id() == robot))
                .expect("the robots are in the player inventory");
            sim.apply_command(&SimCommand::TransferSlot {
                entity_id,
                panel: factory_sim::InventoryPanel::Player,
                slot_index,
            })
            .expect("a roboport should accept robots");
            sim.tick();
        }
        (sim, entity_id)
    }

    #[test]
    fn a_covered_tile_picks_the_stocked_roboport_of_its_network() {
        let (sim, roboport) = world_with_roboport(5);
        let bounds = sim
            .entity_roboport_status(roboport)
            .expect("a placed roboport reports status")
            .construction_bounds;

        assert_eq!(
            dispatching_roboport(&sim, bounds.min_x, bounds.min_y),
            Some(roboport)
        );
        assert_eq!(
            dispatching_roboport(&sim, bounds.max_x + 1, bounds.min_y),
            None,
            "a tile outside every construction square has no roboport to serve it"
        );
    }

    /// A roboport with no robots is not a candidate: the key would otherwise
    /// look broken on a network that has coverage but nothing to send.
    #[test]
    fn an_empty_roboport_is_not_a_candidate() {
        let (sim, roboport) = world_with_roboport(0);
        let bounds = sim
            .entity_roboport_status(roboport)
            .expect("a placed roboport reports status")
            .construction_bounds;

        assert_eq!(dispatching_roboport(&sim, bounds.min_x, bounds.min_y), None);
    }
}
