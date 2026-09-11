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
fn same_seed_same_inputs_same_hash() {
    let inputs = scripted_inputs_for_red_science_factory();

    let mut a = Simulation::new_seeded(123);
    let mut b = Simulation::new_seeded(123);

    for input in &inputs {
        a.apply_command(input).unwrap();
        b.apply_command(input).unwrap();
    }
    for _ in 0..10_000 {
        a.tick();
        b.tick();
    }

    assert_eq!(a.state_hash(), b.state_hash());
}

/// Nightly soak: the scripted chemical science factory must reach its
/// automated chemical science milestones within 100,000 ticks. The full
/// duration is part of the test, so it is ignored by default; run it
/// explicitly with `cargo test -- --ignored` or from a scheduled stress job.
#[test]
#[ignore]
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

/// Advances the scripted chemical factory until `technology` unlocks. The
/// fixture is deterministic, so milestones land on schedule; the bound only
/// guards against hanging forever if progression ever breaks.
fn advance_until_chemical_research(sim: &mut Simulation, technology: &str, max_ticks: u64) {
    for _ in 0..max_ticks {
        if sim.research.is_unlocked(technology) {
            return;
        }
        tick_chemical_science_factory(sim);
    }
    panic!("{technology} should unlock within {max_ticks} ticks");
}

/// Saves `original`, reloads into a fresh simulation, asserts the hashes
/// match immediately, then advances both `post_ticks` and asserts they
/// still match.
fn assert_save_load_continuation_matches(original: &mut Simulation, post_ticks: u64) {
    let bytes = save_to_bytes(original).unwrap();
    let mut loaded = load_from_bytes(&bytes).unwrap();
    assert_eq!(original.state_hash(), loaded.state_hash());

    for _ in 0..post_ticks {
        tick_chemical_science_factory(original);
        tick_chemical_science_factory(&mut loaded);
    }

    assert_eq!(original.state_hash(), loaded.state_hash());
}

#[test]
fn chemical_science_factory_save_load_then_continue_matches_original() {
    let mut a = Simulation::new_scripted_chemical_science_factory();

    // Milestone-driven pre-phase: reach the oil era (refinery running) and
    // progress into plastics (chemical plants assigned) so the save boundary
    // captures mid-game runtime state, not a barely-started factory. The
    // per-tick program assigns each newly unlocked recipe on retry, a path
    // the 100k progression soak validates end to end via blue-science
    // unlocks that are unreachable without real production.
    for milestone in ["oil_processing", "plastics"] {
        advance_until_chemical_research(&mut a, milestone, 40_000);
    }
    for _ in 0..10 {
        tick_chemical_science_factory(&mut a);
    }

    assert_save_load_continuation_matches(&mut a, 2_000);
}

/// Nightly soak: long-duration persistence once blue science is active.
/// Ignored by default; run it explicitly with `cargo test -- --ignored` or
/// from a scheduled stress job.
#[test]
#[ignore]
fn chemical_science_factory_save_load_soak_after_blue_science_then_continue_matches_original() {
    let mut a = Simulation::new_scripted_chemical_science_factory();

    advance_until_chemical_research(&mut a, "chemical_science_pack", 60_000);
    for _ in 0..10 {
        tick_chemical_science_factory(&mut a);
    }

    assert_save_load_continuation_matches(&mut a, 30_000);
}

#[test]
fn save_load_then_continue_matches_original() {
    let mut a = Simulation::new_scripted_red_science_factory();

    for _ in 0..10_000 {
        a.tick();
    }

    let bytes = save_to_bytes(&a).unwrap();
    let mut b = load_from_bytes(&bytes).unwrap();
    assert_eq!(a.state_hash(), b.state_hash());

    for _ in 0..10_000 {
        a.tick();
        b.tick();
    }

    assert_eq!(a.state_hash(), b.state_hash());
}

/// The robot flight fixture has to actually produce the robots it promises, or
/// the performance suites that use it would measure an empty sky.
#[test]
fn robot_flight_fixture_fills_the_sky_and_stays_valid() {
    let mut sim = Simulation::new_robot_flight_fixture(2_000);
    assert_eq!(sim.robot_count(), 2_000);

    for _ in 0..120 {
        sim.tick();
    }

    assert!(sim.robot_count() > 0, "robots should still be flying");
    sim.validate_state()
        .expect("a sky full of robots should stay valid");
}
