use super::{SaveId, SaveKind, SaveMetadata};
use factory_sim::SAVE_HEADER_SIZE;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::{fs, str};

pub const CONTAINER_MAGIC: [u8; 8] = *b"FACTSAVE";
pub const CONTAINER_VERSION: u32 = 1;
pub const METADATA_SCHEMA_VERSION: u32 = 1;
pub const MAX_METADATA_BYTES: usize = 16 * 1024;
const PREFIX_SIZE: usize = 16;

#[derive(Debug)]
pub enum ContainerError {
    Io(io::Error),
    MetadataTooLarge(usize),
    MetadataEncoding(String),
    Truncated,
    InvalidContainerMagic,
}

impl std::fmt::Display for ContainerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::MetadataTooLarge(size) => write!(
                formatter,
                "metadata is {size} bytes (maximum is {MAX_METADATA_BYTES})"
            ),
            Self::MetadataEncoding(error) => write!(formatter, "metadata encoding failed: {error}"),
            Self::Truncated => write!(formatter, "save container is truncated"),
            Self::InvalidContainerMagic => write!(formatter, "invalid save container magic"),
        }
    }
}

impl From<io::Error> for ContainerError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
pub(crate) struct InspectedContainer {
    pub version: u32,
    pub metadata: Option<SaveMetadata>,
    pub simulation_header: Vec<u8>,
}

pub fn encode_container(
    metadata: &SaveMetadata,
    payload: &[u8],
) -> Result<Vec<u8>, ContainerError> {
    let metadata_text = ron::ser::to_string(metadata)
        .map_err(|error| ContainerError::MetadataEncoding(error.to_string()))?;
    let metadata_bytes = metadata_text.as_bytes();
    if metadata_bytes.len() > MAX_METADATA_BYTES {
        return Err(ContainerError::MetadataTooLarge(metadata_bytes.len()));
    }
    let metadata_len = u32::try_from(metadata_bytes.len())
        .map_err(|_| ContainerError::MetadataTooLarge(metadata_bytes.len()))?;
    let mut bytes = Vec::with_capacity(PREFIX_SIZE + metadata_bytes.len() + payload.len());
    bytes.extend_from_slice(&CONTAINER_MAGIC);
    bytes.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
    bytes.extend_from_slice(&metadata_len.to_le_bytes());
    bytes.extend_from_slice(metadata_bytes);
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

pub fn decode_container(bytes: &[u8]) -> Result<(SaveMetadata, &[u8]), ContainerError> {
    if bytes.len() < PREFIX_SIZE {
        return Err(ContainerError::Truncated);
    }
    if bytes[..8] != CONTAINER_MAGIC {
        return Err(ContainerError::InvalidContainerMagic);
    }
    let metadata_len = u32::from_le_bytes(bytes[12..16].try_into().expect("fixed range")) as usize;
    if metadata_len > MAX_METADATA_BYTES {
        return Err(ContainerError::MetadataTooLarge(metadata_len));
    }
    let payload_offset = PREFIX_SIZE
        .checked_add(metadata_len)
        .ok_or(ContainerError::Truncated)?;
    if bytes.len() < payload_offset {
        return Err(ContainerError::Truncated);
    }
    let metadata = ron::de::from_bytes(&bytes[PREFIX_SIZE..payload_offset])
        .map_err(|error| ContainerError::MetadataEncoding(error.to_string()))?;
    Ok((metadata, &bytes[payload_offset..]))
}

pub(crate) fn inspect_container(path: &Path) -> Result<InspectedContainer, ContainerError> {
    let mut file = fs::File::open(path)?;
    let mut prefix = [0; PREFIX_SIZE];
    file.read_exact(&mut prefix).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            ContainerError::Truncated
        } else {
            error.into()
        }
    })?;
    if prefix[..8] != CONTAINER_MAGIC {
        return Err(ContainerError::InvalidContainerMagic);
    }
    let version = u32::from_le_bytes(prefix[8..12].try_into().expect("fixed range"));
    let metadata_len = u32::from_le_bytes(prefix[12..16].try_into().expect("fixed range")) as usize;
    if metadata_len > MAX_METADATA_BYTES {
        return Err(ContainerError::MetadataTooLarge(metadata_len));
    }
    let mut metadata_bytes = vec![0; metadata_len];
    file.read_exact(&mut metadata_bytes)
        .map_err(|_| ContainerError::Truncated)?;
    let metadata = ron::de::from_bytes(&metadata_bytes).ok();
    let mut simulation_header = vec![0; SAVE_HEADER_SIZE];
    file.read_exact(&mut simulation_header)
        .map_err(|_| ContainerError::Truncated)?;
    Ok(InspectedContainer {
        version,
        metadata,
        simulation_header,
    })
}

