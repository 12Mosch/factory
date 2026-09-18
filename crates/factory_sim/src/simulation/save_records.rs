//! Indexed record container for scalable world persistence.
//!
//! This implements the versioned format specification from
//! `docs/save-record-container.md` (issue #296): a fixed header carrying the
//! snapshot identity, a bounded manifest/index, and typed records with stable
//! keys, schema versions, codec IDs, encoded/decoded lengths, and integrity
//! checksums.
//!
//! Terrain travels partitioned by world chunk (`chunk/<x>/<y>` records) while
//! global and cross-chunk systems (entities, construction, power/fluid/heat
//! networks, robots, trains, environment) keep explicit global ownership with
//! stable references. Trains, networks, and robot jobs are never forced into
//! terrain-file boundaries.
//!
//! The container is self-contained: every record needed to rebuild one
//! complete snapshot generation lives in the file, so a copied or exported
//! save loads without external references. Files are written to a temporary
//! path and atomically installed by the existing application container
//! (`factory_app::save_load`), which treats the record payload as opaque
//! bytes. Loading never simulates a partial world: all records are decoded
//! and validated before the candidate simulation is assembled.
//!
//! Compression follows the issue #264 measurements: codec 0 (identity) is the
//! only supported codec. Payloads in the supported envelope sit well below
//! the 64 MiB ceiling, and compression would add CPU, decoded-memory, and
//! compatibility costs without addressing snapshot-capture cost. Codec IDs
//! remain versioned so a future codec can be introduced with decompression
//! budgets enforced before allocation.

use super::robot_ops::RobotLogisticWorkState;
use super::save::{
    PROTOTYPE_FORMAT_VERSION, SAVE_VERSION, SaveLoadError, SimulationSaveSnapshot,
    SimulationSnapshotOwned, capture_save_snapshot_in_generation,
};
use super::*;
use bincode::Options;
use std::collections::BTreeMap;
use std::io::{Read, Write};

/// Magic bytes at the start of a record-container save.
pub const RECORD_MAGIC: [u8; 8] = *b"FACTREC\0";
/// Current record-container format version.
pub const RECORD_FORMAT_VERSION: u32 = 1;
/// Fixed header size in bytes.
pub const RECORD_HEADER_SIZE: usize = 8 + 4 + 4 + 8 + 4 + 8 + 8 + 4 + 4 + 32;
/// Codec ID for identity storage (bincode bytes stored as-is).
pub const RECORD_CODEC_IDENTITY: u32 = 0;
/// Schema version carried by every v1 record.
const RECORD_SCHEMA_VERSION: u32 = 1;
/// Bound on one record key's UTF-8 length.
const MAX_RECORD_KEY_BYTES: usize = 96;
/// Manifest flag marking a record as required for a complete generation.
const FLAG_REQUIRED: u32 = 1;
/// All manifest flag bits defined by container v1.
const KNOWN_FLAGS: u32 = FLAG_REQUIRED;
/// Structural bound on records per generation: 14 globals plus one record
/// per world chunk. Supported worlds stay resident below 4,096 chunks and
/// reach the format ceiling near 8,800, so this leaves wide headroom while
/// keeping manifest, entry, and decode-slot vectors small even for hostile
/// inputs whose decoded payloads still fit the byte budgets.
pub const MAX_RECORD_COUNT: u32 = 32_768;

const KEY_CORE: &str = "core";
const KEY_PROTOTYPES: &str = "prototypes";
const KEY_CHART: &str = "chart";
const KEY_CHUNK_QUEUE: &str = "chunk-queue";
const KEY_STATISTICS: &str = "statistics";
const KEY_ENTITIES: &str = "entities";
const KEY_CONSTRUCTION: &str = "construction";
const KEY_PLAYER: &str = "player";
const KEY_POWER: &str = "power";
const KEY_FLUIDS: &str = "fluids";
const KEY_HEAT: &str = "heat";
const KEY_ROBOTS: &str = "robots";
const KEY_TRAINS: &str = "trains";
const KEY_ENVIRONMENT: &str = "environment";

/// Global records that must all be present for one complete generation.
const REQUIRED_GLOBAL_KEYS: [&str; 14] = [
    KEY_CORE,
    KEY_PROTOTYPES,
    KEY_CHART,
    KEY_CHUNK_QUEUE,
    KEY_STATISTICS,
    KEY_ENTITIES,
    KEY_CONSTRUCTION,
    KEY_PLAYER,
    KEY_POWER,
    KEY_FLUIDS,
    KEY_HEAT,
    KEY_ROBOTS,
    KEY_TRAINS,
    KEY_ENVIRONMENT,
];

const _: () = assert!(
    RECORD_HEADER_SIZE == 84,
    "record header layout is fixed by the format specification"
);

fn record_error(message: impl Into<String>) -> SaveLoadError {
    SaveLoadError::Codec(bincode::ErrorKind::Custom(message.into()).into())
}

/// Fixed record-container header.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RecordHeader {
    save_version: u32,
    prototype_format_version: u32,
    prototype_hash: u64,
    record_format_version: u32,
    tick: u64,
    world_seed: u64,
    record_count: u32,
    index_len: u32,
    index_checksum: [u8; 32],
}

/// One manifest entry: the index row describing a single record.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ManifestEntry {
    key: String,
    schema_version: u32,
    codec_id: u32,
    required: bool,
    offset: u64,
    encoded_len: u64,
    decoded_len: u64,
    checksum: [u8; 32],
}

/// Public summary of one indexed record for selective-access tools.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordSummary {
    pub key: String,
    pub schema_version: u32,
    pub codec_id: u32,
    pub required: bool,
    pub encoded_len: u64,
    pub decoded_len: u64,
}

/// Public index view: header identity plus one summary per record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordIndex {
    pub save_version: u32,
    pub prototype_format_version: u32,
    pub prototype_hash: u64,
    pub tick: u64,
    pub world_seed: u64,
    pub records: Vec<RecordSummary>,
}

// Record group payloads. Each group mirrors a slice of the ordered
// durable-state registry in `save.rs`; together they carry exactly the
// current snapshot. Groups encode as bincode tuples of borrowed fields in
// the documented order, so encoding never clones snapshot subsystems; the
// decoder destructures the same tuples. Field order here is the wire order.

type CoreTuple = (
    u64,
    u64,
    Option<DayNightCycleState>,
    SimulationConfig,
    u64,
    u64,
    u64,
);
type StatisticsTuple = (ItemStatistics, FluidStatistics, PowerStatistics, u64, u64);
type PlayerTuple = (
    PlayerState,
    PlayerEquipmentState,
    PlayerWeaponState,
    DelayedCombatState,
    Inventory,
    BTreeMap<u64, PlayerCorpse>,
    Option<ManualMiningProgress>,
    CraftingQueue,
    OnboardingProgress,
    ResearchState,
);
type PowerTuple = (
    PowerSummary,
    Vec<PowerNetworkSnapshot>,
    DenseEntityMap<EntityPowerStatus>,
);
type FluidsTuple = (Vec<FluidNetworkSnapshot>, bool);
type HeatTuple = (Vec<HeatNetworkSnapshot>, bool);
type RobotsTuple = (
    Vec<RobotNetworkSnapshot>,
    RobotLogisticWorkState,
    RobotFlightSubsystem,
);
type TrainsTuple = (
    RollingStockSubsystem,
    BTreeMap<TrainId, rolling_stock_ops::PendingTrainRouteSearch>,
);
type EnvironmentTuple = (
    PollutionState,
    EnemySubsystem,
    TransportLaneCache,
    enemy::EnemyNavigation,
    enemy::AttackTargetCache,
);

fn chunk_key(coord: ChunkCoord) -> String {
    format!("chunk/{}/{}", coord.x, coord.y)
}

fn parse_chunk_key(key: &str) -> Option<ChunkCoord> {
    let coords = key.strip_prefix("chunk/")?;
    let (x, y) = coords.split_once('/')?;
    Some(ChunkCoord {
        x: x.parse().ok()?,
        y: y.parse().ok()?,
    })
}

fn is_global_key(key: &str) -> bool {
    REQUIRED_GLOBAL_KEYS.contains(&key)
}

