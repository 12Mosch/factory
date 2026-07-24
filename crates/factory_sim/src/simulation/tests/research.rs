use super::super::*;
use super::support::*;

#[test]
fn new_simulations_start_with_automation_locked_and_no_progress() {
    let sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");

    assert!(!sim.is_technology_unlocked(automation));
    assert_eq!(sim.technology_progress(automation), Some(0));
    assert_eq!(sim.research.active, None);
}

#[test]
fn technology_unlocked_recipes_are_unavailable_until_researched() {
    let sim = Simulation::new_test_world(123);
    let assembling_machine = recipe_id(&sim.world.prototypes, "assembling_machine");
    let iron_gear_wheel = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let available_crafting = sim
        .available_recipes(CraftingCategory::Crafting)
        .into_iter()
        .map(|recipe| recipe.id)
        .collect::<Vec<_>>();

    assert!(!sim.is_recipe_unlocked(assembling_machine));
    assert!(sim.is_recipe_unlocked(iron_gear_wheel));
    assert!(!available_crafting.contains(&assembling_machine));
    assert!(available_crafting.contains(&iron_gear_wheel));
}

#[test]
fn locked_manual_craft_fails_without_consuming_ingredients() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "assembling_machine");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let electronic_circuit = item_id(&sim.world.prototypes, "electronic_circuit");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 9)
        .expect("test inventory should accept iron plates");
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_gear_wheel, 5)
        .expect("test inventory should accept gears");
    sim.player_inventory
        .insert(&sim.world.prototypes, electronic_circuit, 3)
        .expect("test inventory should accept circuits");
    let before = sim.player_inventory.clone();

    assert_eq!(
        sim.start_manual_craft(recipe),
        Err(CraftingError::RecipeLocked(recipe))
    );
    assert_eq!(sim.player_inventory, before);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn locked_assembler_recipe_selection_fails_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "assembling_machine");
    let before = crate::entity_access::assembler_state(&sim, assembler_id)
        .expect("assembler should expose state")
        .clone();

    assert_eq!(
        sim.select_assembler_recipe(assembler_id, recipe),
        Err(AssemblerError::RecipeLocked(recipe))
    );
    assert_eq!(
        sim.can_select_assembler_recipe(assembler_id, recipe),
        Ok(false)
    );
    assert_eq!(
        crate::entity_access::assembler_state(&sim, assembler_id)
            .expect("assembler should expose state"),
        &before
    );
}

#[test]
fn research_progress_unlocks_automation_recipe_effects() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let assembling_machine = recipe_id(&sim.world.prototypes, "assembling_machine");

    complete_research_by_name(&mut sim, "logistics");
    sim.select_research(automation)
        .expect("automation should be selectable");
    assert_eq!(
        sim.add_research_units(19),
        Ok(ResearchProgressResult::InProgress {
            technology_id: automation,
            progress_units: 19,
            required_units: 20,
        })
    );
    assert!(!sim.is_technology_unlocked(automation));
    assert!(!sim.is_recipe_unlocked(assembling_machine));
    assert_eq!(
        sim.add_research_units(1),
        Ok(ResearchProgressResult::Completed {
            technology_id: automation
        })
    );

    assert!(sim.is_technology_unlocked(automation));
    assert_eq!(sim.technology_progress(automation), Some(20));
    assert_eq!(sim.research.active, None);
    assert!(sim.is_recipe_unlocked(assembling_machine));
}

#[test]
fn zero_research_units_return_current_progress_without_advancing() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");

    complete_research_by_name(&mut sim, "logistics");
    sim.select_research(automation)
        .expect("automation should be selectable");

    assert_eq!(
        sim.add_research_units(0),
        Ok(ResearchProgressResult::InProgress {
            technology_id: automation,
            progress_units: 0,
            required_units: 20,
        })
    );
    assert_eq!(sim.technology_progress(automation), Some(0));
    assert!(!sim.is_technology_unlocked(automation));
}

