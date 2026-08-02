# Panic-based error handling audit

An inventory and classification of every panic site in non-test code:
`unwrap`, `expect`, `assert!`/`assert_eq!`/`assert_ne!`, `panic!`,
`unreachable!`, and checked integer conversions that panic when a capacity is
exhausted.

The goal is not to remove assertions. It is to make the failure policy
intentional: an assertion should mark something an earlier step already proved,
and anything an outside input can influence — content data, a player command,
a save file — should surface as a typed error instead of terminating the
process.

## Scope

Counted: all `.rs` files under `crates/`, excluding `tests/` directories and
anything reachable only through a `#[cfg(test)]` module declaration. Those are
category 6 by construction and are not enumerated site by site.

| Category | Meaning | Sites | Policy |
| --- | --- | --- | --- |
| 1 | Proven internal invariant | 158 | Retained. Assertion documents a guarantee an earlier step proved. |
| 2 | Prototype / content-data | 18 | Recoverable ones now return `PrototypeLoadError`; the rest are backed by load-time validation. |
| 3 | Player-command validation | 40 | Retained in commit phases only; the matching plan phase returns a typed error. |
| 4 | Capacity or identifier exhaustion | 14 | Retained. Every check runs before the mutation it guards. |
| 5 | Save-data or state corruption | 4 | Retained. Bounds are checked first; malformed files return `ContainerError`/`SaveLoadError`. |
| 6 | Fixture-only code | 29 | Retained. Fixture builders, not reachable from gameplay. |
| | **Total** | **263** | |


### By crate

| Crate | 1 | 2 | 3 | 4 | 5 | 6 | Total |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `factory_app` | 17 | 2 | 0 | 1 | 4 | 0 | 24 |
| `factory_data` | 2 | 10 | 0 | 1 | 0 | 0 | 13 |
| `factory_sim` | 139 | 6 | 40 | 12 | 0 | 29 | 226 |
| **Total** | 158 | 18 | 40 | 14 | 4 | 29 | **263** |


## Category policy

### 1. Proven internal invariant — retained

The largest group, and the reason the count is high without being alarming.
The simulation is written in a plan-then-commit style: a fallible planning step
resolves everything it needs and returns a typed error, and a subsequent commit
step re-resolves the same handles infallibly. The commit-side `expect` is not a
hope, it is a restatement of what planning established.

`entity_transfer/containers.rs` is representative — `plan_transfer` reads the
slot through `item_slot(index)` and returns `ContainerError` if it is absent or
holds nothing transferable, and only then does `commit_transfer` reach for
`item_slot_mut(index)`. Nothing between the two can resize the inventory.

These stay as assertions. They document a guarantee, and demoting them to
errors would mean inventing an unreachable branch for every caller to ignore.

### 2. Prototype or content-data error — partly converted

