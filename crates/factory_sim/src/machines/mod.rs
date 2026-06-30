pub mod assembler;
pub mod burner_energy;
pub mod furnace;
pub mod lab;
pub mod mining_drill;

pub use crate::power::{BoilerError, BoilerState};

pub use self::assembler::{AssemblerError, AssemblerIngredientStatus, AssemblingMachineState};
pub use self::burner_energy::BurnerEnergy;
pub use self::furnace::{FurnaceError, FurnaceState};
pub use self::lab::{LabError, LabState};
pub use self::mining_drill::{BurnerDrillError, BurnerMiningDrillState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MachineStatus {
    NoFuel,
    NoPower,
    NoInput,
    OutputFull,
    Working,
}