fn validate_key_bytes(key: &str) -> Result<(), SaveLoadError> {
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

fn encode_header(header: &RecordHeader) -> [u8; RECORD_HEADER_SIZE] {
    let mut bytes = [0; RECORD_HEADER_SIZE];
    bytes[0..8].copy_from_slice(&RECORD_MAGIC);
    bytes[8..12].copy_from_slice(&header.save_version.to_le_bytes());
    bytes[12..16].copy_from_slice(&header.prototype_format_version.to_le_bytes());
    bytes[16..24].copy_from_slice(&header.prototype_hash.to_le_bytes());
    bytes[24..28].copy_from_slice(&header.record_format_version.to_le_bytes());
    bytes[28..36].copy_from_slice(&header.tick.to_le_bytes());
    bytes[36..44].copy_from_slice(&header.world_seed.to_le_bytes());
    bytes[44..48].copy_from_slice(&header.record_count.to_le_bytes());
    bytes[48..52].copy_from_slice(&header.index_len.to_le_bytes());
    bytes[52..84].copy_from_slice(&header.index_checksum);
    bytes
}

fn parse_header(bytes: &[u8]) -> Result<RecordHeader, SaveLoadError> {
    if bytes.len() < RECORD_HEADER_SIZE {
        return Err(record_error("record container header is truncated"));
    }
    let mut magic = [0; 8];
    magic.copy_from_slice(&bytes[0..8]);
    if magic != RECORD_MAGIC {
        return Err(SaveLoadError::InvalidMagic { found: magic });
    }
    let mut checksum = [0; 32];
    checksum.copy_from_slice(&bytes[52..84]);
    Ok(RecordHeader {
        save_version: u32::from_le_bytes(bytes[8..12].try_into().expect("fixed range")),
        prototype_format_version: u32::from_le_bytes(
            bytes[12..16].try_into().expect("fixed range"),
        ),
        prototype_hash: u64::from_le_bytes(bytes[16..24].try_into().expect("fixed range")),
        record_format_version: u32::from_le_bytes(bytes[24..28].try_into().expect("fixed range")),
        tick: u64::from_le_bytes(bytes[28..36].try_into().expect("fixed range")),
        world_seed: u64::from_le_bytes(bytes[36..44].try_into().expect("fixed range")),
        record_count: u32::from_le_bytes(bytes[44..48].try_into().expect("fixed range")),
        index_len: u32::from_le_bytes(bytes[48..52].try_into().expect("fixed range")),
        index_checksum: checksum,
    })
}

fn validate_header(header: &RecordHeader, limits: crate::SaveLimits) -> Result<(), SaveLoadError> {
    if header.record_format_version != RECORD_FORMAT_VERSION {
        return Err(record_error(format!(
            "unsupported record container version {} (supported: {RECORD_FORMAT_VERSION})",
            header.record_format_version
        )));
    }
    match save_version_support(header.save_version) {
        SaveVersionSupport::Current => {}
        _ => {
            return Err(SaveLoadError::UnsupportedSaveVersion {
                found: header.save_version,
                supported: SAVE_VERSION,
            });
        }
    }
    if header.prototype_format_version != PROTOTYPE_FORMAT_VERSION {
        return Err(SaveLoadError::UnsupportedPrototypeFormatVersion {
            found: header.prototype_format_version,
            supported: PROTOTYPE_FORMAT_VERSION,
        });
    }
    if u64::from(header.record_count) > limits.max_collection_entries {
        return Err(SaveLoadError::TooLarge);
    }
    if header.record_count > MAX_RECORD_COUNT {
        return Err(SaveLoadError::TooLarge);
    }
    if u64::from(header.index_len) > limits.max_encoded_bytes {
        return Err(SaveLoadError::TooLarge);
    }
    if header.record_count == 0 {
        return Err(record_error("record container has no records"));
    }
    Ok(())
}

fn encode_entry(entry: &ManifestEntry, out: &mut Vec<u8>) {
    out.extend_from_slice(&(entry.key.len() as u32).to_le_bytes());
    out.extend_from_slice(entry.key.as_bytes());
    out.extend_from_slice(&entry.schema_version.to_le_bytes());
    out.extend_from_slice(&entry.codec_id.to_le_bytes());
    out.extend_from_slice(&(u32::from(entry.required)).to_le_bytes());
    out.extend_from_slice(&entry.offset.to_le_bytes());
    out.extend_from_slice(&entry.encoded_len.to_le_bytes());
    out.extend_from_slice(&entry.decoded_len.to_le_bytes());
    out.extend_from_slice(&entry.checksum);
}

fn parse_entries(
    manifest: &[u8],
    expected: u32,
    limits: crate::SaveLimits,
) -> Result<Vec<ManifestEntry>, SaveLoadError> {
    let mut entries = Vec::new();
    let mut cursor = 0;
    while cursor < manifest.len() {
        let rest = &manifest[cursor..];
        if rest.len() < 4 {
            return Err(record_error("record index entry is truncated"));
        }
        let key_len = u32::from_le_bytes(rest[0..4].try_into().expect("fixed range")) as usize;
        if key_len == 0 || key_len > MAX_RECORD_KEY_BYTES {
            return Err(record_error(format!(
                "record index entry has invalid key length {key_len}"
            )));
        }
        // 4 (key len) + key + 4*3 (schema/codec/flags) + 8*3 (offset/lengths) + 32 (checksum).
        let entry_len = key_len
            .checked_add(4 + 4 + 4 + 4 + 8 + 8 + 8 + 32)
            .ok_or_else(|| record_error("record index entry length overflows"))?;
        if rest.len() < entry_len {
            return Err(record_error("record index entry is truncated"));
        }
        let key_bytes = &rest[4..4 + key_len];
        let key = std::str::from_utf8(key_bytes)
            .map_err(|_| record_error("record index entry key is not valid UTF-8"))?;
        validate_key_bytes(key)?;
        let mut offset = 4 + key_len;
        let mut take_u32 = |count: usize| {
            let value = u32::from_le_bytes(
                rest[offset..offset + count]
                    .try_into()
                    .expect("fixed range"),
            );
            offset += count;
            value
        };
        let schema_version = take_u32(4);
        let codec_id = take_u32(4);
        let flags = take_u32(4);
        if flags & !KNOWN_FLAGS != 0 {
            return Err(record_error(format!(
                "record {key:?} carries unknown flags {flags:#x}"
            )));
        }
        let mut take_u64 = || {
            let value =
                u64::from_le_bytes(rest[offset..offset + 8].try_into().expect("fixed range"));
            offset += 8;
            value
        };
        let entry_offset = take_u64();
        let encoded_len = take_u64();
        let decoded_len = take_u64();
        let mut checksum = [0; 32];
        checksum.copy_from_slice(&rest[offset..offset + 32]);
        if encoded_len > limits.max_record_bytes || decoded_len > limits.max_record_bytes {
            return Err(SaveLoadError::TooLarge);
        }
        if decoded_len > limits.max_decoded_bytes {
            return Err(SaveLoadError::TooLarge);
        }
        entries.push(ManifestEntry {
            key: key.to_string(),
            schema_version,
            codec_id,
            required: flags & FLAG_REQUIRED != 0,
            offset: entry_offset,
            encoded_len,
            decoded_len,
            checksum,
        });
        cursor += entry_len;
        if entries.len() as u64 > limits.max_collection_entries {
            return Err(SaveLoadError::TooLarge);
        }
    }
    if entries.len() as u32 != expected {
        return Err(record_error(format!(
            "record index declares {expected} records but carries {}",
            entries.len()
        )));
    }
    Ok(entries)
}

/// Enforces stable keys with one linear pass: duplicates and
/// out-of-order keys are both rejected by the adjacent comparison, without
/// cloning every key into a set on hostile or large manifests.
fn validate_index_order(entries: &[ManifestEntry]) -> Result<(), SaveLoadError> {
    for pair in entries.windows(2) {
        match pair[0].key.cmp(&pair[1].key) {
            std::cmp::Ordering::Equal => {
                return Err(record_error(format!(
                    "record index contains duplicate key {:?}",
                    pair[0].key
                )));
            }
            std::cmp::Ordering::Greater => {
                return Err(record_error(format!(
                    "record index is not in deterministic order: {:?} precedes {:?}",
                    pair[0].key, pair[1].key
                )));
            }
            std::cmp::Ordering::Less => {}
        }
    }
    Ok(())
}

/// Enforces contiguous packing: offsets must tile the data region exactly,
/// which rejects overlaps, gaps, and trailing bytes in one checked pass.
fn validate_index_layout(
    entries: &[ManifestEntry],
    data_start: u64,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    let mut expected = data_start;
    let mut decoded_total = 0u64;
    for entry in entries {
        if entry.offset != expected {
            return Err(record_error(format!(
                "record {:?} starts at offset {} but contiguous layout requires {expected}",
                entry.key, entry.offset
            )));
        }
        expected = expected
            .checked_add(entry.encoded_len)
            .ok_or_else(|| record_error("record offsets overflow"))?;
        decoded_total = decoded_total
            .checked_add(entry.decoded_len)
            .ok_or_else(|| record_error("record decoded lengths overflow"))?;
    }
    if decoded_total > limits.max_decoded_bytes {
        return Err(SaveLoadError::TooLarge);
    }
    // The complete artifact is bounded by the encoded budget. `max_record_bytes`
    // deliberately does not apply here: it bounds one record so a partitioned
    // world larger than any single record stays valid.
    let total = expected;
    if total > limits.max_encoded_bytes {
        return Err(SaveLoadError::TooLarge);
    }
    Ok(total)
}

fn encode_group(
    value: &impl serde::Serialize,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    crate::save_limits::check_collections(value, limits)?;
    let bound = limits.max_record_bytes.min(limits.max_decoded_bytes);
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(bound)
        .serialize(value)
        .map_err(SaveLoadError::from)
}

fn decode_group<T: serde::de::DeserializeOwned>(
    key: &str,
    bytes: &[u8],
    limits: crate::SaveLimits,
) -> Result<T, SaveLoadError> {
    let (value, consumed): (T, u64) = crate::save_limits::deserialize_from(&mut &bytes[..], limits)
        .map_err(SaveLoadError::from)?;
    if consumed != bytes.len() as u64 {
        return Err(record_error(format!(
            "record {key:?} has trailing bytes after its payload"
        )));
    }
    Ok(value)
}

fn checksum(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

/// Lists every record key for one snapshot generation: the 14 global keys
/// plus one key per world chunk. Chunks become one record per world chunk;
/// all other durable state keeps explicit global ownership.
fn all_group_keys(state: &SimulationSnapshotOwned) -> Vec<String> {
    let mut keys = Vec::with_capacity(REQUIRED_GLOBAL_KEYS.len() + state.chunks.len());
    keys.extend(REQUIRED_GLOBAL_KEYS.iter().map(|key| key.to_string()));
    keys.extend(state.chunks.keys().map(|coord| chunk_key(*coord)));
    keys
}

/// Encodes one record payload directly from borrowed snapshot state.
///
/// Tuples serialize field-by-field in declaration order, matching the tuple
/// aliases above, so no snapshot subsystem is cloned to encode.
fn encode_group_by_key(
    state: &SimulationSnapshotOwned,
    key: &str,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    match key {
        KEY_CORE => encode_group(
            &(
                state.tick,
                state.world_seed,
                &state.day_night_cycle,
                &state.config,
                state.entity_topology_revision,
                state.world_chunk_revision,
                state.world_walkability_revision,
            ),
            limits,
        ),
        KEY_PROTOTYPES => encode_group(&state.prototypes, limits),
        KEY_CHART => encode_group(&state.chart, limits),
        KEY_CHUNK_QUEUE => encode_group(&state.chunk_generation_queue, limits),
        KEY_STATISTICS => encode_group(
            &(
                &state.item_statistics,
                &state.fluid_statistics,
                &state.power_statistics,
                state.rockets_launched,
                state.player_deaths,
            ),
            limits,
        ),
        KEY_ENTITIES => encode_group(&state.entities, limits),
        KEY_CONSTRUCTION => encode_group(&state.construction, limits),
        KEY_PLAYER => encode_group(
            &(
                &state.player,
                &state.player_equipment,
                &state.player_weapon,
                &state.delayed_combat,
                &state.player_inventory,
                &state.corpses,
                &state.manual_mining_progress,
                &state.crafting_queue,
                &state.onboarding_progress,
                &state.research,
            ),
            limits,
        ),
        KEY_POWER => encode_group(
            &(
                &state.power_summary,
                &state.power_networks,
                &state.entity_power_statuses,
            ),
            limits,
        ),
        KEY_FLUIDS => encode_group(&(&state.fluid_networks, state.fluid_topology_dirty), limits),
        KEY_HEAT => encode_group(&(&state.heat_networks, state.heat_topology_dirty), limits),
        KEY_ROBOTS => encode_group(
            &(
                &state.robot_networks,
                &state.robot_logistic_work,
                &state.robot_flights,
            ),
            limits,
        ),
        KEY_TRAINS => encode_group(
            &(&state.rolling_stock, &state.pending_train_route_searches),
            limits,
        ),
        KEY_ENVIRONMENT => encode_group(
            &(
                &state.pollution,
                &state.enemies,
                &state.transport,
                &state.enemy_navigation,
                &state.attack_targets,
            ),
            limits,
        ),
        _ => match parse_chunk_key(key).and_then(|coord| state.chunks.get(&coord)) {
            Some(chunk) => encode_group(chunk, limits),
            None => Err(record_error(format!(
                "record encoder has no payload for key {key:?}"
            ))),
        },
    }
}

/// Serializes a captured immutable snapshot as an indexed record container.
pub fn save_snapshot_records_to_writer(
    snapshot: &SimulationSaveSnapshot,
    writer: &mut impl Write,
) -> Result<(), SaveLoadError> {
    save_snapshot_records_to_writer_with_limits(snapshot, writer, crate::SaveLimits::default())
}

/// Serializes a captured immutable snapshot with explicit limits.
///
/// The writer runs in two passes so independent records are actually
/// streamed: the first pass encodes each group only to learn its length and
/// checksum (payloads are dropped immediately), and the second pass
/// re-encodes each group straight into the writer. Encoding is a pure
/// function of the immutable snapshot, so the second pass verifies every
/// length and checksum before writing; a mismatch aborts instead of tearing
/// the file. Peak encoding memory is one record payload plus the manifest,
/// at the cost of encoding twice.
pub fn save_snapshot_records_to_writer_with_limits(
    snapshot: &SimulationSaveSnapshot,
    writer: &mut impl Write,
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    let state = snapshot.snapshot_state();
    let prototype_hash_value = prototype_hash(&state.prototypes);
    let keys = all_group_keys(state);
    if keys.len() > MAX_RECORD_COUNT as usize
        || u64::try_from(keys.len()).unwrap_or(u64::MAX) > limits.max_collection_entries
    {
        return Err(SaveLoadError::TooLarge);
    }

    // Pass 1: lengths and checksums only. Each payload is dropped before the
    // next group encodes, so no two record payloads coexist.
    let mut sizes = Vec::with_capacity(keys.len());
    for key in &keys {
        let payload = encode_group_by_key(state, key, limits)?;
        let encoded_len = payload.len() as u64;
        if encoded_len > limits.max_record_bytes || encoded_len > limits.max_decoded_bytes {
            return Err(SaveLoadError::TooLarge);
        }
        sizes.push((encoded_len, checksum(&payload)));
    }

    // Deterministic manifest order.
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by(|&left, &right| keys[left].cmp(&keys[right]));
    let record_count = u32::try_from(keys.len()).map_err(|_| SaveLoadError::TooLarge)?;

    // Manifest sizes are bounded before any byte is written so an oversized
    // world fails without leaving a plausible partial save behind.
    // Aggregate decoded bytes are bounded here; entry framing sizes the
    // manifest from keys alone below.
    let mut decoded_total = 0u64;
    for (encoded_len, _) in &sizes {
        decoded_total = decoded_total
            .checked_add(*encoded_len)
            .ok_or(SaveLoadError::TooLarge)?;
    }
    if decoded_total > limits.max_decoded_bytes {
        return Err(SaveLoadError::TooLarge);
    }
    let mut entries = Vec::with_capacity(keys.len());
    // Size the manifest from keys alone.
    let mut sizing = Vec::new();
    for &index in &order {
        let (encoded_len, digest) = sizes[index];
        encode_entry(
            &ManifestEntry {
                key: keys[index].clone(),
                schema_version: RECORD_SCHEMA_VERSION,
                codec_id: RECORD_CODEC_IDENTITY,
                required: true,
                offset: 0,
                encoded_len,
                // Identity codec: decoded length equals encoded length.
                decoded_len: encoded_len,
                checksum: digest,
            },
            &mut sizing,
        );
    }
    let index_len = u32::try_from(sizing.len()).map_err(|_| SaveLoadError::TooLarge)?;
    if u64::from(index_len) > limits.max_encoded_bytes {
        return Err(SaveLoadError::TooLarge);
    }
    let data_start = (RECORD_HEADER_SIZE as u64)
        .checked_add(u64::from(index_len))
        .ok_or(SaveLoadError::TooLarge)?;
    let mut cursor = data_start;
    for &index in &order {
        let (encoded_len, digest) = sizes[index];
        entries.push(ManifestEntry {
            key: keys[index].clone(),
            schema_version: RECORD_SCHEMA_VERSION,
            codec_id: RECORD_CODEC_IDENTITY,
            required: true,
            offset: cursor,
            encoded_len,
            decoded_len: encoded_len,
            checksum: digest,
        });
        cursor = cursor
            .checked_add(encoded_len)
            .ok_or(SaveLoadError::TooLarge)?;
    }
    let mut manifest_bytes = Vec::with_capacity(sizing.len());
    for entry in &entries {
        encode_entry(entry, &mut manifest_bytes);
    }
    debug_assert_eq!(manifest_bytes.len(), index_len as usize);
    if cursor > limits.max_encoded_bytes {
        return Err(SaveLoadError::TooLarge);
    }

    let header = RecordHeader {
        save_version: SAVE_VERSION,
        prototype_format_version: PROTOTYPE_FORMAT_VERSION,
        prototype_hash: prototype_hash_value,
        record_format_version: RECORD_FORMAT_VERSION,
        tick: state.tick,
        world_seed: state.world_seed,
        record_count,
        index_len,
        index_checksum: checksum(&manifest_bytes),
    };
    writer
        .write_all(&encode_header(&header))
        .map_err(io_save_error)?;
    writer.write_all(&manifest_bytes).map_err(io_save_error)?;

    // Pass 2: stream each record. Lengths and checksums are re-verified
    // against the manifest so a nondeterministic encoder cannot tear the file.
    for entry in &entries {
        let payload = encode_group_by_key(state, &entry.key, limits)?;
        if payload.len() as u64 != entry.encoded_len || checksum(&payload) != entry.checksum {
            return Err(record_error(format!(
                "record {:?} re-encoded differently between passes",
                entry.key
            )));
        }
        writer.write_all(&payload).map_err(io_save_error)?;
    }
    Ok(())
}

/// Serializes a captured immutable snapshot to record-container bytes.
pub fn save_snapshot_records_to_bytes(
    snapshot: &SimulationSaveSnapshot,
) -> Result<Vec<u8>, SaveLoadError> {
    save_snapshot_records_to_bytes_with_limits(snapshot, crate::SaveLimits::default())
}

/// Serializes a captured immutable snapshot to bytes with explicit limits.
pub fn save_snapshot_records_to_bytes_with_limits(
    snapshot: &SimulationSaveSnapshot,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    let mut bytes = Vec::new();
    save_snapshot_records_to_writer_with_limits(snapshot, &mut bytes, limits)?;
    Ok(bytes)
}

/// Captures the current completed tick and serializes it as records.
pub fn save_records_to_writer(
    sim: &Simulation,
    writer: &mut impl Write,
) -> Result<(), SaveLoadError> {
    save_records_to_writer_with_limits(sim, writer, crate::SaveLimits::default())
}

/// Checks record-aware capture budgets before allocating an owned snapshot,
/// then captures one immutable completed-tick generation.
///
/// The preflight walks the borrowed schema (collection counts and the chunk
/// count that determines the record count) without cloning and never runs
/// the monolithic whole-snapshot size pass: a world that partitions into
/// valid records must not be rejected because its monolithic form would
/// exceed one record's budget. Aggregate byte budgets are enforced per
/// record and for the whole container during encoding.
pub fn try_capture_record_snapshot(
    sim: &Simulation,
    world_generation: u64,
) -> Result<SimulationSaveSnapshot, SaveLoadError> {
    try_capture_record_snapshot_with_limits(sim, world_generation, crate::SaveLimits::default())
}

/// Checks record-aware capture budgets with explicit limits.
pub fn try_capture_record_snapshot_with_limits(
    sim: &Simulation,
    world_generation: u64,
    limits: crate::SaveLimits,
) -> Result<SimulationSaveSnapshot, SaveLoadError> {
    super::save::check_borrowed_snapshot_collections(sim, limits)?;
    if sim.world.chunks.len() + REQUIRED_GLOBAL_KEYS.len() > MAX_RECORD_COUNT as usize {
        return Err(SaveLoadError::TooLarge);
    }
    if u64::try_from(sim.world.chunks.len() + REQUIRED_GLOBAL_KEYS.len()).unwrap_or(u64::MAX)
        > limits.max_collection_entries
    {
        return Err(SaveLoadError::TooLarge);
    }
    Ok(capture_save_snapshot_in_generation(sim, world_generation))
}

/// Captures the current completed tick with explicit limits, then serializes.
///
/// Capture uses the record-aware preflight above, so partitioning — not the
/// monolithic size pass — decides what is saveable.
pub fn save_records_to_writer_with_limits(
    sim: &Simulation,
    writer: &mut impl Write,
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    let snapshot = try_capture_record_snapshot_with_limits(sim, 0, limits)?;
    save_snapshot_records_to_writer_with_limits(&snapshot, writer, limits)
}

/// Captures the current completed tick and returns record-container bytes.
pub fn save_records_to_bytes(sim: &Simulation) -> Result<Vec<u8>, SaveLoadError> {
    save_records_to_bytes_with_limits(sim, crate::SaveLimits::default())
}

/// Captures the current completed tick with explicit limits.
pub fn save_records_to_bytes_with_limits(
    sim: &Simulation,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    let mut bytes = Vec::new();
    save_records_to_writer_with_limits(sim, &mut bytes, limits)?;
    Ok(bytes)
}

fn io_save_error(error: std::io::Error) -> SaveLoadError {
    SaveLoadError::Codec(bincode::ErrorKind::Io(error).into())
}

fn read_exact_limited(
    reader: &mut impl Read,
    len: u64,
    context: &str,
) -> Result<Vec<u8>, SaveLoadError> {
    let len = usize::try_from(len)
        .map_err(|_| record_error(format!("{context} length does not fit in memory")))?;
    let mut bytes = vec![0; len];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => record_error(format!("{context} is truncated")),
            _ => io_save_error(error),
        })?;
    Ok(bytes)
}

