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

fn set_launch_payload(sim: &mut Simulation, payload: ItemId) {
    let satellite = item_id(&sim.world.prototypes, "satellite");
    let products =
        std::mem::take(&mut sim.world.prototypes.items[satellite.index()].launch_products);
    sim.world.prototypes.items[payload.index()].launch_products = products;
}

fn set_launch_products(
    sim: &mut Simulation,
    payload: ItemId,
    products: Vec<factory_data::ItemAmount>,
) {
    sim.world.prototypes.items[payload.index()].launch_products = products;
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
    let satellite = sim
        .world
        .prototypes
        .item(item_id(&sim.world.prototypes, "satellite"))
        .expect("satellite should be catalog content");
    assert_eq!(satellite.launch_products.len(), 1);
    assert_eq!(
        satellite.launch_products[0].item,
        item_id(&sim.world.prototypes, "space_science_pack")
    );
    assert_eq!(satellite.launch_products[0].amount, 1_000);
    assert_eq!(metadata.output_slot_count, 5);
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

/// A full rocket stops part construction while it waits for launch cargo: no
/// progress, no ingredients drawn, and a status that says what is missing.
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
        Some(MachineStatus::NoInput)
    );
    assert_eq!(
        sim.rocket_silo_status_for_entity(silo_id).unwrap().state,
        RocketSiloOperationalState::AwaitingPayload
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
    assert_eq!(
        sim.rocket_silo_status_for_entity(silo_id).unwrap().state,
        RocketSiloOperationalState::MissingIngredients
    );
}

#[test]
fn silo_diagnostics_distinguish_build_cargo_output_and_launch_phases() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let satellite = item_id(&sim.world.prototypes, "satellite");
    let space_science = item_id(&sim.world.prototypes, "space_science_pack");
    stock_rocket_silo(&mut sim, silo_id, 2);
    sim.tick();

    let detail = sim.rocket_silo_status_for_entity(silo_id).unwrap();
    assert_eq!(detail.state, RocketSiloOperationalState::BuildingParts);
    assert_eq!(detail.machine_status(), MachineStatus::Working);

    sim.power
        .entity_statuses
        .get_mut(&silo_id)
        .expect("the powered silo should have a power status")
        .satisfaction_permyriad = 0;
    let detail = sim.rocket_silo_status_for_entity(silo_id).unwrap();
    assert_eq!(detail.state, RocketSiloOperationalState::NoPower);
    assert_eq!(detail.machine_status(), MachineStatus::NoPower);
    sim.power
        .entity_statuses
        .get_mut(&silo_id)
        .unwrap()
        .satisfaction_permyriad = 10_000;

    let state = sim.entities.rocket_silos.get_mut(&silo_id).unwrap();
    state.parts_completed = state.parts_per_rocket;
    let detail = sim.rocket_silo_status_for_entity(silo_id).unwrap();
    assert_eq!(detail.state, RocketSiloOperationalState::AwaitingPayload);
    assert_eq!(detail.machine_status(), MachineStatus::NoInput);

    sim.entities
        .rocket_silos
        .get_mut(&silo_id)
        .unwrap()
        .cargo_inventory
        .insert(&sim.world.prototypes, satellite, 1)
        .unwrap();
    let detail = sim.rocket_silo_status_for_entity(silo_id).unwrap();
    assert_eq!(detail.state, RocketSiloOperationalState::ReadyToLaunch);

    sim.entities
        .rocket_silos
        .get_mut(&silo_id)
        .unwrap()
        .output_inventory
        .insert(&sim.world.prototypes, space_science, 1)
        .unwrap();
    let detail = sim.rocket_silo_status_for_entity(silo_id).unwrap();
    assert_eq!(
        detail.state,
        RocketSiloOperationalState::LaunchOutputBlocked
    );
    assert_eq!(detail.machine_status(), MachineStatus::OutputFull);

    sim.entities
        .rocket_silos
        .get_mut(&silo_id)
        .unwrap()
        .output_inventory
        .remove(space_science, 1)
        .unwrap();
    sim.tick();
    let detail = sim.rocket_silo_status_for_entity(silo_id).unwrap();
    assert_eq!(detail.state, RocketSiloOperationalState::Sealing);
    assert_eq!(detail.machine_status(), MachineStatus::Working);
    assert_eq!(detail.progress_ticks, 0);
    assert_eq!(detail.ticks_remaining, Some(detail.required_ticks));

    for _ in 0..60 {
        sim.tick();
    }
    let detail = sim.rocket_silo_status_for_entity(silo_id).unwrap();
    assert_eq!(detail.state, RocketSiloOperationalState::Launching);
    assert_eq!(detail.machine_status(), MachineStatus::Working);
    assert_eq!(detail.progress_ticks, 0);
    assert_eq!(detail.ticks_remaining, Some(detail.required_ticks));

    let loaded = crate::load_from_bytes(&crate::save_to_bytes(&sim).unwrap()).unwrap();
    assert_eq!(
        loaded.rocket_silo_status_for_entity(silo_id),
        Some(detail),
        "the projection should follow durable phase progress after save/load"
    );
}

