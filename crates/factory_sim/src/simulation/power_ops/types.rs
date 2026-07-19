use super::*;

#[derive(Clone, Copy)]
pub(super) struct PoleNode<'a> {
    pub(super) entity_id: EntityId,
    pub(super) placed: &'a PlacedEntity,
    pub(super) prototype: &'a factory_data::ElectricPolePrototype,
    pub(super) center_x2: WorldTileCoord,
    pub(super) center_y2: WorldTileCoord,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SteamEngineAssignment {
    pub(super) network_id: u32,
    pub(super) steam_network_id: u32,
    pub(super) available_power_output_watts: u64,
    pub(super) max_power_output_watts: u64,
    pub(super) steam_budget_milliunits: u64,
    pub(super) steam_consumption_per_tick_milliunits: u64,
}

#[derive(Clone, Debug, Default)]
pub(super) struct NetworkAccumulator {
    pub(super) pole_count: usize,
    pub(super) producer_count: usize,
    pub(super) consumer_count: usize,
    pub(super) available_production_watts: u64,
    pub(super) consumption_watts: u64,
    pub(super) production_watts: u64,
    pub(super) satisfaction_permyriad: u32,
}

/// Reusable, runtime-only storage for per-tick power generation accounting.
/// None of these derived buffers participate in saves or deterministic state.
#[derive(Clone, Debug, Default)]
pub(crate) struct PowerTickScratch {
    pub(super) networks: Vec<NetworkAccumulator>,
    pub(super) engine_assignments: Vec<(EntityId, SteamEngineAssignment)>,
    pub(super) engine_outputs: Vec<(EntityId, u64)>,
    pub(super) remaining_demand_by_network: Vec<u64>,
    pub(super) remaining_steam_by_network: Vec<u64>,
    pub(super) remaining_production_by_network: Vec<u64>,
    pub(super) remaining_available_by_network: Vec<u64>,
}

impl PartialEq for PowerTickScratch {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Hash for PowerTickScratch {
    fn hash<H: Hasher>(&self, _state: &mut H) {}
}
