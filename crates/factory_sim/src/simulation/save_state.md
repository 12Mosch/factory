# Simulation save-state ownership

`save.rs::define_snapshot!` is the ordered durable-state registry. It generates
both the borrowed encoder and the owned background-save capture, including their
capture expressions. Add fields there once. Explicit capture hooks omit scratch
and spatial-index copies; their exhaustive struct construction forces new fields
to be classified when added. `EntityStore` has its own shared
per-kind registry in `entities/store.rs` and `simulation/entity_states.rs`.

An owned handle also carries a `SaveSnapshotIdentity` consisting of the
application's world-generation number and the completed tick. That identity is
orchestration metadata, not portable durable state, so it is not encoded into
the save. Background capture preflights the borrowed registry against the save
limits before cloning it. The application retains at most one such full-copy
generation; a competing request is rejected while the first is encoding.

## Classification

| Owner | Durable gameplay state | Derived data or scratch |
| --- | --- | --- |
| Simulation/world | Tick, day/night phase, seed, catalog, chunks, prioritized generation queues, chart | World generator and terrain absorption rebuild from validated catalog/tiles without ticking. Terrain/resource presentation histories reset. |
| Invalidation | Entity topology, chunk-generation and walkability revisions; the revisions observed by navigation and target selection | Presentation-only revisions reset. Preserve a pending invalidation, even when a placement, removal or generation occurred since the last enemy pass. |
| Entities | All per-kind state, inventories, belt positions/IDs, splitter arbitration cursors, machine work and pending outputs, combinator outputs, radar scan cursors | Occupancy is validated against footprints; module effects and pollution emitter indexes rebuild before return. |
| Belt transport | The next nonzero item ID; active-run order and upstream wake boundaries, incremental graph/run layout, free slots and pending topology edits. Consuming or removing the highest item cannot rewind allocation. Exhausted or forged zero cursors are rejected on load. | Visit/traversal scratch and presentation revisions reset. The graph is an execution plan: rebuilding it would wake jammed lanes earlier and can change cyclic traversal or future patch order. |
| Enemy navigation | Raid field bounds, target/footprint, direction cells, ordered frontier, initialization flag, round-robin cursor and observed revisions; per-unit long-range detour target, axis, wall-follow heading/hand and hit distance | Independent path-search buffers, per-tick expansion allowance and counters reset at `begin_tick`. Fields and detour progress are **not** caches: rebuilding them would spend future ticks or undo progress around wide obstacles. |
| Enemy targeting | Cached base/raid decisions, including negative decisions, and observed topology revision | Spatial target index rebuilds synchronously without clearing decisions or acknowledging a pending invalidation. |
| Enemies/pollution | Unit paths and decision/attack deadlines, bases/missions, allocators, evolution, threat history, emission/absorption remainders | Enemy spatial lookup and spawning scratch, pollution diffusion scratch rebuild/reset. |
| Player/combat | Inventory, equipment buffers/cooldowns, weapon magazine, death/respawn request, corpses, delayed projectiles/status effects and their allocators | Failed respawn-search memoization only avoids repeating an already failed query; rebuilding does not consume a gameplay budget. |
| Crafting/research | Manual mining progress, crafting job IDs, queue order, completion cursor, consumed ingredients/progress, active/queued research and levels | Catalog-derived lookup data. |
| Construction/robots | Plans, reservations, repair/deconstruction progress, flying robots, payloads/deliveries, IDs, charging pads and ordered queues, plus per-network logistic demand/surplus/storage cursors | Coverage/network and logistic candidate indexes rebuild synchronously. Network summaries are validated against durable owners. |
| Trains | Stock/train allocators, positions, fuel/cargo, velocities, destinations/routes, exhausted-search positions and incremental frontiers (including their occupancy snapshots), planning cursor, block/stop reservations, schedule and wait clocks | Rail graph/blocks, stopped-stock index and idle route-search buffers rebuild without a simulation tick. The next tick resets the route-search budget. |
| Power/fluids/heat/circuits | Stored energy/fluid, power status and statistics, clean network summaries (plus whether fluid/heat summaries are pending invalidation), circuit configuration and combinator outputs | Connectivity, demand indexes, tick scratch, circuit signals and module effects rebuild from durable inputs without advancing production/combinators. |
| Statistics/onboarding | Rolling totals, history, launch/death counts and historical milestones | Map/presentation status caches and diagnostic overflow counters reset. |

A bounded work queue is durable whenever its progress changes when gameplay can
proceed. A cache is derived only if its cold and warm forms produce the same next
tick. Allocation capacity and temporary buffers are scratch, not save state.

## Load phases

1. Check header, version, codec/collection limits and catalog hash.
2. Validate catalog and chunk shape/tile references before creating the world
   generator or deriving terrain absorption.
3. Assemble durable state with empty runtime caches, then validate durable
   ownership, inventories, work progress, belt identity uniqueness/cursor and
   navigation bounds/cells/frontiers, transport slots, adjacency, run membership
   and active queue bounds. This phase needs no reconstructed topology.
4. Rebuild rail and stopped-stock indexes from validated inputs. Validate saved
   network summaries and train block references before replacing summaries.
5. Rebuild clean derived indexes synchronously; leave encoded pending fluid and
   heat invalidations pending so their next-tick work and empty published
   summaries are preserved. Validate the complete world before returning it. No
   phase advances a simulation tick or spends a future gameplay work allowance.

## Format boundary and regression coverage

Version **55** is an explicit compatibility boundary for issue #293. Versions
through 54 did not contain navigation progress, target decision history or the
belt allocator cursor, so deterministic migration cannot recover them. They
remain rejected by the version dispatcher. Keep an older save untouched and use
the revision that wrote it for recovery; do not silently default missing history.
Issue #241 remains responsible for the broader supported migration window.

The headless `save::assert_save_continuation` helper compares borrowed and detached
save encodings, command results and state hashes at **every** subsequent tick.
Commands at the same relative tick execute in slice order before the tick. It
reports the first divergence and validates both worlds. The requested #265/#266
harnesses are not yet present; this helper reuses `SimCommand`, `state_hash` and the
existing subsystem fixtures rather than creating a public replay protocol.
Permanent coverage includes warm and partially initialized multi-raid fields,
exhausted navigation budgets, consumed highest belt IDs, belt-jam wake timing, pending generation,
train routes/search exhaustion and serialized frontiers, robot reservations, delayed combat, and pending
crafting/research. Corrupt chunk/navigation/identity inputs are rejected in the
prerequisite or durable validation phase.
