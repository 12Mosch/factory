# Indexed record container for world persistence (issue #296)

Versioned format specification for scalable world persistence. The container
is implemented in `crates/factory_sim/src/simulation/save_records.rs`.
Issue #291 defines the end-state architecture; this document specifies the
record layer. Compatibility policy lives in `docs/save-compatibility.md`;
budgets and the compression decision live in `docs/persistence-budgets.md`.

## Goals

Streaming, bounded decoding, optional per-record compression, and selective
record reuse must coexist without ever mixing simulation ticks. The container
is the scalable format direction: whole-snapshot streaming ships first, and
incremental record reuse is gated on measured need (see below).

## Layout

All integers are little-endian. The file is one self-contained immutable
snapshot generation: every record needed to rebuild the world is inside.

```text
fixed header (84 bytes)
manifest/index (index_len bytes)
record payloads (packed contiguously, manifest order)
```

### Fixed header

| Offset | Size | Field | Notes |
| --- | --- | --- | --- |
| 0 | 8 | magic | `FACTREC\0` |
| 8 | 4 | save_version | Same registry as the monolithic snapshot (`58` current) |
| 12 | 4 | prototype_format_version | Same registry (`35` current) |
| 16 | 8 | prototype_hash | Validated against decoded prototypes after load |
| 24 | 4 | record_format_version | `1` |
| 28 | 8 | tick | Completed simulation tick of this generation |
| 36 | 8 | world_seed | World identity of this generation |
| 44 | 4 | record_count | Number of manifest entries |
| 48 | 4 | index_len | Manifest size in bytes |
| 52 | 32 | index_checksum | BLAKE3 of the manifest bytes |

The first 24 bytes share their layout with the monolithic `FACTSIM\0`
header, so `inspect_save_header` and the save catalog classify both
encodings through the same version table without deserializing.

### Manifest entries

Each entry is length-prefixed by its key:

| Field | Size | Notes |
| --- | --- | --- |
| key_len | 4 | 1..=96 |
| key | key_len | UTF-8, charset `[a-z0-9/+._-]` |
| schema_version | 4 | `1` for every v1 record |
| codec_id | 4 | `0` identity; others rejected when the record must decode |
| flags | 4 | Bit 0 = required; all other bits must be zero |
| offset | 8 | Absolute file offset of the payload |
| encoded_len | 8 | Stored payload bytes |
| decoded_len | 8 | Wire bytes after codec; equals encoded for codec 0 |
| checksum | 32 | BLAKE3 of the encoded payload |

### Record registry (v1)

Global and cross-chunk systems keep explicit global ownership with stable
references. Trains, networks, and robot jobs are never forced into
terrain-file boundaries.

| Key | Schema | Contents | Ownership |
| --- | --- | --- | --- |
| `core` | 1 | tick, world seed, day/night phase, config, topology/chunk/walkability revisions | simulation core |
| `prototypes` | 1 | prototype catalog | global data identity |
| `chart` | 1 | chart state | global |
| `chunk-queue` | 1 | pending chunk-generation requests | global |
| `statistics` | 1 | item/fluid/power statistics, launches, deaths | global |
| `entities` | 1 | entity store | global entity ownership |
| `construction` | 1 | construction state | global |
| `player` | 1 | player, equipment, weapon, combat, inventory, corpses, mining, crafting, onboarding, research | global |
| `power` | 1 | power summary, networks, entity statuses | global network ownership |
| `fluids` | 1 | fluid networks, invalidation flag | global network ownership |
| `heat` | 1 | heat networks, invalidation flag | global network ownership |
| `robots` | 1 | robot networks, logistic work, flights | cross-chunk ownership |
| `trains` | 1 | rolling stock, pending route searches with frontiers | cross-chunk ownership |
| `environment` | 1 | pollution, enemies, transport cache, navigation, targeting | cross-chunk ownership |
| `chunk/<x>/<y>` | 1 | one `Chunk` (coordinate plus tiles) | partitioned terrain |

All 14 global records are required. Chunk records carry their coordinates
in both the key and the payload; a mismatch is rejected. Zero chunk records
is valid (an empty world); every present chunk record is required.

### Deterministic ordering

Manifest entries are sorted byte-wise by key. Writers sort; readers enforce
strictly increasing keys and reject duplicates. Two encodings of the same
generation are byte-identical.

