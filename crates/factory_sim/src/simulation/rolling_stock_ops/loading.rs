//! Where a stopped train meets the factory: the tile index that lets an
//! inserter or a pump find a wagon the same way it finds a chest.
//!
//! Every transfer path in the simulation resolves its endpoint by tile, out of
//! [`OccupancyGrid`]. Rolling stock is deliberately not in that grid — it sits
//! between tiles and moves — so a wagon would be invisible to all of it. Rather
//! than teach each transfer path about trains, this module supplies the one
//! thing they are missing: a second tile lookup to fall back to, with the same
//! shape and the same cost as the occupancy lookup it falls back from.
//!
//! Three rules shape it.
//!
//! * **Only stopped stock is in it.** A moving wagon is not a valid source or
//!   target, and it is stated here once rather than re-checked at every
//!   endpoint: a piece that is not in the index cannot be reached, so a train
//!   pulling away mid-swing takes its cargo out of reach in the same tick it
//!   starts to move.
//! * **Built on arrival, torn down on departure.** The tiles a wagon covers
//!   only change when it moves, so they are walked once when a train comes to
//!   rest and dropped the moment it stirs. Nothing here is proportional to the
//!   number of trains per transfer, which is what a station with a dozen
//!   inserters needs.
//! * **Derived, never saved.** The index is a function of where the stock is
//!   standing and of the rails under it, both of which are durable. It rebuilds
//!   itself as part of loading, before anything can ask what a tile holds, and
//!   it is cleared outright whenever the track changes under it, because a rail
//!   pulled up moves nothing but invalidates every tile derived from its
//!   geometry.

use crate::rolling_stock::{RollingStock, RollingStockId, RollingStockSubsystem, TrainId};
use crate::simulation::*;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

/// Tile → stopped wagon, and what it takes to undo one train's entries.
///
/// A [`BTreeMap`] rather than a hash map on purpose: it is the same structure
/// [`OccupancyGrid`] keys tiles by, so the fallback lookup costs what the
/// lookup it follows costs, and iterating it — which the fluid topology does —
/// is a function of the world rather than of a hash seed.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct StoppedStockIndex {
    /// Every stopped piece lying over each tile, ascending, and almost always
    /// exactly one — hence the inline capacity.
    ///
    /// All of them rather than the first to arrive, because a tile can outlive
    /// one of its claimants: two coupled wagons share the tile where they meet,
    /// and so do two separate trains parked nose to tail. Recording only one
    /// would leave the tile unreachable when *that* one pulled away, with the
    /// wagon still standing on it and its train still indexed — so nothing
    /// would put the tile back until the remaining train moved and stopped
    /// again.
    tiles: BTreeMap<(WorldTileCoord, WorldTileCoord), SmallVec<[RollingStockId; 1]>>,
    /// The tiles each indexed piece put in above, so a departure removes
    /// exactly what the arrival added instead of scanning the map.
    covered: BTreeMap<RollingStockId, Vec<(WorldTileCoord, WorldTileCoord)>>,
    /// Trains currently indexed. What "has this train already stopped?" is
    /// asked against, and the unit a teardown works in — a train moves as one
    /// piece, so its stock enters and leaves the index together.
    trains: BTreeSet<TrainId>,
}

impl_runtime_only_identity!(StoppedStockIndex);

impl StoppedStockIndex {
    /// The stopped piece of stock lying over `(x, y)`, if any.
    ///
    /// The lowest id among the pieces standing there. A rule about *which*
    /// pieces cover the tile rather than about the order they arrived in, so an
    /// inserter reaching into a shared tile swings into the same wagon however
    /// the trains around it came and went.
    pub(in crate::simulation) fn stock_at(
        &self,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Option<RollingStockId> {
        self.tiles.get(&(x, y))?.first().copied()
    }

    /// Every stopped piece, in ascending id order.
    pub(in crate::simulation) fn stopped_stock(&self) -> impl Iterator<Item = RollingStockId> + '_ {
        self.covered.keys().copied()
    }

    fn insert(
        &mut self,
        train_id: TrainId,
        covered: Vec<(RollingStockId, Vec<(WorldTileCoord, WorldTileCoord)>)>,
    ) {
        self.trains.insert(train_id);
        for (stock_id, tiles) in covered {
            for tile in &tiles {
                let claimants = self.tiles.entry(*tile).or_default();
                if let Err(index) = claimants.binary_search(&stock_id) {
                    claimants.insert(index, stock_id);
                }
            }
            self.covered.insert(stock_id, tiles);
        }
    }

