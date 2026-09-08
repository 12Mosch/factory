# Large-world persistence budgets

Issue #264 establishes a measured envelope for the current monolithic owned
snapshot. It does not change the save format or promise unlimited world size.

## Reproduce

```powershell
cargo test -p factory_sim --test performance_budgets persistence_supported_worlds -- --nocapture
cargo test -p factory_sim --test performance_budgets persistence_large_world -- --ignored --nocapture
cargo test -p factory_app --test save_load_ui oversized_world_preserves_previous_quicksave -- --ignored --nocapture
```

Run timing measurements without other builds or benchmarks competing for CPU.
Use the normal optimized development/test profile (opt-level 1, dependencies 3),
which also validates every simulation tick. Release results are not directly
comparable. Small and medium run on every `cargo test`; large and oversized cases
are manual. The exact-ceiling encoder test runs on every test suite.

## Fixtures and scope

The persistence tests reuse the factory builder in `performance_budgets.rs`,
seed 123, including stocked assemblers, belts with durable item identities,
inserters, power poles, fueled offshore-pump/boiler/steam-engine fixtures,
generated resources and enemies. Each runs for 3,600 ticks, generates additional
exploration chunks, then completes one more tick before capture (tick 3,602).
This is a deterministic capacity proxy for a long-lived factory, not a replay of
hours of player actions. Exploration and populated subsystem state dominate
save growth; warmup also exercises production/statistics, fluid amounts,
pollution and combat. It is not a worst-case inventory, railway, or robot-flight
fixture: those workloads need their own measurements before expanding support.

| Fixture | Initial assemblers / belts / inserters | Fluid fixtures | Minimum generated square | Payload regression cap |
| --- | --- | --- | --- | --- |
| Small | 20 / 200 / 20 | 2 | 9 x 9 chunks | 2 MiB |
| Medium | 100 / 1,000 / 100 | 10 | 21 x 21 chunks | 8 MiB |
| Large (manual) | 1,000 / 10,000 / 1,000 | 40 | 49 x 49 chunks | 32 MiB |

Natural combat can remove entities during warmup. Generated terrain and surviving
factory contents are saved, rather than resetting dynamic state for a synthetic
round trip. The benchmark asserts original and loaded `state_hash()` equality,
validates both, and checks ten subsequent ticks in lockstep.

## Measurements and budgets

Each fixture prints wall time for owned snapshot capture, worker encoding,
file write plus `sync_all`, file read, complete load, and an independent
`validate_state()` pass. Load includes deserialization, prototype hashing,
derived-cache reconstruction and the loader's mandatory validation. Independent
validation is a second pass, not a subtraction-based estimate of decode time.
The write probe uses a temporary raw simulation file; app container metadata,
atomic replacement/rollback and directory synchronization are covered by the
app tests, not attributed to this raw write number.

The allocator reports cumulative requested bytes per phase, including realloc
requests. These are temporary allocation-volume budgets, **not** retained heap,
process RSS, or an exact simultaneous memory peak. They exclude fixture setup,
the already-live simulation, OS filesystem cache and thread stacks. Tests sharing
the allocator take the existing benchmark mutex; minor harness allocations can
still add noise. No portable RSS claim is made.

Payload and allocation budgets provide repeatable regression guards. Capture
may allocate at most 8 times the fixture payload cap, encoding 4 times, and load
16 times. Broad wall-time guards are capture <2 s, encode <5 s, validated load
<10 s and validation <5 s. These catch gross regressions on shared CI; they are
not frame-latency targets. Disk time is reported without a hard CI threshold
because antivirus, filesystem and storage scheduling dominate its variance.

Reference machine: Windows, AMD Ryzen 9 9950X3D, Rust 1.98.1, normal test profile,
2026-09-08. One reference run (milliseconds; not portable latency guarantees):

| Fixture | Payload bytes | Capture | Encode | Write + sync | Read | Validated load | Validation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Small | 1,105,640 | 0.676 | 2.084 | 5.046 | 4.003 | 3.200 | 0.324 |
| Medium | 3,864,926 | 2.451 | 8.609 | 3.686 | 5.526 | 12.948 | 1.783 |
| Large | 20,964,898 | 18.489 | 51.691 | 14.496 | 7.942 | 97.776 | 22.055 |

| Fixture | Capture allocation bytes | Encode allocation bytes | Load allocation bytes | Validation allocation bytes |
| --- | ---: | ---: | ---: | ---: |
| Small | 2,710,085 | 3,145,704 | 2,970,585 | 77,945 |
| Medium | 10,749,101 | 12,582,888 | 11,987,257 | 402,881 |
| Large | 58,207,627 | 50,331,624 | 72,603,567 | 3,986,281 |

The stress payload uses about 31% of the format ceiling. Its 18.5 ms capture is
longer than a 60 UPS tick interval, so deferred-tick catch-up remains necessary;
it is not evidence that capture always fits inside one frame. The full manual
fixture run took about 76 seconds, mostly simulation warmup with tick validation.

## The 64 MiB ceiling

`MAX_SNAPSHOT_BYTES` is 67,108,864 bytes of encoded durable state, excluding the
24-byte simulation header and app metadata/container prefix. It is a development
resource/format envelope and a bound on accepted encoded input. It is not an
upper bound on decoded heap usage: collection overhead and rebuilt caches can
exceed the payload considerably.

The same limit now applies to both borrowed and owned save encoders. Bincode's
bounded serialization preflights the payload before writing it, so a valid world
above the ceiling returns `SaveLoadError::Codec(SizeLimit)`. Previously it could
be written successfully but rejected on load. An exact-limit payload is accepted;
one extra byte fails. Background jobs serialize before constructing or writing
the save container, so an oversize failure cannot replace the previous save or
create temporary/backup artifacts. The error is surfaced through the existing
save status; simulation continues. The ignored app test checks this with a valid
128 x 128 generated world and verifies the previous file byte-for-byte and by
loading/validating it.

Treat 48 MiB (75% of the ceiling) as an engineering investigation threshold when
reviewing reported serialized sizes, not an automatic runtime warning. A world
between that threshold and the ceiling remains saveable. Above the ceiling,
retrying the unchanged world still fails; the previous save remains available,
and a developer must extend the measured supported envelope before promising
that the larger world can be saved. Do not silently raise or remove the limit:
update both encoder/decoder policy, memory/load budgets and these stress tests.

## Decision

Keep the monolithic format for the measured supported sizes. No measurements here
justify chunked or incremental persistence yet. Worker memory is reduced by
releasing the owned snapshot before allocating the app container and releasing
the raw payload before writing it. Capture still holds a read lock while copying;
request submission only waits for that lock to pin a completed tick. Fixed steps
are deferred through the existing pending-tick accounting, then consumed after
the lock is released. Existing `save_load_ui` tests exercise this behavior and
compare continuation hashes, including under capture contention.

The large fixture exposed combat invalidating fluid/heat summaries after their
normal tick phases. Completed ticks now rebuild those summaries when topology is
dirty, without advancing either subsystem again. This keeps a post-combat save
valid and preserves the one-step-per-tick rule; the destruction regression test
includes both network types and a save/load hash check.

If observed payloads approach 48 MiB, first measure compression/deduplication on
these same fixtures and record CPU, decoded memory and compatibility costs.
Compression helps disk size but does not eliminate the owned world copy. If
capture latency or simultaneous live-world/snapshot memory becomes unacceptable,
measure that bottleneck directly before selecting structural sharing or chunked
persistence. The broad CI guards must not be interpreted as evidence that a
multi-second capture is an acceptable gameplay experience.