Assumptions about what the loaded RON catalog contains. Content data is an
input, so a well-formed file that simply omits something must be a rejected
load rather than a crash. Two real gaps were found and fixed; see
[Changes made](#changes-made).

The remaining sites in this category are backed by loader validation that
already rejects the bad shape — `EntityKind::MiningDrill` requires a
`mining_drill` section, `EntityKind::Pumpjack` a `pumpjack` section,
`EntityKind::Furnace` a `furnace` section — so the `expect` restates a
load-time guarantee. Their messages now say so.

`PrototypeCatalog::load_base().expect(...)` at the four startup sites is a
deliberate fail-fast: there is no game to run without the catalog, the error is
typed, and it fires before any world exists.

### 3. Player-command validation error — retained, validation verified

Every one of these sits in the commit half of a player-command handler whose
plan half returns a typed error (`ContainerError`, `InventoryError`,
`BuildError`, `TilePlacementError`, `RollingStockTransferError`, and the rest
of the taxonomy in `factory_sim::error`).

An out-of-range slot index, an item the destination refuses, a full target
inventory, and an empty source slot are all rejected by the planning step. No
audited path lets a malformed command reach the commit-side assertion.

### 4. Capacity or identifier exhaustion — retained, ordering verified

Dense-map and belt-cache indices are narrowed to `u32` with a checked
conversion before the matching `push`, so a would-be overflow aborts before any
container is mutated rather than leaving a half-updated index. `DenseMap::insert`
and `SparseSlotMap::insert` both compute the index first and push second.

The identity counters cannot realistically exhaust: `BeltItemId` is a `u64`, and
`initialize_item_tracking` deliberately wraps to `0` as an exhaustion sentinel
that `allocate_item_id` asserts on.

`validate_group`'s `u16::try_from(expected)` is unreachable rather than merely
unlikely: prototype ids are `u16` and the same function rejects duplicates
first, so at most 65,536 prototypes can survive to be enumerated.

### 5. Save-data or state-corruption error — retained, bounds verified

Save containers are parsed defensively. `container_payload_offset` checks
`bytes.len() < PREFIX_SIZE` before slicing, and `inspect_container` reads into a
fixed `[u8; PREFIX_SIZE]`, so the `try_into().expect("fixed range")` converts a
slice whose length is already known. Truncated files, bad magic, and oversized
metadata all return `ContainerError`. Format and version failures return
`SaveLoadError`.

The fourth site is a routing invariant rather than a parse: `jobs::system_path`
rejects `SaveKind::Named` with `unreachable!` because named saves get a path
built from their id. Both callers were checked — `save_system_save` rejects a
named target with a user-facing message before calling it, and `expected_path`
matches `SaveKind::Named` in its own arm — so a named save cannot reach it.

### 6. Test or fixture-only code — retained

`performance_tests` and `test_performance` are `#[cfg(test)]` modules.
`simulation/scripted.rs` builds benchmark and presentation worlds; it compiles
unconditionally but nothing in gameplay calls it, and a fixture that cannot
build its own world should fail loudly.

## Changes made

### Base prototype resolution returns a typed error

`base_ids.rs` resolved names the engine hard-codes — `iron_plate`, `water`,
`grass` — with `unwrap_or_else(|| panic!(...))`. The names come from Rust, but
whether they resolve is decided by the RON file that was loaded, which makes it
a content-data error.

Each lookup now has a `try_` form returning `MissingBasePrototype`, and
`BasePrototypeIds::try_from_catalog` resolves the whole required set.
`load_base` and `load_from_path` run that check and report
`PrototypeLoadError::MissingRequiredPrototype`, so an incomplete data file is a
rejected load naming the missing prototype.

The panicking wrappers are kept for the call sites that run per frame, and are
now honest: they assert a check that catalog loading already performed. The
required-name list exists once, in the `try_` form.

`from_ron_str` stays permissive so focused loader tests can keep loading
fragment catalogs.

### Labs must declare inventory slots

`lab_state_for_prototype` did `prototype.inventory_slot_count.expect(...)`, but
no loader rule required `EntityKind::Lab` to declare one — every other consumer
of that field handles `None`. A catalog whose lab omitted the field, or set it
to `0`, passed validation and then aborted the process when a lab was placed.

`validate_lab_metadata` now rejects both at load time with
`InvalidEntityMetadata`, mirroring the existing cargo-wagon rule. The
placement-time `expect` remains, as the invariant it now genuinely is.

## Regression coverage

New tests in `loader/tests/base_prototypes.rs`:

- the shipped catalog resolves every required base prototype
- a catalog missing one reports group and name rather than panicking
- `load_from_path` rejects such a catalog with `MissingRequiredPrototype`
- `from_ron_str` still loads partial catalogs

New tests in `loader/tests/entities.rs`:

- a lab without `inventory_slot_count` fails to load
- a lab with zero slots fails to load
- a lab declaring slots loads and keeps the count
- every lab in the shipped catalog declares slots

## Re-running the inventory

The counts above are a snapshot. To re-derive them, search for the constructs
listed at the top across `crates/`, excluding `tests/` directories and files
reachable only from a `#[cfg(test)] mod` declaration.

## Appendix: full site inventory

All 263 production panic sites, by file. Category numbers match the
table above. Sites in `#[cfg(test)]` modules and `tests/` directories are
category 6 by construction and are not listed individually.


### `crates/factory_app/src/placement/build.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 57 | `.expect` | 1 | `.expect("validated buildable has a building category"),` |
| 60 | `.expect` | 1 | `.expect("validated buildable has a building menu order"),` |

### `crates/factory_app/src/plugin/simulation.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 20 | `.expect` | 2 | `PrototypeCatalog::load_base().expect("base prototype catalog should load"),` |

### `crates/factory_app/src/plugin/ui.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 101 | `.expect` | 1 | `.expect("the default font asset ID must remain valid");` |

### `crates/factory_app/src/rendering/belts/items/cache.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 167 | `.expect` | 4 | `let index = u32::try_from(self.entries.len()).expect("belt render cache capacity exceeded");` |
| 191 | `.expect` | 1 | `.expect("cached belt item page should exist")[moved_offset] = entry_index;` |

### `crates/factory_app/src/rendering/entities.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 74 | `.expect` | 1 | `.expect("validated renderable entity should still be placed");` |

### `crates/factory_app/src/rendering/map_texture/cache/incremental.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 56 | `.expect` | 1 | `.expect("checked exact dirty chunk history"),` |
| 64 | `.expect` | 1 | `.expect("checked exact resource history")` |
| 75 | `.expect` | 1 | `.expect("checked exact terrain history")` |

### `crates/factory_app/src/rendering/map_texture/cache/repaint.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 82 | `.expect` | 1 | `.expect("bounds must be set before refresh_painted_chunks");` |

### `crates/factory_app/src/rendering/resource_cells.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 78 | `.expect` | 1 | `.expect("resource cache should be initialized before incremental sync");` |

### `crates/factory_app/src/rendering/world/cache_sync.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 143 | `.expect` | 1 | `.expect("cached chunk mesh handle should remain valid");` |

### `crates/factory_app/src/resources.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 29 | `.expect` | 1 | `self.inner.read().expect("simulation lock poisoned")` |
| 37 | `.expect` | 1 | `self.inner.write().expect("simulation lock poisoned")` |

### `crates/factory_app/src/save_load/container.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 85 | `.expect` | 5 | `let metadata_len = u32::from_le_bytes(bytes[12..16].try_into().expect("fixed range")) as usize;` |
| 111 | `.expect` | 5 | `let version = u32::from_le_bytes(prefix[8..12].try_into().expect("fixed range"));` |
| 112 | `.expect` | 5 | `let metadata_len = u32::from_le_bytes(prefix[12..16].try_into().expect("fixed range")) as usize;` |

### `crates/factory_app/src/save_load/jobs.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 167 | `unreachable!` | 5 | `SaveKind::Named => unreachable!("named saves use generated paths"),` |

### `crates/factory_app/src/ui/build_bar/view.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 12 | `assert!` | 1 | `const _: () = assert!(` |

### `crates/factory_app/src/ui/container_window.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 56 | `.expect` | 1 | `let (entity_id, kind) = open.expect("snapshot is only built while a container is open");` |

### `crates/factory_app/src/ui/rolling_stock_window.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 92 | `.expect` | 1 | `let stock_id = open.expect("a snapshot is only built while stock is open");` |

### `crates/factory_app/src/ui/technology_panel/view.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 136 | `.expect` | 1 | `.expect("technology list should contain valid ids");` |

### `crates/factory_app/src/world_setup.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 418 | `.expect` | 2 | `PrototypeCatalog::load_base().expect("base prototype catalog should load");` |

### `crates/factory_data/src/base_ids.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 62 | `panic!` | 2 | `Self::try_from_catalog(catalog).unwrap_or_else(\|missing\| panic!("{missing}"))` |
| 190 | `panic!` | 2 | `Self::try_from_catalog(catalog).unwrap_or_else(\|missing\| panic!("{missing}"))` |
| 228 | `panic!` | 2 | `Self::try_from_catalog(catalog).unwrap_or_else(\|missing\| panic!("{missing}"))` |
| 266 | `panic!` | 2 | `Self::try_from_catalog(catalog).unwrap_or_else(\|missing\| panic!("{missing}"))` |
| 290 | `panic!` | 2 | `try_item_id_by_name(catalog, name).unwrap_or_else(\|missing\| panic!("{missing}"))` |
| 313 | `panic!` | 2 | `try_fluid_id_by_name(catalog, name).unwrap_or_else(\|missing\| panic!("{missing}"))` |
| 336 | `panic!` | 2 | `try_tile_id_by_name(catalog, name).unwrap_or_else(\|missing\| panic!("{missing}"))` |
| 359 | `panic!` | 2 | `try_entity_prototype_id_by_name(catalog, name).unwrap_or_else(\|missing\| panic!("{missing}"))` |
| 382 | `panic!` | 2 | `try_recipe_id_by_name(catalog, name).unwrap_or_else(\|missing\| panic!("{missing}"))` |
| 405 | `panic!` | 2 | `try_technology_id_by_name(catalog, name).unwrap_or_else(\|missing\| panic!("{missing}"))` |

### `crates/factory_data/src/loader/entities.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 470 | `.expect` | 1 | `.expect("presence checked above");` |

### `crates/factory_data/src/loader/items.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 77 | `.expect` | 1 | `.expect("burnt results are collected from the items being loaded");` |

### `crates/factory_data/src/validation.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 44 | `.expect` | 4 | `let expected = u16::try_from(expected).expect("prototype group exceeds u16 id range");` |

### `crates/factory_sim/src/combat.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 88 | `.expect` | 1 | `u32::try_from(scaled).expect("mitigated u32 damage must fit in u32")` |

### `crates/factory_sim/src/entities/dense_map.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 59 | `.expect` | 4 | `u32::try_from(self.entries.len()).expect("dense entity state capacity exceeded");` |
| 90 | `.expect` | 1 | `.expect("stored entity indirection page should exist")[moved_page_offset] =` |
| 267 | `.expect` | 1 | `.expect("iterated entity indirection should exist");` |

### `crates/factory_sim/src/inventory.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 124 | `.expect` | 1 | `.expect("a validated stack always has a catalog prototype"),` |
| 149 | `.expect` | 1 | `.expect("a validated stack always has a catalog prototype");` |
| 195 | `assert!` | 1 | `assert!(` |
| 203 | `assert_eq!` | 1 | `assert_eq!(` |
| 218 | `.expect` | 3 | `.expect("a planned item slot remains occupied during commit");` |
| 219 | `assert_eq!` | 1 | `assert_eq!(` |
| 223 | `assert!` | 1 | `assert!(` |
| 376 | `.expect` | 1 | `.expect("a validated item stack always has a catalog prototype");` |

### `crates/factory_sim/src/simulation/belt_ops/cache/activity.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 227 | `.expect` | 4 | `u32::try_from(start_position).expect("transport run position capacity exceeded");` |

### `crates/factory_sim/src/simulation/belt_ops/cache/graph.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 105 | `.expect` | 1 | `.expect("iterated transport belt should exist")` |
| 121 | `.expect` | 1 | `.expect("iterated splitter should exist")` |
| 396 | `.expect` | 4 | `let _ = u32::try_from(slot).expect("transport lane slot capacity exceeded");` |
| 512 | `.expect` | 4 | `let run = u32::try_from(self.run_records.len()).expect("transport run capacity exceeded");` |
| 519 | `.expect` | 4 | `.expect("transport run position capacity exceeded");` |
| 535 | `.expect` | 4 | `start: u32::try_from(start).expect("transport run lane capacity exceeded"),` |
| 537 | `.expect` | 4 | `.expect("transport run length capacity exceeded"),` |
| 613 | `.expect` | 4 | `let slot = u32::try_from(self.lanes.len()).expect("transport lane slot capacity exceeded");` |

### `crates/factory_sim/src/simulation/belt_ops/cache/item_tracking.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 26 | `assert_ne!` | 4 | `assert_ne!(self.next_item_id, 0, "belt item identity space exhausted");` |
| 31 | `.expect` | 4 | `.expect("belt item identity space exhausted");` |

### `crates/factory_sim/src/simulation/belt_ops/types.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 36 | `.expect` | 4 | `Self(u32::try_from(slot).expect("transport lane slot capacity exceeded"))` |
| 89 | `.expect` | 4 | `Self(u32::try_from(index).expect("transport run capacity exceeded"))` |

### `crates/factory_sim/src/simulation/combat_ops.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 374 | `.expect` | 1 | `.expect("repair item was selected for its repair prototype");` |
| 377 | `.expect` | 3 | `.expect("repair pack was just found in the player inventory");` |

### `crates/factory_sim/src/simulation/commands.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 532 | `.expect` | 1 | `.expect("newly built ghost should be placed")` |
| 535 | `.expect` | 1 | `.expect("placed entity should have a build item");` |

### `crates/factory_sim/src/simulation/construction_ops.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 323 | `.expect` | 1 | `.expect("occupancy grid should only reference placed entities");` |
| 345 | `.expect` | 1 | `.expect("ghost occupancy should only reference existing ghosts");` |
| 364 | `.expect` | 1 | `.expect("a non-empty capture has a leftmost tile");` |
| 369 | `.expect` | 1 | `.expect("a non-empty capture has a topmost tile");` |

### `crates/factory_sim/src/simulation/core.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 17 | `assert!` | 1 | `assert!(config.is_valid(), "invalid enemy simulation configuration");` |
| 22 | `.expect` | 1 | `.expect("test world should contain a walkable player start");` |
| 28 | `.expect` | 3 | `.expect("player starting inventory should accept burner mining drill");` |
| 31 | `.expect` | 3 | `.expect("player starting inventory should accept stone furnace");` |
| 102 | `.expect` | 2 | `PrototypeCatalog::load_base().expect("base prototype catalog should load"),` |

### `crates/factory_sim/src/simulation/enemy/expansion.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 291 | `.expect` | 1 | `.expect("generated destination has a chunk");` |

### `crates/factory_sim/src/simulation/enemy/navigation/flow_field.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 63 | `.expect` | 1 | `.expect("flow-field goal must be within its bounds");` |
| 130 | `unreachable!` | 1 | `_ => unreachable!("flow-field cells contain known direction values"),` |

### `crates/factory_sim/src/simulation/enemy/navigation/mod.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 151 | `.expect` | 1 | `.expect("selected raid field must still exist");` |

### `crates/factory_sim/src/simulation/enemy/navigation/pathfinding.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 56 | `.expect` | 1 | `let start_index = index(start).expect("start is centered in path scratch bounds");` |
| 62 | `.expect` | 1 | `let tile_index = index(tile).expect("open path tile must be within scratch bounds");` |
| 74 | `.expect` | 1 | `.expect("path reconstruction tile must be within scratch bounds");` |
| 126 | `unreachable!` | 1 | `_ => unreachable!("A* path cells always have a return direction"),` |

### `crates/factory_sim/src/simulation/enemy/raids.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 98 | `.expect` | 1 | `.expect("launch base must exist");` |

### `crates/factory_sim/src/simulation/enemy/spawning.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 254 | `.expect` | 1 | `.expect("listed base must exist");` |
| 283 | `unreachable!` | 1 | `unreachable!("only staged enemies consume attack budget");` |
| 289 | `.expect` | 1 | `.expect("a successful staged spawn must retain its base");` |

### `crates/factory_sim/src/simulation/entity_recovery_ops.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 30 | `unreachable!` | 1 | `unreachable!("destroy recovery only inserts items")` |
| 44 | `.expect` | 1 | `.expect("validated placed entity should still be removable");` |
| 66 | `.expect` | 1 | `.expect("an entity's validated build item should form a valid stack"),` |

### `crates/factory_sim/src/simulation/entity_states.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 441 | `.expect` | 1 | `.expect("validated belt items should have valid stack prototypes")` |
| 461 | `.expect` | 1 | `.expect("validated splitter items should have valid stack prototypes")` |

### `crates/factory_sim/src/simulation/entity_store_ops.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 378 | `.expect` | 1 | `.expect("validated footprint reservation should succeed");` |
| 409 | `.expect` | 1 | `.expect("validated footprint reservation should succeed");` |

### `crates/factory_sim/src/simulation/entity_transfer/assemblers.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 35 | `.expect` | 3 | `.expect("a planned player source slot remains in bounds"),` |
| 110 | `.expect` | 3 | `.expect("a planned assembler source slot remains in bounds"),` |

### `crates/factory_sim/src/simulation/entity_transfer/containers.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 36 | `.expect` | 3 | `.expect("a planned player source slot remains in bounds"),` |
| 79 | `.expect` | 3 | `.expect("a planned entity source slot remains in bounds"),` |

### `crates/factory_sim/src/simulation/entity_transfer/furnaces.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 35 | `.expect` | 3 | `.expect("a planned player source slot remains in bounds"),` |
| 80 | `.expect` | 3 | `.expect("a planned furnace fuel transfer targets a burner furnace");` |
| 86 | `.expect` | 3 | `.expect("a planned player source slot remains in bounds"),` |
| 127 | `.expect` | 3 | `.expect("a planned furnace fuel transfer targets a burner furnace")` |

### `crates/factory_sim/src/simulation/entity_transfer/inserters.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 36 | `.expect` | 3 | `.expect("a planned inserter fuel transfer targets a burner inserter");` |
| 42 | `.expect` | 3 | `.expect("a planned player source slot remains in bounds"),` |
| 81 | `.expect` | 3 | `.expect("a planned inserter fuel transfer targets a burner inserter");` |

### `crates/factory_sim/src/simulation/entity_transfer/mining_drills.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 38 | `.expect` | 3 | `.expect("a planned drill fuel transfer targets a burner drill");` |
| 44 | `.expect` | 3 | `.expect("a planned player source slot remains in bounds"),` |
| 85 | `.expect` | 3 | `.expect("a planned drill fuel transfer targets a burner drill");` |

### `crates/factory_sim/src/simulation/entity_transfer/modules.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 55 | `.expect` | 1 | `let slot = slots.slot_mut(slot_index).expect("index is in bounds");` |
| 58 | `.expect` | 3 | `.expect("validated module should fit an empty slot");` |
| 65 | `.expect` | 3 | `.expect("planned player module quantity remains available");` |
| 97 | `.expect` | 3 | `.expect("checked player inventory capacity");` |
| 100 | `.expect` | 3 | `.expect("validated module slot remains in bounds")` |
| 102 | `.expect` | 1 | `.expect("validated occupied module slot remains removable");` |

### `crates/factory_sim/src/simulation/entity_transfer/power_entities.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 34 | `.expect` | 3 | `.expect("a planned player source slot remains in bounds"),` |
| 117 | `.expect` | 3 | `.expect("a planned player source slot remains in bounds"),` |

### `crates/factory_sim/src/simulation/entity_transfer/roboports.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 121 | `.expect` | 3 | `.expect("a planned player source slot remains in bounds"),` |
| 160 | `.expect` | 3 | `.expect("a planned roboport source slot remains in bounds"),` |

### `crates/factory_sim/src/simulation/entity_transfer/rolling_stock.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 77 | `.expect` | 1 | `.expect("the wagon inventory was just read");` |
| 83 | `.expect` | 3 | `.expect("a planned player source slot remains in bounds"),` |
| 122 | `.expect` | 1 | `.expect("the wagon inventory was just read")` |
| 124 | `.expect` | 3 | `.expect("a planned wagon source slot remains in bounds");` |
| 167 | `.expect` | 1 | `.expect("the locomotive fuel slot was just read");` |
| 173 | `.expect` | 3 | `.expect("a planned player source slot remains in bounds"),` |
| 204 | `.expect` | 1 | `.expect("the locomotive fuel slot was just read");` |

### `crates/factory_sim/src/simulation/equipment_ops.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 68 | `.expect` | 3 | `.expect("the equip plan resolved this player inventory slot")` |
| 70 | `.expect` | 3 | `.expect("selected stack contains one armor");` |
| 135 | `.expect` | 3 | `.expect("the equip plan resolved this player inventory slot")` |
| 137 | `.expect` | 3 | `.expect("selected stack contains one equipment item");` |
| 214 | `.expect` | 1 | `amount - u32::try_from(absorbed).expect("absorbed damage is bounded by u32 input")` |
| 379 | `.expect` | 1 | `.expect("installed equipment is validated against the catalog")` |

### `crates/factory_sim/src/simulation/fluid_ops/network_builder.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 43 | `.expect` | 1 | `.expect("component should contain at least one fluid box");` |

### `crates/factory_sim/src/simulation/generation/resources.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 195 | `.expect` | 1 | `u32::try_from(candidate.radius_sq.max(1)).expect("resource radius is bounded");` |
| 197 | `.expect` | 1 | `.expect("resource distance is bounded by radius");` |

### `crates/factory_sim/src/simulation/heat_ops/network_builder.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 44 | `.expect` | 1 | `.expect("component should contain at least one heat buffer");` |

### `crates/factory_sim/src/simulation/inventory_ops.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 12 | `unreachable!` | 1 | `InventoryError::InsufficientItems => unreachable!(concat!(` |
| 18 | `unreachable!` | 1 | `unreachable!("inventory operations only create validated stacks")` |
| 21 | `unreachable!` | 1 | `unreachable!("only setting a slot filter can contradict one")` |

### `crates/factory_sim/src/simulation/machine_ops/burner.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 14 | `.expect` | 1 | `.expect("the available fuel stack contains one item");` |

### `crates/factory_sim/src/simulation/machine_ops/inserters.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 338 | `.expect` | 1 | `.expect("a removed inserter source item should form a valid stack"),` |
| 352 | `.expect` | 1 | `.expect("a removed inserter source item should form a valid stack"),` |
| 366 | `.expect` | 1 | `.expect("a removed inserter source item should form a valid stack"),` |
| 380 | `.expect` | 1 | `.expect("a removed inserter source item should form a valid stack"),` |
| 394 | `.expect` | 1 | `.expect("a removed inserter source item should form a valid stack"),` |
| 408 | `.expect` | 1 | `.expect("a removed inserter source item should form a valid stack"),` |
| 423 | `.expect` | 1 | `.expect("a removed inserter source item should form a valid stack"),` |
| 460 | `.expect` | 1 | `.expect("inventory presence was checked above");` |
| 480 | `.expect` | 1 | `.expect("lab presence was checked above");` |
| 501 | `.expect` | 1 | `.expect("turret presence was checked above");` |
| 533 | `.expect` | 1 | `.expect("furnace presence was checked above");` |
| 538 | `.expect` | 1 | `.expect("an accepting furnace fuel slot exists")` |
| 540 | `.expect` | 1 | `.expect("the checked furnace fuel slot should accept the item");` |
| 548 | `.expect` | 1 | `.expect("the checked furnace input slot should accept the item");` |
| 568 | `.expect` | 1 | `.expect("boiler presence was checked above");` |
| 574 | `.expect` | 1 | `.expect("the checked boiler fuel slot should accept the item");` |
| 597 | `.expect` | 1 | `.expect("reactor presence was checked above")` |
| 601 | `.expect` | 1 | `.expect("the checked reactor fuel slot should accept the item");` |
| 614 | `.expect` | 1 | `.expect("roboport presence was checked above");` |
| 621 | `.expect` | 1 | `.expect("the checked roboport inventory should accept the item");` |
| 646 | `.expect` | 1 | `.expect("an accepting burner inserter fuel slot exists")` |
| 648 | `.expect` | 1 | `.expect("the checked burner inserter fuel slot should accept the item");` |
| 664 | `.expect` | 1 | `.expect("assembler presence was checked above");` |

### `crates/factory_sim/src/simulation/machine_ops/recipes.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 312 | `.expect` | 1 | `.expect("fluid ingredient availability was checked before completion");` |

### `crates/factory_sim/src/simulation/machine_ops/state.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 293 | `.expect` | 2 | `.expect("prototype validation requires labs to declare inventory slots"),` |

### `crates/factory_sim/src/simulation/machine_tick/assemblers.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 88 | `.expect` | 1 | `.expect("assembler checked ingredients before completion");` |
| 95 | `.expect` | 1 | `.expect("assembler checked output capacity before completion");` |
| 104 | `.expect` | 1 | `.expect("fluid recipe availability was checked before completion");` |

### `crates/factory_sim/src/simulation/machine_tick/furnaces.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 27 | `.expect` | 1 | `let furnace = prototype.furnace.as_ref().expect(` |
| 96 | `.expect` | 1 | `.expect("selected furnace input should still contain ingredient");` |
| 100 | `.expect` | 1 | `.expect("the checked furnace output slot should accept the product");` |

### `crates/factory_sim/src/simulation/machine_tick/inserters.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 38 | `.expect` | 1 | `.expect("a source item should exist in the prototype catalog");` |
| 78 | `.expect` | 1 | `.expect("a source item should exist in the prototype catalog");` |

### `crates/factory_sim/src/simulation/machine_tick/labs.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 73 | `.expect` | 1 | `.expect("lab checked science packs before completion");` |
| 84 | `.expect` | 1 | `.expect("lab completion should have active research");` |

### `crates/factory_sim/src/simulation/machine_tick/mining_drills.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 87 | `.expect` | 1 | `.expect("selected drill target should contain a resource");` |
| 148 | `.expect` | 1 | `.expect("productive drill surplus capacity was checked");` |
| 152 | `unreachable!` | 1 | `unreachable!("blocked drill output is checked before mining");` |
| 171 | `.expect` | 1 | `.expect("the checked drill output slot should accept the product");` |
| 176 | `.expect` | 1 | `.expect("validated output inventory should still exist")` |
| 178 | `.expect` | 1 | `.expect("validated output inventory should accept drill product");` |
| 184 | `.expect` | 1 | `.expect("validated output belt should still exist");` |
| 186 | `.expect` | 1 | `.expect("validated belt lane should accept");` |
| 206 | `.expect` | 1 | `.expect("validated output splitter should still exist");` |
| 208 | `.expect` | 1 | `.expect("validated splitter lane should accept");` |
| 223 | `unreachable!` | 1 | `unreachable!("blocked drill output is checked before mining")` |
| 271 | `.expect` | 1 | `.expect("stored drill output should still contain exported item");` |

### `crates/factory_sim/src/simulation/machine_tick/pumpjacks.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 67 | `.expect` | 1 | `.expect("pumpjack fluid box was checked above");` |

### `crates/factory_sim/src/simulation/placement_mutation_ops.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 30 | `.expect` | 3 | `.expect("validated player build item should remain removable");` |

### `crates/factory_sim/src/simulation/placement_preview_ops.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 119 | `unreachable!` | 1 | `Err(_) => unreachable!("footprint validation only reports invalid dimensions"),` |
| 305 | `.expect` | 2 | `.expect("prototype validation requires pumpjack entities to declare pumpjack metadata");` |
| 344 | `.expect` | 2 | `.expect("prototype validation requires mining drills to declare mining metadata");` |

### `crates/factory_sim/src/simulation/player_ops.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 120 | `.expect` | 3 | `.expect("validated manual mining target should still contain a resource");` |
| 125 | `.expect` | 3 | `.expect("manual mining checked inventory capacity before inserting");` |
| 180 | `.expect` | 3 | `.expect("manual crafting checked ingredients before removing");` |
| 210 | `.expect` | 1 | `.expect("queued manual craft should reference an existing recipe");` |

### `crates/factory_sim/src/simulation/power_ops/demand.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 420 | `.expect` | 1 | `.expect("a source item should exist in the prototype catalog"),` |

### `crates/factory_sim/src/simulation/power_ops/generation.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 163 | `assert_eq!` | 1 | `assert_eq!(` |

### `crates/factory_sim/src/simulation/radar_ops.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 132 | `.expect` | 1 | `let radius = i32::try_from(radius).expect("u16 radar radius always fits in i32");` |
| 135 | `.expect` | 1 | `\|offset\| i32::try_from(offset).expect("radar ring offset always fits in i32");` |

### `crates/factory_sim/src/simulation/rail_ops/blocks.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 204 | `.expect` | 1 | `.expect("a component holds at least one rail piece");` |

### `crates/factory_sim/src/simulation/rail_ops/geometry.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 24 | `assert!` | 1 | `const _: () = assert!(crate::POSITION_SCALE == POSITION_SCALE as i64);` |

### `crates/factory_sim/src/simulation/rail_ops/graph_builder.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 88 | `.expect` | 1 | `.expect("component should contain at least one rail piece");` |

### `crates/factory_sim/src/simulation/rail_ops/signalling.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 346 | `.expect` | 1 | `.expect("the train was just read");` |

### `crates/factory_sim/src/simulation/robot_ops/construction_jobs.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 287 | `.expect` | 1 | `.expect("catalog material forms a one-item payload"),` |

### `crates/factory_sim/src/simulation/robot_ops/deliveries.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 618 | `.expect` | 1 | `.expect("the withdrawal was clamped to what the chest holds");` |
| 668 | `.expect` | 1 | `.expect("the insertion was clamped to the chest's capacity");` |
| 677 | `.expect` | 1 | `.expect("a leftover preserves a validated stack"),` |

### `crates/factory_sim/src/simulation/robot_ops/dispatch.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 127 | `.expect` | 1 | `.expect("the selected roboport is placed");` |
| 132 | `.expect` | 1 | `.expect("the selected roboport still exists");` |
| 136 | `.expect` | 1 | `.expect("the selected robot is still stationed");` |

### `crates/factory_sim/src/simulation/robot_ops/flight.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 122 | `.expect` | 1 | `.expect("the roboport was just read");` |
| 126 | `.expect` | 1 | `.expect("the robot stack was just read from these slots");` |
| 445 | `.expect` | 1 | `.expect("cargo insertion was bounded by inventory capacity");` |
| 455 | `.expect` | 1 | `.expect("remaining cargo preserves a validated stack"),` |

### `crates/factory_sim/src/simulation/robot_ops/network_builder.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 31 | `.expect` | 1 | `.expect("component should contain at least one roboport");` |

### `crates/factory_sim/src/simulation/rolling_stock_ops/loading.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 256 | `.expect` | 1 | `.expect("a removed inserter source item should form a valid stack"),` |
| 278 | `.expect` | 1 | `.expect("the checked locomotive fuel slot should accept the item");` |

### `crates/factory_sim/src/simulation/rolling_stock_ops/mod.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 760 | `.expect` | 1 | `.expect("id was collected");` |
| 1083 | `.expect` | 1 | `.expect("the train was just read");` |
| 1109 | `.expect` | 1 | `.expect("the stock was just read")` |
| 1118 | `.expect` | 1 | `.expect("the train was just read");` |

### `crates/factory_sim/src/simulation/rolling_stock_ops/placement.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 86 | `.expect` | 3 | `.expect("the build item was just counted in the player inventory");` |
| 308 | `.expect` | 1 | `.expect("the members list is non-empty");` |
| 614 | `.expect` | 1 | `.expect("the train was just created or found")` |
| 696 | `.expect` | 1 | `.expect("the train was just read");` |
| 759 | `.expect` | 1 | `.expect("a coupled piece follows an existing group")` |
| 828 | `.expect` | 1 | `.expect("the group's train was just created or kept")` |

### `crates/factory_sim/src/simulation/scripted.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 90 | `.expect` | 6 | `.expect("the run was validated piece by piece before it was laid");` |
| 160 | `.expect` | 6 | `ItemStack::new(catalog, coal, 50).expect("fixture coal forms a valid stack"),` |
| 162 | `.expect` | 6 | `.expect("a locomotive fuel slot accepts coal");` |
| 172 | `.expect` | 6 | `.expect("scripted red science commands should apply");` |
| 185 | `.expect` | 6 | `.expect("logistics research should be queueable");` |
| 192 | `.expect` | 6 | `.expect("automation research should be queueable after logistics");` |
| 198 | `.expect` | 6 | `.expect("scripted red science fixture should be able to place power");` |
| 208 | `.expect` | 6 | `.expect("scripted red science fixture should be able to place a lab");` |
| 214 | `.expect` | 6 | `.expect("placed boiler should expose boiler state")` |
| 219 | `.expect` | 6 | `.expect("scripted coal should form a valid stack"),` |
| 221 | `.expect` | 6 | `.expect("scripted fuel slot should be valid");` |
| 224 | `.expect` | 6 | `.expect("placed lab should expose lab state")` |
| 227 | `.expect` | 6 | `.expect("scripted lab inventory should accept research packs");` |
| 369 | `.expect` | 6 | `.expect("the fixture roboport was just placed")` |
| 386 | `.expect` | 6 | `.expect("fixture cargo should form a valid stack"),` |
| 653 | `panic!` | 6 | `panic!("expected a world seed in 0..64 that fits the chemical science factory fixture");` |
| 657 | `assert!` | 6 | `assert!(` |
| 776 | `.expect` | 6 | `.expect("validated chemical science fixture entity should be placeable");` |
| 802 | `panic!` | 6 | `.unwrap_or_else(\|_\| panic!("{technology_name} should be queueable"));` |
| 809 | `.expect` | 6 | `.expect("placed boiler should expose boiler state")` |
| 814 | `.expect` | 6 | `.expect("scripted coal should form a valid stack"),` |
| 816 | `.expect` | 6 | `.expect("scripted fuel slot should be valid");` |
| 822 | `.expect` | 6 | `.expect("scripted fixture chest should have an inventory")` |
| 824 | `.expect` | 6 | `.expect("scripted fixture chest should accept items");` |
| 830 | `.expect` | 6 | `.expect("placed lab should expose lab state")` |
| 833 | `.expect` | 6 | `.expect("scripted lab inventory should accept research packs");` |
| 840 | `.expect` | 6 | `.expect("placed turret should expose turret state")` |
| 843 | `.expect` | 6 | `.expect("scripted turret ammo inventory should accept magazines");` |
| 853 | `.expect` | 6 | `.expect("scripted fixture tank should expose a fluid box");` |

### `crates/factory_sim/src/simulation/tile_placement_ops.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 149 | `.expect` | 3 | `.expect("validated tile placement should be writable");` |
| 152 | `.expect` | 3 | `.expect("validated placement item should remain removable");` |

### `crates/factory_sim/src/simulation/validation/inventory.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 84 | `unreachable!` | 1 | `unreachable!("validating one item slot cannot report inventory operation errors")` |

### `crates/factory_sim/src/simulation/validation/machines.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 382 | `unreachable!` | 1 | `_ => unreachable!("stack construction cannot report inventory capacity errors"),` |

### `crates/factory_sim/src/simulation/world_ops.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 31 | `.expect` | 2 | `PrototypeCatalog::load_base().expect("base prototype catalog should load"),` |
| 481 | `.expect` | 2 | `.expect("the mining_drill section was just checked above");` |

### `crates/factory_sim/src/tick.rs`

| Line | Construct | Cat | Site |
| ---- | --------- | --- | ---- |
| 14 | `.unwrap()` | 1 | `profiler.measure(ProfilePhase::Validation, \|\| sim.validate().unwrap());` |
