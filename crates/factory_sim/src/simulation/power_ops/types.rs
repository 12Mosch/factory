use super::*;

#[derive(Clone, Copy)]
pub(super) struct PoleNode<'a> {
    pub(super) entity_id: EntityId,
    pub(super) placed: &'a PlacedEntity,
    pub(super) prototype: &'a factory_data::ElectricPolePrototype,
    pub(super) center_x2: i32,
    pub(super) center_y2: i32,
}

#[derive(Clone, Copy)]
pub(super) struct SteamEngineAssignment {
    pub(super) network_id: u32,
    pub(super) steam_network_id: u32,
    pub(super) available_power_output_watts: u64,
    pub(super) max_power_output_watts: u64,
    pub(super) steam_budget_milliunits: u64,
    pub(super) steam_consumption_per_tick_milliunits: u64,
}

#[derive(Default)]
pub(super) struct NetworkAccumulator {
    pub(super) pole_count: usize,
    pub(super) producer_count: usize,
    pub(super) consumer_count: usize,
    pub(super) available_production_watts: u64,
    pub(super) consumption_watts: u64,
    pub(super) production_watts: u64,
    pub(super) satisfaction_permyriad: u32,
}