### Required/optional handling

Each entry carries a required flag set by its writer. Readers enforce:

- Known records: presence is required by the registry above; the flag is
  informational.
- Unknown key with required set: abort the load. A newer build's mandatory
  state cannot be silently dropped.
- Unknown key with required clear: skip after bounds and checksum
  verification. The payload is never decoded, so unknown schemas and codecs
  in optional records remain forward-compatible.

### Codecs and compression

Codec 0 stores bincode wire bytes as-is. No other codec is supported:
decoding a record this build must interpret with any other codec ID fails.

This follows the issue #264 measurements (`docs/persistence-budgets.md`):
supported payloads (about 1 MiB small, 4 MiB medium, 22 MiB large) sit well
below the 64 MiB ceiling, and compression would add CPU, decoded-memory, and
compatibility costs without addressing snapshot-capture cost. If observed
payloads approach the 48 MiB investigation threshold, measure
compression/deduplication on the same fixtures and record CPU, decoded
memory, and compatibility costs before assigning a new codec ID. A future
codec must enforce its decompression budget (`decoded_len` against
`max_record_bytes`/`max_decoded_bytes`) before allocating.

## Validation

Decoding validates, in order: magic, container version, save version
(through the same support table as the monolithic path), prototype format
version, record/collection bounds, manifest presence, manifest checksum,
entry framing, key charset, unknown flags, entry count, key uniqueness,
deterministic order, contiguous packing (offsets must tile the data region
exactly, which rejects overlaps, gaps, and trailing bytes with
overflow-checked arithmetic), per-record length budgets, per-record
checksums, codecs for records that must decode, schemas, unknown-required
rejection, required-record presence, chunk coordinate references,
header/core generation identity (tick and seed must agree, so all records
resolve to one complete snapshot generation), prototype hash, durable-state
validation, derived-state reconstruction, and final full validation. No
partial world is ever simulated: the candidate is assembled only after every
record verifies.

Truncation at any stage is a load error. The total decoded size and the
complete file size stay within `SaveLimits`; oversized worlds fail with
`TooLarge` before any unbounded allocation.

## Transport and commit

Record bytes are opaque payload bytes to the application container, which
keeps its existing guarantees: streaming writes to a temporary file,
`sync_all`, atomic installation preserving a rollback backup, and catalog
validation before the world is replaced. A copied or exported record file
loads without external references, so plain file copy is the portable
export. Loading validates before mutating the active world, and failed work
leaves the previous save intact.

The loader dispatches on magic: `FACTSIM\0` decodes the monolithic snapshot
(including the v57 migration path), `FACTREC\0` decodes the record
container. Record v1 only encodes the current snapshot layout
(`SAVE_VERSION` 58); historical versions keep using the monolithic path, so
no new migration step or compatibility-window change was needed. Per
`docs/save-compatibility.md`, introducing a new snapshot version later
requires a schema, a migration step, or an explicit boundary decision, and
that rule applies to record payloads unchanged.

## Selective access

`inspect_record_index` validates the header and manifest and lists every
record without decoding payloads. `extract_record_bytes` verifies one
record's checksum and returns its payload. Both enable chunk- or
record-granular tools without simulating a partial world.

## Incremental reuse gate (deferred)

Rewriting unchanged records has not been demonstrated to be a material
bottleneck: the #264 budgets show encode at 1.8 ms (small), 8.1 ms (medium),
and 44.7 ms (large) against multi-second capture-adjacent budgets, and the
decision in `docs/persistence-budgets.md` keeps the monolithic full-copy
format for measured sizes. The record container is deliberately shaped so
incremental reuse can be added without changing record semantics:

- Immutable record blobs content-addressed by checksum, reused across
  generations instead of rewritten.
- An atomically committed generation manifest listing exactly the records of
  one generation; readers resolve a single manifest, never a mixture.
- Bounded compaction plus reachability-based reclamation over retained
  generations. Never patch the active save in place, and never delete a
  record referenced by another retained generation.
- A tested portable export that folds one generation back into a
  self-contained file (plain copy semantics, as today).

Adopting that design additionally requires a benchmark proving unchanged
records dominate save cost on representative worlds, plus CPU, memory, and
complexity accounting for the manifest store. Until then, every save remains
a self-contained file, and this gate stays a documented decision rather than
a prerequisite.
