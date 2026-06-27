use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{Direction, EntityId, Simulation};

use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::resources::{DebugBuildDirection, DebugInventorySelection, SimResource};
use crate::utils::find_entity_prototype_id;

pub(crate) fn handle_debug_entity_placement(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    mut sim: ResMut<SimResource>,
    mut build_direction: ResMut<DebugBuildDirection>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        if keyboard.just_pressed(KeyCode::KeyR) {
            build_direction.direction = next_debug_build_direction(build_direction.direction);
        }
        return;
    };
    let _ = handle_debug_build_action_at_tile(&mut sim.sim, &keyboard, &mut build_direction, x, y);
}

pub(crate) fn handle_debug_belt_item_insertion_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    inventory_selection: Res<DebugInventorySelection>,
    mut sim: ResMut<SimResource>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };
    if !keyboard.just_pressed(KeyCode::KeyV) {
        return;
    }

    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        return;
    };

    let _ = handle_debug_belt_item_insertion_at_tile(&mut sim.sim, &inventory_selection, x, y);
}

pub fn handle_debug_build_action_at_tile(
    sim: &mut Simulation,
    keyboard: &ButtonInput<KeyCode>,
    build_direction: &mut DebugBuildDirection,
    x: i32,
    y: i32,
) -> Option<EntityId> {
    if keyboard.just_pressed(KeyCode::KeyR) {
        build_direction.direction = next_debug_build_direction(build_direction.direction);
    }

    let prototype_name = debug_build_prototype_name(keyboard)?;
    let prototype = find_entity_prototype_id(sim.catalog(), prototype_name);
    sim.place_entity(prototype, x, y, build_direction.direction)
        .ok()
}

pub fn handle_debug_belt_item_insertion_at_tile(
    sim: &mut Simulation,
    inventory_selection: &DebugInventorySelection,
    x: i32,
    y: i32,
) -> Option<()> {
    let item_id = sim
        .catalog()
        .items
        .get(inventory_selection.selected_index % sim.catalog().items.len().max(1))?
        .id;
    let entity_id = sim.entities().occupancy().entity_at(x, y)?;
    if sim.belt_segment(entity_id).is_err() {
        return None;
    }

    for lane_index in 0..2 {
        if sim
            .insert_item_onto_belt(entity_id, lane_index, item_id)
            .is_ok()
        {
            return Some(());
        }
    }

    None
}

fn debug_build_prototype_name(keyboard: &ButtonInput<KeyCode>) -> Option<&'static str> {
    if keyboard.just_pressed(KeyCode::KeyC) {
        Some("chest")
    } else if keyboard.just_pressed(KeyCode::KeyB) {
        Some("burner_mining_drill")
    } else if keyboard.just_pressed(KeyCode::KeyF) {
        Some("stone_furnace")
    } else if keyboard.just_pressed(KeyCode::KeyT) {
        Some("transport_belt")
    } else if keyboard.just_pressed(KeyCode::KeyA) {
        Some("assembling_machine")
    } else if keyboard.just_pressed(KeyCode::KeyL) {
        Some("lab")
    } else {
        None
    }
}

fn next_debug_build_direction(direction: Direction) -> Direction {
    match direction {
        Direction::North => Direction::East,
        Direction::East => Direction::South,
        Direction::South => Direction::West,
        Direction::West => Direction::North,
    }
}
