use crate::entities::{Direction, EntityFootprint, OccupancyGrid};
use crate::fluids::FluidBoxState;
use crate::ids::EntityId;
use crate::inventory::Inventory;
use crate::logistics::{BeltSegment, InserterState, SplitterState};
use crate::machines::{AssemblingMachineState, BurnerMiningDrillState, FurnaceState, LabState};
use crate::power::{
    BoilerState, ElectricConsumerState, ElectricPoleState, OffshorePumpState, SteamEngineState,
};
use factory_data::EntityPrototypeId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityStore {
    pub(crate) entities: Vec<SimEntity>,
    pub(crate) placed_entities: BTreeMap<EntityId, PlacedEntity>,
    pub(crate) entity_inventories: BTreeMap<EntityId, Inventory>,
    pub(crate) burner_mining_drills: BTreeMap<EntityId, BurnerMiningDrillState>,
    pub(crate) furnaces: BTreeMap<EntityId, FurnaceState>,
    pub(crate) assembling_machines: BTreeMap<EntityId, AssemblingMachineState>,
    pub(crate) labs: BTreeMap<EntityId, LabState>,
    pub(crate) electric_poles: BTreeMap<EntityId, ElectricPoleState>,
    pub(crate) electric_consumers: BTreeMap<EntityId, ElectricConsumerState>,
    pub(crate) steam_engines: BTreeMap<EntityId, SteamEngineState>,
    pub(crate) boilers: BTreeMap<EntityId, BoilerState>,
    pub(crate) offshore_pumps: BTreeMap<EntityId, OffshorePumpState>,
    pub(crate) fluid_boxes: BTreeMap<EntityId, Vec<FluidBoxState>>,
    pub(crate) transport_belts: BTreeMap<EntityId, BeltSegment>,
    pub(crate) splitters: BTreeMap<EntityId, SplitterState>,
    pub(crate) inserters: BTreeMap<EntityId, InserterState>,
    pub(crate) occupancy: OccupancyGrid,
    pub(crate) next_entity_id: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SimEntity {
    pub id: EntityId,
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PlacedEntity {
    pub id: EntityId,
    pub prototype_id: EntityPrototypeId,
    pub x: i32,
    pub y: i32,
    pub direction: Direction,
    pub footprint: EntityFootprint,
}
