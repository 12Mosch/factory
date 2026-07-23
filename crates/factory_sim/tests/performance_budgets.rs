use factory_data::{entity_prototype_id_by_name, item_id_by_name, recipe_id_by_name};
use factory_sim::{
    CHUNK_SIZE, ChunkCoord, Direction, EnemyDifficultyPreset, EnemyMode, EntityId, Inventory,
    Simulation, SimulationCounts, SimulationTickProfile, load_from_bytes, save_to_bytes,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const SIXTY_UPS_TICK_BUDGET: Duration = Duration::from_nanos(16_667_000);
const SMOKE_ALLOC_P95_BYTES_BUDGET: u64 = 64 * 1024;
const SMOKE_ALLOC_P95_COUNT_BUDGET: u64 = 256;
const SMALL_ALLOC_P95_BYTES_BUDGET: u64 = 1024 * 1024;
const SMALL_ALLOC_P95_COUNT_BUDGET: u64 = 2_000;
const ENEMY_HEAVY_PHASE_P95_BUDGET: Duration = Duration::from_millis(8);
const ENEMY_HEAVY_PHASE_P99_BUDGET: Duration = Duration::from_millis(10);
const ENEMY_HEAVY_PHASE_HITCH_BUDGET: Duration = Duration::from_nanos(16_667_000);
const ENEMY_HEAVY_ALLOC_P95_BYTES_BUDGET: u64 = 1024 * 1024;
const ENEMY_HEAVY_ALLOC_P95_COUNT_BUDGET: u64 = 2_000;
const ENEMY_HEAVY_ALLOC_P99_BYTES_BUDGET: u64 = 1024 * 1024;
const ENEMY_HEAVY_ALLOC_P99_COUNT_BUDGET: u64 = 2_000;
const ENEMY_HEAVY_ALLOC_HITCH_BYTES_BUDGET: u64 = 4 * 1024 * 1024;
const ENEMY_HEAVY_ALLOC_HITCH_COUNT_BUDGET: u64 = 8_000;

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
fn scripted_red_science_tick_allocation_smoke_budget() {
    let _guard = BENCHMARK_LOCK
        .lock()
        .expect("benchmark lock should not poison");
    let mut sim = Simulation::new_scripted_red_science_factory();
    run_warmup_ticks(&mut sim, 30);

    let stats = collect_benchmark_stats(&mut sim, 60);
    print_benchmark_stats("scripted_red_science_smoke", stats);

    sim.validate_state()
        .expect("smoke budget run should leave a valid simulation state");
    assert!(
        stats.alloc_p95_bytes <= SMOKE_ALLOC_P95_BYTES_BUDGET,
        "allocation p95 {} bytes exceeded smoke budget {} bytes",
        stats.alloc_p95_bytes,
        SMOKE_ALLOC_P95_BYTES_BUDGET
    );
    assert!(
        stats.alloc_p95_count <= SMOKE_ALLOC_P95_COUNT_BUDGET,
        "allocation p95 {} allocs exceeded smoke budget {} allocs",
        stats.alloc_p95_count,
        SMOKE_ALLOC_P95_COUNT_BUDGET
    );
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
fn fluid_tick_allocation_benchmark_72_fixtures() {
    run_factory_benchmark(FactoryBenchmarkSpec {
        name: "fluid_tick_72_fixtures",
        machines: 0,
        belts: 0,
        inserters: 0,
        fluid_fixtures: 72,
        warmup_ticks: 120,
        measurement_ticks: 600,
        assert_60_ups: false,
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

/// Diagnostic companion to the large stress benchmark: reports the worst
/// belt-phase ticks with their tick indices and allocation activity so
/// worst-case spikes can be attributed instead of averaged away.
#[test]
#[ignore]
fn large_headless_belt_spike_diagnostics() {
    let _guard = BENCHMARK_LOCK
        .lock()
        .expect("benchmark lock should not poison");
    let spec = FactoryBenchmarkSpec {
        name: "large_headless_spike_diagnostics",
        machines: 5_000,
        belts: 50_000,
        inserters: 5_000,
        fluid_fixtures: 500,
        warmup_ticks: 60,
        measurement_ticks: 300,
        assert_60_ups: false,
    };
    let mut sim = build_factory_benchmark(spec);
    run_warmup_ticks(&mut sim, spec.warmup_ticks);

    let mut samples = Vec::with_capacity(spec.measurement_ticks);
    for tick_index in 0..spec.measurement_ticks {
        reset_allocation_counters();
        let profile = sim.profiled_tick();
        let allocations = allocation_sample();
        samples.push((tick_index, profile, allocations));
    }

    let mut worst = samples.clone();
    worst.sort_by_key(|(_, profile, _)| std::cmp::Reverse(profile.belts));
    println!("worst belt-phase ticks:");
    for (tick_index, profile, allocations) in worst.iter().take(8) {
        println!(
            "  tick {tick_index}: belts {:.3} ms, total {:.3} ms, allocations {} bytes / {} allocs",
            ms(profile.belts),
            ms(profile.total),
            allocations.bytes,
            allocations.count
        );
    }

    worst.sort_by_key(|(_, profile, _)| std::cmp::Reverse(profile.total));
    println!("worst total ticks:");
    for (tick_index, profile, allocations) in worst.iter().take(8) {
        println!(
            "  tick {tick_index}: total {:.3} ms, belts {:.3} ms, machines {:.3} ms, fluids {:.3} ms, power {:.3} ms, enemies {:.3} ms, allocations {} bytes / {} allocs",
            ms(profile.total),
            ms(profile.belts),
            ms(profile.machines),
            ms(profile.fluids),
            ms(profile.power),
            ms(profile.enemies),
            allocations.bytes,
            allocations.count
        );
    }
}

/// Isolates the topology-refresh cost after one placement and removal in a
/// 50k-belt world. The edited ticks should exercise the scoped graph patch,
/// while the baseline captures ordinary belt advancement.
#[test]
#[ignore]
fn large_transport_topology_patch_diagnostics() {
    let _guard = BENCHMARK_LOCK
        .lock()
        .expect("benchmark lock should not poison");
    let spec = FactoryBenchmarkSpec {
        name: "large_transport_topology_patch_diagnostics",
        machines: 0,
        belts: 50_000,
        inserters: 0,
        fluid_fixtures: 0,
        warmup_ticks: 10,
        measurement_ticks: 0,
        assert_60_ups: false,
    };
    let mut sim = build_factory_benchmark(spec);
    run_warmup_ticks(&mut sim, spec.warmup_ticks);

    let baseline = (0..16)
        .map(|_| sim.profiled_tick().belts)
        .max()
        .unwrap_or_default();
    let belt = entity_prototype_id_by_name(sim.catalog(), "transport_belt");
    let request = deterministic_tile_coords(&sim)
        .into_iter()
        .map(|(x, y)| benchmark_placement_request(belt, x, y, Direction::East))
        .find(|request| can_place(&sim, *request))
        .expect("benchmark world should retain one free belt tile");
    let placed = place_validated(&mut sim, request, "validated diagnostic belt should place");

    reset_allocation_counters();
    let placed_tick = sim.profiled_tick();
    let placed_allocations = allocation_sample();
    factory_sim::entity_mutation::remove(&mut sim, placed)
        .expect("diagnostic belt should be removable");
    reset_allocation_counters();
    let removed_tick = sim.profiled_tick();
    let removed_allocations = allocation_sample();

    println!(
        "large transport topology patch:\n  baseline belt max {:.3} ms\n  placement tick belts {:.3} ms, total {:.3} ms, {} bytes / {} allocs\n  removal tick belts {:.3} ms, total {:.3} ms, {} bytes / {} allocs",
        ms(baseline),
        ms(placed_tick.belts),
        ms(placed_tick.total),
        placed_allocations.bytes,
        placed_allocations.count,
        ms(removed_tick.belts),
        ms(removed_tick.total),
        removed_allocations.bytes,
        removed_allocations.count,
    );
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

/// Manual regression budget for combat-dominated worlds. Unlike the factory
/// fixtures, this creates enough independent colonies to keep hundreds of
/// attackers active at once and a broad structure field for target lookup.
#[test]
#[ignore]
fn enemy_heavy_benchmark_500_attackers_2000_structures() {
    let _guard = BENCHMARK_LOCK
        .lock()
        .expect("benchmark lock should not poison");
    let mut config = EnemyDifficultyPreset::Aggressive.config();
    config.preset = EnemyDifficultyPreset::Custom;
    config.world.base_density_percent = 0;
    let mut catalog =
        factory_data::PrototypeCatalog::load_base().expect("base catalog should load");
    let chest = entity_prototype_id_by_name(&catalog, "chest");
    catalog.entities[chest.index()].max_health = Some(u32::MAX);
    let mut sim = Simulation::new_with_config(123, catalog, config);
    for y in -12..=12 {
        for x in -12..=12 {
            sim.ensure_chunk_generated(ChunkCoord { x, y });
        }
    }

    let spawners = place_spaced_entities(&mut sim, "biter_spawner", 64, Direction::North, 256);
    place_spaced_entities(&mut sim, "chest", 2_000, Direction::North, 4);
    for spawner_id in spawners {
        let placed = sim
            .entities()
            .placed_entity(spawner_id)
            .expect("benchmark spawner should remain placed");
        let chunk = ChunkCoord::from_tile(placed.x, placed.y)
            .expect("placed spawner should have a valid chunk");
        sim.add_pollution_micro(chunk, 2_000_000_000);
    }

    run_warmup_ticks(&mut sim, 7_200);
    let structure_count = sim
        .entities()
        .placed_entities()
        .filter(|placed| placed.prototype_id == chest)
        .count();
    assert_eq!(
        structure_count, 2_000,
        "enemy-heavy fixture must retain all benchmark structures through warmup"
    );
    let attacker_count = sim
        .enemies()
        .iter()
        .filter(|enemy| enemy.mode == EnemyMode::Attack)
        .count();
    assert!(
        attacker_count >= 500,
        "enemy-heavy fixture should retain at least 500 attackers, found {attacker_count}"
    );
    let retained_attacker_ids = sim
        .enemies()
        .iter()
        .filter(|enemy| enemy.mode == EnemyMode::Attack)
        .take(500)
        .map(|enemy| enemy.id)
        .collect::<HashSet<_>>();
    assert_eq!(
        retained_attacker_ids.len(),
        500,
        "enemy-heavy fixture should select 500 retained attacker identities"
    );

    let mut min_retained_structures = usize::MAX;
    let mut min_retained_fixture_attackers = usize::MAX;
    let stats = collect_benchmark_stats_with_observer(&mut sim, 300, |sample| {
        min_retained_structures = min_retained_structures.min(
            sample
                .entities()
                .placed_entities()
                .filter(|placed| placed.prototype_id == chest)
                .count(),
        );
        min_retained_fixture_attackers = min_retained_fixture_attackers.min(
            sample
                .enemies()
                .iter()
                .filter(|enemy| {
                    enemy.mode == EnemyMode::Attack && retained_attacker_ids.contains(&enemy.id)
                })
                .count(),
        );
    });
    print_benchmark_stats("enemy_heavy_500_attackers", stats);
    sim.validate_state()
        .expect("enemy-heavy budget run should leave a valid state");
    assert_eq!(
        min_retained_structures, 2_000,
        "enemy-heavy fixture must retain all structures throughout measurement"
    );
    assert_eq!(
        min_retained_fixture_attackers, 500,
        "enemy-heavy fixture must retain the same 500 attacking units throughout measurement"
    );
    assert!(
        stats.p95.enemies <= ENEMY_HEAVY_PHASE_P95_BUDGET,
        "enemy phase p95 {:.3} ms exceeded {:.3} ms",
        ms(stats.p95.enemies),
        ms(ENEMY_HEAVY_PHASE_P95_BUDGET)
    );
    assert!(
        stats.p99.enemies <= ENEMY_HEAVY_PHASE_P99_BUDGET,
        "enemy phase p99 {:.3} ms exceeded {:.3} ms",
        ms(stats.p99.enemies),
        ms(ENEMY_HEAVY_PHASE_P99_BUDGET)
    );
    assert!(
        stats.max.enemies <= ENEMY_HEAVY_PHASE_HITCH_BUDGET,
        "enemy phase max hitch {:.3} ms exceeded {:.3} ms",
        ms(stats.max.enemies),
        ms(ENEMY_HEAVY_PHASE_HITCH_BUDGET)
    );
    assert!(
        stats.alloc_p95_bytes <= ENEMY_HEAVY_ALLOC_P95_BYTES_BUDGET,
        "allocation p95 {} bytes exceeded {} bytes",
        stats.alloc_p95_bytes,
        ENEMY_HEAVY_ALLOC_P95_BYTES_BUDGET
    );
    assert!(
        stats.alloc_p95_count <= ENEMY_HEAVY_ALLOC_P95_COUNT_BUDGET,
        "allocation p95 {} allocs exceeded {} allocs",
        stats.alloc_p95_count,
        ENEMY_HEAVY_ALLOC_P95_COUNT_BUDGET
    );
    assert!(
        stats.alloc_p99_bytes <= ENEMY_HEAVY_ALLOC_P99_BYTES_BUDGET,
        "allocation p99 {} bytes exceeded {} bytes",
        stats.alloc_p99_bytes,
        ENEMY_HEAVY_ALLOC_P99_BYTES_BUDGET
    );
    assert!(
        stats.alloc_p99_count <= ENEMY_HEAVY_ALLOC_P99_COUNT_BUDGET,
        "allocation p99 {} allocs exceeded {} allocs",
        stats.alloc_p99_count,
        ENEMY_HEAVY_ALLOC_P99_COUNT_BUDGET
    );
    assert!(
        stats.alloc_max_bytes <= ENEMY_HEAVY_ALLOC_HITCH_BYTES_BUDGET,
        "allocation hitch {} bytes exceeded {} bytes",
        stats.alloc_max_bytes,
        ENEMY_HEAVY_ALLOC_HITCH_BYTES_BUDGET
    );
    assert!(
        stats.alloc_max_count <= ENEMY_HEAVY_ALLOC_HITCH_COUNT_BUDGET,
        "allocation-count hitch {} exceeded {}",
        stats.alloc_max_count,
        ENEMY_HEAVY_ALLOC_HITCH_COUNT_BUDGET
    );
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
    p99: SimulationTickProfile,
    max: SimulationTickProfile,
    counts: SimulationCounts,
    alloc_average_bytes: u64,
    alloc_p95_bytes: u64,
    alloc_p99_bytes: u64,
    alloc_max_bytes: u64,
    alloc_average_count: u64,
    alloc_p95_count: u64,
    alloc_p99_count: u64,
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
        .saturating_add(spec.fluid_fixtures.saturating_mul(256));
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
        let request = benchmark_placement_request(prototype_id, x, y, direction);
        let Some(entity_id) =
            place_if_valid(sim, request, "validated benchmark placement should succeed")
        else {
            continue;
        };
        placed.push(entity_id);
    }

    panic!(
        "could only place {} of {count} {prototype_name}",
        placed.len()
    );
}

fn place_spaced_entities(
    sim: &mut Simulation,
    prototype_name: &str,
    count: usize,
    direction: Direction,
    stride: usize,
) -> Vec<EntityId> {
    let prototype_id = entity_prototype_id_by_name(sim.catalog(), prototype_name);
    let mut placed = Vec::with_capacity(count);

    for (x, y) in deterministic_tile_coords(sim).into_iter().step_by(stride) {
        if placed.len() == count {
            return placed;
        }
        let request = benchmark_placement_request(prototype_id, x, y, direction);
        let Some(entity_id) = place_if_valid(
            sim,
            request,
            "validated spaced benchmark placement should succeed",
        ) else {
            continue;
        };
        placed.push(entity_id);
    }

    panic!(
        "could only place {} of {count} spaced {prototype_name}",
        placed.len()
    );
}

fn benchmark_placement_request(
    prototype_id: factory_data::EntityPrototypeId,
    x: i64,
    y: i64,
    direction: Direction,
) -> factory_sim::placement::EntityPlacementRequest {
    factory_sim::placement::EntityPlacementRequest {
        prototype_id,
        x,
        y,
        direction,
    }
}

fn can_place(sim: &Simulation, request: factory_sim::placement::EntityPlacementRequest) -> bool {
    factory_sim::placement::validate(sim, request).is_ok()
}

fn place_validated(
    sim: &mut Simulation,
    request: factory_sim::placement::EntityPlacementRequest,
    message: &str,
) -> EntityId {
    factory_sim::placement::place(sim, request).expect(message)
}

fn place_if_valid(
    sim: &mut Simulation,
    request: factory_sim::placement::EntityPlacementRequest,
    message: &str,
) -> Option<EntityId> {
    can_place(sim, request).then(|| place_validated(sim, request, message))
}

fn seed_assemblers(sim: &mut Simulation, machine_ids: &[EntityId]) {
    let catalog = sim.catalog().clone();
    let recipe = recipe_id_by_name(sim.catalog(), "iron_gear_wheel");
    let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");

    for machine_id in machine_ids {
        sim.select_assembler_recipe(*machine_id, recipe)
            .expect("benchmark assembler recipe should be selectable");
        *sim.player_inventory_mut() = Inventory::player();
        sim.player_inventory_mut()
            .insert(&catalog, iron_plate, 100)
            .expect("benchmark player inventory should accept iron plates");
        factory_sim::entity_transfer::player_slot_to_assembler_input(sim, *machine_id, 0)
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
        let request = benchmark_placement_request(pole, x, y, Direction::North);
        if place_if_valid(
            sim,
            request,
            "validated benchmark pole placement should succeed",
        )
        .is_none()
        {
            continue;
        }
        placed += 1;
    }
}

fn place_fluid_fixtures(sim: &mut Simulation, count: usize) {
    const MAX_CHUNK_RADIUS: i32 = 64;

    let catalog = sim.catalog().clone();
    let pump = entity_prototype_id_by_name(sim.catalog(), "offshore_pump");
    let boiler = entity_prototype_id_by_name(sim.catalog(), "boiler");
    let steam_engine = entity_prototype_id_by_name(sim.catalog(), "steam_engine");
    let coal = item_id_by_name(sim.catalog(), "coal");
    let mut placed = 0;
    let mut candidate_tiles = deterministic_tile_coords(sim);

    loop {
        for (x, y) in candidate_tiles {
            if placed == count {
                return;
            }
            let pump_request = benchmark_placement_request(pump, x, y, Direction::North);
            let boiler_request = benchmark_placement_request(boiler, x, y + 1, Direction::North);
            let steam_engine_request =
                benchmark_placement_request(steam_engine, x + 2, y + 1, Direction::North);
            if [pump_request, boiler_request, steam_engine_request]
                .into_iter()
                .any(|request| !can_place(sim, request))
            {
                continue;
            }

            place_validated(sim, pump_request, "validated benchmark pump should place");
            let boiler_id = place_validated(
                sim,
                boiler_request,
                "validated benchmark boiler should place",
            );
            place_validated(
                sim,
                steam_engine_request,
                "validated benchmark engine should place",
            );
            *sim.player_inventory_mut() = Inventory::player();
            sim.player_inventory_mut()
                .insert(&catalog, coal, 50)
                .expect("benchmark player inventory should accept coal");
            factory_sim::entity_transfer::player_slot_to_boiler_fuel(sim, boiler_id, 0)
                .expect("benchmark boiler should accept fuel");
            placed += 1;
        }
        if placed == count {
            return;
        }

        // Fixtures need shoreline, which is far sparser than the open ground
        // the initial world estimate is sized for. Grow the map one chunk ring
        // at a time and keep placing on the newly generated tiles.
        let radius = generated_chunk_radius(sim) + 1;
        assert!(
            radius <= MAX_CHUNK_RADIUS,
            "could only place {placed} of {count} fluid benchmark fixtures within chunk radius {MAX_CHUNK_RADIUS}"
        );
        candidate_tiles = generate_chunk_ring(sim, radius);
    }
}

fn generated_chunk_radius(sim: &Simulation) -> i32 {
    sim.world()
        .chunks
        .keys()
        .map(|coord| coord.x.abs().max(coord.y.abs()))
        .max()
        .unwrap_or(0)
}

fn generate_chunk_ring(sim: &mut Simulation, radius: i32) -> Vec<(i64, i64)> {
    let mut ring = Vec::new();
    for y in -radius..=radius {
        for x in -radius..=radius {
            if x.abs().max(y.abs()) < radius {
                continue;
            }
            let coord = ChunkCoord { x, y };
            if !sim.world().chunks.contains_key(&coord) {
                sim.ensure_chunk_generated(coord);
                ring.push(coord);
            }
        }
    }
    ring.sort_unstable();
    ring.into_iter().flat_map(chunk_tiles).collect()
}

fn deterministic_tile_coords(sim: &Simulation) -> Vec<(i64, i64)> {
    let mut chunks = sim.world().chunks.keys().copied().collect::<Vec<_>>();
    chunks.sort_unstable();
    chunks.into_iter().flat_map(chunk_tiles).collect()
}

fn chunk_tiles(coord: ChunkCoord) -> impl Iterator<Item = (i64, i64)> {
    (0..CHUNK_SIZE * CHUNK_SIZE).map(move |index| {
        let local_x = index.rem_euclid(CHUNK_SIZE);
        let local_y = index.div_euclid(CHUNK_SIZE);
        coord.tile_at(local_x, local_y)
    })
}

fn run_warmup_ticks(sim: &mut Simulation, ticks: usize) {
    for _ in 0..ticks {
        sim.tick();
    }
}

fn collect_benchmark_stats(sim: &mut Simulation, ticks: usize) -> BenchmarkStats {
    collect_benchmark_stats_with_observer(sim, ticks, |_| {})
}

fn collect_benchmark_stats_with_observer(
    sim: &mut Simulation,
    ticks: usize,
    mut observe: impl FnMut(&Simulation),
) -> BenchmarkStats {
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
        observe(sim);
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
    let p99_index = ((samples.len() * 99).div_ceil(100)).saturating_sub(1);
    let p95 = percentile_profile(&samples, p95_index);
    let p99 = percentile_profile(&samples, p99_index);
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
        p99,
        max,
        counts,
        alloc_average_bytes: total_bytes / allocation_bytes.len() as u64,
        alloc_p95_bytes: allocation_bytes[p95_index],
        alloc_p99_bytes: allocation_bytes[p99_index],
        alloc_max_bytes: *allocation_bytes
            .last()
            .expect("allocation bytes should exist"),
        alloc_average_count: total_counts / allocation_counts.len() as u64,
        alloc_p95_count: allocation_counts[p95_index],
        alloc_p99_count: allocation_counts[p99_index],
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
        power: average_duration(samples, len, |profile| profile.power),
        machines: average_duration(samples, len, |profile| profile.machines),
        inserters: average_duration(samples, len, |profile| profile.inserters),
        inventory_transfers: average_duration(samples, len, |profile| profile.inventory_transfers),
        chunk_lookup: average_duration(samples, len, |profile| profile.chunk_lookup),
        manual_crafting: average_duration(samples, len, |profile| profile.manual_crafting),
        pollution: average_duration(samples, len, |profile| profile.pollution),
        enemies: average_duration(samples, len, |profile| profile.enemies),
        validation: average_duration(samples, len, |profile| profile.validation),
    }
}

fn percentile_profile(samples: &[TickSample], index: usize) -> SimulationTickProfile {
    SimulationTickProfile {
        total: percentile_duration(samples, index, |profile| profile.total),
        entity_motion: percentile_duration(samples, index, |profile| profile.entity_motion),
        belts: percentile_duration(samples, index, |profile| profile.belts),
        fluids: percentile_duration(samples, index, |profile| profile.fluids),
        power: percentile_duration(samples, index, |profile| profile.power),
        machines: percentile_duration(samples, index, |profile| profile.machines),
        inserters: percentile_duration(samples, index, |profile| profile.inserters),
        inventory_transfers: percentile_duration(samples, index, |profile| {
            profile.inventory_transfers
        }),
        chunk_lookup: percentile_duration(samples, index, |profile| profile.chunk_lookup),
        manual_crafting: percentile_duration(samples, index, |profile| profile.manual_crafting),
        pollution: percentile_duration(samples, index, |profile| profile.pollution),
        enemies: percentile_duration(samples, index, |profile| profile.enemies),
        validation: percentile_duration(samples, index, |profile| profile.validation),
    }
}

fn max_profile(samples: &[TickSample]) -> SimulationTickProfile {
    SimulationTickProfile {
        total: max_duration(samples, |profile| profile.total),
        entity_motion: max_duration(samples, |profile| profile.entity_motion),
        belts: max_duration(samples, |profile| profile.belts),
        fluids: max_duration(samples, |profile| profile.fluids),
        power: max_duration(samples, |profile| profile.power),
        machines: max_duration(samples, |profile| profile.machines),
        inserters: max_duration(samples, |profile| profile.inserters),
        inventory_transfers: max_duration(samples, |profile| profile.inventory_transfers),
        chunk_lookup: max_duration(samples, |profile| profile.chunk_lookup),
        manual_crafting: max_duration(samples, |profile| profile.manual_crafting),
        pollution: max_duration(samples, |profile| profile.pollution),
        enemies: max_duration(samples, |profile| profile.enemies),
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
        "{name}:\n  counts: entities {}, enemies {}, belts {}, belt_items {}, machines {}, inserters {}, active_machines {}\n  total: avg {:.3} ms, p95 {:.3} ms, p99 {:.3} ms, max {:.3} ms\n  belts: avg {:.3} ms, p95 {:.3} ms, p99 {:.3} ms, max {:.3} ms\n  inserters: avg {:.3} ms, p95 {:.3} ms, p99 {:.3} ms, max {:.3} ms\n  machines: avg {:.3} ms, p95 {:.3} ms, p99 {:.3} ms, max {:.3} ms\n  fluids: avg {:.3} ms, p95 {:.3} ms, p99 {:.3} ms, max {:.3} ms\n  power: avg {:.3} ms, p95 {:.3} ms, p99 {:.3} ms, max {:.3} ms\n  enemies: avg {:.3} ms, p95 {:.3} ms, p99 {:.3} ms, max {:.3} ms\n  allocations: avg {} bytes/{} allocs, p95 {} bytes/{} allocs, p99 {} bytes/{} allocs, max {} bytes/{} allocs",
        stats.counts.entity_count,
        stats.counts.enemy_count,
        stats.counts.belt_count,
        stats.counts.belt_item_count,
        stats.counts.machine_count,
        stats.counts.inserter_count,
        stats.counts.active_machines,
        ms(stats.average.total),
        ms(stats.p95.total),
        ms(stats.p99.total),
        ms(stats.max.total),
        ms(stats.average.belts),
        ms(stats.p95.belts),
        ms(stats.p99.belts),
        ms(stats.max.belts),
        ms(stats.average.inserters),
        ms(stats.p95.inserters),
        ms(stats.p99.inserters),
        ms(stats.max.inserters),
        ms(stats.average.machines),
        ms(stats.p95.machines),
        ms(stats.p99.machines),
        ms(stats.max.machines),
        ms(stats.average.fluids),
        ms(stats.p95.fluids),
        ms(stats.p99.fluids),
        ms(stats.max.fluids),
        ms(stats.average.power),
        ms(stats.p95.power),
        ms(stats.p99.power),
        ms(stats.max.power),
        ms(stats.average.enemies),
        ms(stats.p95.enemies),
        ms(stats.p99.enemies),
        ms(stats.max.enemies),
        stats.alloc_average_bytes,
        stats.alloc_average_count,
        stats.alloc_p95_bytes,
        stats.alloc_p95_count,
        stats.alloc_p99_bytes,
        stats.alloc_p99_count,
        stats.alloc_max_bytes,
        stats.alloc_max_count,
    );
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
