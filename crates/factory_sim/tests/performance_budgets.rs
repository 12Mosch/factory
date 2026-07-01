use factory_data::{entity_prototype_id_by_name, item_id_by_name, recipe_id_by_name};
use factory_sim::{
    CHUNK_SIZE, ChunkCoord, Direction, EntityId, Inventory, ItemStack, Simulation,
    SimulationCounts, SimulationTickProfile, load_from_bytes, save_to_bytes,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const SIXTY_UPS_TICK_BUDGET: Duration = Duration::from_nanos(16_667_000);
const SMALL_ALLOC_P95_BYTES_BUDGET: u64 = 1024 * 1024;
const SMALL_ALLOC_P95_COUNT_BUDGET: u64 = 2_000;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static BENCHMARK_LOCK: Mutex<()> = Mutex::new(());

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[test]
#[ignore]
fn small_factory_benchmark_100_machines_1000_belts() {
    run_factory_benchmark(FactoryBenchmarkSpec {
        name: "small_factory",
        machines: 100,
        belts: 1_000,
        inserters: 100,
        fluid_fixtures: 10,
        warmup_ticks: 120,
        measurement_ticks: 600,
        assert_60_ups: true,
    });
}

#[test]
#[ignore]
fn medium_factory_benchmark_1000_machines_10000_belts() {
    run_factory_benchmark(FactoryBenchmarkSpec {
        name: "medium_factory",
        machines: 1_000,
        belts: 10_000,
        inserters: 1_000,
        fluid_fixtures: 100,
        warmup_ticks: 120,
        measurement_ticks: 600,
        assert_60_ups: false,
    });
}

#[test]
#[ignore]
fn large_headless_stress_5000_machines_50000_belts() {
    run_factory_benchmark(FactoryBenchmarkSpec {
        name: "large_headless_stress",
        machines: 5_000,
        belts: 50_000,
        inserters: 5_000,
        fluid_fixtures: 500,
        warmup_ticks: 60,
        measurement_ticks: 300,
        assert_60_ups: false,
    });
}

#[test]
#[ignore]
fn one_hundred_thousand_headless_ticks_no_panic_or_invalid_state() {
    let _guard = BENCHMARK_LOCK
        .lock()
        .expect("benchmark lock should not poison");
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
    let _guard = BENCHMARK_LOCK
        .lock()
        .expect("benchmark lock should not poison");
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

#[derive(Clone, Copy)]
struct FactoryBenchmarkSpec {
    name: &'static str,
    machines: usize,
    belts: usize,
    inserters: usize,
    fluid_fixtures: usize,
    warmup_ticks: usize,
    measurement_ticks: usize,
    assert_60_ups: bool,
}

#[derive(Clone, Copy)]
struct AllocationSample {
    count: u64,
    bytes: u64,
}

#[derive(Clone, Copy)]
struct TickSample {
    profile: SimulationTickProfile,
    allocations: AllocationSample,
}

#[derive(Clone, Copy)]
struct BenchmarkStats {
    average: SimulationTickProfile,
    p95: SimulationTickProfile,
    max: SimulationTickProfile,
    counts: SimulationCounts,
    alloc_average_bytes: u64,
    alloc_p95_bytes: u64,
    alloc_max_bytes: u64,
    alloc_average_count: u64,
    alloc_p95_count: u64,
    alloc_max_count: u64,
}

fn run_factory_benchmark(spec: FactoryBenchmarkSpec) {
    let _guard = BENCHMARK_LOCK
        .lock()
        .expect("benchmark lock should not poison");
    let mut sim = build_factory_benchmark(spec);
    run_warmup_ticks(&mut sim, spec.warmup_ticks);

    let stats = collect_benchmark_stats(&mut sim, spec.measurement_ticks);
    print_benchmark_stats(spec.name, stats);

    sim.validate_state()
        .expect("benchmark run should leave a valid simulation state");
    if spec.assert_60_ups {
        assert!(
            stats.p95.total <= SIXTY_UPS_TICK_BUDGET,
            "p95 {:.3} ms exceeded 60 UPS budget {:.3} ms",
            ms(stats.p95.total),
            ms(SIXTY_UPS_TICK_BUDGET)
        );
        assert!(
            stats.alloc_p95_bytes <= SMALL_ALLOC_P95_BYTES_BUDGET,
            "allocation p95 {} bytes exceeded {} bytes",
            stats.alloc_p95_bytes,
            SMALL_ALLOC_P95_BYTES_BUDGET
        );
        assert!(
            stats.alloc_p95_count <= SMALL_ALLOC_P95_COUNT_BUDGET,
            "allocation p95 {} allocs exceeded {} allocs",
            stats.alloc_p95_count,
            SMALL_ALLOC_P95_COUNT_BUDGET
        );
    }
}

fn build_factory_benchmark(spec: FactoryBenchmarkSpec) -> Simulation {
    let mut sim = Simulation::new_seeded(123);
    generate_extra_chunks(&mut sim, spec);

    let machine_ids = place_entities(
        &mut sim,
        "assembling_machine",
        spec.machines,
        Direction::North,
    );
    seed_assemblers(&mut sim, &machine_ids);

    let belt_ids = place_entities(&mut sim, "transport_belt", spec.belts, Direction::East);
    seed_belts(&mut sim, &belt_ids);

    place_entities(&mut sim, "inserter", spec.inserters, Direction::East);
    place_representative_power_poles(&mut sim, spec);
    place_fluid_fixtures(&mut sim, spec.fluid_fixtures);

    let counts = sim.counts();
    assert_eq!(counts.machine_count, spec.machines);
    assert_eq!(counts.belt_count, spec.belts);
    assert_eq!(counts.inserter_count, spec.inserters);
    sim.tick();
    sim.validate_state()
        .expect("constructed benchmark fixture should be valid");
    sim
}

fn generate_extra_chunks(sim: &mut Simulation, spec: FactoryBenchmarkSpec) {
    let requested_tiles = spec
        .belts
        .saturating_add(spec.machines.saturating_mul(12))
        .saturating_add(spec.inserters.saturating_mul(2))
        .saturating_add(spec.fluid_fixtures.saturating_mul(12));
    let chunks_needed = requested_tiles.div_ceil((CHUNK_SIZE * CHUNK_SIZE) as usize);
    let radius = (((chunks_needed as f64).sqrt() / 2.0).ceil() as i32 + 3).max(4);

    for y in -radius..=radius {
        for x in -radius..=radius {
            sim.ensure_chunk_generated(ChunkCoord { x, y });
        }
    }
}

fn place_entities(
    sim: &mut Simulation,
    prototype_name: &str,
    count: usize,
    direction: Direction,
) -> Vec<EntityId> {
    let prototype_id = entity_prototype_id_by_name(sim.catalog(), prototype_name);
    let mut placed = Vec::with_capacity(count);

    for (x, y) in deterministic_tile_coords(sim) {
        if placed.len() == count {
            return placed;
        }
        if sim.can_place_entity(prototype_id, x, y, direction).is_err() {
            continue;
        }
        let entity_id = sim
            .place_entity(prototype_id, x, y, direction)
            .expect("validated benchmark placement should succeed");
        placed.push(entity_id);
    }

    panic!(
        "could only place {} of {count} {prototype_name}",
        placed.len()
    );
}

fn seed_assemblers(sim: &mut Simulation, machine_ids: &[EntityId]) {
    let recipe = recipe_id_by_name(sim.catalog(), "iron_gear_wheel");
    let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");

    for machine_id in machine_ids {
        sim.select_assembler_recipe(*machine_id, recipe)
            .expect("benchmark assembler recipe should be selectable");
        *sim.player_inventory_mut() = Inventory::player();
        sim.player_inventory_mut().slots[0] = Some(ItemStack {
            item_id: iron_plate,
            count: 100,
        });
        sim.transfer_player_slot_to_assembler_input(*machine_id, 0)
            .expect("benchmark assembler should accept seeded iron plates");
    }
    *sim.player_inventory_mut() = Inventory::player();
}

fn seed_belts(sim: &mut Simulation, belt_ids: &[EntityId]) {
    let iron_ore = item_id_by_name(sim.catalog(), "iron_ore");
    for belt_id in belt_ids {
        let _ = sim.insert_item_onto_belt(*belt_id, 0, iron_ore);
        let _ = sim.insert_item_onto_belt(*belt_id, 1, iron_ore);
    }
}

fn place_representative_power_poles(sim: &mut Simulation, spec: FactoryBenchmarkSpec) {
    let pole_target = (spec.machines.saturating_add(spec.inserters))
        .div_ceil(8)
        .max(1);
    let pole = entity_prototype_id_by_name(sim.catalog(), "small_electric_pole");
    let mut placed = 0;

    for (x, y) in deterministic_tile_coords(sim).into_iter().step_by(5) {
        if placed == pole_target {
            return;
        }
        if sim.can_place_entity(pole, x, y, Direction::North).is_err() {
            continue;
        }
        sim.place_entity(pole, x, y, Direction::North)
            .expect("validated benchmark pole placement should succeed");
        placed += 1;
    }
}

fn place_fluid_fixtures(sim: &mut Simulation, count: usize) {
    let pump = entity_prototype_id_by_name(sim.catalog(), "offshore_pump");
    let boiler = entity_prototype_id_by_name(sim.catalog(), "boiler");
    let steam_engine = entity_prototype_id_by_name(sim.catalog(), "steam_engine");
    let coal = item_id_by_name(sim.catalog(), "coal");
    let mut placed = 0;

    for (x, y) in deterministic_tile_coords(sim) {
        if placed == count {
            return;
        }
        if sim.can_place_entity(pump, x, y, Direction::North).is_err()
            || sim
                .can_place_entity(boiler, x, y + 1, Direction::North)
                .is_err()
            || sim
                .can_place_entity(steam_engine, x + 2, y + 1, Direction::North)
                .is_err()
        {
            continue;
        }

        sim.place_entity(pump, x, y, Direction::North)
            .expect("validated benchmark pump should place");
        let boiler_id = sim
            .place_entity(boiler, x, y + 1, Direction::North)
            .expect("validated benchmark boiler should place");
        sim.place_entity(steam_engine, x + 2, y + 1, Direction::North)
            .expect("validated benchmark engine should place");
        *sim.player_inventory_mut() = Inventory::player();
        sim.player_inventory_mut().slots[0] = Some(ItemStack {
            item_id: coal,
            count: 50,
        });
        sim.transfer_player_slot_to_boiler_fuel(boiler_id, 0)
            .expect("benchmark boiler should accept fuel");
        placed += 1;
    }

    panic!("could only place {placed} of {count} fluid benchmark fixtures");
}

fn deterministic_tile_coords(sim: &Simulation) -> Vec<(i32, i32)> {
    let mut chunks = sim.world().chunks.keys().copied().collect::<Vec<_>>();
    chunks.sort_unstable();
    chunks
        .into_iter()
        .flat_map(|coord| {
            (0..CHUNK_SIZE * CHUNK_SIZE).map(move |index| {
                let local_x = index.rem_euclid(CHUNK_SIZE);
                let local_y = index.div_euclid(CHUNK_SIZE);
                (
                    coord.x * CHUNK_SIZE + local_x,
                    coord.y * CHUNK_SIZE + local_y,
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

fn collect_benchmark_stats(sim: &mut Simulation, ticks: usize) -> BenchmarkStats {
    assert!(ticks > 0);
    let mut samples = Vec::with_capacity(ticks);

    for _ in 0..ticks {
        reset_allocation_counters();
        let profile = sim.profiled_tick();
        let allocations = allocation_sample();
        samples.push(TickSample {
            profile,
            allocations,
        });
    }

    benchmark_stats(samples, sim.counts())
}

fn reset_allocation_counters() {
    ALLOCATION_COUNT.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
}

fn allocation_sample() -> AllocationSample {
    AllocationSample {
        count: ALLOCATION_COUNT.load(Ordering::Relaxed),
        bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
    }
}

fn benchmark_stats(mut samples: Vec<TickSample>, counts: SimulationCounts) -> BenchmarkStats {
    samples.sort_by_key(|sample| sample.profile.total);
    let average = average_profile(&samples);
    let p95_index = ((samples.len() * 95).div_ceil(100)).saturating_sub(1);
    let p95 = percentile_profile(&samples, p95_index);
    let max = max_profile(&samples);

    let mut allocation_bytes = samples
        .iter()
        .map(|sample| sample.allocations.bytes)
        .collect::<Vec<_>>();
    let mut allocation_counts = samples
        .iter()
        .map(|sample| sample.allocations.count)
        .collect::<Vec<_>>();
    allocation_bytes.sort_unstable();
    allocation_counts.sort_unstable();
    let total_bytes = allocation_bytes.iter().sum::<u64>();
    let total_counts = allocation_counts.iter().sum::<u64>();

    BenchmarkStats {
        average,
        p95,
        max,
        counts,
        alloc_average_bytes: total_bytes / allocation_bytes.len() as u64,
        alloc_p95_bytes: allocation_bytes[p95_index],
        alloc_max_bytes: *allocation_bytes
            .last()
            .expect("allocation bytes should exist"),
        alloc_average_count: total_counts / allocation_counts.len() as u64,
        alloc_p95_count: allocation_counts[p95_index],
        alloc_max_count: *allocation_counts
            .last()
            .expect("allocation counts should exist"),
    }
}

fn average_profile(samples: &[TickSample]) -> SimulationTickProfile {
    let len = samples.len() as u128;
    SimulationTickProfile {
        total: average_duration(samples, len, |profile| profile.total),
        entity_motion: average_duration(samples, len, |profile| profile.entity_motion),
        belts: average_duration(samples, len, |profile| profile.belts),
        fluids: average_duration(samples, len, |profile| profile.fluids),
        power_rebuild: average_duration(samples, len, |profile| profile.power_rebuild),
        machines: average_duration(samples, len, |profile| profile.machines),
        inserters: average_duration(samples, len, |profile| profile.inserters),
        inventory_transfers: average_duration(samples, len, |profile| profile.inventory_transfers),
        chunk_lookup: average_duration(samples, len, |profile| profile.chunk_lookup),
        manual_crafting: average_duration(samples, len, |profile| profile.manual_crafting),
        validation: average_duration(samples, len, |profile| profile.validation),
    }
}

fn percentile_profile(samples: &[TickSample], index: usize) -> SimulationTickProfile {
    SimulationTickProfile {
        total: percentile_duration(samples, index, |profile| profile.total),
        entity_motion: percentile_duration(samples, index, |profile| profile.entity_motion),
        belts: percentile_duration(samples, index, |profile| profile.belts),
        fluids: percentile_duration(samples, index, |profile| profile.fluids),
        power_rebuild: percentile_duration(samples, index, |profile| profile.power_rebuild),
        machines: percentile_duration(samples, index, |profile| profile.machines),
        inserters: percentile_duration(samples, index, |profile| profile.inserters),
        inventory_transfers: percentile_duration(samples, index, |profile| {
            profile.inventory_transfers
        }),
        chunk_lookup: percentile_duration(samples, index, |profile| profile.chunk_lookup),
        manual_crafting: percentile_duration(samples, index, |profile| profile.manual_crafting),
        validation: percentile_duration(samples, index, |profile| profile.validation),
    }
}

fn max_profile(samples: &[TickSample]) -> SimulationTickProfile {
    SimulationTickProfile {
        total: max_duration(samples, |profile| profile.total),
        entity_motion: max_duration(samples, |profile| profile.entity_motion),
        belts: max_duration(samples, |profile| profile.belts),
        fluids: max_duration(samples, |profile| profile.fluids),
        power_rebuild: max_duration(samples, |profile| profile.power_rebuild),
        machines: max_duration(samples, |profile| profile.machines),
        inserters: max_duration(samples, |profile| profile.inserters),
        inventory_transfers: max_duration(samples, |profile| profile.inventory_transfers),
        chunk_lookup: max_duration(samples, |profile| profile.chunk_lookup),
        manual_crafting: max_duration(samples, |profile| profile.manual_crafting),
        validation: max_duration(samples, |profile| profile.validation),
    }
}

fn average_duration(
    samples: &[TickSample],
    len: u128,
    duration: impl Fn(SimulationTickProfile) -> Duration,
) -> Duration {
    let nanos = samples
        .iter()
        .map(|sample| duration(sample.profile).as_nanos())
        .sum::<u128>()
        / len;
    Duration::from_nanos(nanos as u64)
}

fn percentile_duration(
    samples: &[TickSample],
    index: usize,
    duration: impl Fn(SimulationTickProfile) -> Duration,
) -> Duration {
    let mut durations = samples
        .iter()
        .map(|sample| duration(sample.profile))
        .collect::<Vec<_>>();
    durations.sort_unstable();
    durations[index]
}

fn max_duration(
    samples: &[TickSample],
    duration: impl Fn(SimulationTickProfile) -> Duration,
) -> Duration {
    samples
        .iter()
        .map(|sample| duration(sample.profile))
        .max()
        .expect("samples should not be empty")
}

fn print_benchmark_stats(name: &str, stats: BenchmarkStats) {
    println!(
        "{name}:\n  counts: entities {}, belts {}, belt_items {}, machines {}, inserters {}, active_machines {}\n  total: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  belts: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  inserters: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  machines: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  fluids: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  power_rebuild: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  allocations: avg {} bytes/{} allocs, p95 {} bytes/{} allocs, max {} bytes/{} allocs",
        stats.counts.entity_count,
        stats.counts.belt_count,
        stats.counts.belt_item_count,
        stats.counts.machine_count,
        stats.counts.inserter_count,
        stats.counts.active_machines,
        ms(stats.average.total),
        ms(stats.p95.total),
        ms(stats.max.total),
        ms(stats.average.belts),
        ms(stats.p95.belts),
        ms(stats.max.belts),
        ms(stats.average.inserters),
        ms(stats.p95.inserters),
        ms(stats.max.inserters),
        ms(stats.average.machines),
        ms(stats.p95.machines),
        ms(stats.max.machines),
        ms(stats.average.fluids),
        ms(stats.p95.fluids),
        ms(stats.max.fluids),
        ms(stats.average.power_rebuild),
        ms(stats.p95.power_rebuild),
        ms(stats.max.power_rebuild),
        stats.alloc_average_bytes,
        stats.alloc_average_count,
        stats.alloc_p95_bytes,
        stats.alloc_p95_count,
        stats.alloc_max_bytes,
        stats.alloc_max_count,
    );
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