#[test]
fn lab_consumes_science_and_increases_research_progress() {
    let mut sim = Simulation::new_test_world(123);
    let logistics = technology_id(&sim.world.prototypes, "logistics");
    let science_pack = item_id(&sim.world.prototypes, "automation_science_pack");
    let (chest_id, inserter_id, lab_id) = place_chest_inserter_lab_line(&mut sim);
    sim.select_research(logistics)
        .expect("logistics should be selectable");
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, chest_id)
            .expect("chest should expose inventory"),
        0,
        science_pack,
        1,
    );

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        crate::entity_access::inventory(&sim, lab_id)
            .expect("lab should expose inventory")
            .count(science_pack),
        1
    );

    let progress_after_insert = crate::entity_access::lab_state(&sim, lab_id)
        .expect("lab should expose state")
        .progress_ticks;
    for _ in progress_after_insert..599 {
        sim.tick();
    }

    assert_eq!(sim.technology_progress(logistics), Some(0));
    assert_eq!(
        crate::entity_access::inventory(&sim, lab_id)
            .expect("lab should expose inventory")
            .count(science_pack),
        1
    );

    sim.tick();

    assert_eq!(
        crate::entity_access::inventory(&sim, lab_id)
            .expect("lab should expose inventory")
            .count(science_pack),
        0
    );
    assert_eq!(sim.technology_progress(logistics), Some(1));
    assert!(!sim.is_technology_unlocked(logistics));
}

#[test]
fn multiple_labs_contribute_research_units_in_parallel() {
    let mut sim = Simulation::new_test_world(123);
    let logistics = technology_id(&sim.world.prototypes, "logistics");
    let science_pack = item_id(&sim.world.prototypes, "automation_science_pack");
    let first_lab = place_lab(&mut sim);
    let second_lab = place_lab(&mut sim);
    sim.select_research(logistics)
        .expect("logistics should be selectable");
    for lab_id in [first_lab, second_lab] {
        set_inventory_slot(
            crate::entity_access::inventory_mut(&mut sim, lab_id)
                .expect("lab should expose inventory"),
            0,
            science_pack,
            1,
        );
    }

    for _ in 0..600 {
        sim.tick();
    }

    assert_eq!(sim.technology_progress(logistics), Some(2));
    assert_eq!(
        crate::entity_access::inventory(&sim, first_lab)
            .expect("lab should expose inventory")
            .count(science_pack),
        0
    );
    assert_eq!(
        crate::entity_access::inventory(&sim, second_lab)
            .expect("lab should expose inventory")
            .count(science_pack),
        0
    );
}

#[test]
fn no_active_research_leaves_labs_idle() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let science_pack = item_id(&sim.world.prototypes, "automation_science_pack");
    let lab_id = place_lab(&mut sim);
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, lab_id).expect("lab should expose inventory"),
        0,
        science_pack,
        1,
    );

    for _ in 0..1_000 {
        sim.tick();
    }

    let lab = crate::entity_access::lab_state(&sim, lab_id).expect("lab should expose state");
    assert_eq!(lab.active_technology, None);
    assert_eq!(lab.progress_ticks, 0);
    assert_eq!(lab.required_ticks, 0);
    assert_eq!(lab.inventory.count(science_pack), 1);
    assert_eq!(sim.technology_progress(automation), Some(0));
}

#[test]
fn lab_completed_research_unlocks_recipe() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let science_pack = item_id(&sim.world.prototypes, "automation_science_pack");
    let assembling_machine = recipe_id(&sim.world.prototypes, "assembling_machine");
    let lab_id = place_lab(&mut sim);
    complete_research_by_name(&mut sim, "logistics");
    sim.select_research(automation)
        .expect("automation should be selectable");
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, lab_id).expect("lab should expose inventory"),
        0,
        science_pack,
        20,
    );

    for _ in 0..12_000 {
        sim.tick();
    }

    assert!(sim.is_technology_unlocked(automation));
    assert!(sim.is_recipe_unlocked(assembling_machine));
    assert_eq!(sim.research.active, None);
    assert_eq!(sim.technology_progress(automation), Some(20));
    assert_eq!(
        crate::entity_access::inventory(&sim, lab_id)
            .expect("lab should expose inventory")
            .count(science_pack),
        0
    );
}