    /// Drops every entry `train_id` put in.
    fn remove_train(&mut self, train_id: TrainId, stock_ids: &[RollingStockId]) {
        self.trains.remove(&train_id);
        for stock_id in stock_ids {
            let Some(tiles) = self.covered.remove(stock_id) else {
                continue;
            };
            for tile in tiles {
                let Some(claimants) = self.tiles.get_mut(&tile) else {
                    continue;
                };
                if let Ok(index) = claimants.binary_search(stock_id) {
                    claimants.remove(index);
                }
                // Only once nothing is standing there any more: a tile another
                // stopped wagon still covers goes on answering with that wagon.
                if claimants.is_empty() {
                    self.tiles.remove(&tile);
                }
            }
        }
    }

    fn clear(&mut self) {
        self.tiles.clear();
        self.covered.clear();
        self.trains.clear();
    }
}

/// The stopped rolling stock, reachable by tile, as one borrow.
///
/// The index alone cannot answer anything — it names pieces, and the pieces
/// live in the subsystem — so the two travel together rather than as two
/// parameters every transfer path has to remember to pass in the same order.
#[derive(Clone, Copy)]
pub(in crate::simulation) struct StoppedStock<'a> {
    index: &'a StoppedStockIndex,
    stock: &'a RollingStockSubsystem,
}

impl<'a> StoppedStock<'a> {
    pub(in crate::simulation) fn new(
        index: &'a StoppedStockIndex,
        stock: &'a RollingStockSubsystem,
    ) -> Self {
        Self { index, stock }
    }

    /// The stopped piece lying over `(x, y)`, or `None` — which is the answer
    /// for empty track, for a tile no wagon is on, and for a wagon that is
    /// moving, all three of which mean the same thing to a transfer.
    pub(in crate::simulation) fn at(
        self,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Option<&'a RollingStock> {
        self.stock.get(self.index.stock_at(x, y)?)
    }
}

/// The same borrow, for the paths that move items rather than ask about them.
pub(in crate::simulation) struct StoppedStockMut<'a> {
    index: &'a StoppedStockIndex,
    stock: &'a mut RollingStockSubsystem,
}

impl<'a> StoppedStockMut<'a> {
    pub(in crate::simulation) fn new(
        index: &'a StoppedStockIndex,
        stock: &'a mut RollingStockSubsystem,
    ) -> Self {
        Self { index, stock }
    }

    pub(in crate::simulation) fn at_mut(
        &mut self,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Option<&mut RollingStock> {
        self.stock.get_mut(self.index.stock_at(x, y)?)
    }
}

/// What an inserter would take out of a stopped piece of stock.
///
/// Cargo only. A locomotive's fuel slot is an input the way a furnace's is:
/// letting an inserter empty it would make a fuelling inserter and an unloading
/// one indistinguishable, and a locomotive parked beside either would end up
/// stripped of the coal that was just put in it.
pub(in crate::simulation) fn stock_pickup_item(stock: &RollingStock) -> Option<ItemId> {
    stock
        .inventory
        .as_ref()?
        .slots()
        .iter()
        .filter_map(|slot| slot.stack())
        .map(|stack| stack.item_id())
        .next()
}

/// Whether a stopped piece of stock would take `item` from an inserter.
///
/// Two destinations, in the order a wagon and a locomotive respectively have
/// one: cargo goes in the inventory under the ordinary unrestricted container
/// rules, including the slot filters a player set on it, and fuel goes in a
/// burner's fuel slot under the same [`ItemSlotPolicy::Fuel`] rule every other
/// burner machine follows.
pub(in crate::simulation) fn stock_can_accept(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    entities: &EntityStore,
    stock: &RollingStock,
    item: ItemStack,
) -> bool {
    if let Some(inventory) = stock.inventory.as_ref() {
        return item_slot_policy_accepts(
            catalog,
            research,
            entities,
            ItemSlotPolicy::Unrestricted,
            ItemSlotOperation::InserterInsert,
            item.item_id(),
        ) && inventory.can_insert(catalog, item.item_id(), item.count());
    }

    stock.energy.as_ref().is_some_and(|energy| {
        item_slot_can_accept(
            catalog,
            research,
            entities,
            ItemSlotPolicy::Fuel,
            ItemSlotOperation::InserterInsert,
            energy.fuel_slot,
            item,
        )
    })
}

/// Takes one `item_id` out of a stopped piece of stock.
pub(in crate::simulation) fn take_stock_item(
    catalog: &PrototypeCatalog,
    stock: &mut RollingStock,
    item_id: ItemId,
) -> Option<ItemStack> {
    if !item_slot_policy_allows_operation(
        ItemSlotPolicy::Unrestricted,
        ItemSlotOperation::InserterExtract,
    ) {
        return None;
    }
    stock.inventory.as_mut()?.remove(item_id, 1).ok()?;
    Some(
        ItemStack::new(catalog, item_id, 1)
            .expect("a removed inserter source item should form a valid stack"),
    )
}

/// Puts `item` into a stopped piece of stock, having been told it fits.
pub(in crate::simulation) fn drop_stock_item(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    entities: &EntityStore,
    stock: &mut RollingStock,
    item: ItemStack,
) -> bool {
    if !stock_can_accept(catalog, research, entities, stock, item) {
        return false;
    }
    if let Some(inventory) = stock.inventory.as_mut() {
        return inventory.insert_stack(catalog, item).is_ok();
    }
    stock.energy.as_mut().is_some_and(|energy| {
        energy
            .fuel_slot
            .insert_stack(catalog, item)
            .expect("the checked locomotive fuel slot should accept the item");
        true
    })
}

impl Simulation {
    /// The stopped stock, for the transfer paths that resolve endpoints by
    /// tile.
    pub(in crate::simulation) fn stopped_stock(&self) -> StoppedStock<'_> {
        StoppedStock::new(&self.stopped_stock_index, &self.rolling_stock)
    }