#[test]
fn rocket_program_progress_is_derived_and_survives_save_load() {
    let mut sim = Simulation::new_test_world(123);
    let production_science = item_id(&sim.world.prototypes, "production_science_pack");
    let utility_science = item_id(&sim.world.prototypes, "utility_science_pack");
    sim.record_item_produced(production_science, 12);
    sim.record_item_produced(utility_science, 15);

    let silo_id = place_powered_rocket_silo(&mut sim);
    for _ in 0..600 {
        sim.tick();
        if sim
            .entity_power_status(silo_id)
            .is_some_and(|status| status.satisfaction_permyriad > 0)
        {
            break;
        }
    }
    let satellite = item_id(&sim.world.prototypes, "satellite");
    let silo = sim.entities.rocket_silos.get_mut(&silo_id).unwrap();
    silo.parts_completed = silo.parts_per_rocket;
    silo.cargo_inventory
        .insert(&sim.world.prototypes, satellite, 1)
        .unwrap();
    sim.onboarding_progress.record_rocket_parts_completed();

    let progress = sim.rocket_program_progress();
    assert_eq!(progress.production_science_packs_produced, 12);
    assert_eq!(progress.utility_science_packs_produced, 15);
    assert!(progress.rocket_silo_researched);
    assert!(progress.powered_rocket_silo);
    assert_eq!(
        progress.rocket_parts_completed,
        progress.rocket_parts_required
    );
    assert!(progress.satellite_prepared);
    assert_eq!(progress.rockets_launched, 0);
    assert!(sim.onboarding_progress().rocket_silo_powered);
    assert!(sim.onboarding_progress().rocket_parts_completed);

    let satisfaction = {
        let status = sim
            .power
            .entity_statuses
            .get_mut(&silo_id)
            .expect("the silo should retain a power status");
        std::mem::replace(&mut status.satisfaction_permyriad, 0)
    };
    assert_eq!(sim.rocket_program_progress(), progress);
    sim.power
        .entity_statuses
        .get_mut(&silo_id)
        .unwrap()
        .satisfaction_permyriad = satisfaction;

    let bytes = crate::save_to_bytes(&sim).expect("late-game progress should save");
    let loaded = crate::load_from_bytes(&bytes).expect("late-game progress should load");
    assert_eq!(loaded.rocket_program_progress(), progress);
    assert!(loaded.onboarding_progress().rocket_silo_powered);
    assert!(loaded.onboarding_progress().rocket_parts_completed);
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
        .ingredients[0];
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
    assert!(
        sim.rocket_silo_status_for_entity(silo_id)
            .unwrap()
            .required_ticks
            > 0,
        "diagnostics should derive the unlocked duration before the next silo tick"
    );

    sim.tick();

    assert!(
        sim.entities.rocket_silos[&silo_id].crafting_required_ticks > 0,
        "the next tick derives the count the silo was missing"
    );
    sim.validate()
        .expect("and the corrected world is valid too");
}

