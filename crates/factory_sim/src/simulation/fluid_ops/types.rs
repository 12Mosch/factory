use crate::simulation::edge_geometry::EdgeEndpoint;
use crate::simulation::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::simulation) struct FluidBoxKey {
    pub(in crate::simulation) owner: FluidBoxOwner,
    pub(in crate::simulation) box_index: usize,
}

impl FluidBoxKey {
    /// The overwhelmingly common case, kept short so the entity call sites read
    /// the way they did before wagons could join a network.
    pub(in crate::simulation) const fn entity(entity_id: EntityId, box_index: usize) -> Self {
        Self {
            owner: FluidBoxOwner::Entity(entity_id),
            box_index,
        }
    }

    pub(in crate::simulation) const fn rolling_stock(
        stock_id: crate::rolling_stock::RollingStockId,
        box_index: usize,
    ) -> Self {
        Self {
            owner: FluidBoxOwner::RollingStock(stock_id),
            box_index,
        }
    }
}

/// Every fluid box in the world, whichever kind of holder it belongs to.
///
/// One borrow rather than two parameters, and one `get` rather than a
/// `match` repeated at each of the dozen places the solve reads a box: the
/// network arithmetic is about capacities and amounts, and where a box lives is
/// exactly the detail it should not have to carry.
#[derive(Clone, Copy)]
pub(in crate::simulation) struct FluidBoxes<'a> {
    entities: &'a EntityStore,
    stock: &'a RollingStockSubsystem,
}

impl<'a> FluidBoxes<'a> {
    pub(in crate::simulation) fn new(
        entities: &'a EntityStore,
        stock: &'a RollingStockSubsystem,
    ) -> Self {
        Self { entities, stock }
    }

    pub(in crate::simulation) fn get(self, key: FluidBoxKey) -> Option<&'a FluidBoxState> {
        match key.owner {
            FluidBoxOwner::Entity(entity_id) => self
                .entities
                .fluid_boxes
                .get(&entity_id)
                .and_then(|boxes| boxes.get(key.box_index)),
            FluidBoxOwner::RollingStock(stock_id) => self
                .stock
                .get(stock_id)
                .and_then(|stock| stock.fluid_boxes.get(key.box_index)),
        }
    }
}

/// The same, for the passes that move fluid rather than count it.
pub(in crate::simulation) struct FluidBoxesMut<'a> {
    entities: &'a mut EntityStore,
    stock: &'a mut RollingStockSubsystem,
}

impl<'a> FluidBoxesMut<'a> {
    pub(in crate::simulation) fn new(
        entities: &'a mut EntityStore,
        stock: &'a mut RollingStockSubsystem,
    ) -> Self {
        Self { entities, stock }
    }

    pub(in crate::simulation) fn as_ref(&self) -> FluidBoxes<'_> {
        FluidBoxes::new(self.entities, self.stock)
    }

    pub(in crate::simulation) fn get_mut(
        &mut self,
        key: FluidBoxKey,
    ) -> Option<&mut FluidBoxState> {
        match key.owner {
            FluidBoxOwner::Entity(entity_id) => self
                .entities
                .fluid_boxes
                .get_mut(&entity_id)
                .and_then(|boxes| boxes.get_mut(key.box_index)),
            FluidBoxOwner::RollingStock(stock_id) => self
                .stock
                .get_mut(stock_id)
                .and_then(|stock| stock.fluid_boxes.get_mut(key.box_index)),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct FluidBoxNode {
    pub(super) key: FluidBoxKey,
    pub(super) capacity_milliunits: u64,
    pub(super) filter: Option<FluidId>,
    pub(super) endpoints: Vec<EdgeEndpoint>,
    pub(super) underground_pairs: Vec<(EntityId, EntityId)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::simulation) struct FluidNetworkBoxTopology {
    pub(in crate::simulation) key: FluidBoxKey,
    pub(in crate::simulation) capacity_milliunits: u64,
    pub(in crate::simulation) filter: Option<FluidId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::simulation) struct FluidNetworkTopology {
    pub(in crate::simulation) network_id: u32,
    pub(in crate::simulation) boxes: Vec<FluidNetworkBoxTopology>,
    pub(in crate::simulation) capacity_milliunits: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::simulation) struct FluidNetworkDynamicSummary {
    pub(in crate::simulation) total_milliunits: u64,
    pub(in crate::simulation) fluid_id: Option<FluidId>,
    pub(in crate::simulation) blocked: bool,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::simulation) struct FluidBoxAssignment {
    pub(in crate::simulation) key: FluidBoxKey,
    pub(in crate::simulation) capacity_milliunits: u64,
    pub(in crate::simulation) amount_milliunits: u64,
}