    /// Brings the stopped-stock index up to date with the trains as they now
    /// stand.
    ///
    /// Runs once at the end of the rail phase, after every train has been
    /// stepped, so it sees the positions the rest of the tick will transfer
    /// against. Trains that neither stopped nor started cost one set lookup
    /// each; only the ones that changed state are walked.
    ///
    /// Departures are handled here as well as at the moment a train actually
    /// moves, and the two are not the same event. A train that has just been
    /// given the throttle has speed before it has covered a whole unit of
    /// track, and it is under way from the moment it has speed — an inserter
    /// that kept loading it until the first unit went by would be loading a
    /// train that is already leaving.
    pub(in crate::simulation) fn refresh_stopped_stock_index(&mut self) {
        let departed = self
            .rolling_stock
            .trains()
            .filter(|train| {
                !train.is_stationary() && self.stopped_stock_index.trains.contains(&train.id)
            })
            .map(|train| train.id)
            .collect::<Vec<_>>();
        for train_id in departed {
            self.forget_stopped_train(train_id);
        }

        let arrived = self
            .rolling_stock
            .trains()
            .filter(|train| {
                train.is_stationary() && !self.stopped_stock_index.trains.contains(&train.id)
            })
            .map(|train| train.id)
            .collect::<Vec<_>>();
        if arrived.is_empty() {
            return;
        }

        let mut fluid_boxes_changed = false;
        for train_id in arrived {
            let Some(stock_ids) = self
                .rolling_stock
                .train(train_id)
                .map(|train| train.stock.clone())
            else {
                continue;
            };
            let mut covered = Vec::with_capacity(stock_ids.len());
            for stock_id in stock_ids {
                let Some(stock) = self.rolling_stock.get(stock_id) else {
                    continue;
                };
                fluid_boxes_changed |= !stock.fluid_boxes.is_empty();
                covered.push((stock_id, self.rolling_stock_tiles(stock)));
            }
            self.stopped_stock_index.insert(train_id, covered);
        }

        if fluid_boxes_changed {
            self.invalidate_fluid_state();
        }
    }

    /// Takes a train back out of the index, because it has moved or is about to.
    ///
    /// The counterpart of the arrival above and deliberately the only way out:
    /// anything that could make the tiles a train covers wrong calls this while
    /// the train still names every piece it is carrying, and the next refresh
    /// re-walks the train from where it then stands.
    ///
    /// Cheap on the tick's hot path — one set lookup for a train that is not in
    /// the index, which every train already under way is.
    pub(in crate::simulation) fn forget_stopped_train(&mut self, train_id: TrainId) {
        if !self.stopped_stock_index.trains.contains(&train_id) {
            return;
        }
        let stock_ids = self
            .rolling_stock
            .train(train_id)
            .map(|train| train.stock.clone())
            .unwrap_or_default();
        let carried_fluid = stock_ids
            .iter()
            .filter_map(|stock_id| self.rolling_stock.get(*stock_id))
            .any(|stock| !stock.fluid_boxes.is_empty());

        self.stopped_stock_index.remove_train(train_id, &stock_ids);
        if carried_fluid {
            self.invalidate_fluid_state();
        }
    }

