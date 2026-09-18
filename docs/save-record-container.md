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
| 44 | 4 | record_count | Number of manifest entries (at most `MAX_RECORD_COUNT` = 32,768) |
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
in both the key and the payload; a mismatch is rejected, and keys must be
canonical (formatting the parsed coordinates reproduces the key), so aliases
such as `chunk/01/2` cannot load under a key that selective access and
re-saving would not reproduce. Zero chunk records is valid (an empty world);
every present chunk record is required.

Record count is structurally capped at 32,768 (14 globals plus one record
per chunk). Supported worlds stay resident below 4,096 chunks and reach the
format ceiling near 8,800, so the cap leaves wide headroom while keeping
manifest, entry, and decode-slot vectors small even for hostile inputs whose
payloads still fit the byte budgets.

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
version, record/collection bounds including the structural record-count cap,
manifest presence, manifest checksum, entry framing, key charset, unknown
flags, entry count, key uniqueness and deterministic order in one linear
pass, contiguous packing (offsets must tile the data region exactly, which
rejects overlaps, gaps, and trailing bytes with overflow-checked
arithmetic), per-record length budgets, then per record in manifest order:
bounded read, checksum, codec (for records this build decodes), schema,
and immediate decode into its assembly slot with the payload dropped before
the next record reads. Unknown-required records, missing required records,
non-canonical chunk keys, and chunk coordinate mismatches abort; unknown
optional records skip after bounds and checksum verification. Assembly then
checks required-record presence, header/core generation identity (tick and
seed must agree, so all records resolve to one complete snapshot
generation), prototype hash, durable-state validation, derived-state
reconstruction, and final full validation. No partial world is ever
simulated, and encoded payloads never accumulate: retained decode memory is
the manifest plus decoded state. Index inspection additionally verifies the
physical file length, so missing or trailing payload bytes cannot inspect
as a valid index.

Truncation at any stage is a load error. Aggregate budgets are
record-aware: each record is bounded by `max_record_bytes` and the decoded
total by `max_decoded_bytes`, while the complete artifact is bounded by
`max_encoded_bytes`. `max_record_bytes` never applies to the whole
container, so a world that partitions into valid records is not rejected
because its monolithic form would exceed one record's budget. Oversized
worlds fail with `TooLarge` before any unbounded allocation; the
borrowed-schema preflight (collection counts plus the chunk-derived record
count) fails before cloning.

## Transport and commit

The application save pipeline writes record payloads end to end.
Background jobs capture with the record-aware entry point
(`try_capture_record_snapshot`): a borrowed collection walk plus a
per-record serialized-size preflight over borrowed tuples bounds every
record, the decoded total, and the framed artifact total before anything
is cloned, and the monolithic size pass never runs — so the simulation
read lock can be released without duplicating an unsaveable world first.
Encoding then runs through the two-pass record writer (peak encoding memory
is one record payload plus the manifest, at the cost of encoding twice).
The container keeps its existing guarantees around those bytes — streaming
writes to a temporary file, `sync_all`, atomic installation preserving a
rollback backup, and catalog validation before the world is replaced. The
payload allowance is the artifact budget minus the outer container
overhead, so the record header and manifest framing cannot push a
within-budget generation over the edge; the writer is scoped to that
allowance and its pre-write total check guarantees fit before any byte
reaches the file. Loading applies the same format-aware rule: the inner
magic is peeked after the metadata, record payloads are allowed up to the
artifact budget while monolithic payloads keep the legacy allowance, and
the record decoder still enforces aggregate decoded and per-record limits —
so a committed near-limit save always reopens. The outer magic and metadata
framing are unchanged. A copied or exported record file loads without
external references, so plain file copy is the portable export. Loading
validates before mutating the active world, and failed work leaves the
previous save intact.

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
record's checksum and returns its payload from a complete file, while
`extract_record_from_reader` retains only the header, manifest, and target
payload — records after the target are never touched. Preceding payload
bytes are still read and discarded (no `Seek` API), so selective access is
a bounded-memory guarantee, not a range-I/O one. All three enable chunk- or
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
