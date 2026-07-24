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
pub(super) struct NetworkPowerBalance {
    pub(super) pole_count: usize,
    pub(super) producer_count: usize,
    pub(super) consumer_count: usize,
    /// Regular-generation capability plus discharge-capable storage; reported
    /// as `available_production_watts`.
    pub(super) available_production_watts: u64,
    /// Ordinary consumer demand (from the demand cache).
    pub(super) consumption_watts: u64,
    /// Ordinary demand actually served, including accumulator discharge but
    /// excluding power diverted into charging.
    pub(super) production_watts: u64,
    pub(super) satisfaction_permyriad: u32,
    /// Connected fuel-free solar generation at the current daylight ratio.
    pub(super) solar_watts: u64,
    /// Total assigned steam-engine output for this network (equals its steam
    /// production, since assignment caps output to the post-solar target).
    pub(super) steam_available_watts: u64,
    pub(super) accumulator_count: usize,
    /// Per-tick charge headroom summed across the network's accumulators.
    pub(super) charge_capability_watts: u64,
    /// Per-tick discharge capability summed across the network's accumulators.
    pub(super) discharge_capability_watts: u64,
    /// Charge actually applied this tick (mutually exclusive with discharge).
    pub(super) charge_watts: u64,
    /// Discharge actually applied this tick.
    pub(super) discharge_watts: u64,
    /// Stored energy summed across the network's accumulators after this tick.
    pub(super) stored_energy_joules: u64,
    /// Storage capacity summed across the network's accumulators.
    pub(super) capacity_joules: u64,
}

/// One accumulator's per-tick charge/discharge capability, grouped by network
/// for proportional allocation.
#[derive(Clone, Copy, Debug)]
pub(super) struct AccumulatorEntry {
    pub(super) entity_id: EntityId,
    pub(super) charge_capability_watts: u64,
    pub(super) discharge_capability_watts: u64,
}

/// Reusable, runtime-only storage for per-tick power generation accounting.
/// None of these derived buffers participate in saves or deterministic state.
#[derive(Clone, Debug, Default)]
pub(crate) struct PowerTickScratch {
    pub(super) networks: Vec<NetworkPowerBalance>,
    pub(super) engine_assignments: Vec<(EntityId, SteamEngineAssignment)>,
    pub(super) engine_outputs: Vec<(EntityId, u64)>,
    pub(super) remaining_demand_by_network: Vec<u64>,
    pub(super) remaining_steam_by_network: Vec<u64>,
    pub(super) remaining_production_by_network: Vec<u64>,
    pub(super) remaining_available_by_network: Vec<u64>,
    /// Per-network steam target after solar allocation (drives assignment).
    pub(super) steam_targets_by_network: Vec<u64>,
    /// Accumulators grouped by network, each inner vec sorted by ascending
    /// `EntityId` for deterministic integer-leftover allocation.
    pub(super) accumulators_by_network: Vec<Vec<AccumulatorEntry>>,
    /// Per-accumulator allocation scratch reused for each network.
    pub(super) allocation_scratch: Vec<u64>,
}

impl_runtime_only_identity!(PowerTickScratch);
