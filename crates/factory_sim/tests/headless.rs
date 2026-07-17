use factory_sim::{
    SimCommand, Simulation, load_from_bytes, save_to_bytes, scripted_inputs_for_red_science_factory,
};

/// Advances the scripted chemical science factory by one tick, applying the
/// fixture's recipe-selection program first, exactly like a scripted player
/// would.
fn tick_chemical_science_factory(sim: &mut Simulation) {
    sim.apply_command(&SimCommand::RunChemicalScienceFactoryProgram)
        .expect("chemical science program should apply");
    sim.tick();
}

#[test]
fn sim_runs_3600_ticks_without_bevy() {
    let mut sim = Simulation::new_test_world(123);
    for _ in 0..3600 {
        sim.tick();
    }
}

#[test]
fn same_seed_same_hash_after_ticks() {
    let mut a = Simulation::new_test_world(42);
    let mut b = Simulation::new_test_world(42);

    for _ in 0..10_000 {
        a.tick();
        b.tick();
    }

    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn same_seed_same_inputs_same_hash() {
    let inputs = scripted_inputs_for_red_science_factory();

    let mut a = Simulation::new_seeded(123);
    let mut b = Simulation::new_seeded(123);

    for input in inputs {
        a.apply_command(&input).unwrap();
        b.apply_command(&input).unwrap();
        a.tick();
        b.tick();
    }

    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn red_science_factory_is_stable_for_100k_ticks() {
    let mut sim = Simulation::new_scripted_red_science_factory();

    for _ in 0..100_000 {
        sim.tick();
    }

    assert!(sim.research.is_unlocked("basic-automation"));
    assert!(sim.validate_item_conservation());
}

#[test]
fn save_load_preserves_state_hash() {
    let mut sim = Simulation::new_scripted_red_science_factory();

    for _ in 0..10_000 {
        sim.tick();
    }

    let before = sim.state_hash();
    let bytes = save_to_bytes(&sim).unwrap();
    let loaded = load_from_bytes(&bytes).unwrap();

    assert_eq!(before, loaded.state_hash());
}

#[test]
fn chemical_science_factory_reaches_automated_chemical_science() {
    let mut sim = Simulation::new_scripted_chemical_science_factory();

    for _ in 0..100_000 {
        tick_chemical_science_factory(&mut sim);
    }

    for technology in [
        "chemical_science_pack",
        "advanced_oil_processing",
        "lubricant",
        "advanced_material_processing_2",
        "electric_energy_distribution_2",
    ] {
        assert!(
            sim.research.is_unlocked(technology),
            "{technology} should be researched by the scripted factory"
        );
    }
    assert!(sim.validate_item_conservation());
}

#[test]
fn chemical_science_factory_same_construction_same_hash() {
    let mut a = Simulation::new_scripted_chemical_science_factory();
    let mut b = Simulation::new_scripted_chemical_science_factory();

    for _ in 0..5_000 {
        tick_chemical_science_factory(&mut a);
        tick_chemical_science_factory(&mut b);
    }

    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn chemical_science_factory_save_load_then_continue_matches_original() {
    let mut a = Simulation::new_scripted_chemical_science_factory();

    for _ in 0..30_000 {
        tick_chemical_science_factory(&mut a);
    }

    let bytes = save_to_bytes(&a).unwrap();
    let mut b = load_from_bytes(&bytes).unwrap();
    assert_eq!(a.state_hash(), b.state_hash());

    for _ in 0..30_000 {
        tick_chemical_science_factory(&mut a);
        tick_chemical_science_factory(&mut b);
    }

    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn save_load_then_continue_matches_original() {
    let mut a = Simulation::new_scripted_red_science_factory();

    for _ in 0..10_000 {
        a.tick();
    }

    let bytes = save_to_bytes(&a).unwrap();
    let mut b = load_from_bytes(&bytes).unwrap();

    for _ in 0..10_000 {
        a.tick();
        b.tick();
    }

    assert_eq!(a.state_hash(), b.state_hash());
}
