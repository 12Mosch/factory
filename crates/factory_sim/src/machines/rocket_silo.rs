use super::MachineModuleState;
use super::MachineStatus;
use crate::ids::EntityId;
use crate::inventory::Inventory;
use factory_data::ItemId;
use serde::{Deserialize, Serialize};

pub(crate) const LAUNCH_SEAL_TICKS: u16 = 60;
pub(crate) const LAUNCH_RISE_TICKS: u16 = 120;

/// A rocket silo mid-build.
///
/// The crafting half is an assembler's — ingredients in an input inventory,
/// progress against a required tick count, a speed fraction scaled by modules —
/// and the silo shares that machinery rather than restating it. What is its own
/// is the part counter: a finished part is not an output item, so it raises
/// `parts_completed`, and at `parts_per_rocket` the rocket is whole. There is no
/// separate "rocket ready" flag because there is nothing
/// it could disagree with: the counter reaching the target *is* the rocket.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RocketSiloState {
    pub modules: MachineModuleState,
    pub input_inventory: Inventory,
    /// The payload carried by the completed rocket. Launch cargo is deliberately
    /// separate from part ingredients so every transfer path can route items to
    /// the correct one-slot holder.
    pub cargo_inventory: Inventory,
    /// Products returned by completed launches. This is output-only so launch
    /// rewards cannot be confused with either part ingredients or cargo.
    pub output_inventory: Inventory,
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
    /// Durable launch progress, advanced by the fixed simulation tick.
    pub launch_phase: RocketLaunchPhase,
}

/// Simulation-owned rocket launch animation state.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum RocketLaunchPhase {
    #[default]
    Idle,
    Sealed {
        ticks_remaining: u16,
    },
    Rising {
        ticks_remaining: u16,
    },
}

/// Player-facing operating state derived from a rocket silo's durable state.
///
/// This stays separate from [`MachineStatus`]: cargo and launch phases only
/// make sense for a silo, while the generic status remains useful for shared
/// diagnostics, audio, and production-problem overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RocketSiloOperationalState {
    RecipeLocked,
    BuildingParts,
    MissingIngredients,
    NoPower,
    AwaitingPayload,
    ReadyToLaunch,
    Sealing,
    Launching,
    LaunchOutputBlocked,
}

impl RocketSiloOperationalState {
    /// Projection used by diagnostics that apply to every machine kind.
    pub const fn machine_status(self) -> MachineStatus {
        match self {
            Self::RecipeLocked => MachineStatus::NoRecipe,
            Self::BuildingParts | Self::ReadyToLaunch | Self::Sealing | Self::Launching => {
                MachineStatus::Working
            }
            Self::MissingIngredients | Self::AwaitingPayload => MachineStatus::NoInput,
            Self::NoPower => MachineStatus::NoPower,
            Self::LaunchOutputBlocked => MachineStatus::OutputFull,
        }
    }
}

/// Read-only diagnostic projection for a rocket silo.
///
/// Progress is simulation-tick based. During part construction it is the
/// current part craft; during sealing and launch it is the active phase. This
/// makes UI progress independent of render-frame cadence and naturally durable
/// across save/load because all source fields live in [`RocketSiloState`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RocketSiloStatusDetail {
    pub state: RocketSiloOperationalState,
    pub progress_ticks: u32,
    pub required_ticks: u32,
    pub ticks_remaining: Option<u32>,
}

impl RocketSiloStatusDetail {
    pub const fn machine_status(self) -> MachineStatus {
        self.state.machine_status()
    }
}

impl RocketSiloState {
    /// Whether a whole rocket stands in the silo.
    ///
    /// While this holds the silo builds nothing further: the rocket occupies the
    /// pad, and the parts for the next one have nowhere to go until it leaves.
    pub fn rocket_ready(&self) -> bool {
        self.parts_completed >= self.parts_per_rocket
    }

    /// Whether the cargo holder contains exactly the configured single payload.
    pub(crate) fn has_launch_payload(&self, launch_payload: ItemId) -> bool {
        self.cargo_inventory
            .slots()
            .first()
            .and_then(|slot| slot.stack())
            .is_some_and(|stack| stack.item_id() == launch_payload && stack.count() == 1)
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
