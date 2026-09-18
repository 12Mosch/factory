//! Record registry: stable keys, ownership, and key helpers.
//!
//! The [`RECORD_REGISTRY`] literal below is the single source for keys,
//! ownership, and requiredness. The operational handler table in
//! [`super::groups`] borrows one descriptor per row and attaches the
//! encode/measure/decode handlers, so dispatch and manifest flags resolve
//! through these descriptors without copying them. Adding a record means
//! adding one descriptor plus one handler row plus encode/decode helpers in
//! the owning subsystem module — never editing scattered matches.
//!
//! Terrain travels partitioned by world chunk (`chunk/<x>/<y>` records) while
//! global and cross-chunk systems keep explicit global ownership with stable
//! references. Trains, networks, and robot jobs are never forced into
//! terrain-file boundaries.

use super::super::*;

/// Magic bytes at the start of a record-container save.
pub const RECORD_MAGIC: [u8; 8] = *b"FACTREC\0";
/// Current record-container format version.
pub const RECORD_FORMAT_VERSION: u32 = 1;
/// Fixed header size in bytes.
pub const RECORD_HEADER_SIZE: usize = 8 + 4 + 4 + 8 + 4 + 8 + 8 + 4 + 4 + 32;
/// Codec ID for identity storage (bincode bytes stored as-is).
pub const RECORD_CODEC_IDENTITY: u32 = 0;
/// Schema version carried by every v1 record.
pub(crate) const RECORD_SCHEMA_VERSION: u32 = 1;
/// Bound on one record key's UTF-8 length.
pub(crate) const MAX_RECORD_KEY_BYTES: usize = 96;
/// Manifest flag marking a record as required for a complete generation.
pub(crate) const FLAG_REQUIRED: u32 = 1;
/// All manifest flag bits defined by container v1.
pub(crate) const KNOWN_FLAGS: u32 = FLAG_REQUIRED;
/// Structural bound on records per generation: one record per global handler
/// row plus one record per world chunk. Supported worlds stay resident below
/// 4,096 chunks and reach the format ceiling near 8,800, so this leaves wide
/// headroom while keeping manifest, entry, and decode-slot vectors small
/// even for hostile inputs whose decoded payloads still fit the byte budgets.
pub const MAX_RECORD_COUNT: u32 = 32_768;

pub(crate) const KEY_CORE: &str = "core";
pub(crate) const KEY_PROTOTYPES: &str = "prototypes";
pub(crate) const KEY_CHART: &str = "chart";
pub(crate) const KEY_CHUNK_QUEUE: &str = "chunk-queue";
pub(crate) const KEY_STATISTICS: &str = "statistics";
pub(crate) const KEY_ENTITIES: &str = "entities";
pub(crate) const KEY_CONSTRUCTION: &str = "construction";
pub(crate) const KEY_PLAYER: &str = "player";
pub(crate) const KEY_POWER: &str = "power";
pub(crate) const KEY_FLUIDS: &str = "fluids";
pub(crate) const KEY_HEAT: &str = "heat";
pub(crate) const KEY_ROBOTS: &str = "robots";
pub(crate) const KEY_TRAINS: &str = "trains";
pub(crate) const KEY_ENVIRONMENT: &str = "environment";

/// One global record of a complete snapshot generation: its stable key, the
/// subsystem that owns its bytes, whether a generation is complete without
/// it, and what it carries.
///
/// This metadata is operational, not documentary: the handler table in
/// [`super::groups`] borrows one descriptor per row, so the `required` flag
/// below controls the manifest flags on the wire (see `record_required`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecordDescriptor {
    pub key: &'static str,
    pub owner: &'static str,
    pub required: bool,
    pub description: &'static str,
}

/// Global records that must all be present for one complete generation.
///
/// This literal table is the single source for keys, ownership, and
/// requiredness. The operational handler table in [`super::groups`] borrows
/// one descriptor per row and attaches the encode/measure/decode handlers,
/// so key enumeration, dispatch, and the manifest `required` flags all
/// resolve through these descriptors without copying them.
///
/// Order follows the format specification for readability; the manifest
/// order on the wire is the sorted key order (see `validate_index_order`).
pub(crate) const RECORD_REGISTRY: [RecordDescriptor; 14] = [
    RecordDescriptor {
        key: KEY_CORE,
        owner: "simulation core",
        required: true,
        description: "tick, world seed, day/night phase, config, topology/chunk/walkability revisions",
    },
    RecordDescriptor {
        key: KEY_PROTOTYPES,
        owner: "global data identity",
        required: true,
        description: "prototype catalog",
    },
    RecordDescriptor {
        key: KEY_CHART,
        owner: "global",
        required: true,
        description: "chart state",
    },
    RecordDescriptor {
        key: KEY_CHUNK_QUEUE,
        owner: "global",
        required: true,
        description: "pending chunk-generation requests",
    },
    RecordDescriptor {
        key: KEY_STATISTICS,
        owner: "global",
        required: true,
        description: "item/fluid/power statistics, launches, deaths",
    },
    RecordDescriptor {
        key: KEY_ENTITIES,
        owner: "global entity ownership",
        required: true,
        description: "entity store",
    },
    RecordDescriptor {
        key: KEY_CONSTRUCTION,
        owner: "global",
        required: true,
        description: "construction state",
    },
    RecordDescriptor {
        key: KEY_PLAYER,
        owner: "global",
        required: true,
        description: "player, equipment, weapon, combat, inventory, corpses, mining, crafting, onboarding, research",
    },
    RecordDescriptor {
        key: KEY_POWER,
        owner: "global network ownership",
        required: true,
        description: "power summary, networks, entity statuses",
    },
    RecordDescriptor {
        key: KEY_FLUIDS,
        owner: "global network ownership",
        required: true,
        description: "fluid networks, invalidation flag",
    },
    RecordDescriptor {
        key: KEY_HEAT,
        owner: "global network ownership",
        required: true,
        description: "heat networks, invalidation flag",
    },
    RecordDescriptor {
        key: KEY_ROBOTS,
        owner: "cross-chunk ownership",
        required: true,
        description: "robot networks, logistic work, flights",
    },
    RecordDescriptor {
        key: KEY_TRAINS,
        owner: "cross-chunk ownership",
        required: true,
        description: "rolling stock, pending route searches with frontiers",
    },
    RecordDescriptor {
        key: KEY_ENVIRONMENT,
        owner: "cross-chunk ownership",
        required: true,
        description: "pollution, enemies, transport cache, navigation, targeting",
    },
];

