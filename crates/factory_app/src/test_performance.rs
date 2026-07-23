use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

pub(crate) static BENCHMARK_LOCK: Mutex<()> = Mutex::new(());

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

#[derive(Clone, Copy, Debug)]
pub(crate) struct AllocationSample {
    pub(crate) count: u64,
    pub(crate) bytes: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PerformanceSample {
    elapsed: Duration,
    allocations: AllocationSample,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PerformanceStats {
    pub(crate) average: Duration,
    pub(crate) p95: Duration,
    pub(crate) p99: Duration,
    pub(crate) max: Duration,
    pub(crate) alloc_average_bytes: u64,
    pub(crate) alloc_p95_bytes: u64,
    pub(crate) alloc_p99_bytes: u64,
    pub(crate) alloc_max_bytes: u64,
    pub(crate) alloc_average_count: u64,
    pub(crate) alloc_p95_count: u64,
    pub(crate) alloc_p99_count: u64,
    pub(crate) alloc_max_count: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PerformanceBudget {
    pub(crate) p99: Duration,
    pub(crate) hitch: Duration,
    pub(crate) alloc_p99_bytes: u64,
    pub(crate) alloc_hitch_bytes: u64,
    pub(crate) alloc_p99_count: u64,
    pub(crate) alloc_hitch_count: u64,
}

pub(crate) fn collect_performance_stats(
    samples: usize,
    mut run_sample: impl FnMut(),
) -> PerformanceStats {
    collect_prepared_performance_stats(samples, || measure_performance_sample(&mut run_sample))
}

pub(crate) fn collect_prepared_performance_stats(
    samples: usize,
    mut prepare_and_measure: impl FnMut() -> PerformanceSample,
) -> PerformanceStats {
    assert!(samples >= 100, "p99 timings require at least 100 samples");
    let mut collected = Vec::with_capacity(samples);

    for _ in 0..samples {
        collected.push(prepare_and_measure());
    }

    performance_stats(&collected)
}

pub(crate) fn measure_performance_sample(mut run_sample: impl FnMut()) -> PerformanceSample {
    reset_allocation_counters();
    let started = Instant::now();
    run_sample();
    let elapsed = started.elapsed();
    PerformanceSample {
        elapsed,
        allocations: allocation_sample(),
    }
}

pub(crate) fn assert_performance_budget(
    name: &str,
    stats: PerformanceStats,
    budget: PerformanceBudget,
) {
    assert!(
        stats.p99 <= budget.p99,
        "{name} p99 {:.3} ms exceeded {:.3} ms",
        ms(stats.p99),
        ms(budget.p99)
    );
    assert!(
        stats.max <= budget.hitch,
        "{name} max hitch {:.3} ms exceeded {:.3} ms",
        ms(stats.max),
        ms(budget.hitch)
    );
    assert!(
        stats.alloc_p99_bytes <= budget.alloc_p99_bytes,
        "{name} allocation p99 {} bytes exceeded {} bytes",
        stats.alloc_p99_bytes,
        budget.alloc_p99_bytes
    );
    assert!(
        stats.alloc_max_bytes <= budget.alloc_hitch_bytes,
        "{name} allocation hitch {} bytes exceeded {} bytes",
        stats.alloc_max_bytes,
        budget.alloc_hitch_bytes
    );
    assert!(
        stats.alloc_p99_count <= budget.alloc_p99_count,
        "{name} allocation-count p99 {} exceeded {}",
        stats.alloc_p99_count,
        budget.alloc_p99_count
    );
    assert!(
        stats.alloc_max_count <= budget.alloc_hitch_count,
        "{name} allocation-count hitch {} exceeded {}",
        stats.alloc_max_count,
        budget.alloc_hitch_count
    );
}

pub(crate) fn print_performance_stats(name: &str, stats: PerformanceStats) {
    println!(
        "{name}:\n  timing: avg {:.3} ms, p95 {:.3} ms, p99 {:.3} ms, max {:.3} ms\n  allocations: avg {} bytes/{} allocs, p95 {} bytes/{} allocs, p99 {} bytes/{} allocs, max {} bytes/{} allocs",
        ms(stats.average),
        ms(stats.p95),
        ms(stats.p99),
        ms(stats.max),
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

pub(crate) fn reset_allocation_counters() {
    ALLOCATION_COUNT.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
}

pub(crate) fn allocation_sample() -> AllocationSample {
    AllocationSample {
        count: ALLOCATION_COUNT.load(Ordering::Relaxed),
        bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
    }
}

fn performance_stats(samples: &[PerformanceSample]) -> PerformanceStats {
    assert!(!samples.is_empty());
    let mut durations = samples
        .iter()
        .map(|sample| sample.elapsed)
        .collect::<Vec<_>>();
    let mut allocation_bytes = samples
        .iter()
        .map(|sample| sample.allocations.bytes)
        .collect::<Vec<_>>();
    let mut allocation_counts = samples
        .iter()
        .map(|sample| sample.allocations.count)
        .collect::<Vec<_>>();
    durations.sort_unstable();
    allocation_bytes.sort_unstable();
    allocation_counts.sort_unstable();

    let p95_index = percentile_index(samples.len(), 95);
    let p99_index = percentile_index(samples.len(), 99);
    PerformanceStats {
        average: Duration::from_nanos(
            (durations.iter().map(Duration::as_nanos).sum::<u128>() / durations.len() as u128)
                as u64,
        ),
        p95: durations[p95_index],
        p99: durations[p99_index],
        max: *durations.last().expect("duration samples should exist"),
        alloc_average_bytes: allocation_bytes.iter().sum::<u64>() / samples.len() as u64,
        alloc_p95_bytes: allocation_bytes[p95_index],
        alloc_p99_bytes: allocation_bytes[p99_index],
        alloc_max_bytes: *allocation_bytes
            .last()
            .expect("allocation-byte samples should exist"),
        alloc_average_count: allocation_counts.iter().sum::<u64>() / samples.len() as u64,
        alloc_p95_count: allocation_counts[p95_index],
        alloc_p99_count: allocation_counts[p99_index],
        alloc_max_count: *allocation_counts
            .last()
            .expect("allocation-count samples should exist"),
    }
}

fn percentile_index(len: usize, percentile: usize) -> usize {
    ((len * percentile).div_ceil(100)).saturating_sub(1)
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
