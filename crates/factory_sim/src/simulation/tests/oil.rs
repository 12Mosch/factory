use super::super::*;
use super::support::*;

fn unlock_oil_processing(sim: &mut Simulation) {
    for technology in [
        "logistics",
        "automation",
        "electric_power",
        "logistic_science_pack",
        "logistics_2",
        "fluid_handling",
        "oil_processing",
    ] {
        complete_research_by_name(sim, technology);
    }
}

fn sim_with_powered_pumpjack() -> (Simulation, EntityId) {
    for seed in 0..64 {
        let mut sim = Simulation::new_test_world(seed);
        if let Some(pumpjack_id) = place_powered_pumpjack(&mut sim) {
            return (sim, pumpjack_id);
        }
    }

    panic!("expected a seed with a powerable crude oil patch");
}

fn sim_with_powered_refinery() -> (Simulation, EntityId) {
    for seed in 0..64 {
        let mut sim = Simulation::new_test_world(seed);
        let refinery = entity_id_by_name(&sim.world.prototypes, "oil_refinery");
        // The target pole sits west of the 5x5 refinery so it stays within
        // wire reach of the power plant's source pole.
        let Some((x, y, _)) =
            place_powered_fixture_origin_where(&mut sim, 5, 5, (-1, 2), fixture_is_clear_buildable)
        else {
            continue;
        };
        let refinery_id = crate::placement::place(
            &mut sim,
            crate::placement::EntityPlacementRequest {
                prototype_id: refinery,
                x,
                y,
                direction: Direction::North,
            },
        )
        .expect("oil refinery should be placeable");
        return (sim, refinery_id);
    }

    panic!("expected a seed with room for a powered oil refinery");
}

fn place_powered_chemical_plant(sim: &mut Simulation) -> EntityId {
    let chemical_plant = entity_id_by_name(&sim.world.prototypes, "chemical_plant");
    let (x, y) = place_powered_fixture_origin(sim, 3, 3, (3, 1));
    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chemical_plant,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("chemical plant should be placeable")
}

fn fluid_box_amount(
    sim: &Simulation,
    entity_id: EntityId,
    box_index: usize,
) -> (Option<FluidId>, u64) {
    let state = sim
        .entities
        .fluid_boxes
        .get(&entity_id)
        .and_then(|boxes| boxes.get(box_index))
        .expect("entity should expose requested fluid box");
    (state.fluid_id, state.amount_milliunits)
}

#[test]
fn starting_world_contains_crude_oil_patches() {
    let sim = Simulation::new_test_world(123);
    let crude_oil = item_id(&sim.world.prototypes, "crude_oil");

    let (x, y, amount) = first_resource_tile_for_item(&sim.world, crude_oil);
    assert!(amount > 0);

    let tile = sim.world.tile_at(x, y).expect("oil tile should exist");
    assert!(tile.collision.walkable);
    assert!(!tile.collision.buildable);
    assert!(
        !tile.collision.minable,
        "crude oil tiles must not be solid-minable"
    );
}

#[test]
fn manual_mining_rejects_crude_oil_tiles() {
    let mut sim = Simulation::new_test_world(123);
    let crude_oil = item_id(&sim.world.prototypes, "crude_oil");
    let (x, y, _) = first_resource_tile_for_item(&sim.world, crude_oil);

    sim.player = PlayerState::centered_on_tile(x, y - 1);
    sim.update_manual_mining(Some(ManualMiningTarget { x, y }));

    assert!(
        sim.manual_mining_progress().is_none(),
        "crude oil should not be a manual mining target"
    );
}

