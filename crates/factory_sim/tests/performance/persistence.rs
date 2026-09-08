//! Persistence budgets share deterministic populated factory builders with tick budgets.
use super::*;
use factory_sim::{
    MAX_SNAPSHOT_BYTES, SAVE_HEADER_SIZE, capture_save_snapshot, save_snapshot_to_bytes,
};
use std::io::Write;

#[test]
fn persistence_supported_worlds() {
    let _guard = BENCHMARK_LOCK.lock().unwrap();
    benchmark("small", 20, 200, 2, 4, 2 * 1024 * 1024);
    benchmark("medium", 100, 1_000, 10, 10, 8 * 1024 * 1024);
}

#[test]
#[ignore = "manual large-world persistence stress budget"]
fn persistence_large_world() {
    let _guard = BENCHMARK_LOCK.lock().unwrap();
    benchmark("large", 1_000, 10_000, 40, 24, 32 * 1024 * 1024);
}

fn measure<T>(operation: impl FnOnce() -> T) -> (T, Duration, u64) {
    let allocated = ALLOCATED_BYTES.load(Ordering::Relaxed);
    let started = Instant::now();
    let value = operation();
    (
        value,
        started.elapsed(),
        ALLOCATED_BYTES.load(Ordering::Relaxed) - allocated,
    )
}

fn benchmark(
    name: &'static str,
    machines: usize,
    belts: usize,
    fluids: usize,
    radius: i32,
    size_budget: u64,
) {
    let mut sim = build_factory_benchmark(FactoryBenchmarkSpec {
        name,
        machines,
        belts,
        inserters: machines,
        fluid_fixtures: fluids,
        warmup_ticks: 0,
        measurement_ticks: 0,
        assert_60_ups: false,
    });
    // Exercise accumulated statistics, belt identities, fluid and power caches.
    run_warmup_ticks(&mut sim, 3_600);
    // Exploration persists even when these chunks have no active machines.
    for y in -radius..=radius {
        for x in -radius..=radius {
            sim.ensure_chunk_generated(ChunkCoord { x, y });
        }
    }
    sim.tick();
    sim.validate_state().unwrap();
    let hash = sim.state_hash();
    let tick = sim.tick_count();
    let (snapshot, capture, capture_alloc) = measure(|| capture_save_snapshot(&sim));
    assert_eq!(snapshot.tick_count(), tick);
    // Move encoding and writing to the same kind of worker used by the app.
    let worker = std::thread::spawn(move || {
        let (bytes, encode, encode_alloc) = measure(|| save_snapshot_to_bytes(&snapshot).unwrap());
        drop(snapshot);
        let path = std::env::temp_dir().join(format!(
            "factory-persistence-{}-{name}.factsim",
            std::process::id()
        ));
        let (_, write, write_alloc) = measure(|| {
            let mut file = std::fs::File::create(&path).unwrap();
            file.write_all(&bytes).unwrap();
            file.sync_all().unwrap();
        });
        let (disk_bytes, read, _) = measure(|| std::fs::read(&path).unwrap());
        std::fs::remove_file(path).unwrap();
        assert_eq!(bytes, disk_bytes);
        (bytes, encode, encode_alloc, write, write_alloc, read)
    });
    let (bytes, encode, encode_alloc, write, write_alloc, read) = worker.join().unwrap();
    let (mut loaded, load, load_alloc) = measure(|| load_from_bytes(&bytes).unwrap());
    let (_, validate, validation_alloc) = measure(|| loaded.validate_state().unwrap());
    assert_eq!(loaded.state_hash(), hash);
    assert_eq!(loaded.tick_count(), tick);
    for expected in tick + 1..=tick + 10 {
        sim.tick();
        loaded.tick();
        assert_eq!(sim.tick_count(), expected);
        assert_eq!(loaded.tick_count(), expected);
        assert_eq!(loaded.state_hash(), sim.state_hash());
    }
    let payload = bytes.len() as u64 - SAVE_HEADER_SIZE as u64;
    assert!(size_budget <= MAX_SNAPSHOT_BYTES);
    eprintln!(
        "persistence {name}: chunks={} machines={machines} belts={belts} tick={tick} payload={payload} capture_ms={:.3} encode_ms={:.3} write_sync_ms={:.3} read_ms={:.3} load_validated_ms={:.3} validation_ms={:.3} allocated_bytes(capture/encode/write/load/validation)={capture_alloc}/{encode_alloc}/{write_alloc}/{load_alloc}/{validation_alloc}",
        sim.world().chunks.len(),
        ms(capture),
        ms(encode),
        ms(write),
        ms(read),
        ms(load),
        ms(validate)
    );
    assert!(
        payload <= size_budget,
        "{name}: payload {payload} exceeds {size_budget}"
    );
    // Broad wall-time guards tolerate shared CI; allocation/size budgets are stricter.
    assert!(
        capture < Duration::from_secs(2),
        "{name}: capture {capture:?}"
    );
    assert!(encode < Duration::from_secs(5), "{name}: encode {encode:?}");
    assert!(load < Duration::from_secs(10), "{name}: load {load:?}");
    assert!(
        validate < Duration::from_secs(5),
        "{name}: validation {validate:?}"
    );
    assert!(
        capture_alloc <= size_budget * 8,
        "{name}: capture allocation {capture_alloc}"
    );
    assert!(
        encode_alloc <= size_budget * 4,
        "{name}: encode allocation {encode_alloc}"
    );
    assert!(
        load_alloc <= size_budget * 16,
        "{name}: load allocation {load_alloc}"
    );
}
