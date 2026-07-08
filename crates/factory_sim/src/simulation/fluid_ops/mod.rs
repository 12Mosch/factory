mod equalization;
mod geometry;
mod machines;
mod math;
mod network_access;
mod network_builder;
mod types;

#[allow(unused_imports)]
pub(in crate::simulation) use math::{ceil_div_u64, per_tick_milliunits};
pub(in crate::simulation) use types::FluidBoxKey;

use super::*;

impl Simulation {
    pub(super) fn invalidate_fluid_state(&mut self) {
        self.fluids.networks.clear();
    }
}