#[test]
fn burner_drill_cannot_target_crude_oil() {
    let sim = Simulation::new_test_world(123);
    let crude_oil = item_id(&sim.world.prototypes, "crude_oil");
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let drill_prototype = &sim.world.prototypes.entities[drill.index()];
    let mining_drill = drill_prototype
        .mining_drill
        .as_ref()
        .expect("burner drill should have mining metadata");

    for (x, y) in all_tile_coords(&sim.world) {
        let footprint = EntityFootprint {
            x,
            y,
            width: drill_prototype.size.x,
            height: drill_prototype.size.y,
        };
        let area_is_pure_oil =
            mining_area_tiles(&footprint, mining_drill)
                .into_iter()
                .all(|(tile_x, tile_y)| {
                    sim.world
                        .tile_at(tile_x, tile_y)
                        .and_then(|tile| tile.resource)
                        .is_some_and(|resource| resource.resource_item == crude_oil)
                });
        if !area_is_pure_oil {
            continue;
        }

        assert!(
            first_resource_in_mining_area(&sim.world, &footprint, mining_drill).is_none(),
            "drills must not find crude oil resources"
        );
        assert!(
            crate::placement::validate(
                &sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: drill,
                    x,
                    y,
                    direction: Direction::North,
                },
            )
            .is_err(),
            "drills must not be placeable over pure crude oil"
        );
        return;
    }

    panic!("expected a drill-sized area of pure crude oil");
}

#[test]
fn pumpjack_requires_crude_oil_under_footprint() {
    let sim = Simulation::new_test_world(123);
    let pumpjack = entity_id_by_name(&sim.world.prototypes, "pumpjack");
    let (x, y) = first_buildable_rect(&sim.world, 3, 3);

    let result = crate::placement::validate(
        &sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pumpjack,
            x,
            y,
            direction: Direction::North,
        },
    );

    assert!(
        result.is_err(),
        "pumpjack placement away from oil should be rejected"
    );
}

#[test]
fn powered_pumpjack_produces_crude_oil_into_its_output_box() {
    let (mut sim, pumpjack_id) = sim_with_powered_pumpjack();
    let crude_oil = fluid_id(&sim.world.prototypes, "crude_oil");

    for _ in 0..240 {
        sim.tick();
    }

    let (fluid, amount) = fluid_box_amount(&sim, pumpjack_id, 0);
    assert_eq!(fluid, Some(crude_oil));
    // 100 milliunits per tick once powered; allow a warm-up margin for the
    // boiler and steam engine to spin up.
    assert!(
        amount >= 10_000,
        "pumpjack should have produced crude oil, got {amount}"
    );
}

#[test]
fn oil_refinery_converts_crude_oil_to_petroleum_gas() {
    let (mut sim, refinery_id) = sim_with_powered_refinery();
    unlock_oil_processing(&mut sim);
    let crude_oil = fluid_id(&sim.world.prototypes, "crude_oil");
    let petroleum_gas = fluid_id(&sim.world.prototypes, "petroleum_gas");
    let recipe = recipe_id(&sim.world.prototypes, "basic_oil_processing");

    sim.select_assembler_recipe(refinery_id, recipe)
        .expect("refinery should accept basic oil processing");
    set_fluid_box(&mut sim, refinery_id, 0, crude_oil, 200_000);

    for _ in 0..800 {
        sim.tick();
    }

    let (input_fluid, input_amount) = fluid_box_amount(&sim, refinery_id, 0);
    let (output_fluid, output_amount) = fluid_box_amount(&sim, refinery_id, 1);
    assert_eq!(input_fluid, None, "both crude batches should be consumed");
    assert_eq!(input_amount, 0);
    assert_eq!(output_fluid, Some(petroleum_gas));
    assert_eq!(output_amount, 90_000, "two crafts of 45 gas each");
}

#[test]
fn refinery_rejects_crafting_and_locked_recipes() {
    let (mut sim, refinery_id) = sim_with_powered_refinery();
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let oil_recipe = recipe_id(&sim.world.prototypes, "basic_oil_processing");

    assert!(matches!(
        sim.select_assembler_recipe(refinery_id, gear_recipe),
        Err(AssemblerError::InvalidRecipe(_))
    ));
    assert!(matches!(
        sim.select_assembler_recipe(refinery_id, oil_recipe),
        Err(AssemblerError::RecipeLocked(_))
    ));
}

#[test]
fn assembler_rejects_oil_processing_recipes() {
    let mut sim = Simulation::new_test_world(123);
    unlock_oil_processing(&mut sim);
    let assembler_id = place_assembling_machine(&mut sim);
    let oil_recipe = recipe_id(&sim.world.prototypes, "basic_oil_processing");

    assert!(matches!(
        sim.select_assembler_recipe(assembler_id, oil_recipe),
        Err(AssemblerError::InvalidRecipe(_))
    ));
}

