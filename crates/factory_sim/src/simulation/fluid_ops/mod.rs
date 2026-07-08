mod equalization;
mod geometry;
mod machines;
mod math;
mod network_access;
mod network_builder;
mod types;

#[allow(unused_imports)]
pub(in crate::simulation) use math::{ceil_div_u64, per_tick_milliunits};
pub(in crate::simulation) use types::{FluidBoxKey, FluidNetworkTopology};

use super::*;

impl Simulation {
    pub(super) fn invalidate_fluid_state(&mut self) {
        self.fluids.clear_networks();
    }

    #[cfg(test)]
    pub(super) fn fluid_topology_rebuild_count(&self) -> u64 {
        self.fluids.topology_rebuilds
    }
}
