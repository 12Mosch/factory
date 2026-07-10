pub mod assembler;
pub mod burner_energy;
pub mod furnace;
pub mod lab;
pub mod mining_drill;
pub mod pumpjack;

pub use crate::power::{BoilerError, BoilerState};

pub use self::assembler::{AssemblerError, AssemblerIngredientStatus, AssemblingMachineState};
pub use self::burner_energy::BurnerEnergy;
pub use self::furnace::{FurnaceError, FurnaceState};
pub use self::lab::{LabError, LabState};
pub use self::mining_drill::{BurnerDrillError, BurnerMiningDrillState};
pub use self::pumpjack::PumpjackState;

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
    OutputFull,
}