const _: () = assert!(
    RECORD_HEADER_SIZE == 84,
    "record header layout is fixed by the format specification"
);

pub(crate) fn record_error(message: impl Into<String>) -> SaveLoadError {
    SaveLoadError::Codec(bincode::ErrorKind::Custom(message.into()).into())
}

pub(crate) fn chunk_key(coord: ChunkCoord) -> String {
    format!("chunk/{}/{}", coord.x, coord.y)
}

pub(crate) fn parse_chunk_key(key: &str) -> Option<ChunkCoord> {
    let coords = key.strip_prefix("chunk/")?;
    let (x, y) = coords.split_once('/')?;
    Some(ChunkCoord {
        x: x.parse().ok()?,
        y: y.parse().ok()?,
    })
}

pub(crate) fn validate_key_bytes(key: &str) -> Result<(), SaveLoadError> {
    if key.is_empty() || key.len() > MAX_RECORD_KEY_BYTES {
        return Err(record_error(format!(
            "record key has invalid length {}",
            key.len()
        )));
    }
    if !key.bytes().all(|byte| {
        matches!(
            byte,
            b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'+'
        )
    }) {
        return Err(record_error(format!(
            "record key {key:?} uses invalid characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lists_fourteen_required_globals() {
        assert_eq!(RECORD_REGISTRY.len(), 14);
        assert!(RECORD_REGISTRY.iter().all(|entry| entry.required));
        for entry in &RECORD_REGISTRY {
            assert!(
                !entry.owner.is_empty(),
                "record {:?} needs an owner",
                entry.key
            );
            assert!(
                !entry.description.is_empty(),
                "record {:?} needs a description",
                entry.key
            );
            validate_key_bytes(entry.key)
                .unwrap_or_else(|_| panic!("registry key {:?} must be a valid key", entry.key));
        }
    }

    #[test]
    fn every_registry_row_has_exactly_one_handler() {
        // Handler rows borrow registry entries by index, so only two
        // failures are possible here: a wrong or duplicated index (caught by
        // the coverage below) and a duplicated key (caught by the uniqueness
        // check). Length and field equality would merely re-assert the
        // borrow itself.
        let handlers = &super::super::groups::RECORD_HANDLERS;
        let mut seen = vec![false; RECORD_REGISTRY.len()];
        for handler in handlers.iter() {
            let index = RECORD_REGISTRY
                .iter()
                .position(|entry| entry.key == handler.descriptor.key)
                .expect("handler must reference a registry key");
            assert!(
                !seen[index],
                "duplicate handler for {:?}",
                handler.descriptor.key
            );
            seen[index] = true;
        }
        assert!(
            seen.iter().all(|&seen| seen),
            "every registry row needs exactly one handler"
        );
        let mut keys: Vec<&str> = RECORD_REGISTRY.iter().map(|entry| entry.key).collect();
        let before = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), before, "registry keys must be unique");
    }

    #[test]
    fn chunk_keys_never_collide_with_globals() {
        let globals: Vec<&str> = RECORD_REGISTRY.iter().map(|entry| entry.key).collect();
        for coord in [
            ChunkCoord { x: 0, y: 0 },
            ChunkCoord { x: -1, y: 2 },
            ChunkCoord { x: 8800, y: -8800 },
        ] {
            let key = chunk_key(coord);
            assert!(!globals.contains(&key.as_str()));
            assert_eq!(parse_chunk_key(&key), Some(coord));
            // Canonical form round-trips: formatting the parsed coordinates
            // reproduces the key, so aliases cannot load under a wrong key.
            assert_eq!(chunk_key(parse_chunk_key(&key).expect("parseable")), key);
        }
        assert_eq!(parse_chunk_key("core"), None);
        // `chunk/01/2` parses numerically but is not canonical: reformatting
        // gives `chunk/1/2`, so the loader must reject it under the alias key.
        let alias = parse_chunk_key("chunk/01/2").expect("parses numerically");
        assert_ne!(chunk_key(alias), "chunk/01/2");
    }
}