#[test]
fn lab_unlock_is_visible_after_but_not_before_the_silo_pass() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let lab_id = place_lab(&mut sim);
    stock_rocket_silo(&mut sim, silo_id, 1);
    sim.tick();

    let technology_id = technology_id(&sim.world.prototypes, "rocket_silo");
    let technology = sim
        .world
        .prototypes
        .technology(technology_id)
        .expect("rocket silo technology should exist")
        .clone();
    let research = sim
        .research
        .technology_state_mut(technology_id)
        .expect("rocket silo research state should exist");
    research.completed_levels = 0;
    research.progress_units = technology.required_units - 1;
    sim.research.active = Some(technology_id);

    let lab = sim.entities.labs.get_mut(&lab_id).expect("lab was placed");
    lab.active_technology = Some(technology_id);
    lab.required_ticks = technology.research_time_ticks;
    lab.progress_ticks = lab.required_ticks - 1;
    for (slot_index, science_pack) in technology.science_packs.iter().enumerate() {
        set_inventory_slot(
            &mut lab.inventory,
            slot_index,
            science_pack.item,
            science_pack.amount,
        );
    }
    sim.power_demand_cache.invalidate();

    sim.tick();

    assert!(sim.is_technology_unlocked(technology_id));
    assert_eq!(
        sim.entities.rocket_silos[&silo_id].crafting_required_ticks, 0,
        "the silo pass must retain the recipe snapshot from before labs complete"
    );
    assert!(
        sim.rocket_silo_status_for_entity(silo_id)
            .expect("silo status should exist")
            .required_ticks
            > 0,
        "post-lab diagnostics must resolve the newly unlocked recipe"
    );
    sim.validate()
        .expect("the research-completion tick should remain valid");

    sim.tick();
    assert!(sim.entities.rocket_silos[&silo_id].crafting_required_ticks > 0);
}

#[test]
fn recipe_resolution_snapshots_do_not_affect_deterministic_state() {
    let mut observed = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut observed);
    stock_rocket_silo(&mut observed, silo_id, 1);
    let mut unobserved = observed.clone();
    observed.tick();
    unobserved.tick();

    for _ in 0..32 {
        let _ = observed.rocket_silo_recipe();
        let _ = observed.rocket_silo_status_for_entity(silo_id);
        let _ = observed.machine_statuses();
        let _ = observed.counts();
        observed.tick();
        unobserved.tick();
    }

    assert_eq!(observed.state_hash(), unobserved.state_hash());
}

