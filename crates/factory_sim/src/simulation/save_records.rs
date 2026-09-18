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

use super::save::{
    PROTOTYPE_FORMAT_VERSION, SAVE_VERSION, SaveLoadError, SimulationSaveSnapshot,
    SimulationSnapshotOwned, capture_save_snapshot_in_generation,
};
use super::*;
use std::io::{Read, Write};

#[cfg(test)]
use bincode::Options;

pub(crate) mod codec;
pub(crate) mod groups;
pub(crate) mod registry;

pub(crate) use codec::{
    ManifestEntry, RecordHeader, checksum, encode_entry, encode_header, parse_entries,
    parse_header, read_exact_limited, validate_header, validate_index_layout, validate_index_order,
};
pub use codec::{RecordIndex, RecordSummary};
use groups::{
    BorrowedRecordFields, PartialSnapshot, assemble_snapshot, decode_into_slot,
    encode_group_by_key, preflight_record_sizes, record_required,
};
pub use registry::{
    MAX_RECORD_COUNT, RECORD_CODEC_IDENTITY, RECORD_FORMAT_VERSION, RECORD_HEADER_SIZE,
    RECORD_MAGIC,
};
pub(crate) use registry::{RECORD_SCHEMA_VERSION, record_error};
#[cfg(test)]
pub(crate) use registry::{chunk_key, parse_chunk_key};

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
    let fields = BorrowedRecordFields::from_snapshot(state);
    let keys = fields.keys();
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
                required: record_required(&keys[index]),
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
            required: record_required(&keys[index]),
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
    preflight_record_sizes(&BorrowedRecordFields::from_simulation(sim), limits)?;
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

/// Extracts one record payload from a stream with bounded memory.
///
/// Only the header, manifest, and the target payload are retained: records
/// after the target are never touched, and no trailing-length check runs.
/// Preceding payload bytes are still read and discarded through the `Read`
/// interface (there is deliberately no `Seek` API yet), so this saves
/// memory, not I/O. The manifest and the payload checksum are still fully
/// verified.
pub fn extract_record_from_reader(
    reader: &mut impl Read,
    key: &str,
) -> Result<Vec<u8>, SaveLoadError> {
    extract_record_from_reader_with_limits(reader, key, crate::SaveLimits::default())
}

