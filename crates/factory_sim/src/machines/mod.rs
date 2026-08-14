pub mod assembler;
pub mod burner_energy;
pub mod furnace;
pub mod lab;
pub mod mining_drill;
pub mod modules;
pub mod pumpjack;
pub mod rocket_silo;

pub use crate::power::{BoilerError, BoilerState};

pub use self::assembler::{AssemblerError, AssemblerIngredientStatus, AssemblingMachineState};
pub use self::burner_energy::{BurnerEnergy, MachineEnergy};
pub use self::furnace::{FurnaceError, FurnaceState};
pub use self::lab::{LabError, LabState};
pub use self::mining_drill::{MiningDrillError, MiningDrillState, PendingMiningOutput};
pub use self::modules::{
    BeaconState, MachineModuleState, ModuleError, ModuleSlots, ResolvedModuleEffects,
};
pub use self::pumpjack::PumpjackState;
pub use self::rocket_silo::{
    RocketLaunchPhase, RocketSiloError, RocketSiloOperationalState, RocketSiloState,
    RocketSiloStatusDetail,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MachineStatus {
    Working,
    Idle,
    NoRecipe,
    NoResearch,
    NoFuel,
    NoPower,
    NoInput,
    NoFluid,
    /// A heat consumer whose buffer has not reached its minimum working
    /// temperature. Distinct from `NoFuel`: the network needs more reactor
    /// output or time to warm up, not fuel in this machine.
    NoHeat,
    OutputFull,
}