#[test]
fn after_automation_unlock_assembling_machine_can_be_manually_crafted() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let recipe = recipe_id(&sim.world.prototypes, "assembling_machine");
    let assembling_machine = item_id(&sim.world.prototypes, "assembling_machine");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let electronic_circuit = item_id(&sim.world.prototypes, "electronic_circuit");
    complete_research_by_name(&mut sim, "logistics");
    sim.select_research(automation)
        .expect("automation should be selectable");
    sim.add_research_units(20)
        .expect("automation research should complete");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 9)
        .expect("test inventory should accept iron plates");
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_gear_wheel, 5)
        .expect("test inventory should accept gears");
    sim.player_inventory
        .insert(&sim.world.prototypes, electronic_circuit, 3)
        .expect("test inventory should accept circuits");

    sim.start_manual_craft(recipe)
        .expect("unlocked recipe should craft with enough ingredients");
    for _ in 0..30 {
        sim.tick();
    }

    assert_eq!(sim.player_inventory.count(assembling_machine), 1);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn research_progress_participates_in_state_hash_deterministically() {
    let mut first = Simulation::new_test_world(123);
    let mut second = Simulation::new_test_world(123);
    let logistics = technology_id(&first.world.prototypes, "logistics");
    let initial_hash = first.state_hash();

    first
        .select_research(logistics)
        .expect("logistics should be selectable");
    first
        .add_research_units(4)
        .expect("research should accept units");
    second
        .select_research(logistics)
        .expect("logistics should be selectable");
    second
        .add_research_units(4)
        .expect("research should accept units");

    assert_ne!(first.state_hash(), initial_hash);
    assert_eq!(first.state_hash(), second.state_hash());
}

#[test]
fn selecting_research_requires_completed_prerequisites() {
    let mut sim = Simulation::new_test_world(123);
    let logistics_2 = technology_id(&sim.world.prototypes, "logistics_2");
    let logistic_science_pack = technology_id(&sim.world.prototypes, "logistic_science_pack");

    assert_eq!(
        sim.select_research(logistics_2),
        Err(ResearchError::PrerequisiteLocked {
            technology_id: logistics_2,
            prerequisite_id: logistic_science_pack,
        })
    );
}

#[test]
fn research_queue_allows_prerequisites_before_dependents() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let logistics = technology_id(&sim.world.prototypes, "logistics");
    let electric_power = technology_id(&sim.world.prototypes, "electric_power");
    let logistic_science_pack = technology_id(&sim.world.prototypes, "logistic_science_pack");
    let logistics_2 = technology_id(&sim.world.prototypes, "logistics_2");

    sim.enqueue_research(logistics)
        .expect("logistics should enqueue and start");
    sim.enqueue_research(automation)
        .expect("automation should queue behind logistics");
    sim.enqueue_research(electric_power)
        .expect("electric power should queue behind automation");
    sim.enqueue_research(logistic_science_pack)
        .expect("logistic science should queue behind prerequisites");
    sim.enqueue_research(logistics_2)
        .expect("logistics 2 should queue behind prerequisites");

    assert_eq!(sim.active_research(), Some(logistics));
    assert_eq!(
        sim.research_queue(),
        &[
            automation,
            electric_power,
            logistic_science_pack,
            logistics_2
        ]
    );
}

#[test]
fn research_queue_auto_starts_next_technology_after_completion() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let logistics = technology_id(&sim.world.prototypes, "logistics");

    sim.enqueue_research(logistics)
        .expect("logistics should enqueue and start");
    sim.enqueue_research(automation)
        .expect("automation should queue");
    sim.add_research_units(15)
        .expect("logistics should complete");

    assert!(sim.is_technology_unlocked(logistics));
    assert_eq!(sim.active_research(), Some(automation));
    assert!(sim.research_queue().is_empty());
}

#[test]
fn logistics_3_unlocks_express_recipes_and_entities() {
    let mut sim = Simulation::new_test_world(123);
    let express_recipes = [
        recipe_id(&sim.world.prototypes, "express_transport_belt"),
        recipe_id(&sim.world.prototypes, "express_underground_belt"),
        recipe_id(&sim.world.prototypes, "express_splitter"),
    ];
    let express_entities = [
        entity_id_by_name(&sim.world.prototypes, "express_transport_belt"),
        entity_id_by_name(&sim.world.prototypes, "express_underground_belt_entrance"),
        entity_id_by_name(&sim.world.prototypes, "express_underground_belt_exit"),
        entity_id_by_name(&sim.world.prototypes, "express_splitter"),
    ];

    for recipe in express_recipes {
        assert!(!sim.is_recipe_unlocked(recipe));
    }
    for entity in express_entities {
        assert!(!sim.is_entity_unlocked(entity));
    }

    complete_research_by_name(&mut sim, "logistics");
    complete_research_by_name(&mut sim, "automation");
    complete_research_by_name(&mut sim, "electric_power");
    complete_research_by_name(&mut sim, "logistic_science_pack");
    complete_research_by_name(&mut sim, "logistics_2");

    for recipe in express_recipes {
        assert!(!sim.is_recipe_unlocked(recipe));
    }
    for entity in express_entities {
        assert!(!sim.is_entity_unlocked(entity));
    }

    complete_research_by_name(&mut sim, "fluid_handling");
    complete_research_by_name(&mut sim, "logistics_3");

    for recipe in express_recipes {
        assert!(sim.is_recipe_unlocked(recipe));
    }
    for entity in express_entities {
        assert!(sim.is_entity_unlocked(entity));
    }
}