/// Extracts one record payload from a stream with explicit limits.
pub fn extract_record_from_reader_with_limits(
    reader: &mut impl Read,
    key: &str,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    let header_bytes = read_exact_limited(reader, RECORD_HEADER_SIZE as u64, "record header")?;
    let header = parse_header(&header_bytes)?;
    validate_header(&header, limits)?;
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
    let entry = entries
        .iter()
        .find(|entry| entry.key == key)
        .ok_or_else(|| record_error(format!("record container has no record {key:?}")))?;
    // Discard every byte before the target; records are packed contiguously
    // in manifest order, so this never skips backwards.
    let mut skip = entry
        .offset
        .checked_sub(data_start)
        .ok_or_else(|| record_error("record offsets overflow"))?;
    let mut discard = [0; 8192];
    while skip > 0 {
        let count = usize::try_from(skip.min(discard.len() as u64))
            .expect("discard length is bounded by the buffer length");
        match reader.read(&mut discard[..count]) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_save_error(error)),
            Ok(0) => return Err(record_error(format!("record {key:?} is truncated"))),
            Ok(read) => skip -= read as u64,
        }
    }
    let payload = read_exact_limited(reader, entry.encoded_len, &format!("record {key:?}"))?;
    if checksum(&payload) != entry.checksum {
        return Err(record_error(format!("record {key:?} checksum mismatch")));
    }
    Ok(payload)
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
        let tick = super::groups::decoded_core_tick(&core_bytes, SaveLimits::default())
            .expect("core decodes");
        assert_eq!(tick, sim.tick_count());
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
    fn manifest_required_flags_follow_registry_descriptors() {
        // The `required` flag on the wire must come from the handler table,
        // not from a writer-side literal: every global entry carries its
        // descriptor's flag, and every chunk entry is required.
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).expect("record save should encode");
        let index = inspect_record_index(&bytes).expect("index inspects");
        assert!(!index.records.is_empty());
        for record in &index.records {
            if let Some(handler) = super::groups::find_handler(&record.key) {
                assert_eq!(
                    record.required, handler.required,
                    "wire flag must follow {:?}",
                    record.key
                );
            } else {
                assert!(
                    record.key.starts_with("chunk/"),
                    "unexpected global record {:?}",
                    record.key
                );
                assert!(record.required, "chunk records are required");
            }
        }
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
    fn impossible_manifest_size_is_rejected_up_front() {
        let sim = record_test_sim();
        let mut bytes = save_records_to_bytes(&sim).unwrap();
        // Declare one record while keeping the full manifest: no
        // multi-kilobyte index can hold a single entry, so parsing must
        // stop before allocating.
        bytes[44..48].copy_from_slice(&1u32.to_le_bytes());
        assert!(matches!(
            load_from_bytes(&bytes),
            Err(SaveLoadError::Codec(_))
        ));
        assert!(inspect_record_index(&bytes).is_err());
    }

    #[test]
    fn parse_stops_once_entries_exceed_declared_count() {
        // Two minimal entries (73 bytes each) fit the size window for a
        // declared count of one, so the overrun must stop parsing itself.
        let mut manifest = Vec::new();
        for key in ["a", "b"] {
            encode_entry(
                &ManifestEntry {
                    key: key.into(),
                    schema_version: RECORD_SCHEMA_VERSION,
                    codec_id: RECORD_CODEC_IDENTITY,
                    required: true,
                    offset: 0,
                    encoded_len: 0,
                    decoded_len: 0,
                    checksum: [0; 32],
                },
                &mut manifest,
            );
        }
        assert_eq!(manifest.len(), 2 * 73);
        assert!(matches!(
            parse_entries(&manifest, 1, SaveLimits::default()),
            Err(SaveLoadError::Codec(_))
        ));
        assert_eq!(
            parse_entries(&manifest, 2, SaveLimits::default())
                .expect("declared pair parses")
                .len(),
            2
        );
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
    fn streaming_extract_reads_one_record_without_full_file() {
        let sim = record_test_sim();
        let bytes = save_records_to_bytes(&sim).unwrap();
        // Full-file equivalence with the slice API.
        let via_reader =
            extract_record_from_reader(&mut &bytes[..], "core").expect("core extracts");
        assert_eq!(via_reader, extract_record_bytes(&bytes, "core").unwrap());
        // A file truncated right after the target record still yields it:
        // selective tools never retain the whole save.
        let (_, parsed) = parse_test_file(&bytes);
        let (entry, _) = parsed
            .iter()
            .find(|(entry, _)| entry.key == "core")
            .expect("core exists");
        let end = entry.offset as usize + entry.encoded_len as usize;
        let partial = &bytes[..end];
        assert!(load_from_bytes(partial).is_err());
        let got = extract_record_from_reader(&mut &partial[..], "core").expect("prefix extracts");
        assert_eq!(got, via_reader);
        assert!(extract_record_from_reader(&mut &bytes[..], "no-such-record").is_err());
        let mut empty = &[][..];
        assert!(extract_record_from_reader(&mut empty, "core").is_err());
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
    fn record_capture_enforces_byte_budgets() {
        let sim = record_test_sim();
        let tight = SaveLimits {
            max_record_bytes: 1,
            ..SaveLimits::default()
        };
        assert!(matches!(
            try_capture_record_snapshot_with_limits(&sim, 0, tight),
            Err(SaveLoadError::TooLarge)
        ));
        let snapshot = try_capture_record_snapshot_with_limits(&sim, 4, SaveLimits::default())
            .expect("adequate budgets capture");
        assert_eq!(snapshot.identity().world_generation, 4);
        assert_eq!(snapshot.tick_count(), sim.tick_count());
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