/// Decoded record slots: one generation under assembly.
///
/// Each payload is decoded into its slot immediately after its checksum
/// verifies, and the payload bytes are dropped before the next record reads.
/// Retained decode memory is the manifest plus decoded state, never decoded
/// state plus every encoded payload at once.
#[derive(Default)]
struct PartialSnapshot {
    core: Option<CoreTuple>,
    prototypes: Option<PrototypeCatalog>,
    chart: Option<ChartState>,
    chunk_queue: Option<ChunkGenerationQueue>,
    statistics: Option<StatisticsTuple>,
    entities: Option<EntityStore>,
    construction: Option<ConstructionState>,
    player: Option<PlayerTuple>,
    power: Option<PowerTuple>,
    fluids: Option<FluidsTuple>,
    heat: Option<HeatTuple>,
    robots: Option<RobotsTuple>,
    trains: Option<TrainsTuple>,
    environment: Option<EnvironmentTuple>,
    chunks: BTreeMap<ChunkCoord, Chunk>,
}

/// Enforces the identity codec for records this build must decode. Unknown
/// optional records skip this check: their bounds and checksums were already
/// verified, and their payloads are never decoded.
fn check_identity_codec(entry: &ManifestEntry) -> Result<(), SaveLoadError> {
    if entry.codec_id != RECORD_CODEC_IDENTITY {
        return Err(record_error(format!(
            "record {:?} uses unsupported codec {} (supported: {RECORD_CODEC_IDENTITY})",
            entry.key, entry.codec_id
        )));
    }
    if entry.decoded_len != entry.encoded_len {
        return Err(record_error(format!(
            "record {:?} declares mismatched encoded and decoded lengths for the identity codec",
            entry.key
        )));
    }
    Ok(())
}

