use factory_data::{entity_prototype_id_by_name, item_id_by_name};
use factory_sim::{CHUNK_SIZE, Direction, EntityId, Simulation, load_from_bytes, save_to_bytes};
use std::time::{Duration, Instant};

const SIXTY_UPS_TICK_BUDGET: Duration = Duration::from_nanos(16_667_000);

#[test]
#[ignore]
fn one_thousand_machines_stays_within_60_ups_budget() {
    let mut sim = Simulation::new_seeded(123);
    place_entities(&mut sim, "assembling_machine", 1_000, Direction::North);
    run_warmup_ticks(&mut sim, 120);

    let stats = collect_tick_stats(&mut sim, 600);
    print_tick_stats("one_thousand_machines", stats);
    assert_within_60_ups_budget(stats);
}

#[test]
#[ignore]
fn five_thousand_belts_stays_within_60_ups_budget() {
    let mut sim = Simulation::new_seeded(123);
    place_entities(&mut sim, "transport_belt", 5_000, Direction::East);
    run_warmup_ticks(&mut sim, 120);

    let stats = collect_tick_stats(&mut sim, 600);
    print_tick_stats("five_thousand_belts", stats);
    assert_within_60_ups_budget(stats);
}

#[test]
#[ignore]
fn ten_thousand_belt_items_stays_within_60_ups_budget() {
    let mut sim = Simulation::new_seeded(123);
    let belts = place_entities(&mut sim, "transport_belt", 5_000, Direction::East);
    let iron_ore = item_id_by_name(sim.catalog(), "iron_ore");
    for belt_id in belts {
        sim.insert_item_onto_belt(belt_id, 0, iron_ore)
            .expect("empty belt lane 0 should accept a budget item");
        sim.insert_item_onto_belt(belt_id, 1, iron_ore)
            .expect("empty belt lane 1 should accept a budget item");
    }
    run_warmup_ticks(&mut sim, 120);

    let stats = collect_tick_stats(&mut sim, 600);
    print_tick_stats("ten_thousand_belt_items", stats);
    assert_within_60_ups_budget(stats);
}

#[test]
#[ignore]
fn one_hundred_thousand_headless_ticks_no_panic_or_invalid_state() {
    let mut sim = Simulation::new_scripted_red_science_factory();
    let started = Instant::now();

    for _ in 0..100_000 {
        sim.tick();
    }

    println!(
        "one_hundred_thousand_headless_ticks: {:.3} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    sim.validate_state()
        .expect("100k headless tick budget run should leave a valid state");
}

#[test]
#[ignore]
fn save_load_state_hash_identical() {
    let mut sim = Simulation::new_scripted_red_science_factory();
    run_warmup_ticks(&mut sim, 10_000);

    let started = Instant::now();
    let before = sim.state_hash();
    let bytes = save_to_bytes(&sim).expect("budget fixture should save");
    let loaded = load_from_bytes(&bytes).expect("budget fixture should load");
    println!(
        "save_load_state_hash_identical: {:.3} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );

    assert_eq!(before, loaded.state_hash());
}

fn place_entities(
    sim: &mut Simulation,
    prototype_name: &str,
    count: usize,
    direction: Direction,
) -> Vec<EntityId> {
    let prototype_id = entity_prototype_id_by_name(sim.catalog(), prototype_name);
    let mut placed = Vec::with_capacity(count);
    let coords = deterministic_tile_coords(sim);

    for (x, y) in coords {
        if placed.len() == count {
            return placed;
        }
        if sim.can_place_entity(prototype_id, x, y, direction).is_err() {
            continue;
        }
        let entity_id = sim
            .place_entity(prototype_id, x, y, direction)
            .expect("validated budget placement should succeed");
        placed.push(entity_id);
    }

    panic!(
        "could only place {} of {count} {prototype_name}",
        placed.len()
    );
}

fn deterministic_tile_coords(sim: &Simulation) -> Vec<(i32, i32)> {
    sim.world()
        .chunks
        .values()
        .flat_map(|chunk| {
            chunk.tiles.iter().enumerate().map(move |(index, _)| {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                (
                    chunk.coord.x * CHUNK_SIZE + local_x,
                    chunk.coord.y * CHUNK_SIZE + local_y,
                )
            })
        })
        .collect()
}

fn run_warmup_ticks(sim: &mut Simulation, ticks: usize) {
    for _ in 0..ticks {
        sim.tick();
    }
}

#[derive(Clone, Copy)]
struct TickStats {
    average: Duration,
    p95: Duration,
}

fn collect_tick_stats(sim: &mut Simulation, ticks: usize) -> TickStats {
    assert!(ticks > 0);
    let mut durations = Vec::with_capacity(ticks);

    for _ in 0..ticks {
        let started = Instant::now();
        sim.profiled_tick();
        durations.push(started.elapsed());
    }

    durations.sort_unstable();
    let total_nanos = durations.iter().map(Duration::as_nanos).sum::<u128>();
    let average = Duration::from_nanos((total_nanos / ticks as u128) as u64);
    let p95_index = ((ticks * 95).div_ceil(100)).saturating_sub(1);

    TickStats {
        average,
        p95: durations[p95_index],
    }
}

fn print_tick_stats(name: &str, stats: TickStats) {
    println!(
        "{name}: average {:.3} ms, p95 {:.3} ms",
        stats.average.as_secs_f64() * 1000.0,
        stats.p95.as_secs_f64() * 1000.0
    );
}

fn assert_within_60_ups_budget(stats: TickStats) {
    assert!(
        stats.p95 <= SIXTY_UPS_TICK_BUDGET,
        "p95 {:.3} ms exceeded 60 UPS budget {:.3} ms",
        stats.p95.as_secs_f64() * 1000.0,
        SIXTY_UPS_TICK_BUDGET.as_secs_f64() * 1000.0
    );
}