#[test]
fn silo_validation_rejects_cargo_without_a_rocket_and_launch_without_cargo() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let satellite = item_id(&sim.world.prototypes, "satellite");
    sim.entities
        .rocket_silos
        .get_mut(&silo_id)
        .unwrap()
        .cargo_inventory
        .insert(&sim.world.prototypes, satellite, 1)
        .unwrap();
    assert!(sim.validate().is_err());

    {
        let state = sim.entities.rocket_silos.get_mut(&silo_id).unwrap();
        state.cargo_inventory.take_slot(0).unwrap();
        state.parts_completed = state.parts_per_rocket;
        state.launch_phase = RocketLaunchPhase::Sealed { ticks_remaining: 1 };
    }
    assert!(sim.validate().is_err());

    sim.entities
        .rocket_silos
        .get_mut(&silo_id)
        .unwrap()
        .cargo_inventory
        .insert(&sim.world.prototypes, satellite, 1)
        .unwrap();
    for invalid_phase in [
        RocketLaunchPhase::Sealed { ticks_remaining: 0 },
        RocketLaunchPhase::Sealed {
            ticks_remaining: crate::machines::rocket_silo::LAUNCH_SEAL_TICKS + 1,
        },
        RocketLaunchPhase::Rising { ticks_remaining: 0 },
        RocketLaunchPhase::Rising {
            ticks_remaining: crate::machines::rocket_silo::LAUNCH_RISE_TICKS + 1,
        },
    ] {
        sim.entities
            .rocket_silos
            .get_mut(&silo_id)
            .unwrap()
            .launch_phase = invalid_phase;
        assert!(
            crate::simulation::validation::machines::validate_rocket_silo(
                &sim,
                silo_id,
                &sim.entities.rocket_silos[&silo_id],
            )
            .is_err(),
            "accepted {invalid_phase:?}"
        );
    }
    for valid_phase in [
        RocketLaunchPhase::Sealed {
            ticks_remaining: crate::machines::rocket_silo::LAUNCH_SEAL_TICKS,
        },
        RocketLaunchPhase::Rising {
            ticks_remaining: crate::machines::rocket_silo::LAUNCH_RISE_TICKS,
        },
    ] {
        sim.entities
            .rocket_silos
            .get_mut(&silo_id)
            .unwrap()
            .launch_phase = valid_phase;
        crate::simulation::validation::machines::validate_rocket_silo(
            &sim,
            silo_id,
            &sim.entities.rocket_silos[&silo_id],
        )
        .unwrap_or_else(|error| {
            panic!("rejected reachable launch phase {valid_phase:?}: {error:?}")
        });
    }
}

#[test]
fn standard_inventory_routing_loads_and_unloads_completed_rocket_cargo() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let satellite = item_id(&sim.world.prototypes, "satellite");
    sim.entities
        .rocket_silos
        .get_mut(&silo_id)
        .unwrap()
        .parts_completed = sim.entities.rocket_silos[&silo_id].parts_per_rocket;
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, satellite, 1);

    crate::entity_transfer::transfer_container_slot(&mut sim, silo_id, InventoryPanel::Player, 0)
        .expect("a player click should route a satellite to completed rocket cargo");
    assert_eq!(
        sim.entities.rocket_silos[&silo_id]
            .cargo_inventory
            .count(satellite),
        1
    );

    crate::entity_transfer::transfer_container_slot(
        &mut sim,
        silo_id,
        InventoryPanel::RocketSiloCargo,
        0,
    )
    .expect("the visible cargo slot should return its satellite to the player");
    assert_eq!(sim.player_inventory.count(satellite), 1);

    let space_science = item_id(&sim.world.prototypes, "space_science_pack");
    sim.entities
        .rocket_silos
        .get_mut(&silo_id)
        .unwrap()
        .output_inventory
        .insert(&sim.world.prototypes, space_science, 200)
        .unwrap();
    crate::entity_transfer::transfer_container_slot(
        &mut sim,
        silo_id,
        InventoryPanel::RocketSiloOutput,
        0,
    )
    .expect("the visible output slot should return space science to the player");
    assert_eq!(sim.player_inventory.count(space_science), 200);
}

#[test]
fn player_routing_uses_the_configured_payload_and_loads_only_one() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    set_launch_payload(&mut sim, iron_plate);
    let state = sim.entities.rocket_silos.get_mut(&silo_id).unwrap();
    state.parts_completed = state.parts_per_rocket;
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, iron_plate, 5);

    let outcome = crate::entity_transfer::transfer_container_slot(
        &mut sim,
        silo_id,
        InventoryPanel::Player,
        0,
    )
    .expect("the configured stackable payload should route to rocket cargo");

    assert_eq!(outcome.moved_quantity, 1);
    assert_eq!(
        sim.entities.rocket_silos[&silo_id]
            .cargo_inventory
            .count(iron_plate),
        1
    );
    assert_eq!(sim.player_inventory.count(iron_plate), 4);
    assert!(
        crate::entity_transfer::transfer_container_slot(
            &mut sim,
            silo_id,
            InventoryPanel::Player,
            0,
        )
        .is_err(),
        "a second payload must not stack in the cargo holder"
    );
}

