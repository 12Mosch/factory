use super::SaveCompatibility;
use factory_sim::{
    OLDEST_SUPPORTED_SAVE_VERSION, PROTOTYPE_FORMAT_VERSION, SAVE_VERSION, SaveHeaderInfo,
    is_save_version_migratable,
};

pub(crate) fn classify_header(
    header: SaveHeaderInfo,
    current_prototype_hash: u64,
) -> SaveCompatibility {
    if header.save_version < OLDEST_SUPPORTED_SAVE_VERSION {
        SaveCompatibility::SaveFormatOlder {
            found: header.save_version,
            supported: OLDEST_SUPPORTED_SAVE_VERSION,
        }
    } else if header.save_version > SAVE_VERSION {
        SaveCompatibility::SaveFormatNewer {
            found: header.save_version,
            supported: SAVE_VERSION,
        }
    } else if header.prototype_format_version < PROTOTYPE_FORMAT_VERSION {
        SaveCompatibility::PrototypeFormatOlder {
            found: header.prototype_format_version,
            supported: PROTOTYPE_FORMAT_VERSION,
        }
    } else if header.prototype_format_version > PROTOTYPE_FORMAT_VERSION {
        SaveCompatibility::PrototypeFormatNewer {
            found: header.prototype_format_version,
            supported: PROTOTYPE_FORMAT_VERSION,
        }
    } else if header.prototype_hash != current_prototype_hash {
        SaveCompatibility::PrototypeHashMismatch
    } else if is_save_version_migratable(header.save_version) {
        SaveCompatibility::MigratableSaveFormat {
            found: header.save_version,
            current: SAVE_VERSION,
        }
    } else {
        SaveCompatibility::Compatible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(save: u32, prototypes: u32, hash: u64) -> SaveHeaderInfo {
        SaveHeaderInfo {
            save_version: save,
            prototype_format_version: prototypes,
            prototype_hash: hash,
        }
    }

    #[test]
    fn version_messages_are_direction_specific() {
        let migratable = classify_header(
            header(OLDEST_SUPPORTED_SAVE_VERSION, PROTOTYPE_FORMAT_VERSION, 1),
            1,
        );
        let older = classify_header(
            header(
                OLDEST_SUPPORTED_SAVE_VERSION - 1,
                PROTOTYPE_FORMAT_VERSION,
                1,
            ),
            1,
        );
        let newer = classify_header(header(SAVE_VERSION + 1, PROTOTYPE_FORMAT_VERSION, 1), 1);
        assert!(migratable.can_load());
        assert_eq!(migratable.short_label(), "Migratable");
        assert!(older.reason().unwrap().contains("oldest supported"));
        assert!(newer.reason().unwrap().contains("newer build"));
    }
}
