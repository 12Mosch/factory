# Persistent generated-world and corpse scaling

Issues #331 and #332 measure two intentional sources of persistent state. This
document defines the supported policy from those measurements. It does not add
chunk unloading, corpse expiry, or item deletion.

## Reproduce

```powershell
cargo test -p factory_sim --test performance_budgets generated_world_chunk_scaling -- --ignored --nocapture
cargo test -p factory_sim --test performance_budgets player_corpse_scaling -- --ignored --nocapture
```

Run each filtered test alone. They share the process-wide counting allocator
with the other performance tests, so unrelated concurrent tests would make the
live-heap results noisy. The fixtures use seed 123 and the normal optimized
development/test profile (workspace code at opt-level 1, dependencies at 3).

`retained_heap_bytes` is the allocator-visible heap still owned by the complete
`Simulation` after fixture construction. It includes the fixed simulation
baseline. It is not process RSS and excludes stacks and allocator/OS overhead.
The generated-world delta also includes durable state created by the normal
chunk initialization path, such as chart and enemy-generation metadata; it is
more representative than `size_of::<Chunk>()` alone. `payload_bytes` excludes
the 24-byte simulation save header. Load time includes decoding, derived-cache
reconstruction, and the loader's mandatory validation. Load peak is additional
allocator-visible live heap above the encoded input and original fixture.

Reference machine: Windows, AMD Ryzen 9 9950X3D, Rust 1.98.1, normal test
profile, 2026-09-18. Times are diagnostic, not portable budgets.

## Generated chunks

The fixture generates centered squares through `Simulation::ensure_chunk_generated`
and round-trips every size with an equal state hash.

| Generated chunks | Retained heap | Save payload | Build | Encode | Validated load | Load peak |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 25 | 1,457,983 B | 650,410 B | 8.001 ms | 0.928 ms | 2.588 ms | 1,532,788 B |
| 256 | 6,247,039 B | 2,239,776 B | 57.729 ms | 4.504 ms | 14.757 ms | 6,327,355 B |
| 1,024 | 22,195,383 B | 7,807,078 B | 229.915 ms | 16.897 ms | 57.389 ms | 22,299,019 B |
| 4,096 | 85,992,111 B | 30,990,556 B | 921.659 ms | 66.908 ms | 227.050 ms | 86,176,083 B |

From 1,024 to 4,096 chunks, the measured marginal cost is 20,767 retained
heap bytes and 7,547 encoded bytes per generated chunk. Growth is linear over
the measured range. The save payload, rather than load time, is the first
existing system limit: continuing that slope reaches the 48 MiB investigation
threshold near 6,600 chunks and the 64 MiB format ceiling near 8,800 chunks for
an otherwise similar world. Populated factories consume part of both margins,
so those projections are not supported capacities.

Decision: keep chunks resident. The practical supported exploration target is
up to 4,096 generated chunks, subject to the existing 48 MiB payload
investigation threshold and 64 MiB hard save ceiling described in
`persistence-budgets.md`. At the measured target, retained simulation heap is
about 82 MiB and the chunk-heavy payload is about 30 MiB, while save/load costs
remain modest. Larger worlds are not promised by this policy. Revisit layout
compaction first if real saves approach 48 MiB; create a separate streaming
design issue only if observed workloads need substantially more than 4,096
resident chunks or retained heap becomes the limiting resource. Any streaming
design must preserve deterministic durable terrain and save compatibility.

## Unrecovered player corpses

The corpse fixture contains one full stack of sixteen distinct base-game items
per corpse. It constructs valid durable state directly so setup time does not
measure thousands of combat/respawn cycles. Every size is validated and
round-tripped with all corpse identifiers and inventories intact.

| Unrecovered corpses | Retained heap | Save payload | Build | Encode | Validated load | Load peak |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 1,457,983 B | 650,410 B | 7.598 ms | 0.905 ms | 2.508 ms | 1,532,788 B |
| 1,000 | 1,909,991 B | 885,410 B | 7.515 ms | 1.039 ms | 3.150 ms | 1,984,812 B |
| 10,000 | 5,973,375 B | 3,000,410 B | 9.063 ms | 2.782 ms | 11.855 ms | 6,048,196 B |
| 50,000 | 24,037,231 B | 12,400,410 B | 16.569 ms | 11.924 ms | 52.073 ms | 24,112,052 B |

The 10,000-to-50,000 tail is exactly 451.6 retained heap bytes and 235 encoded
bytes per representative corpse. Even 50,000 unrecovered deaths add about
21.5 MiB of live heap and 11.2 MiB to the save over the zero-corpse baseline.
This is pathological gameplay rather than a plausible ordinary save, and the
cost remains well below the existing persistence envelope.

Decision: keep unlimited corpse persistence. A fixed expiry or count cap is not
justified, and no player-owned items may be silently deleted. Complete recovery
removes the corpse map entry; an always-on regression test checks that 128
corpses round-trip, one fully recovered corpse stays absent after another
save/load, and every other identifier and inventory remains intact. Reconsider
compaction or an item-preserving archive only if observed saves accumulate tens
of thousands of corpses or corpse state becomes a material share of the 48 MiB
investigation threshold. That would be a separate gameplay-facing issue.
