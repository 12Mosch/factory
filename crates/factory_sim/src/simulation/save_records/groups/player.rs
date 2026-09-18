//! Player record: player state plus everything that travels with the
//! player across a save — equipment, weapon, combat, inventory, corpses,
//! mining, crafting, onboarding, and research.

use super::super::super::*;
use super::super::codec::*;
use super::super::registry::KEY_PLAYER;
use super::{BorrowedRecordFields, PartialSnapshot, RecordHandler};

pub(super) type PlayerTuple = (
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

#[allow(clippy::type_complexity)]
fn tuple<'a>(
    fields: &'a BorrowedRecordFields<'a>,
) -> (
    &'a PlayerState,
    &'a PlayerEquipmentState,
    &'a PlayerWeaponState,
    &'a DelayedCombatState,
    &'a Inventory,
    &'a BTreeMap<u64, PlayerCorpse>,
    &'a Option<ManualMiningProgress>,
    &'a CraftingQueue,
    &'a OnboardingProgress,
    &'a ResearchState,
) {
    (
        fields.player,
        fields.player_equipment,
        fields.player_weapon,
        fields.delayed_combat,
        fields.player_inventory,
        fields.corpses,
        fields.manual_mining_progress,
        fields.crafting_queue,
        fields.onboarding_progress,
        fields.research,
    )
}

pub(super) fn encode(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    encode_group(&tuple(fields), limits)
}

pub(super) fn measure(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    super::measure_tuple(&tuple(fields), limits)
}

pub(super) fn decode(
    partial: &mut PartialSnapshot,
    key: &str,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    partial.player = Some(decode_group(key, payload, limits)?);
    Ok(())
}

/// This record's table row: the stable key travels with its own handlers,
/// so no assembly site can pair a key with another subsystem's codecs.
pub(super) const HANDLER: RecordHandler = RecordHandler {
    key: KEY_PLAYER,
    required: true,
    encode,
    measure,
    decode,
};
