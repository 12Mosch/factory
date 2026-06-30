use super::super::*;
use super::support::*;

#[test]
fn furnace_smelts_iron_ore_to_iron_plate() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, iron_ore, coal);

    for _ in 0..210 {
        sim.tick();
    }

    let state = sim
        .furnace_state(entity_id)
        .expect("furnace should expose state");
    assert_eq!(state.input_slot, None);
    assert_eq!(
        state.output_slot,
        Some(ItemStack {
            item_id: iron_plate,
            count: 1,
        })
    );
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(state.energy.energy_remaining_joules, 3_685_000.0);
}

#[test]
fn furnace_does_not_smelts_without_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let entity_id = place_stone_furnace(&mut sim);
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });
    sim.transfer_player_slot_to_furnace_input(entity_id, 0)
        .expect("ore should transfer to furnace input");

    for _ in 0..210 {
        sim.tick();
    }

    let state = sim
        .furnace_state(entity_id)
        .expect("furnace should expose state");
    assert_eq!(
        state.input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        })
    );
    assert_eq!(state.output_slot, None);
    assert_eq!(state.energy.energy_remaining_joules, 0.0);
    assert_eq!(state.crafting_progress_ticks, 0);
}

#[test]
fn furnace_blocks_when_output_full() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let copper_plate = item_id(&sim.world.prototypes, "copper_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, iron_ore, coal);
    let state = sim
        .entities
        .furnace_state_mut(entity_id)
        .expect("furnace should expose state");
    state.output_slot = Some(ItemStack {
        item_id: copper_plate,
        count: 1,
    });

    for _ in 0..210 {
        sim.tick();
    }

    let state = sim
        .furnace_state(entity_id)
        .expect("furnace should expose state");
    assert_eq!(
        state.input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        })
    );
    assert_eq!(
        state.energy.fuel_slot,
        Some(ItemStack {
            item_id: coal,
            count: 1,
        })
    );
    assert_eq!(state.energy.energy_remaining_joules, 0.0);
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(
        state.output_slot.map(|stack| stack.item_id),
        Some(copper_plate)
    );
    assert_eq!(
        state.output_slot.map(|stack| stack.item_id == iron_plate),
        Some(false)
    );
}

#[test]
fn furnace_smelts_copper_ore_to_copper_plate() {
    let mut sim = Simulation::new_test_world(123);
    let copper_ore = item_id(&sim.world.prototypes, "copper_ore");
    let copper_plate = item_id(&sim.world.prototypes, "copper_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, copper_ore, coal);

    for _ in 0..210 {
        sim.tick();
    }

    assert_eq!(
        sim.furnace_state(entity_id)
            .expect("furnace should expose state")
            .output_slot,
        Some(ItemStack {
            item_id: copper_plate,
            count: 1,
        })
    );
}

#[test]
fn furnace_smelts_stone_to_stone_brick() {
    let mut sim = Simulation::new_test_world(123);
    let stone = item_id(&sim.world.prototypes, "stone");
    let stone_brick = item_id(&sim.world.prototypes, "stone_brick");
    let coal = item_id(&sim.world.prototypes, "coal");
    let recipe = recipe_id(&sim.world.prototypes, "stone_brick");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, stone, coal);

    for _ in 0..210 {
        sim.tick();
    }

    let state = sim
        .furnace_state(entity_id)
        .expect("furnace should expose state");
    assert_eq!(state.active_recipe, Some(recipe));
    assert_eq!(
        state.output_slot,
        Some(ItemStack {
            item_id: stone_brick,
            count: 1,
        })
    );
}

#[test]
fn invalid_furnace_input_is_rejected() {
    let mut sim = Simulation::new_test_world(123);
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_stone_furnace(&mut sim);
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: coal,
        count: 1,
    });

    assert_eq!(
        sim.transfer_player_slot_to_furnace_input(entity_id, 0),
        Err(FurnaceError::InvalidInput(coal))
    );
    assert_eq!(
        sim.furnace_state(entity_id)
            .expect("furnace should expose state")
            .input_slot,
        None
    );
    assert_eq!(
        sim.player_inventory.slots[0],
        Some(ItemStack {
            item_id: coal,
            count: 1,
        })
    );
}

#[test]
fn locked_smelting_recipes_are_not_selected_by_furnaces() {
    let mut catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let stone_brick = recipe_id(&catalog, "stone_brick");
    catalog.technologies[0]
        .effects
        .push(TechnologyEffect::UnlockRecipe(stone_brick));
    let mut sim = Simulation::new(123, catalog);
    let furnace_id = place_stone_furnace(&mut sim);
    let stone = item_id(&sim.world.prototypes, "stone");
    sim.entities
        .furnace_state_mut(furnace_id)
        .expect("furnace should expose state")
        .input_slot = Some(ItemStack {
        item_id: stone,
        count: 1,
    });

    for _ in 0..240 {
        sim.tick();
    }

    let furnace = sim
        .furnace_state(furnace_id)
        .expect("furnace should expose state");
    assert_eq!(furnace.active_recipe, None);
    assert_eq!(furnace.input_slot.unwrap().count, 1);
    assert_eq!(
        sim.technology_progress(technology_id(&sim.world.prototypes, "automation")),
        Some(0)
    );
}
