use crate::entities::{Direction, EntityFootprint};
use crate::fluids::FluidBoxState;
use crate::logistics::{BeltSegment, InserterState, SplitterState};
use crate::machines::{AssemblingMachineState, BurnerMiningDrillState, FurnaceState, LabState};
use crate::power::{
    BoilerState, ElectricConsumerState, ElectricPoleState, OffshorePumpState, SteamEngineState,
};
use factory_data::EntityPrototypeId;

pub(crate) struct EntityReservation {
    pub(crate) prototype_id: EntityPrototypeId,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) direction: Direction,
    pub(crate) footprint: EntityFootprint,
    pub(crate) inventory_slot_count: Option<usize>,
    pub(crate) burner_mining_drill: Option<BurnerMiningDrillState>,
    pub(crate) furnace: Option<FurnaceState>,
    pub(crate) assembling_machine: Option<AssemblingMachineState>,
    pub(crate) lab: Option<LabState>,
    pub(crate) electric_pole: Option<ElectricPoleState>,
    pub(crate) electric_consumer: Option<ElectricConsumerState>,
    pub(crate) steam_engine: Option<SteamEngineState>,
    pub(crate) boiler: Option<BoilerState>,
    pub(crate) offshore_pump: Option<OffshorePumpState>,
    pub(crate) fluid_boxes: Option<Vec<FluidBoxState>>,
    pub(crate) transport_belt: Option<BeltSegment>,
    pub(crate) splitter: Option<SplitterState>,
    pub(crate) inserter: Option<InserterState>,
}