pub(crate) fn read_simulation_payload(path: &Path) -> Result<Vec<u8>, ContainerError> {
    let bytes = fs::read(path)?;
    if bytes.starts_with(&CONTAINER_MAGIC) {
        if bytes.len() < PREFIX_SIZE {
            return Err(ContainerError::Truncated);
        }
        let metadata_len =
            u32::from_le_bytes(bytes[12..16].try_into().expect("fixed range")) as usize;
        if metadata_len > MAX_METADATA_BYTES {
            return Err(ContainerError::MetadataTooLarge(metadata_len));
        }
        let payload_offset = PREFIX_SIZE
            .checked_add(metadata_len)
            .ok_or(ContainerError::Truncated)?;
        bytes
            .get(payload_offset..)
            .map(<[u8]>::to_vec)
            .ok_or(ContainerError::Truncated)
    } else {
        Ok(bytes)
    }
}

pub(crate) fn write_save_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_path = sibling_with_suffix(path, "tmp");
    let backup_path = sibling_with_suffix(path, "bak");
    let _ = fs::remove_file(&temp_path);
    fs::write(&temp_path, bytes)?;
    if !path.exists() {
        return fs::rename(&temp_path, path);
    }
    let _ = fs::remove_file(&backup_path);
    fs::rename(path, &backup_path)?;
    match fs::rename(&temp_path, path) {
        Ok(()) => {
            let _ = fs::remove_file(&backup_path);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(&backup_path, path);
            Err(error)
        }
    }
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("save.factsim");
    path.with_file_name(format!("{file_name}.{suffix}-{}", std::process::id()))
}

pub(crate) fn fallback_metadata(
    id: SaveId,
    kind: SaveKind,
    display_name: String,
    timestamp: u64,
) -> SaveMetadata {
    SaveMetadata {
        schema_version: METADATA_SCHEMA_VERSION,
        id,
        display_name,
        kind,
        completed_at_unix_ms: timestamp,
        application_version: env!("CARGO_PKG_VERSION").into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::{Simulation, load_from_bytes, save_to_bytes};

    fn metadata(name: &str) -> SaveMetadata {
        fallback_metadata(SaveId::new("test"), SaveKind::Named, name.into(), 42)
    }

    #[test]
    fn metadata_and_payload_round_trip() {
        let metadata = metadata("Iron Works");
        let bytes = encode_container(&metadata, b"FACTSIM\0payload").unwrap();
        let (decoded, payload) = decode_container(&bytes).unwrap();
        assert_eq!(decoded, metadata);
        assert_eq!(payload, b"FACTSIM\0payload");
    }

    #[test]
    fn metadata_limit_is_enforced() {
        let error =
            encode_container(&metadata(&"x".repeat(MAX_METADATA_BYTES)), b"payload").unwrap_err();
        assert!(matches!(error, ContainerError::MetadataTooLarge(_)));
    }

    #[test]
    fn simulation_payload_round_trip_preserves_tick_and_state_hash() {
        let mut simulation = Simulation::new_test_world(77);
        for _ in 0..12 {
            simulation.tick();
        }
        let expected = (simulation.tick_count(), simulation.state_hash());
        let payload = save_to_bytes(&simulation).unwrap();
        let bytes = encode_container(&metadata("Round Trip"), &payload).unwrap();
        let (_, payload) = decode_container(&bytes).unwrap();
        let loaded = load_from_bytes(payload).unwrap();
        assert_eq!((loaded.tick_count(), loaded.state_hash()), expected);
    }

    #[test]
    fn atomic_writer_creates_and_replaces_one_file() {
        let root = std::env::temp_dir().join(format!(
            "factory-container-atomic-{}-{}",
            std::process::id(),
            crate::save_load::catalog::now_unix_ms()
        ));
        let path = root.join("manual-test.factsim");
        write_save_bytes(&path, b"first").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"first");
        write_save_bytes(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