#[test]
fn removing_queued_prerequisite_removes_dependent_technologies() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let logistics = technology_id(&sim.world.prototypes, "logistics");
    let electric_power = technology_id(&sim.world.prototypes, "electric_power");
    let logistic_science_pack = technology_id(&sim.world.prototypes, "logistic_science_pack");
    let logistics_2 = technology_id(&sim.world.prototypes, "logistics_2");

    sim.enqueue_research(logistics).unwrap();
    sim.enqueue_research(automation).unwrap();
    sim.enqueue_research(electric_power).unwrap();
    sim.enqueue_research(logistic_science_pack).unwrap();
    sim.enqueue_research(logistics_2).unwrap();

    assert_eq!(
        sim.remove_queued_research(0),
        Ok(vec![
            automation,
            electric_power,
            logistic_science_pack,
            logistics_2
        ])
    );
    assert_eq!(sim.active_research(), Some(logistics));
    assert!(sim.research_queue().is_empty());
}

#[test]
fn moving_queued_research_rejects_invalid_prerequisite_order() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let logistics = technology_id(&sim.world.prototypes, "logistics");
    let electric_power = technology_id(&sim.world.prototypes, "electric_power");
    let logistic_science_pack = technology_id(&sim.world.prototypes, "logistic_science_pack");
    let logistics_2 = technology_id(&sim.world.prototypes, "logistics_2");

    sim.enqueue_research(logistics).unwrap();
    sim.enqueue_research(automation).unwrap();
    sim.enqueue_research(electric_power).unwrap();
    sim.enqueue_research(logistic_science_pack).unwrap();
    sim.enqueue_research(logistics_2).unwrap();

    let queue_before_move = sim.research_queue().to_vec();

    assert_eq!(
        sim.can_move_queued_research(0, 1),
        Err(ResearchError::PrerequisiteLocked {
            technology_id: electric_power,
            prerequisite_id: automation,
        })
    );
    assert_eq!(sim.research_queue(), queue_before_move);
    assert_eq!(
        sim.move_queued_research(0, 1),
        Err(ResearchError::PrerequisiteLocked {
            technology_id: electric_power,
            prerequisite_id: automation,
        })
    );
    assert_eq!(
        sim.research_queue(),
        &[
            automation,
            electric_power,
            logistic_science_pack,
            logistics_2
        ]
    );
}

#[test]
#[ignore = "manual performance measurement"]
fn queued_research_move_validation_benchmark() {
    const ITERATIONS: usize = 10_000;

    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let logistics = technology_id(&sim.world.prototypes, "logistics");
    let electric_power = technology_id(&sim.world.prototypes, "electric_power");
    let logistic_science_pack = technology_id(&sim.world.prototypes, "logistic_science_pack");
    let logistics_2 = technology_id(&sim.world.prototypes, "logistics_2");

    sim.enqueue_research(logistics).unwrap();
    sim.enqueue_research(automation).unwrap();
    sim.enqueue_research(electric_power).unwrap();
    sim.enqueue_research(logistic_science_pack).unwrap();
    sim.enqueue_research(logistics_2).unwrap();

    for _ in 0..100 {
        let mut trial = sim.clone();
        assert!(trial.move_queued_research(0, 1).is_err());
        assert!(sim.can_move_queued_research(0, 1).is_err());
    }

    let clone_validation_start = std::time::Instant::now();
    for _ in 0..ITERATIONS {
        let mut trial = sim.clone();
        assert!(std::hint::black_box(trial.move_queued_research(0, 1)).is_err());
    }
    let clone_validation = clone_validation_start.elapsed();

    let research_validation_start = std::time::Instant::now();
    for _ in 0..ITERATIONS {
        assert!(std::hint::black_box(sim.can_move_queued_research(0, 1)).is_err());
    }
    let research_validation = research_validation_start.elapsed();

    eprintln!(
        "queued_research_move_validation: full_sim_clone={:.3} us/op, research_state={:.3} us/op, speedup={:.1}x",
        clone_validation.as_secs_f64() * 1_000_000.0 / ITERATIONS as f64,
        research_validation.as_secs_f64() * 1_000_000.0 / ITERATIONS as f64,
        clone_validation.as_secs_f64() / research_validation.as_secs_f64(),
    );
}