#[test]
fn completed_rocket_still_accepts_an_inserters_held_part_ingredient() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let ingredient = sim
        .rocket_silo_recipe()
        .expect("the part recipe should be unlocked")
        .ingredients[0]
        .item;
    let held_item = ItemStack::new(&sim.world.prototypes, ingredient, 1)
        .expect("a recipe ingredient should form a valid held stack");
    let silo = sim.entities.rocket_silos.get_mut(&silo_id).unwrap();
    silo.parts_completed = silo.parts_per_rocket;
    let drop_tile = {
        let footprint = sim.entities.placed_entity(silo_id).unwrap().footprint;
        (footprint.x, footprint.y)
    };

    assert!(crate::simulation::inserter_target_can_accept(
        &sim.world.prototypes,
        &sim.research,
        &sim.entities,
        sim.stopped_stock(),
        drop_tile,
        held_item,
    ));
}

#[test]
fn inserters_do_not_stack_a_second_configured_payload() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    set_launch_payload(&mut sim, iron_plate);
    let silo = sim.entities.rocket_silos.get_mut(&silo_id).unwrap();
    silo.parts_completed = silo.parts_per_rocket;
    silo.cargo_inventory
        .insert(&sim.world.prototypes, iron_plate, 1)
        .unwrap();
    let held_item = ItemStack::new(&sim.world.prototypes, iron_plate, 1).unwrap();
    let footprint = sim.entities.placed_entity(silo_id).unwrap().footprint;

    assert!(!crate::simulation::inserter_target_can_accept(
        &sim.world.prototypes,
        &sim.research,
        &sim.entities,
        sim.stopped_stock(),
        (footprint.x, footprint.y),
        held_item,
    ));
}

#[test]
fn completed_rocket_launches_satellite_over_fixed_ticks() {
    let mut sim = Simulation::new_test_world(123);
    assert_eq!(sim.rockets_launched(), 0);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let satellite = item_id(&sim.world.prototypes, "satellite");
    let space_science = item_id(&sim.world.prototypes, "space_science_pack");
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
    for _ in 0..179 {
        sim.tick();
    }
    assert_eq!(sim.rockets_launched(), 0);
    assert!(matches!(
        sim.entities.rocket_silos[&silo_id].launch_phase,
        RocketLaunchPhase::Rising { .. }
    ));
    sim.tick();

    let state = &sim.entities.rocket_silos[&silo_id];
    assert_eq!(state.launch_phase, RocketLaunchPhase::Idle);
    assert_eq!(state.parts_completed, 0);
    assert_eq!(state.cargo_inventory.count(satellite), 0);
    assert_eq!(state.output_inventory.count(space_science), 1_000);
    assert_eq!(sim.rockets_launched(), 1);
    assert_eq!(
        sim.item_statistics()
            .rows
            .iter()
            .find(|row| row.item_id == satellite)
            .map(|row| row.consumed_total),
        Some(1)
    );
    assert_eq!(
        sim.item_statistics()
            .rows
            .iter()
            .find(|row| row.item_id == space_science)
            .map(|row| row.produced_total),
        Some(1_000)
    );
}

