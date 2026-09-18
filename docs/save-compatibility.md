# Save compatibility policy

Factory currently writes simulation save format **58** and supports loading every
format from the deliberate baseline **57** through the current format. Older
formats are not silently interpreted as current data.

## Supported window

| Format | Status | Load behavior |
| --- | --- | --- |
| 58 | Current | Decode and fully validate. |
| 57 | Migratable | Decode the immutable v57 schema, migrate to v58 in memory, then fully validate. |
| 56 and older | Unsupported old | Keep the file untouched and direct the player to a compatible pinned build. |
| 59 and newer | Newer version | Reject and ask the player to update the game. |

Version 57 is the compatibility baseline. Version 54 and earlier lack
deterministic navigation, targeting, invalidation, and belt scheduling state;
version 55 uses an older prototype schema; and version 56 lacks durable enemy
wall-follow progress. Reconstructing those values would silently change a running
world, so the project deliberately starts the supported migration chain at v57
rather than pretending those formats can be restored faithfully.

The baseline is not a rolling “previous version only” promise. New formats must
keep the chain from v57 forward unless a breaking boundary is explicitly chosen,
documented here, surfaced in the UI, and covered by boundary tests.

## Migration guarantees

Migration is dispatched by the version in the fixed save header. Each supported
source version has its own immutable wire schema and an explicit step to the next
version. The resulting current snapshot is subjected to the same prototype-hash,
durable-state, rebuilt-cache, and full simulation invariant validation as a native
v58 save before it can replace the live simulation.

The v57-to-v58 step preserves every v57 field unchanged. V58 introduced durable
frontiers for unfinished train route searches; v57 contains no such frontier, so
the documented default is an empty pending-search set. A train that still needs a
route starts a new bounded search under v58 rules.

Loading and migration are read-only. They do not rewrite the source file. A user
must explicitly save the validated, loaded world to create current-format bytes;
normal save replacement keeps the prior file until the new save has been encoded,
validated by the write path, flushed, and atomically promoted.

The sanitized fixture
`crates/factory_sim/tests/fixtures/save-v57-sanitized.factsim` is the canonical v57
source. Tests require it to migrate, pass full validation, and remain in lockstep
with the equivalent simulation after continuing. Unknown future versions,
unsupported old versions, truncated data, trailing data, and malformed payloads
remain errors.

## Breaking changes and recovery

Changing `SAVE_VERSION` requires one of the following in the same change:

1. Add an immutable schema and explicit migration step for the previous current
   format, plus a sanitized fixture and deterministic-continuation coverage.
2. Make an explicit compatibility-boundary decision, explain which state cannot
   be preserved, update this policy and UI messages, and test the new boundary.

For an unsupported save, preserve a copy and run the repository revision (or a
released binary) that wrote that format. Load and re-save it with intermediate
versions until it reaches a supported source format, then open it with the current
build. Never change only the header version: binary layouts and simulation
semantics may differ even when the payload appears to decode.
