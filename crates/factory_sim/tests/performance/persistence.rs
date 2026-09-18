//! Persistence budgets share deterministic populated factory builders with tick budgets.
use super::*;
use factory_sim::{
    MAX_SNAPSHOT_BYTES, SAVE_HEADER_SIZE, save_snapshot_to_writer, try_capture_save_snapshot,
};
use std::io::{BufWriter, Write};

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
    let (snapshot, capture, capture_alloc, capture_peak, capture_retained) =
        measure(|| try_capture_save_snapshot(&sim, 1).unwrap());
    assert_eq!(snapshot.tick_count(), tick);
    assert_eq!(snapshot.identity().world_generation, 1);
    // Move encoding and writing to the same kind of worker used by the app.
    let worker = std::thread::spawn(move || {
        let path = std::env::temp_dir().join(format!(
            "factory-persistence-{}-{name}.factsim",
            std::process::id()
        ));
        let (_, stream_write, stream_alloc, stream_peak, stream_retained) = measure(|| {
            let file = std::fs::File::create(&path).unwrap();
            let mut file = BufWriter::new(file);
            save_snapshot_to_writer(&snapshot, &mut file).unwrap();
            file.flush().unwrap();
            file.get_ref().sync_all().unwrap();
        });
        // The encoded payload was never retained alongside the captured world.
        drop(snapshot);
        let (disk_bytes, read, _, read_peak, _) = measure(|| std::fs::read(&path).unwrap());
        std::fs::remove_file(path).unwrap();
        (
            disk_bytes,
            stream_write,
            stream_alloc,
            stream_peak,
            stream_retained,
            read,
            read_peak,
        )
    });
    let (bytes, stream_write, stream_alloc, stream_peak, stream_retained, read, read_peak) =
        worker.join().unwrap();
    let (mut loaded, load, load_alloc, load_peak, _) = measure(|| load_from_bytes(&bytes).unwrap());
    let (_, validate, validation_alloc, validation_peak, _) =
        measure(|| loaded.validate_state().unwrap());
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
    let save_peak_retained = capture_peak.max(capture_retained.saturating_add(stream_peak));
    assert!(size_budget <= MAX_SNAPSHOT_BYTES);
    eprintln!(
        "persistence {name}: chunks={} machines={machines} belts={belts} tick={tick} payload={payload} capture_ms={:.3} stream_encode_write_sync_ms={:.3} test_read_ms={:.3} load_validated_ms={:.3} validation_ms={:.3} allocated_bytes(capture/stream/load/validation)={capture_alloc}/{stream_alloc}/{load_alloc}/{validation_alloc} peak_extra_bytes(save/capture/stream/test_read/load/validation)={save_peak_retained}/{capture_peak}/{stream_peak}/{read_peak}/{load_peak}/{validation_peak} retained_bytes(snapshot_capture/stream)={capture_retained}/{stream_retained}",
        sim.world().chunks.len(),
        ms(capture),
        ms(stream_write),
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
    assert!(
        stream_write < Duration::from_secs(5),
        "{name}: stream encode/write {stream_write:?}"
    );
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
        stream_alloc <= size_budget * 4,
        "{name}: stream encode/write allocation {stream_alloc}"
    );
    assert_eq!(
        stream_retained, 0,
        "{name}: streaming must not retain an encoded payload"
    );
    assert!(
        load_alloc <= size_budget * 16,
        "{name}: load allocation {load_alloc}"
    );
    assert!(
        save_peak_retained <= size_budget * 12,
        "{name}: save peak retained memory {save_peak_retained}"
    );
    assert!(
        load_peak <= size_budget * 16,
        "{name}: load peak retained memory {load_peak}"
    );
}
