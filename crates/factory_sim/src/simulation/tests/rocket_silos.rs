use super::super::*;
use super::support::*;
use crate::machines::RocketLaunchPhase;

/// Ticks until the silo has built `parts`, or fails.
///
/// Bounded rather than counted: a silo wants far more power than the one steam
/// engine the test fixture stands on, so the ticks a part takes are the recipe's
/// ticks stretched by whatever satisfaction the network reaches — a number the
/// test has no business predicting.
fn tick_until_parts_built(sim: &mut Simulation, entity_id: EntityId, parts: u32) {
    const TICK_LIMIT: u32 = 20_000;

    for _ in 0..TICK_LIMIT {
        if sim.entities.rocket_silos[&entity_id].parts_completed >= parts {
            return;
        }
        sim.tick();
    }

    panic!("the silo should have built {parts} part(s) within {TICK_LIMIT} ticks");
}

#[test]
fn catalog_loads_rocket_silo_metadata() {
    let sim = Simulation::new_test_world(123);
    let silo = entity_id_by_name(&sim.world.prototypes, "rocket_silo");
    let prototype = &sim.world.prototypes.entities[silo.index()];
    let metadata = prototype
        .rocket_silo
        .expect("rocket silo prototype should load metadata");

    assert_eq!(prototype.entity_kind, EntityKind::RocketSilo);
    assert_eq!((prototype.size.x, prototype.size.y), (9, 9));
    assert!(prototype.module_slot_count > 0);
    assert!(prototype.electric_energy_source.is_some());
    assert!(metadata.parts_per_rocket > 1);
    assert!(metadata.input_slot_count > 0);
}

/// The part recipe is reachable only through the silo, and only the silo can
/// reach it. Both halves matter: an assembler that could take the recipe would
/// make the silo optional, and a silo that could not find it would never build.
#[test]
fn rocket_part_recipe_belongs_to_the_silo_alone() {
    let mut sim = Simulation::new_test_world(123);
    let rocket_part = recipe_id(&sim.world.prototypes, "rocket_part");

    assert_eq!(
        sim.world.prototypes.recipes[rocket_part.index()].category,
        CraftingCategory::RocketBuilding
    );
    assert!(
        !sim.world
            .prototypes
            .entities
            .iter()
            .any(
                |prototype| prototype.assembling_machine.as_ref().is_some_and(
                    |assembler| assembler.crafting_category == CraftingCategory::RocketBuilding
                )
            ),
        "no assembling machine should serve the rocket-building category"
    );

    // Locked until the technology lands, and derived from the catalog after.
    assert!(sim.rocket_silo_recipe().is_none());
    unlock_with_prerequisites(&mut sim, "rocket_silo");
    assert_eq!(
        sim.rocket_silo_recipe().map(|recipe| recipe.id),
        Some(rocket_part)
    );

    let assembler_id = place_assembling_machine(&mut sim);
    assert_eq!(
        sim.select_assembler_recipe(assembler_id, rocket_part),
        Err(AssemblerError::InvalidRecipe(rocket_part))
    );
}

#[test]
fn silo_builds_parts_and_counts_them_toward_a_rocket() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let rocket_part = item_id(&sim.world.prototypes, "rocket_part");
    stock_rocket_silo(&mut sim, silo_id, 2);

    let ingredients = sim
        .rocket_silo_recipe()
        .expect("the part recipe should be unlocked")
        .ingredients
        .clone();
    assert!(
        sim.entities.rocket_silos[&silo_id].crafting_required_ticks > 0,
        "a stocked silo should have work to do"
    );

    tick_until_parts_built(&mut sim, silo_id, 1);

    let state = &sim.entities.rocket_silos[&silo_id];
    assert_eq!(state.parts_completed, 1);
    for ingredient in &ingredients {
        assert_eq!(
            state.input_inventory.count(ingredient.item),
            u32::from(ingredient.amount),
            "one part's worth of ingredients should have been consumed"
        );
    }
    // The part is production, not an item anyone can pick up: it is counted and
    // never lands in a slot.
    assert_eq!(
        sim.item_statistics()
            .rows
            .iter()
            .find(|row| row.item_id == rocket_part)
            .map(|row| row.produced_total),
        Some(1)
    );
    assert_eq!(sim.player_inventory.count(rocket_part), 0);
}

/// A full rocket blocks the silo the way a full output blocks an assembler: no
/// progress, no ingredients drawn, and a status that says so.
#[test]
fn silo_stops_at_a_whole_rocket() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    stock_rocket_silo(&mut sim, silo_id, 2);

    let parts_per_rocket = sim.entities.rocket_silos[&silo_id].parts_per_rocket;
    let state = sim
        .entities
        .rocket_silos
        .get_mut(&silo_id)
        .expect("the silo was just placed");
    state.parts_completed = parts_per_rocket;
    let ingredients_before = state.input_inventory.clone();

    for _ in 0..600 {
        sim.tick();
    }

    let state = &sim.entities.rocket_silos[&silo_id];
    assert!(state.rocket_ready());
    assert_eq!(state.parts_completed, parts_per_rocket);
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(
        state.input_inventory, ingredients_before,
        "a full silo should draw no ingredients"
    );
    assert_eq!(
        sim.machine_status_for_entity(silo_id),
        Some(MachineStatus::OutputFull)
    );

    // And it resumes the moment the rocket leaves, which is what #199 will do.
    sim.entities
        .rocket_silos
        .get_mut(&silo_id)
        .expect("the silo is still placed")
        .parts_completed = 0;
    tick_until_parts_built(&mut sim, silo_id, 1);
}