#[test]
fn labs_with_only_red_packs_cannot_progress_red_green_research() {
    let mut sim = Simulation::new_test_world(123);
    complete_research_by_name(&mut sim, "logistics");
    complete_research_by_name(&mut sim, "automation");
    complete_research_by_name(&mut sim, "electric_power");
    complete_research_by_name(&mut sim, "logistic_science_pack");
    let logistics_2 = technology_id(&sim.world.prototypes, "logistics_2");
    let red = item_id(&sim.world.prototypes, "automation_science_pack");
    let lab_id = place_lab(&mut sim);
    sim.select_research(logistics_2)
        .expect("logistics 2 prerequisites should be complete");
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, lab_id).expect("lab should expose inventory"),
        0,
        red,
        1,
    );

    for _ in 0..600 {
        sim.tick();
    }

    assert_eq!(sim.technology_progress(logistics_2), Some(0));
    assert_eq!(
        crate::entity_access::inventory(&sim, lab_id)
            .unwrap()
            .count(red),
        1,
        "red pack should not be consumed without green"
    );
}

#[test]
fn labs_consume_exact_required_red_and_green_packs_per_unit() {
    let mut sim = Simulation::new_test_world(123);
    complete_research_by_name(&mut sim, "logistics");
    complete_research_by_name(&mut sim, "automation");
    complete_research_by_name(&mut sim, "electric_power");
    complete_research_by_name(&mut sim, "logistic_science_pack");
    let logistics_2 = technology_id(&sim.world.prototypes, "logistics_2");
    let red = item_id(&sim.world.prototypes, "automation_science_pack");
    let green = item_id(&sim.world.prototypes, "logistic_science_pack");
    let lab_id = place_lab(&mut sim);
    sim.select_research(logistics_2)
        .expect("logistics 2 prerequisites should be complete");
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, lab_id).expect("lab should expose inventory"),
        0,
        red,
        1,
    );
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, lab_id).unwrap(),
        1,
        green,
        1,
    );

    for _ in 0..600 {
        sim.tick();
    }

    assert_eq!(sim.technology_progress(logistics_2), Some(1));
    let inventory = crate::entity_access::inventory(&sim, lab_id).unwrap();
    assert_eq!(inventory.count(red), 0);
    assert_eq!(inventory.count(green), 0);
}

#[test]
fn red_only_research_does_not_consume_green_packs() {
    let mut sim = Simulation::new_test_world(123);
    let logistics = technology_id(&sim.world.prototypes, "logistics");
    let red = item_id(&sim.world.prototypes, "automation_science_pack");
    let green = item_id(&sim.world.prototypes, "logistic_science_pack");
    let lab_id = place_lab(&mut sim);
    sim.select_research(logistics)
        .expect("logistics should be selectable");
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, lab_id).unwrap(),
        0,
        red,
        1,
    );
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, lab_id).unwrap(),
        1,
        green,
        1,
    );

    for _ in 0..600 {
        sim.tick();
    }

    let inventory = crate::entity_access::inventory(&sim, lab_id).unwrap();
    assert_eq!(sim.technology_progress(logistics), Some(1));
    assert_eq!(inventory.count(red), 0);
    assert_eq!(inventory.count(green), 1);
}