    /// Empties the index outright.
    ///
    /// For the changes that invalidate every entry at once rather than one
    /// train's: the tiles in it are derived from rail geometry, so track laid
    /// or pulled up can move the answer for a wagon that never moved itself.
    pub(in crate::simulation) fn clear_stopped_stock_index(&mut self) {
        if self.stopped_stock_index.trains.is_empty() {
            return;
        }
        let carried_fluid = self.any_stopped_stock_carries_fluid();
        self.stopped_stock_index.clear();
        if carried_fluid {
            self.invalidate_fluid_state();
        }
    }

    /// Whether a piece of stock is stopped and therefore reachable. Public so
    /// the container UI can say why a wagon it is showing will not take items.
    pub fn rolling_stock_is_stopped(&self, stock_id: RollingStockId) -> bool {
        self.stopped_stock_index.covered.contains_key(&stock_id)
    }

    /// The stopped piece of stock lying over `(x, y)`.
    ///
    /// The public face of the index, for the cursor: clicking a parked train is
    /// the common way a player opens a wagon, and answering it from here costs
    /// one map lookup rather than a walk over every piece of stock in the
    /// world. Moving stock is deliberately absent — a cursor that wants it too
    /// falls back to [`Simulation::rolling_stock_covers_tile`].
    pub fn stopped_rolling_stock_at_tile(
        &self,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Option<RollingStockId> {
        self.stopped_stock_index.stock_at(x, y)
    }

    /// Whether any stopped piece of stock has a tank at all.
    ///
    /// What the fluid topology asks before it goes looking for pumps: the whole
    /// pump-side search exists to serve parked tankers, so with none parked
    /// there is nothing to find.
    pub(in crate::simulation) fn any_stopped_stock_carries_fluid(&self) -> bool {
        self.stopped_stock_index
            .stopped_stock()
            .filter_map(|stock_id| self.rolling_stock.get(stock_id))
            .any(|stock| !stock.fluid_boxes.is_empty())
    }

    /// The distinct tiles one piece of stock lies over.
    fn rolling_stock_tiles(&self, stock: &RollingStock) -> Vec<(WorldTileCoord, WorldTileCoord)> {
        let mut tiles = Vec::new();
        self.for_each_rolling_stock_tile(stock, |tile| {
            // A linear scan rather than a set: a body is a handful of tiles, and
            // a curve can return to one it has already left, so "was it the last
            // one?" is not enough to keep the list distinct.
            if !tiles.contains(&tile) {
                tiles.push(tile);
            }
            ControlFlow::Continue(())
        });
        tiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two trains parked nose to tail share the tile where they meet, and one of
    /// them leaving must not take that tile out of the index with it: the other
    /// wagon is still standing on it, and its train stays indexed, so nothing
    /// else would ever put the tile back.
    #[test]
    fn a_tile_two_trains_share_still_answers_when_one_departs() {
        let mut index = StoppedStockIndex::default();
        let (first, second) = (RollingStockId::new(1), RollingStockId::new(2));
        let shared = (4, 7);

        index.insert(TrainId::new(1), vec![(first, vec![shared, (4, 8)])]);
        index.insert(TrainId::new(2), vec![(second, vec![shared, (4, 6)])]);
        assert_eq!(index.stock_at(shared.0, shared.1), Some(first));

        index.remove_train(TrainId::new(1), &[first]);
        assert_eq!(
            index.stock_at(shared.0, shared.1),
            Some(second),
            "a wagon still standing on the tile should still be reachable"
        );
        assert_eq!(
            index.stock_at(4, 8),
            None,
            "the departed train left nothing"
        );

        index.remove_train(TrainId::new(2), &[second]);
        assert_eq!(index.stock_at(shared.0, shared.1), None);
        assert!(index.tiles.is_empty(), "an emptied tile keeps no entry");
    }

    /// Which wagon a shared tile answers with follows from the pieces standing
    /// there, not from the order their trains happened to stop in.
    #[test]
    fn a_shared_tile_answers_the_same_whichever_train_stopped_first() {
        let (first, second) = (RollingStockId::new(1), RollingStockId::new(2));
        let shared = (0, 0);

        let mut ascending = StoppedStockIndex::default();
        ascending.insert(TrainId::new(1), vec![(first, vec![shared])]);
        ascending.insert(TrainId::new(2), vec![(second, vec![shared])]);

        let mut descending = StoppedStockIndex::default();
        descending.insert(TrainId::new(2), vec![(second, vec![shared])]);
        descending.insert(TrainId::new(1), vec![(first, vec![shared])]);

        assert_eq!(ascending.stock_at(0, 0), Some(first));
        assert_eq!(descending.stock_at(0, 0), Some(first));
    }
}
