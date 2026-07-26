use crate::robots::TileBounds;
use crate::simulation::*;

/// One roboport as the topology builder sees it.
///
/// Both squares are resolved once here so neither the union-find nor the
/// coverage queries ever recompute them from the prototype, and so a roboport
/// whose prototype disappeared simply never becomes a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) struct RoboportNode {
    pub(in crate::simulation) entity_id: EntityId,
    pub(in crate::simulation) construction_bounds: TileBounds,
    pub(in crate::simulation) logistic_bounds: TileBounds,
    pub(in crate::simulation) charge_capacity_joules: u64,
}

/// A connected set of roboports.
///
/// `roboports` is ordered by ascending entity id, which is what makes the
/// per-network aggregates a deterministic function of the world rather than of
/// iteration order. `construction_bounds` is the bounding box of the member
/// construction squares — a summary, not the coverage itself; coverage is the
/// union of `roboports[..].construction_bounds`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::simulation) struct RobotNetworkTopology {
    pub(in crate::simulation) network_id: u32,
    pub(in crate::simulation) roboports: Vec<RoboportNode>,
    pub(in crate::simulation) construction_bounds: TileBounds,
    pub(in crate::simulation) logistic_bounds: TileBounds,
    pub(in crate::simulation) charge_capacity_joules: u64,
}
