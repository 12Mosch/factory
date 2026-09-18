//! Record-container framing: fixed header, manifest/index, and checksums.
//!
//! This module owns the byte layout only. Which records exist and who owns
//! them is declared in [`super::registry`]; what each record carries is
//! assembled in [`super::groups`].

use super::super::*;
use super::registry::*;
use bincode::Options;

/// Fixed record-container header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecordHeader {
    pub(crate) save_version: u32,
    pub(crate) prototype_format_version: u32,
    pub(crate) prototype_hash: u64,
    pub(crate) record_format_version: u32,
    pub(crate) tick: u64,
    pub(crate) world_seed: u64,
    pub(crate) record_count: u32,
    pub(crate) index_len: u32,
    pub(crate) index_checksum: [u8; 32],
}

/// One manifest entry: the index row describing a single record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ManifestEntry {
    pub(crate) key: String,
    pub(crate) schema_version: u32,
    pub(crate) codec_id: u32,
    pub(crate) required: bool,
    pub(crate) offset: u64,
    pub(crate) encoded_len: u64,
    pub(crate) decoded_len: u64,
    pub(crate) checksum: [u8; 32],
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

pub(crate) fn encode_header(header: &RecordHeader) -> [u8; RECORD_HEADER_SIZE] {
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

pub(crate) fn parse_header(bytes: &[u8]) -> Result<RecordHeader, SaveLoadError> {
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

pub(crate) fn validate_header(
    header: &RecordHeader,
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
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

pub(crate) fn encode_entry(entry: &ManifestEntry, out: &mut Vec<u8>) {
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

/// Framing bytes per manifest entry besides the key: length prefix,
/// schema/codec/flags, offsets/lengths, and checksum.
pub(crate) const ENTRY_FRAMING_BYTES: u64 = 4 + 4 + 4 + 4 + 8 + 8 + 8 + 32;

pub(crate) fn parse_entries(
    manifest: &[u8],
    expected: u32,
    limits: crate::SaveLimits,
) -> Result<Vec<ManifestEntry>, SaveLoadError> {
    // Each entry is `ENTRY_FRAMING_BYTES + key_len` with `1..=96` key bytes,
    // so a manifest for `expected` records can only be this large. Rejecting
    // impossible size/count combinations up front keeps a hostile manifest
    // from driving allocation before a single entry parses.
    let min_len = u64::from(expected).saturating_mul(ENTRY_FRAMING_BYTES + 1);
    let max_len =
        u64::from(expected).saturating_mul(ENTRY_FRAMING_BYTES + MAX_RECORD_KEY_BYTES as u64);
    if (manifest.len() as u64) < min_len || (manifest.len() as u64) > max_len {
        return Err(record_error(format!(
            "record index of {} bytes cannot hold {expected} records",
            manifest.len()
        )));
    }
    // Capacity follows the declared count (itself capped by the header
    // check), and parsing stops the moment entries exceed it.
    let capacity = usize::try_from(expected.min(MAX_RECORD_COUNT)).unwrap_or(usize::MAX);
    let mut entries = Vec::with_capacity(capacity);
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
        // Stop the moment parsing would exceed the declared count instead of
        // allocating the whole manifest first.
        if entries.len() as u64 > u64::from(expected) {
            return Err(record_error(format!(
                "record index declares {expected} records but carries more"
            )));
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
pub(crate) fn validate_index_order(entries: &[ManifestEntry]) -> Result<(), SaveLoadError> {
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
pub(crate) fn validate_index_layout(
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

pub(crate) fn encode_group(
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

pub(crate) fn decode_group<T: serde::de::DeserializeOwned>(
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

pub(crate) fn checksum(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

pub(crate) fn read_exact_limited(
    reader: &mut impl std::io::Read,
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
            _ => SaveLoadError::Codec(bincode::ErrorKind::Io(error).into()),
        })?;
    Ok(bytes)
}
