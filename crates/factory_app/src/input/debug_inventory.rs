use bevy::prelude::*;
use factory_data::PrototypeCatalog;
use factory_sim::Simulation;

use crate::resources::{DebugInventorySelection, SimResource};

pub(crate) fn handle_debug_inventory_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut inventory_selection: ResMut<DebugInventorySelection>,
    mut sim: ResMut<SimResource>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    let item_count = sim.sim.catalog().items.len();
    if item_count == 0 {
        inventory_selection.selected_index = 0;
        return;
    }

    inventory_selection.selected_index %= item_count;

    if keyboard.just_pressed(KeyCode::BracketLeft) {
        inventory_selection.selected_index =
            (inventory_selection.selected_index + item_count - 1) % item_count;
    }
    if keyboard.just_pressed(KeyCode::BracketRight) {
        inventory_selection.selected_index = (inventory_selection.selected_index + 1) % item_count;
    }

    let selected_item = sim.sim.catalog().items[inventory_selection.selected_index].id;
    if keyboard.just_pressed(KeyCode::KeyI) {
        let sim = &mut sim.sim;
        let catalog = sim.catalog().clone();
        let inventory = sim.player_inventory_mut();
        let _ = inventory.insert(&catalog, selected_item, 1);
    }
    if keyboard.just_pressed(KeyCode::KeyO) {
        let _ = sim.sim.player_inventory_mut().remove(selected_item, 1);
    }
}

pub(crate) fn selected_inventory_item_state(
    sim: &Simulation,
    inventory_selection: &DebugInventorySelection,
    catalog: &PrototypeCatalog,
) -> (String, u32) {
    let Some(item) = catalog
        .items
        .get(inventory_selection.selected_index % catalog.items.len().max(1))
    else {
        return ("<none>".to_string(), 0);
    };

    (item.name.clone(), sim.player_inventory().count(item.id))
}
