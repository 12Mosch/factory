use super::MachineModuleState;
use crate::ids::EntityId;
use crate::inventory::Inventory;
use factory_data::ItemId;
use serde::{Deserialize, Serialize};

/// A rocket silo mid-build.
///
/// The crafting half is an assembler's — ingredients in an input inventory,
/// progress against a required tick count, a speed fraction scaled by modules —
/// and the silo shares that machinery rather than restating it. What is its own
/// is the two fields below the inventory: a silo has no output slot, so a
/// finished part raises `parts_completed`, and at `parts_per_rocket` the rocket
/// is whole. There is no separate "rocket ready" flag because there is nothing
/// it could disagree with: the counter reaching the target *is* the rocket.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RocketSiloState {
    pub modules: MachineModuleState,
    pub input_inventory: Inventory,
    pub crafting_progress_ticks: u32,
    pub crafting_required_ticks: u32,
    pub crafting_speed_numerator: u32,
    pub crafting_speed_denominator: u32,
    /// Parts built toward the rocket standing in the silo, never above
    /// `parts_per_rocket`.
    pub parts_completed: u32,
    /// Parts that make one whole rocket, copied from the prototype at placement
    /// so a silo already holding a half-built rocket keeps counting to the
    /// target it started against.
    pub parts_per_rocket: u32,
}

impl RocketSiloState {
    /// Whether a whole rocket stands in the silo.
    ///
    /// While this holds the silo builds nothing further: the rocket occupies the
    /// pad, and the parts for the next one have nowhere to go until it leaves.
    pub fn rocket_ready(&self) -> bool {
        self.parts_completed >= self.parts_per_rocket
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RocketSiloError {
    MissingEntity(EntityId),
    NotRocketSilo(EntityId),
    /// The item is not an ingredient of the silo's part recipe, so the silo has
    /// no use for it and refuses it rather than filling its slots with it.
    InvalidInput(ItemId),
    InvalidSlot {
        slot_index: usize,
    },
    EmptySlot {
        slot_index: usize,
    },
    InsufficientSpace,
    UnknownItem,
}
