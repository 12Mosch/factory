//! Manual measurements for persistent generated chunks and player corpses.
use super::*;
use factory_sim::{SAVE_HEADER_SIZE, load_from_bytes, save_to_bytes};

#[derive(Clone, Copy)]
struct ScalingSample {
    count: usize,
    build: Duration,
    retained_heap: u64,
    payload_bytes: u64,
    encode: Duration,
    load: Duration,
    load_peak: u64,
}

#[test]
#[ignore = "manual generated-world retained-memory and persistence scaling measurement"]
fn generated_world_chunk_scaling() {
    let _guard = BENCHMARK_LOCK.lock().unwrap();
    let samples = [5, 16, 32, 64]
        .into_iter()
        .map(measure_generated_world)
        .collect::<Vec<_>>();
    print_scaling_samples("generated_chunks", &samples);
    assert_linear_tail(&samples, "generated chunks");
}

fn measure_generated_world(side: i32) -> ScalingSample {
    let ((sim, count), build, _, _, retained_heap) = measure(|| {
        let mut sim = Simulation::new_seeded(123);
        let min = -(side / 2);
        for y in min..min + side {
            for x in min..min + side {
                sim.ensure_chunk_generated(ChunkCoord { x, y });
            }
        }
        let count = sim.world().chunks.len();
        (sim, count)
    });
    assert_eq!(count, (side * side) as usize);
    measure_round_trip(count, sim, build, retained_heap)
}

#[test]
#[ignore = "manual unrecovered-corpse retained-memory and persistence scaling measurement"]
fn player_corpse_scaling() {
    let _guard = BENCHMARK_LOCK.lock().unwrap();
    let samples = [0, 1_000, 10_000, 50_000]
        .into_iter()
        .map(measure_corpses)
        .collect::<Vec<_>>();
    print_scaling_samples("unrecovered_corpses", &samples);
    assert_linear_tail(&samples, "unrecovered corpses");

    let mut recovered = Simulation::new_player_corpse_fixture(1);
    recovered
        .recover_corpse(1)
        .expect("representative corpse should fit in the empty player inventory");
    assert_eq!(recovered.corpses().count(), 0);
    recovered
        .validate_state()
        .expect("complete recovery should remove all corpse durable state");
    let loaded = load_from_bytes(&save_to_bytes(&recovered).unwrap()).unwrap();
    assert_eq!(loaded.corpses().count(), 0);
    assert_eq!(loaded.state_hash(), recovered.state_hash());
}

fn measure_corpses(count: usize) -> ScalingSample {
    let (sim, build, _, _, retained_heap) =
        measure(|| Simulation::new_player_corpse_fixture(count));
    assert_eq!(sim.corpses().count(), count);
    measure_round_trip(count, sim, build, retained_heap)
}

fn measure_round_trip(
    count: usize,
    sim: Simulation,
    build: Duration,
    retained_heap: u64,
) -> ScalingSample {
    sim.validate_state().expect("fixture should be valid");
    let expected_hash = sim.state_hash();
    let (bytes, encode, _, _, _) = measure(|| save_to_bytes(&sim).unwrap());
    let payload_bytes = bytes.len() as u64 - SAVE_HEADER_SIZE as u64;
    let (loaded, load, _, load_peak, _) = measure(|| load_from_bytes(&bytes).unwrap());
    assert_eq!(loaded.state_hash(), expected_hash);
    loaded
        .validate_state()
        .expect("loaded fixture should remain valid");
    ScalingSample {
        count,
        build,
        retained_heap,
        payload_bytes,
        encode,
        load,
        load_peak,
    }
}

fn print_scaling_samples(name: &str, samples: &[ScalingSample]) {
    for sample in samples {
        eprintln!(
            "{name}: count={} retained_heap_bytes={} payload_bytes={} build_ms={:.3} encode_ms={:.3} load_validated_ms={:.3} load_peak_extra_bytes={}",
            sample.count,
            sample.retained_heap,
            sample.payload_bytes,
            ms(sample.build),
            ms(sample.encode),
            ms(sample.load),
            sample.load_peak,
        );
    }
}

fn assert_linear_tail(samples: &[ScalingSample], fixture: &str) {
    let [.., previous, last] = samples else {
        panic!("scaling measurement needs at least two samples");
    };
    let count_growth = (last.count - previous.count) as u64;
    let heap_growth = last.retained_heap.saturating_sub(previous.retained_heap);
    let payload_growth = last.payload_bytes.saturating_sub(previous.payload_bytes);
    assert!(count_growth > 0);
    assert!(heap_growth > 0, "{fixture} should retain additional heap");
    assert!(payload_growth > 0, "{fixture} should grow the save payload");
    eprintln!(
        "{fixture} tail slope: retained_heap_bytes_per_unit={:.2} payload_bytes_per_unit={:.2}",
        heap_growth as f64 / count_growth as f64,
        payload_growth as f64 / count_growth as f64,
    );
}