#[test]
fn labs_atomically_consume_five_science_pack_types_per_research_unit() {
    let mut catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let pack_ids = [
        "automation_science_pack",
        "logistic_science_pack",
        "chemical_science_pack",
        "production_science_pack",
        "utility_science_pack",
    ]
    .map(|name| item_id(&catalog, name));
    let technology_id = technology_id(&catalog, "logistics");
    let technology = catalog
        .technologies
        .iter_mut()
        .find(|technology| technology.id == technology_id)
        .expect("logistics technology should exist");
    technology.science_packs = pack_ids
        .map(|item| factory_data::ItemAmount { item, amount: 1 })
        .to_vec();
    technology.required_units = 2;
    technology.research_time_ticks = 600;

    let (complete_sim, complete_lab_id) =
        run_five_pack_lab_scenario(&catalog, technology_id, pack_ids, None);
    let military = item_id(&catalog, "military_science_pack");

    assert_eq!(complete_sim.technology_progress(technology_id), Some(1));
    let complete_inventory =
        crate::entity_access::inventory(&complete_sim, complete_lab_id).unwrap();
    for pack in pack_ids {
        assert_eq!(
            complete_inventory.count(pack),
            0,
            "exactly one of every required pack should be consumed"
        );
    }
    assert_eq!(
        complete_inventory.count(military),
        1,
        "an unrelated science pack should not be consumed"
    );

    let missing_pack = pack_ids[4];
    let (missing_sim, missing_lab_id) =
        run_five_pack_lab_scenario(&catalog, technology_id, pack_ids, Some(missing_pack));

    assert_eq!(missing_sim.technology_progress(technology_id), Some(0));
    let lab = crate::entity_access::lab_state(&missing_sim, missing_lab_id).unwrap();
    assert_eq!(lab.progress_ticks, 0);
    for pack in pack_ids {
        assert_eq!(
            lab.inventory.count(pack),
            u32::from(pack != missing_pack),
            "missing a required pack should prevent all partial consumption"
        );
    }
    assert_eq!(lab.inventory.count(military), 1);
}

fn run_five_pack_lab_scenario(
    catalog: &PrototypeCatalog,
    technology_id: TechnologyId,
    pack_ids: [ItemId; 5],
    missing_pack: Option<ItemId>,
) -> (Simulation, EntityId) {
    let mut sim = Simulation::new(123, catalog.clone());
    let military = item_id(catalog, "military_science_pack");
    let lab_id = place_lab(&mut sim);
    sim.select_research(technology_id)
        .expect("five-pack test technology should be selectable");

    // Reverse slot order proves consumption follows required-pack data rather
    // than relying on science packs occupying matching inventory positions.
    for (slot, pack) in pack_ids.iter().rev().enumerate() {
        if Some(*pack) != missing_pack {
            set_inventory_slot(
                crate::entity_access::inventory_mut(&mut sim, lab_id)
                    .expect("lab should expose inventory"),
                slot,
                *pack,
                1,
            );
        }
    }
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, lab_id).unwrap(),
        pack_ids.len(),
        military,
        1,
    );

    for _ in 0..600 {
        sim.tick();
    }

    (sim, lab_id)
}

#[test]
fn research_progress_and_queue_survive_save_load() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let logistics = technology_id(&sim.world.prototypes, "logistics");
    sim.enqueue_research(logistics)
        .expect("logistics should start");
    sim.enqueue_research(automation)
        .expect("automation should queue");
    sim.add_research_units(4)
        .expect("partial research should be recorded");
    let before_hash = sim.state_hash();

    let bytes = save_to_bytes(&sim).expect("save should serialize");
    let loaded = load_from_bytes(&bytes).expect("save should load");

    assert_eq!(loaded.active_research(), Some(logistics));
    assert_eq!(loaded.research_queue(), &[automation]);
    assert_eq!(loaded.technology_progress(logistics), Some(4));
    assert_eq!(loaded.state_hash(), before_hash);
}

#[test]
fn completing_research_unlocks_recipe_and_derived_entity() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id(&sim.world.prototypes, "automation");
    let assembler_recipe = recipe_id(&sim.world.prototypes, "assembling_machine");
    let assembler_entity = entity_id_by_name(&sim.world.prototypes, "assembling_machine");

    assert!(!sim.is_recipe_unlocked(assembler_recipe));
    assert!(!sim.is_entity_unlocked(assembler_entity));
    complete_research_by_name(&mut sim, "logistics");
    sim.select_research(automation)
        .expect("automation should be selectable");
    sim.add_research_units(20)
        .expect("automation should complete");

    assert!(sim.is_recipe_unlocked(assembler_recipe));
    assert!(sim.is_entity_unlocked(assembler_entity));
}
