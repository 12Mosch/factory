mod flight;
mod network_access;
mod network_builder;
mod roboports;
mod types;

pub(in crate::simulation) use network_access::coverage_bounds;
pub(in crate::simulation) use roboports::roboport_is_charging;
pub(in crate::simulation) use types::RobotNetworkTopology;

use super::*;

impl Simulation {
    pub(super) fn invalidate_robot_state(&mut self) {
        self.robots.clear_networks();
        // Robots outlive the roboport they came from, so the moment the set of
        // roboports changes is the moment their references can dangle.
        self.prune_robot_flight_state();
    }

    /// Whether placing or destroying this prototype can change robot-network
    /// connectivity. Only roboports define the graph, so nothing else needs to
    /// pay for a rebuild.
    pub(super) fn prototype_affects_robot_network(
        &self,
        prototype: &factory_data::EntityPrototype,
    ) -> bool {
        prototype.roboport.is_some()
    }
}