#[test]
fn chemical_plant_crafts_plastic_from_coal_and_petroleum_gas() {
    let mut sim = Simulation::new_test_world(123);
    unlock_oil_processing(&mut sim);
    complete_research_by_name(&mut sim, "plastics");
    let plant_id = place_powered_chemical_plant(&mut sim);
    let petroleum_gas = fluid_id(&sim.world.prototypes, "petroleum_gas");
    let coal = item_id(&sim.world.prototypes, "coal");
    let plastic_bar = item_id(&sim.world.prototypes, "plastic_bar");
    let recipe = recipe_id(&sim.world.prototypes, "plastic_bar");

    sim.select_assembler_recipe(plant_id, recipe)
        .expect("chemical plant should accept plastic bar recipe");
    set_fluid_box(&mut sim, plant_id, 0, petroleum_gas, 100_000);
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: coal,
        count: 2,
    });
    crate::entity_transfer::player_slot_to_assembler_input(&mut sim, plant_id, 0)
        .expect("chemical plant should accept coal");

    for _ in 0..400 {
        sim.tick();
    }

    let state = sim
        .entities
        .assembler_state(plant_id)
        .expect("chemical plant should expose assembler state");
    assert_eq!(state.output_inventory.count(plastic_bar), 4);
    assert_eq!(state.input_inventory.count(coal), 0);
    let (_, gas_amount) = fluid_box_amount(&sim, plant_id, 0);
    assert_eq!(gas_amount, 60_000, "two crafts consume 20 gas each");
}

#[test]
fn chemical_plant_crafts_sulfur_from_gas_and_water() {
    let mut sim = Simulation::new_test_world(123);
    unlock_oil_processing(&mut sim);
    complete_research_by_name(&mut sim, "sulfur_processing");
    let plant_id = place_powered_chemical_plant(&mut sim);
    let petroleum_gas = fluid_id(&sim.world.prototypes, "petroleum_gas");
    let water = fluid_id(&sim.world.prototypes, "water");
    let sulfur = item_id(&sim.world.prototypes, "sulfur");
    let recipe = recipe_id(&sim.world.prototypes, "sulfur");

    sim.select_assembler_recipe(plant_id, recipe)
        .expect("chemical plant should accept sulfur recipe");
    set_fluid_box(&mut sim, plant_id, 0, petroleum_gas, 60_000);
    set_fluid_box(&mut sim, plant_id, 1, water, 60_000);

    for _ in 0..400 {
        sim.tick();
    }

    let state = sim
        .entities
        .assembler_state(plant_id)
        .expect("chemical plant should expose assembler state");
    assert_eq!(state.output_inventory.count(sulfur), 4);
    let (_, gas_amount) = fluid_box_amount(&sim, plant_id, 0);
    let (_, water_amount) = fluid_box_amount(&sim, plant_id, 1);
    assert_eq!(gas_amount, 0);
    assert_eq!(water_amount, 0);
}

#[test]
fn chemical_plant_stalls_without_fluid_ingredients() {
    let mut sim = Simulation::new_test_world(123);
    unlock_oil_processing(&mut sim);
    complete_research_by_name(&mut sim, "plastics");
    let plant_id = place_powered_chemical_plant(&mut sim);
    let coal = item_id(&sim.world.prototypes, "coal");
    let plastic_bar = item_id(&sim.world.prototypes, "plastic_bar");
    let recipe = recipe_id(&sim.world.prototypes, "plastic_bar");

    sim.select_assembler_recipe(plant_id, recipe)
        .expect("chemical plant should accept plastic bar recipe");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: coal,
        count: 2,
    });
    crate::entity_transfer::player_slot_to_assembler_input(&mut sim, plant_id, 0)
        .expect("chemical plant should accept coal");

    for _ in 0..200 {
        sim.tick();
    }

    let state = sim
        .entities
        .assembler_state(plant_id)
        .expect("chemical plant should expose assembler state");
    assert_eq!(
        state.output_inventory.count(plastic_bar),
        0,
        "no petroleum gas means no plastic"
    );
    assert_eq!(state.input_inventory.count(coal), 2);
}