#[test]
fn a_second_payload_launches_its_own_multi_product_reward() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let payload = item_id(&sim.world.prototypes, "iron_plate");
    let copper = item_id(&sim.world.prototypes, "copper_plate");
    let steel = item_id(&sim.world.prototypes, "steel_plate");
    set_launch_products(
        &mut sim,
        payload,
        vec![
            factory_data::ItemAmount {
                item: copper,
                amount: 125,
            },
            factory_data::ItemAmount {
                item: steel,
                amount: 40,
            },
        ],
    );
    let state = sim.entities.rocket_silos.get_mut(&silo_id).unwrap();
    state.parts_completed = state.parts_per_rocket;
    let held_payload = ItemStack::new(&sim.world.prototypes, payload, 1).unwrap();
    let footprint = sim.entities.placed_entity(silo_id).unwrap().footprint;
    assert!(crate::simulation::inserter_target_can_accept(
        &sim.world.prototypes,
        &sim.research,
        &sim.entities,
        sim.stopped_stock(),
        (footprint.x, footprint.y),
        held_payload,
    ));
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, payload, 1);
    crate::entity_transfer::player_slot_to_rocket_silo_cargo(&mut sim, silo_id, 0)
        .expect("the second data-defined payload should route to cargo");

    for _ in 0..181 {
        sim.tick();
    }

    let state = &sim.entities.rocket_silos[&silo_id];
    assert_eq!(state.output_inventory.count(copper), 125);
    assert_eq!(state.output_inventory.count(steel), 40);
    assert_eq!(state.cargo_inventory.count(payload), 0);
    assert_eq!(sim.rockets_launched(), 1);
    sim.validate()
        .expect("heterogeneous launch output should remain valid save state");
    for (product, expected) in [(copper, 125), (steel, 40)] {
        assert_eq!(
            sim.item_statistics()
                .rows
                .iter()
                .find(|row| row.item_id == product)
                .map(|row| row.produced_total),
            Some(expected)
        );
    }
}

#[test]
fn multi_product_launch_reward_is_atomic_when_the_output_is_fragmented() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let payload = item_id(&sim.world.prototypes, "iron_plate");
    let copper = item_id(&sim.world.prototypes, "copper_plate");
    let steel = item_id(&sim.world.prototypes, "steel_plate");
    set_launch_products(
        &mut sim,
        payload,
        vec![
            factory_data::ItemAmount {
                item: copper,
                amount: 1,
            },
            factory_data::ItemAmount {
                item: steel,
                amount: 1,
            },
        ],
    );
    let state = sim.entities.rocket_silos.get_mut(&silo_id).unwrap();
    state.parts_completed = state.parts_per_rocket;
    state.launch_phase = RocketLaunchPhase::Rising { ticks_remaining: 1 };
    state
        .cargo_inventory
        .insert(&sim.world.prototypes, payload, 1)
        .unwrap();
    for _ in 0..4 {
        state
            .output_inventory
            .insert(&sim.world.prototypes, copper, 100)
            .unwrap();
    }
    let before = state.output_inventory.clone();

    sim.advance_one_tick(&mut NoopTickProfiler);

    let state = &sim.entities.rocket_silos[&silo_id];
    assert_eq!(state.output_inventory, before);
    assert_eq!(
        state.launch_phase,
        RocketLaunchPhase::Rising { ticks_remaining: 1 }
    );
    assert_eq!(state.cargo_inventory.count(payload), 1);
    assert_eq!(sim.rockets_launched(), 0);
}

#[test]
fn malformed_final_launch_tick_does_not_produce_a_reward() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let space_science = item_id(&sim.world.prototypes, "space_science_pack");
    set_launch_payload(&mut sim, iron_plate);
    let state = sim.entities.rocket_silos.get_mut(&silo_id).unwrap();
    state.parts_completed = state.parts_per_rocket;
    state.launch_phase = RocketLaunchPhase::Rising { ticks_remaining: 1 };
    state
        .cargo_inventory
        .insert(&sim.world.prototypes, iron_plate, 2)
        .unwrap();
    assert!(sim.validate().is_err());

    sim.advance_one_tick(&mut NoopTickProfiler);

    let state = &sim.entities.rocket_silos[&silo_id];
    assert_eq!(
        state.launch_phase,
        RocketLaunchPhase::Rising { ticks_remaining: 1 }
    );
    assert_eq!(state.cargo_inventory.count(iron_plate), 2);
    assert_eq!(state.output_inventory.count(space_science), 0);
}