fn check_record_schema(entry: &ManifestEntry) -> Result<(), SaveLoadError> {
    if entry.schema_version != RECORD_SCHEMA_VERSION {
        return Err(record_error(format!(
            "record {:?} uses unsupported schema version {} (supported: {RECORD_SCHEMA_VERSION})",
            entry.key, entry.schema_version
        )));
    }
    Ok(())
}

/// Decodes one verified payload into its assembly slot.
///
/// Unknown records annotated optional are skipped after their bounds and
/// checksums were verified; unknown records annotated required abort the
/// load. Chunk keys must be canonical: formatting the parsed coordinates
/// must reproduce the key, so aliases such as `chunk/01/2` cannot load
/// under a key that selective access and re-saving would not reproduce.
fn decode_into_slot(
    partial: &mut PartialSnapshot,
    entry: &ManifestEntry,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    let known = is_global_key(&entry.key) || parse_chunk_key(&entry.key).is_some();
    if known {
        check_identity_codec(entry)?;
    }
    match entry.key.as_str() {
        KEY_CORE => {
            check_record_schema(entry)?;
            partial.core = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_PROTOTYPES => {
            check_record_schema(entry)?;
            partial.prototypes = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_CHART => {
            check_record_schema(entry)?;
            partial.chart = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_CHUNK_QUEUE => {
            check_record_schema(entry)?;
            partial.chunk_queue = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_STATISTICS => {
            check_record_schema(entry)?;
            partial.statistics = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_ENTITIES => {
            check_record_schema(entry)?;
            partial.entities = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_CONSTRUCTION => {
            check_record_schema(entry)?;
            partial.construction = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_PLAYER => {
            check_record_schema(entry)?;
            partial.player = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_POWER => {
            check_record_schema(entry)?;
            partial.power = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_FLUIDS => {
            check_record_schema(entry)?;
            partial.fluids = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_HEAT => {
            check_record_schema(entry)?;
            partial.heat = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_ROBOTS => {
            check_record_schema(entry)?;
            partial.robots = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_TRAINS => {
            check_record_schema(entry)?;
            partial.trains = Some(decode_group(&entry.key, payload, limits)?);
        }
        KEY_ENVIRONMENT => {
            check_record_schema(entry)?;
            partial.environment = Some(decode_group(&entry.key, payload, limits)?);
        }
        _ => {
            if let Some(coord) = parse_chunk_key(&entry.key) {
                check_record_schema(entry)?;
                if chunk_key(coord) != entry.key {
                    return Err(record_error(format!(
                        "record {:?} is not a canonical chunk key",
                        entry.key
                    )));
                }
                let chunk: Chunk = decode_group(&entry.key, payload, limits)?;
                if chunk.coord != coord {
                    return Err(record_error(format!(
                        "record {:?} carries chunk at ({}, {})",
                        entry.key, chunk.coord.x, chunk.coord.y
                    )));
                }
                if partial.chunks.insert(coord, chunk).is_some() {
                    return Err(record_error(format!(
                        "record index carries duplicate chunk {coord:?}"
                    )));
                }
            } else if entry.required {
                return Err(record_error(format!(
                    "record {:?} is required but unknown to this build",
                    entry.key
                )));
            }
            // Unknown optional records were already bounds- and
            // checksum-verified; their payloads need no decoding.
        }
    }
    Ok(())
}

/// Assembles one complete snapshot generation from decoded slots.
///
/// Missing required records and header/core identity mismatches abort before
/// any world is built, so every record resolves to one generation.
fn assemble_snapshot(
    header: &RecordHeader,
    partial: PartialSnapshot,
) -> Result<SimulationSnapshotOwned, SaveLoadError> {
    let missing = |key: &str| {
        record_error(format!(
            "record container is missing required record {key:?}"
        ))
    };
    let core = partial.core.ok_or_else(|| missing(KEY_CORE))?;
    // Every record resolves to the single generation named by the header.
    if core.0 != header.tick || core.1 != header.world_seed {
        return Err(record_error(
            "record container mixes snapshot generations between its header and core record",
        ));
    }
    let statistics = partial.statistics.ok_or_else(|| missing(KEY_STATISTICS))?;
    let player = partial.player.ok_or_else(|| missing(KEY_PLAYER))?;
    let power = partial.power.ok_or_else(|| missing(KEY_POWER))?;
    let fluids = partial.fluids.ok_or_else(|| missing(KEY_FLUIDS))?;
    let heat = partial.heat.ok_or_else(|| missing(KEY_HEAT))?;
    let robots = partial.robots.ok_or_else(|| missing(KEY_ROBOTS))?;
    let trains = partial.trains.ok_or_else(|| missing(KEY_TRAINS))?;
    let environment = partial
        .environment
        .ok_or_else(|| missing(KEY_ENVIRONMENT))?;

    Ok(SimulationSnapshotOwned {
        tick: core.0,
        day_night_cycle: core.2,
        world_seed: core.1,
        prototypes: partial.prototypes.ok_or_else(|| missing(KEY_PROTOTYPES))?,
        chunks: partial.chunks,
        chunk_generation_queue: partial
            .chunk_queue
            .ok_or_else(|| missing(KEY_CHUNK_QUEUE))?,
        chart: partial.chart.ok_or_else(|| missing(KEY_CHART))?,
        item_statistics: statistics.0,
        fluid_statistics: statistics.1,
        power_statistics: statistics.2,
        rockets_launched: statistics.3,
        player_deaths: statistics.4,
        entities: partial.entities.ok_or_else(|| missing(KEY_ENTITIES))?,
        construction: partial
            .construction
            .ok_or_else(|| missing(KEY_CONSTRUCTION))?,
        player: player.0,
        player_equipment: player.1,
        player_weapon: player.2,
        delayed_combat: player.3,
        player_inventory: player.4,
        corpses: player.5,
        manual_mining_progress: player.6,
        crafting_queue: player.7,
        onboarding_progress: player.8,
        research: player.9,
        power_summary: power.0,
        power_networks: power.1,
        entity_power_statuses: power.2,
        fluid_networks: fluids.0,
        fluid_topology_dirty: fluids.1,
        heat_networks: heat.0,
        heat_topology_dirty: heat.1,
        robot_networks: robots.0,
        robot_logistic_work: robots.1,
        robot_flights: robots.2,
        rolling_stock: trains.0,
        pending_train_route_searches: trains.1,
        pollution: environment.0,
        enemies: environment.1,
        config: core.3,
        entity_topology_revision: core.4,
        world_chunk_revision: core.5,
        world_walkability_revision: core.6,
        transport: environment.2,
        enemy_navigation: environment.3,
        attack_targets: environment.4,
    })
}

/// Reads the manifest and decodes every record one at a time.
///
/// Each payload is bounded by the manifest, checksum-verified, decoded into
/// its assembly slot, and dropped before the next record reads, so encoded
/// payloads never accumulate. No trailing bytes may follow the packed
/// records.
fn read_records_into_snapshot(
    header: RecordHeader,
    reader: &mut impl Read,
    limits: crate::SaveLimits,
) -> Result<SimulationSnapshotOwned, SaveLoadError> {
    let manifest_bytes = read_exact_limited(reader, u64::from(header.index_len), "record index")?;
    if checksum(&manifest_bytes) != header.index_checksum {
        return Err(record_error("record index checksum mismatch"));
    }
    let entries = parse_entries(&manifest_bytes, header.record_count, limits)?;
    validate_index_order(&entries)?;
    let data_start = (RECORD_HEADER_SIZE as u64)
        .checked_add(u64::from(header.index_len))
        .ok_or_else(|| record_error("record offsets overflow"))?;
    validate_index_layout(&entries, data_start, limits)?;

    let mut partial = PartialSnapshot::default();
    for entry in &entries {
        let payload = read_exact_limited(
            reader,
            entry.encoded_len,
            &format!("record {:?}", entry.key),
        )?;
        if checksum(&payload) != entry.checksum {
            return Err(record_error(format!(
                "record {:?} checksum mismatch",
                entry.key
            )));
        }
        decode_into_slot(&mut partial, entry, &payload, limits)?;
        // The payload is dropped here, before the next record reads.
    }
    // No trailing bytes may follow the packed records.
    let mut trailing = [0; 1];
    loop {
        match reader.read(&mut trailing) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_save_error(error)),
            Ok(0) => break,
            Ok(_) => return Err(record_error("record container has trailing bytes")),
        }
    }
    assemble_snapshot(&header, partial)
}

fn finish_record_load(
    header: &RecordHeader,
    snapshot: SimulationSnapshotOwned,
) -> Result<Simulation, SaveLoadError> {
    let computed_hash = prototype_hash(&snapshot.prototypes);
    if header.prototype_hash != computed_hash {
        return Err(SaveLoadError::PrototypeHashMismatch {
            stored: header.prototype_hash,
            computed: computed_hash,
        });
    }
    let sim = snapshot.into_simulation()?;
    sim.validate_state()
        .map_err(SaveLoadError::InvalidSimulationState)?;
    Ok(sim)
}

/// Decodes a record container after its 24-byte shared prefix was consumed.
///
/// Called by the simulation loader once the magic identifies a record file.
pub(crate) fn load_after_prefix(
    prefix: &[u8; crate::SAVE_HEADER_SIZE],
    reader: &mut impl Read,
    limits: crate::SaveLimits,
) -> Result<Simulation, SaveLoadError> {
    let rest = read_exact_limited(
        reader,
        (RECORD_HEADER_SIZE - crate::SAVE_HEADER_SIZE) as u64,
        "record container header",
    )?;
    let mut full = [0; RECORD_HEADER_SIZE];
    full[..crate::SAVE_HEADER_SIZE].copy_from_slice(prefix);
    full[crate::SAVE_HEADER_SIZE..].copy_from_slice(&rest);
    load_from_full_header(&full, reader, limits)
}

fn load_from_full_header(
    header_bytes: &[u8; RECORD_HEADER_SIZE],
    reader: &mut impl Read,
    limits: crate::SaveLimits,
) -> Result<Simulation, SaveLoadError> {
    let header = parse_header(header_bytes)?;
    validate_header(&header, limits)?;
    let snapshot = read_records_into_snapshot(header.clone(), reader, limits)?;
    finish_record_load(&header, snapshot)
}

/// Inspects a record container without decoding its payloads.
///
/// Validates the header and index (including the index checksum, ordering,
/// and layout) so selective-access tools can list records without paying for
/// a full decode.
pub fn inspect_record_index(bytes: &[u8]) -> Result<RecordIndex, SaveLoadError> {
    inspect_record_index_with_limits(bytes, crate::SaveLimits::default())
}

/// Inspects a record container index with explicit limits.
pub fn inspect_record_index_with_limits(
    bytes: &[u8],
    limits: crate::SaveLimits,
) -> Result<RecordIndex, SaveLoadError> {
    if bytes.len() < RECORD_HEADER_SIZE {
        return Err(record_error("record container header is truncated"));
    }
    let header = parse_header(&bytes[..RECORD_HEADER_SIZE])?;
    validate_header(&header, limits)?;
    let manifest_end = RECORD_HEADER_SIZE
        .checked_add(header.index_len as usize)
        .ok_or_else(|| record_error("record offsets overflow"))?;
    if bytes.len() < manifest_end {
        return Err(record_error("record index is truncated"));
    }
    let manifest = &bytes[RECORD_HEADER_SIZE..manifest_end];
    if checksum(manifest) != header.index_checksum {
        return Err(record_error("record index checksum mismatch"));
    }
    let entries = parse_entries(manifest, header.record_count, limits)?;
    validate_index_order(&entries)?;
    let data_start = manifest_end as u64;
    // Inspection verifies the physical file length, not just the manifest:
    // missing or trailing payload bytes must not inspect as a valid index.
    let expected_len = validate_index_layout(&entries, data_start, limits)?;
    if bytes.len() as u64 != expected_len {
        return Err(record_error(
            "record container length does not match its index",
        ));
    }
    Ok(RecordIndex {
        save_version: header.save_version,
        prototype_format_version: header.prototype_format_version,
        prototype_hash: header.prototype_hash,
        tick: header.tick,
        world_seed: header.world_seed,
        records: entries
            .into_iter()
            .map(|entry| RecordSummary {
                key: entry.key,
                schema_version: entry.schema_version,
                codec_id: entry.codec_id,
                required: entry.required,
                encoded_len: entry.encoded_len,
                decoded_len: entry.decoded_len,
            })
            .collect(),
    })
}

/// Extracts one record payload after verifying its checksum.
///
/// This is the selective-access primitive: a single chunk or global record
/// can be read without decoding (or simulating) the rest of the world.
pub fn extract_record_bytes(bytes: &[u8], key: &str) -> Result<Vec<u8>, SaveLoadError> {
    extract_record_bytes_with_limits(bytes, key, crate::SaveLimits::default())
}

/// Extracts one record payload with explicit limits.
pub fn extract_record_bytes_with_limits(
    bytes: &[u8],
    key: &str,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    if bytes.len() < RECORD_HEADER_SIZE {
        return Err(record_error("record container header is truncated"));
    }
    let header = parse_header(&bytes[..RECORD_HEADER_SIZE])?;
    validate_header(&header, limits)?;
    let manifest_end = RECORD_HEADER_SIZE
        .checked_add(header.index_len as usize)
        .ok_or_else(|| record_error("record offsets overflow"))?;
    if bytes.len() < manifest_end {
        return Err(record_error("record index is truncated"));
    }
    let manifest = &bytes[RECORD_HEADER_SIZE..manifest_end];
    if checksum(manifest) != header.index_checksum {
        return Err(record_error("record index checksum mismatch"));
    }
    let entries = parse_entries(manifest, header.record_count, limits)?;
    validate_index_order(&entries)?;
    validate_index_layout(&entries, manifest_end as u64, limits)?;
    let entry = entries
        .iter()
        .find(|entry| entry.key == key)
        .ok_or_else(|| record_error(format!("record container has no record {key:?}")))?;
    let start = usize::try_from(entry.offset)
        .map_err(|_| record_error("record offset does not fit in memory"))?;
    let len = usize::try_from(entry.encoded_len)
        .map_err(|_| record_error("record length does not fit in memory"))?;
    let end = start
        .checked_add(len)
        .ok_or_else(|| record_error("record offsets overflow"))?;
    if bytes.len() < end {
        return Err(record_error(format!("record {key:?} is truncated")));
    }
    // Contiguous packing implies the file ends exactly after the last record.
    let last_end = entries
        .last()
        .and_then(|last| {
            usize::try_from(last.offset)
                .ok()?
                .checked_add(last.encoded_len as usize)
        })
        .ok_or_else(|| record_error("record offsets overflow"))?;
    if bytes.len() != last_end {
        return Err(record_error("record container has trailing bytes"));
    }
    let payload = &bytes[start..end];
    if checksum(payload) != entry.checksum {
        return Err(record_error(format!("record {key:?} checksum mismatch")));
    }
    Ok(payload.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SaveLimits;

    fn record_test_sim() -> Simulation {
        let mut sim = Simulation::new_test_world(123);
        for _ in 0..16 {
            sim.tick();
        }
        sim
    }

    fn parse_test_file(bytes: &[u8]) -> (RecordHeader, Vec<(ManifestEntry, Vec<u8>)>) {
        let header = parse_header(&bytes[..RECORD_HEADER_SIZE]).expect("test file has a header");
        let manifest_end = RECORD_HEADER_SIZE + header.index_len as usize;
        let manifest = &bytes[RECORD_HEADER_SIZE..manifest_end];
        assert_eq!(checksum(manifest), header.index_checksum);
        let entries = parse_entries(manifest, header.record_count, SaveLimits::default())
            .expect("index parses");
        let mut records = Vec::new();
        for entry in entries {
            let start = entry.offset as usize;
            let end = start + entry.encoded_len as usize;
            records.push((entry, bytes[start..end].to_vec()));
        }
        (header, records)
    }

    struct TestRecord {
        key: String,
        schema_version: u32,
        codec_id: u32,
        required: bool,
        offset_override: Option<u64>,
        payload: Vec<u8>,
    }

    impl TestRecord {
        fn from_parsed(entry: &ManifestEntry, payload: &[u8]) -> Self {
            Self {
                key: entry.key.clone(),
                schema_version: entry.schema_version,
                codec_id: entry.codec_id,
                required: entry.required,
                offset_override: None,
                payload: payload.to_vec(),
            }
        }
    }

    /// Rebuilds a container from manipulated records, recomputing offsets,
    /// the manifest, and all checksums so each test triggers exactly one
    /// validation layer.
    fn rebuild_test_file(
        header: &RecordHeader,
        mut records: Vec<TestRecord>,
        sort: bool,
    ) -> Vec<u8> {
        if sort {
            records.sort_by(|left, right| left.key.cmp(&right.key));
        }
        let manifest_len: usize = records.iter().map(|record| 72 + record.key.len()).sum();
        let index_len = u32::try_from(manifest_len).expect("test manifest fits");
        let data_start = RECORD_HEADER_SIZE as u64 + manifest_len as u64;
        let mut expected = data_start;
        let mut manifest = Vec::with_capacity(manifest_len);
        for record in &records {
            let offset = record.offset_override.unwrap_or(expected);
            encode_entry(
                &ManifestEntry {
                    key: record.key.clone(),
                    schema_version: record.schema_version,
                    codec_id: record.codec_id,
                    required: record.required,
                    offset,
                    encoded_len: record.payload.len() as u64,
                    decoded_len: record.payload.len() as u64,
                    checksum: checksum(&record.payload),
                },
                &mut manifest,
            );
            expected = offset + record.payload.len() as u64;
        }
        let rebuilt = RecordHeader {
            save_version: header.save_version,
            prototype_format_version: header.prototype_format_version,
            prototype_hash: header.prototype_hash,
            record_format_version: header.record_format_version,
            tick: header.tick,
            world_seed: header.world_seed,
            record_count: records.len() as u32,
            index_len,
            index_checksum: checksum(&manifest),
        };
        let mut bytes = encode_header(&rebuilt).to_vec();
        bytes.extend_from_slice(&manifest);
        for record in &records {
            bytes.extend_from_slice(&record.payload);
        }
        bytes
    }

    fn to_test_records(parsed: &[(ManifestEntry, Vec<u8>)]) -> Vec<TestRecord> {
        parsed
            .iter()
            .map(|(entry, payload)| TestRecord::from_parsed(entry, payload))
            .collect()
    }

    #[test]
    fn round_trip_preserves_state_and_continues_deterministically() {
        let mut sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).expect("record save should encode");
        let mut loaded = load_from_bytes(&bytes).expect("record save should load");
        assert_eq!(loaded.state_hash(), sim.state_hash());
        assert_eq!(loaded.tick_count(), sim.tick_count());
        loaded.validate_state().expect("loaded world validates");
        for _ in 0..8 {
            sim.tick();
            loaded.tick();
            assert_eq!(loaded.state_hash(), sim.state_hash());
        }
    }

    #[test]
    fn snapshot_and_simulation_encodings_observe_one_generation() {
        let sim = record_test_sim();
        let snapshot = capture_save_snapshot(&sim);
        assert_eq!(
            save_records_to_bytes(&sim).expect("sim encode"),
            save_snapshot_records_to_bytes(&snapshot).expect("snapshot encode"),
            "both paths encode the same completed tick"
        );
    }

    #[test]
    fn encoding_is_deterministic_and_canonically_ordered() {
        let sim = record_test_sim();
        assert_eq!(
            save_records_to_bytes(&sim).unwrap(),
            save_records_to_bytes(&sim).unwrap()
        );
        let bytes = save_records_to_bytes(&sim).unwrap();
        let index = inspect_record_index(&bytes).expect("index inspects");
        let canonical: Vec<&str> = index
            .records
            .iter()
            .map(|record| record.key.as_str())
            .collect();
        let mut sorted = canonical.clone();
        sorted.sort();
        assert_eq!(sorted, canonical, "manifest is in deterministic order");
        assert_eq!(index.records.len(), 14 + sim.world.chunks.len());
        assert!(
            index
                .records
                .iter()
                .any(|record| record.key.starts_with("chunk/")),
            "terrain is partitioned by chunk"
        );
        assert!(
            index.records.iter().any(|record| record.key == "trains"),
            "trains keep global ownership"
        );
        assert!(
            index.records.iter().any(|record| record.key == "robots"),
            "robots keep global ownership"
        );
    }

    #[test]
    fn header_prefix_shares_version_classification() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        let header = inspect_save_header(&bytes).expect("shared prefix inspects");
        assert_eq!(header.save_version, SAVE_VERSION);
        assert_eq!(header.prototype_format_version, PROTOTYPE_FORMAT_VERSION);
        let index = inspect_record_index(&bytes).expect("index inspects");
        assert_eq!(index.tick, sim.tick_count());
        assert_eq!(index.world_seed, sim.seed());
    }

    #[test]
    fn selective_access_reads_single_records_without_full_decode() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        let core_bytes = extract_record_bytes(&bytes, "core").expect("core extracts");
        let core: CoreTuple = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .deserialize(&core_bytes)
            .expect("core decodes");
        assert_eq!(core.0, sim.tick_count());
        let chunk_keys: Vec<String> = inspect_record_index(&bytes)
            .unwrap()
            .records
            .into_iter()
            .map(|record| record.key)
            .filter(|key| key.starts_with("chunk/"))
            .collect();
        assert!(!chunk_keys.is_empty());
        for key in &chunk_keys {
            let payload = extract_record_bytes(&bytes, key).expect("chunk extracts");
            let chunk: Chunk = bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .deserialize(&payload)
                .expect("chunk decodes");
            assert_eq!(&chunk_key(chunk.coord), key);
        }
        assert!(extract_record_bytes(&bytes, "no-such-record").is_err());
    }

    #[test]
    fn noncanonical_chunk_key_alias_is_rejected() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        let (header, parsed) = parse_test_file(&bytes);
        let mut records = to_test_records(&parsed);
        let chunk = records
            .iter_mut()
            .find(|record| record.key.starts_with("chunk/"))
            .expect("a chunk record exists");
        let coord = parse_chunk_key(&chunk.key).expect("test key parses");
        // Same coordinates, non-canonical spelling: selective access by the
        // canonical key would miss it and re-saving would rename it.
        chunk.key = format!("chunk/{:+}/{:03}", coord.x, coord.y);
        assert_ne!(chunk.key, chunk_key(coord));
        let rebuilt = rebuild_test_file(&header, records, true);
        assert!(matches!(
            load_from_bytes(&rebuilt),
            Err(SaveLoadError::Codec(_))
        ));
    }

    #[test]
    fn record_count_cap_rejects_hostile_manifests() {
        let header = RecordHeader {
            save_version: SAVE_VERSION,
            prototype_format_version: PROTOTYPE_FORMAT_VERSION,
            prototype_hash: 0,
            record_format_version: RECORD_FORMAT_VERSION,
            tick: 0,
            world_seed: 0,
            record_count: MAX_RECORD_COUNT + 1,
            index_len: 0,
            index_checksum: [0; 32],
        };
        assert!(matches!(
            validate_header(&header, SaveLimits::default()),
            Err(SaveLoadError::TooLarge)
        ));
    }

    #[test]
    fn per_record_budgets_allow_partitioned_worlds_above_one_record() {
        let sim = record_test_sim();
        let mono = save_to_bytes(&sim).unwrap();
        let record_bytes = save_records_to_bytes(&sim).unwrap();
        let index = inspect_record_index(&record_bytes).unwrap();
        let biggest = index
            .records
            .iter()
            .map(|record| record.encoded_len)
            .max()
            .expect("records exist");
        // The monolithic form exceeds one record's budget, but every
        // individual record fits: partitioning must stay valid.
        assert!(
            mono.len() as u64 > biggest,
            "fixture too small to separate per-record from aggregate budgets"
        );
        let limits = SaveLimits {
            max_record_bytes: biggest,
            ..SaveLimits::default()
        };
        // The monolithic capture preflight still rejects this world, while
        // the record-aware capture accepts it: partitioning decides.
        assert!(matches!(
            try_capture_save_snapshot_with_limits(&sim, 0, limits),
            Err(SaveLoadError::TooLarge)
        ));
        let snapshot = try_capture_record_snapshot_with_limits(&sim, 7, limits)
            .expect("record-aware capture accepts partitioned worlds");
        assert_eq!(snapshot.identity().world_generation, 7);
        let bytes = save_records_to_bytes_with_limits(&sim, limits).unwrap();
        let loaded = load_from_bytes_with_limits(&bytes, limits).unwrap();
        assert_eq!(loaded.state_hash(), sim.state_hash());
    }

    #[test]
    fn index_inspection_verifies_physical_file_length() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        assert!(inspect_record_index(&bytes).is_ok());
        assert!(inspect_record_index(&bytes[..bytes.len() - 10]).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(inspect_record_index(&trailing).is_err());
    }

    #[test]
    fn corrupt_index_checksum_is_rejected() {
        let sim = record_test_sim();
        let mut bytes = save_records_to_bytes(&sim).unwrap();
        bytes[RECORD_HEADER_SIZE] ^= 0xff;
        assert!(load_from_bytes(&bytes).is_err());
        assert!(inspect_record_index(&bytes).is_err());
    }

    #[test]
    fn corrupt_record_checksum_is_rejected() {
        let sim = record_test_sim();
        let mut bytes = save_records_to_bytes(&sim).unwrap();
        let (_, records) = parse_test_file(&bytes);
        let (entry, _) = records.first().expect("records exist");
        let target = entry.offset as usize;
        bytes[target] ^= 0xff;
        assert!(matches!(
            load_from_bytes(&bytes),
            Err(SaveLoadError::Codec(_))
        ));
    }

    #[test]
    fn unknown_required_record_is_rejected() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        let (header, parsed) = parse_test_file(&bytes);
        let mut records = to_test_records(&parsed);
        records.push(TestRecord {
            key: "zzz-future-system".into(),
            schema_version: 1,
            codec_id: RECORD_CODEC_IDENTITY,
            required: true,
            offset_override: None,
            payload: b"future".to_vec(),
        });
        let rebuilt = rebuild_test_file(&header, records, true);
        let result = load_from_bytes(&rebuilt);
        assert!(
            matches!(result, Err(SaveLoadError::Codec(_))),
            "unknown required record must abort the load, got {result:?}"
        );
    }

    #[test]
    fn unknown_optional_record_is_skipped() {
        let mut sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        let (header, parsed) = parse_test_file(&bytes);
        let mut records = to_test_records(&parsed);
        records.push(TestRecord {
            key: "zzz-future-optional".into(),
            schema_version: 99,
            codec_id: 99,
            required: false,
            offset_override: None,
            payload: b"future".to_vec(),
        });
        let rebuilt = rebuild_test_file(&header, records, true);
        let mut loaded = load_from_bytes(&rebuilt).expect("optional record skips");
        assert_eq!(loaded.state_hash(), sim.state_hash());
        sim.tick();
        loaded.tick();
        assert_eq!(loaded.state_hash(), sim.state_hash());
    }

    #[test]
    fn unsupported_codec_is_rejected() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        let (header, parsed) = parse_test_file(&bytes);
        let mut records = to_test_records(&parsed);
        let core = records
            .iter_mut()
            .find(|record| record.key == "core")
            .expect("core record exists");
        core.codec_id = 7;
        let rebuilt = rebuild_test_file(&header, records, true);
        assert!(matches!(
            load_from_bytes(&rebuilt),
            Err(SaveLoadError::Codec(_))
        ));
    }

    #[test]
    fn truncation_at_any_stage_is_rejected() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        let (_, parsed) = parse_test_file(&bytes);
        let first_offset = parsed
            .first()
            .map(|(entry, _)| entry.offset as usize)
            .unwrap_or(0);
        for cutoff in [
            10,
            RECORD_HEADER_SIZE - 1,
            RECORD_HEADER_SIZE + 3,
            first_offset,
            first_offset + 1,
            bytes.len() - 1,
        ] {
            assert!(
                load_from_bytes(&bytes[..cutoff]).is_err(),
                "truncation at {cutoff} must fail"
            );
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let sim = record_test_sim();
        let mut bytes = save_records_to_bytes(&sim).unwrap();
        bytes.push(0);
        assert!(matches!(
            load_from_bytes(&bytes),
            Err(SaveLoadError::Codec(_))
        ));
    }

    #[test]
    fn unsupported_container_version_is_rejected() {
        let sim = record_test_sim();
        let mut bytes = save_records_to_bytes(&sim).unwrap();
        bytes[24..28].copy_from_slice(&(RECORD_FORMAT_VERSION + 1).to_le_bytes());
        assert!(load_from_bytes(&bytes).is_err());
    }

    #[test]
    fn historical_save_version_has_no_record_encoding() {
        let sim = record_test_sim();
        let mut bytes = save_records_to_bytes(&sim).unwrap();
        bytes[8..12].copy_from_slice(&57u32.to_le_bytes());
        assert!(matches!(
            load_from_bytes(&bytes),
            Err(SaveLoadError::UnsupportedSaveVersion { found: 57, .. })
        ));
    }

    #[test]
    fn duplicate_keys_are_rejected() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        let (header, parsed) = parse_test_file(&bytes);
        let mut records = to_test_records(&parsed);
        records.push(TestRecord::from_parsed(&parsed[0].0, &parsed[0].1));
        let rebuilt = rebuild_test_file(&header, records, true);
        assert!(load_from_bytes(&rebuilt).is_err());
    }

    #[test]
    fn overlapping_offsets_are_rejected() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        let (header, parsed) = parse_test_file(&bytes);
        let mut records = to_test_records(&parsed);
        records[1].offset_override = Some(parsed[0].0.offset);
        let rebuilt = rebuild_test_file(&header, records, false);
        assert!(load_from_bytes(&rebuilt).is_err());
    }

    #[test]
    fn shuffled_manifest_order_is_rejected() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        let (header, parsed) = parse_test_file(&bytes);
        let mut records = to_test_records(&parsed);
        records.swap(0, 1);
        let rebuilt = rebuild_test_file(&header, records, false);
        assert!(load_from_bytes(&rebuilt).is_err());
    }

    #[test]
    fn chunk_key_mismatch_is_rejected() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        let (header, parsed) = parse_test_file(&bytes);
        let mut records = to_test_records(&parsed);
        let chunk = records
            .iter_mut()
            .find(|record| record.key.starts_with("chunk/"))
            .expect("a chunk record exists");
        chunk.key = "chunk/99999/99999".into();
        let rebuilt = rebuild_test_file(&header, records, true);
        assert!(load_from_bytes(&rebuilt).is_err());
    }

    #[test]
    fn generation_mismatch_between_header_and_core_is_rejected() {
        let sim = record_test_sim();
        let mut bytes = save_records_to_bytes(&sim).unwrap();
        let tick = u64::from_le_bytes(bytes[28..36].try_into().unwrap());
        bytes[28..36].copy_from_slice(&(tick + 1).to_le_bytes());
        assert!(load_from_bytes(&bytes).is_err());
    }

    #[test]
    fn missing_required_record_is_rejected() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        let (header, parsed) = parse_test_file(&bytes);
        let records: Vec<TestRecord> = to_test_records(&parsed)
            .into_iter()
            .filter(|record| record.key != "core")
            .collect();
        let rebuilt = rebuild_test_file(&header, records, true);
        assert!(load_from_bytes(&rebuilt).is_err());
    }

    #[test]
    fn record_budgets_bind_encoding_and_decoding() {
        let sim = record_test_sim();
        let tight = SaveLimits {
            max_record_bytes: 1,
            ..SaveLimits::default()
        };
        assert!(matches!(
            save_records_to_bytes_with_limits(&sim, tight),
            Err(SaveLoadError::TooLarge)
        ));
        let bytes = save_records_to_bytes(&sim).unwrap();
        assert!(matches!(
            load_from_bytes_with_limits(&bytes, tight),
            Err(SaveLoadError::TooLarge)
        ));
        let tiny_decoded = SaveLimits {
            max_decoded_bytes: 16,
            ..SaveLimits::default()
        };
        assert!(matches!(
            save_records_to_bytes_with_limits(&sim, tiny_decoded),
            Err(SaveLoadError::TooLarge)
        ));
    }

    #[test]
    #[ignore = "fixture regeneration is an explicit maintainer action"]
    fn regenerate_record_fixture() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).expect("fixture encodes");
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures");
        std::fs::create_dir_all(&directory).expect("fixture dir");
        std::fs::write(directory.join("save-record-v1-testworld.factrec"), bytes)
            .expect("fixture writes");
    }

    #[test]
    fn record_fixture_loads_and_validates() {
        let bytes = include_bytes!("../../tests/fixtures/save-record-v1-testworld.factrec");
        let index = inspect_record_index(bytes).expect("fixture index inspects");
        assert_eq!(index.save_version, SAVE_VERSION);
        assert_eq!(index.prototype_format_version, PROTOTYPE_FORMAT_VERSION);
        let expected = record_test_sim();
        assert_eq!(index.tick, expected.tick_count());
        assert_eq!(index.world_seed, expected.seed());
        let loaded = load_from_bytes(bytes).expect("fixture loads");
        loaded.validate_state().expect("fixture validates");
        assert_eq!(loaded.state_hash(), expected.state_hash());
        assert_eq!(loaded.tick_count(), expected.tick_count());
    }
}