#[test]
fn silo_without_ingredients_reports_missing_input_and_makes_no_progress() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);

    for _ in 0..120 {
        sim.tick();
    }

    let state = &sim.entities.rocket_silos[&silo_id];
    assert_eq!(state.parts_completed, 0);
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(
        sim.machine_status_for_entity(silo_id),
        Some(MachineStatus::NoInput)
    );
}

/// The silo takes what its recipe asks for and nothing else, on every route in:
/// a player's click and an inserter's arm answer the same question.
#[test]
fn silo_accepts_only_part_ingredients() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let coal = item_id(&sim.world.prototypes, "coal");
    let ingredient = sim
        .rocket_silo_recipe()
        .expect("the part recipe should be unlocked")
        .ingredients[0]
        .item;

    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, coal, 5);
    assert_eq!(
        crate::entity_transfer::player_slot_to_rocket_silo_input(&mut sim, silo_id, 0),
        Err(RocketSiloError::InvalidInput(coal))
    );

    set_inventory_slot(&mut sim.player_inventory, 1, ingredient, 5);
    crate::entity_transfer::player_slot_to_rocket_silo_input(&mut sim, silo_id, 1)
        .expect("an ingredient of the part recipe should transfer");
    assert_eq!(
        sim.entities.rocket_silos[&silo_id]
            .input_inventory
            .count(ingredient),
        5
    );
}

/// Mining a part-built silo hands back the ingredients still in its slots. The
/// parts already counted are not items and never were, so they are lost with the
/// building rather than refunded.
#[test]
fn destroying_a_silo_recovers_ingredients_but_not_parts() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let rocket_part = item_id(&sim.world.prototypes, "rocket_part");
    stock_rocket_silo(&mut sim, silo_id, 1);
    let ingredient = sim
        .rocket_silo_recipe()
        .expect("the part recipe should be unlocked")
        .ingredients[0]
        .clone();
    sim.entities
        .rocket_silos
        .get_mut(&silo_id)
        .expect("the silo was just placed")
        .parts_completed = 5;

    crate::entity_mutation::destroy_to_player_inventory(&mut sim, silo_id)
        .expect("a placed silo should be minable");

    assert_eq!(
        sim.player_inventory.count(ingredient.item),
        u32::from(ingredient.amount)
    );
    assert_eq!(sim.player_inventory.count(rocket_part), 0);
}

/// Research completes in `advance_labs`, which runs after `advance_rocket_silos`
/// in the same tick, so on the tick the silo technology lands every silo still
/// holds the tick count it derived while the recipe was locked. Validation runs
/// at the end of that tick and has to accept it: the silo corrects itself on the
/// next one, and a world that is one tick behind is not a corrupt world.
///
/// Pinned as a test because it is the reason `validate_rocket_silo` bounds the
/// two counts against each other rather than against the recipe.
#[test]
fn a_silo_stays_valid_on_the_tick_its_technology_lands() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    stock_rocket_silo(&mut sim, silo_id, 1);
    // One tick first so the fixture's fluid network settles; what is under test
    // is the silo's own state, not the boiler beside it.
    sim.tick();
    sim.entities
        .rocket_silos
        .get_mut(&silo_id)
        .expect("the silo was just placed")
        .crafting_required_ticks = 0;

    sim.validate()
        .expect("a silo whose recipe has not reached it yet is a valid world");

    sim.tick();

    assert!(
        sim.entities.rocket_silos[&silo_id].crafting_required_ticks > 0,
        "the next tick derives the count the silo was missing"
    );
    sim.validate()
        .expect("and the corrected world is valid too");
}

#[test]
fn completed_rocket_launches_satellite_over_fixed_ticks() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let satellite = item_id(&sim.world.prototypes, "satellite");
    let state = sim.entities.rocket_silos.get_mut(&silo_id).unwrap();
    state.parts_completed = state.parts_per_rocket;
    state
        .cargo_inventory
        .insert(&sim.world.prototypes, satellite, 1)
        .unwrap();

    sim.tick();
    assert!(matches!(
        sim.entities.rocket_silos[&silo_id].launch_phase,
        RocketLaunchPhase::Sealed { .. }
    ));
    for _ in 0..180 {
        sim.tick();
    }

    let state = &sim.entities.rocket_silos[&silo_id];
    assert_eq!(state.launch_phase, RocketLaunchPhase::Idle);
    assert_eq!(state.parts_completed, 0);
    assert_eq!(state.cargo_inventory.count(satellite), 0);
}

#[test]
fn mid_launch_save_round_trip_preserves_phase_and_finishes_headlessly() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let satellite = item_id(&sim.world.prototypes, "satellite");
    let state = sim.entities.rocket_silos.get_mut(&silo_id).unwrap();
    state.parts_completed = state.parts_per_rocket;
    state
        .cargo_inventory
        .insert(&sim.world.prototypes, satellite, 1)
        .unwrap();
    for _ in 0..80 {
        sim.tick();
    }

    let bytes = crate::save_to_bytes(&sim).unwrap();
    let mut loaded = crate::load_from_bytes(&bytes).unwrap();
    assert_eq!(sim.state_hash(), loaded.state_hash());
    for _ in 0..101 {
        loaded.tick();
    }
    assert_eq!(loaded.entities.rocket_silos[&silo_id].parts_completed, 0);
}