#[test]
fn launch_waits_until_the_full_reward_fits() {
    let mut sim = Simulation::new_test_world(123);
    let silo_id = place_powered_rocket_silo(&mut sim);
    let satellite = item_id(&sim.world.prototypes, "satellite");
    let space_science = item_id(&sim.world.prototypes, "space_science_pack");
    let state = sim.entities.rocket_silos.get_mut(&silo_id).unwrap();
    state.parts_completed = state.parts_per_rocket;
    state
        .cargo_inventory
        .insert(&sim.world.prototypes, satellite, 1)
        .unwrap();
    state
        .output_inventory
        .insert(&sim.world.prototypes, space_science, 1)
        .unwrap();

    sim.tick();
    assert_eq!(
        sim.entities.rocket_silos[&silo_id].launch_phase,
        RocketLaunchPhase::Idle
    );

    sim.entities
        .rocket_silos
        .get_mut(&silo_id)
        .unwrap()
        .output_inventory
        .remove(space_science, 1)
        .unwrap();
    sim.tick();
    assert!(matches!(
        sim.entities.rocket_silos[&silo_id].launch_phase,
        RocketLaunchPhase::Sealed { .. }
    ));
}

#[test]
fn inserter_extracts_space_science_from_the_launch_output() {
    let mut sim = Simulation::new_test_world(123);
    unlock_with_prerequisites(&mut sim, "rocket_silo");
    let silo = entity_id_by_name(&sim.world.prototypes, "rocket_silo");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let space_science = item_id(&sim.world.prototypes, "space_science_pack");
    let (x, y) = place_powered_fixture_origin(&mut sim, 9, 11, (3, 1));
    let silo_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: silo,
            x,
            y: y + 2,
            direction: Direction::North,
        },
    )
    .expect("rocket silo should be placeable");
    let inserter_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: inserter,
            x: x + 4,
            y: y + 1,
            direction: Direction::South,
        },
    )
    .expect("output inserter should be placeable");
    let chest_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chest,
            x: x + 4,
            y,
            direction: Direction::North,
        },
    )
    .expect("output chest should be placeable");
    sim.entities
        .rocket_silos
        .get_mut(&silo_id)
        .unwrap()
        .output_inventory
        .insert(&sim.world.prototypes, space_science, 1)
        .unwrap();

    run_inserter_until_holding(&mut sim, inserter_id);
    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        sim.entities.rocket_silos[&silo_id]
            .output_inventory
            .count(space_science),
        0
    );
    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .unwrap()
            .count(space_science),
        1
    );
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
    let saved_phase = sim.entities.rocket_silos[&silo_id].launch_phase;
    assert!(!matches!(saved_phase, RocketLaunchPhase::Idle));

    let bytes = crate::save_to_bytes(&sim).unwrap();
    let mut loaded = crate::load_from_bytes(&bytes).unwrap();
    assert_eq!(sim.state_hash(), loaded.state_hash());
    assert_eq!(
        loaded.entities.rocket_silos[&silo_id].launch_phase,
        saved_phase
    );
    for _ in 0..101 {
        loaded.tick();
    }
    assert_eq!(loaded.rockets_launched(), 1);
    assert_eq!(loaded.entities.rocket_silos[&silo_id].parts_completed, 0);
    assert_eq!(
        loaded.entities.rocket_silos[&silo_id]
            .output_inventory
            .count(item_id(&loaded.world.prototypes, "space_science_pack")),
        1_000
    );

    let finished_bytes = crate::save_to_bytes(&loaded).unwrap();
    let restored = crate::load_from_bytes(&finished_bytes).unwrap();
    assert_eq!(restored.rockets_launched(), 1);
    assert_eq!(restored.state_hash(), loaded.state_hash());
}
