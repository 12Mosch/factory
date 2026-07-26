//! Per-network item index for logistic chests.
//!
//! The index answers "what can this network supply, and what does it want" in
//! one lookup keyed by item, which is what a logistic dispatcher needs and what
//! a circuit connector reads out. Its whole reason for existing is that the
//! obvious implementation — walk every logistic chest and sum its slots — costs
//! the world's entire chest count every tick, and a factory has thousands of
//! chests.
//!
//! So it is maintained by delta instead. Each chest's contribution is cached as
//! it was last counted, the entity store records which inventories changed (see
//! `EntityStore::chest_inventory_mut`), and a refresh subtracts the old
//! contribution and adds the new one for exactly those chests. A settled
//! factory costs nothing; a busy one costs the chests that actually moved.
//!
//! Alongside the per-item totals it keeps the *candidate sets* the delivery
//! matcher works from: which chests currently want an item and which currently
//! offer one, ordered by the role priority the matcher resolves ties with. They
//! are maintained by the same delta pass, so finding work never walks a chest
//! that has neither supply nor demand.
//!
//! Membership follows the *logistic* squares, not the construction ones: a
//! chest belongs to the network whose roboports could reach it with a logistic
//! robot, which is the same rule that decides whether two roboports share a
//! network in the first place.

use std::collections::{BTreeMap, BTreeSet};

use crate::robots::LogisticItemTotals;
use crate::simulation::*;
use factory_data::LogisticChestMode;

/// One chest's contribution, in ascending item order.
///
/// A `Vec` rather than a map because it is only ever built once and replayed
/// twice — subtracted when it goes stale, added when it is fresh — and a chest
/// holds at most a few distinct items.
type ChestContribution = Vec<(ItemId, LogisticItemTotals)>;

/// Order supply is drawn in when several chests could serve one request.
///
/// Declaration order *is* the priority: the candidate sets are `BTreeSet`s keyed
/// by this, so iterating one already visits active providers before storage.
/// Emptying active providers first is what makes them active; draining storage
/// before the passive providers is what keeps storage from silting up while a
/// producer keeps refilling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::simulation) enum SupplyPriority {
    ActiveProvider,
    Storage,
    PassiveProvider,
    Buffer,
}

/// Order demand is served in: a requester is the point of the network, a buffer
/// is a convenience stocked out of what is left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::simulation) enum DemandPriority {
    Requester,
    Buffer,
}

/// One chest's place in the index, as it was last counted.
///
/// The chest id travels inside the entry rather than beside it so `add` and
/// `remove` cannot be handed an entry and a mismatched id.
#[derive(Clone, Debug)]
struct PublishedChest {
    entity_id: EntityId,
    network_id: u32,
    mode: LogisticChestMode,
    contribution: ChestContribution,
}

/// Candidate sets and totals of one network.
///
/// The three ordered collections are flat rather than nested maps because the
/// matcher reads them as resumable cursors: it works a bounded prefix per pass
/// and picks up where it left off, which a `range(cursor..)` over one ordered
/// set expresses directly.
#[derive(Clone, Debug, Default)]
struct NetworkIndex {
    contents: BTreeMap<ItemId, LogisticItemTotals>,
    /// Chests with unmet demand, in the order it is served.
    demand: BTreeSet<(DemandPriority, ItemId, EntityId)>,
    /// Chests offering an item, in the order supply is drawn.
    supply: BTreeMap<ItemId, BTreeSet<(SupplyPriority, EntityId)>>,
    /// Active providers holding something, which the network moves into storage
    /// even when nothing asked for it.
    surplus: BTreeSet<(ItemId, EntityId)>,
    /// Every storage chest, including the empty ones: an empty storage chest
    /// contributes no totals but is exactly where a delivery with nowhere else
    /// to go should land.
    storage_chests: BTreeSet<EntityId>,
    /// Where the last matching pass stopped, so a network larger than one pass'
    /// budget still works through all of its demand.
    demand_cursor: Option<(DemandPriority, ItemId, EntityId)>,
    surplus_cursor: Option<(ItemId, EntityId)>,
}

/// Cached logistic contents of every robot network.
///
/// Derived entirely from the entity store and the settled topology, so it is
/// rebuilt on load rather than saved, and takes no part in the simulation hash.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct LogisticIndex {
    /// Per network, in `topology_networks` order.
    networks: Vec<NetworkIndex>,
    /// What each indexed chest last contributed and which network it was
    /// counted into, so an update is a subtract followed by an add rather than
    /// a rescan of the network.
    published: BTreeMap<EntityId, PublishedChest>,
    /// Chests whose contribution is stale.
    dirty: BTreeSet<EntityId>,
    /// Set when the topology changed underneath the index: network ids are
    /// positional and a rebuild can renumber them, so every chest has to be
    /// placed again and the next refresh reseeds `dirty` from the chest map.
    rebuild_all: bool,
}

impl LogisticIndex {
    /// Drops everything and schedules a full rebuild.
    pub(in crate::simulation) fn reset(&mut self, network_count: usize) {
        self.networks.clear();
        self.networks.resize(network_count, NetworkIndex::default());
        self.published.clear();
        self.dirty.clear();
        self.rebuild_all = true;
    }

    pub(in crate::simulation) fn contents(
        &self,
        network_id: u32,
    ) -> Option<&BTreeMap<ItemId, LogisticItemTotals>> {
        self.networks
            .get(network_id as usize)
            .map(|network| &network.contents)
    }

    /// Network a chest is currently counted into, or `None` when no roboport
    /// reaches it.
    pub(in crate::simulation) fn network_of(&self, entity_id: EntityId) -> Option<u32> {
        self.published
            .get(&entity_id)
            .map(|published| published.network_id)
    }

    /// What one chest last contributed of a single item.
    ///
    /// Read by the matcher instead of the chest's inventory: the totals are at
    /// most one refresh old, and every path that acts on them clamps against
    /// the real inventory when it commits.
    pub(in crate::simulation) fn chest_totals(
        &self,
        entity_id: EntityId,
        item_id: ItemId,
    ) -> LogisticItemTotals {
        let Some(published) = self.published.get(&entity_id) else {
            return LogisticItemTotals::default();
        };
        published
            .contribution
            .binary_search_by_key(&item_id, |(item, _)| *item)
            .map_or_else(
                |_| LogisticItemTotals::default(),
                |index| published.contribution[index].1,
            )
    }

    pub(in crate::simulation) fn mode_of(&self, entity_id: EntityId) -> Option<LogisticChestMode> {
        self.published
            .get(&entity_id)
            .map(|published| published.mode)
    }

    /// Unmet demand of one network, from `cursor` onward in serving order.
    pub(in crate::simulation) fn demand_from(
        &self,
        network_id: u32,
        cursor: Option<(DemandPriority, ItemId, EntityId)>,
    ) -> impl Iterator<Item = (DemandPriority, ItemId, EntityId)> + '_ {
        let network = self.networks.get(network_id as usize);
        network
            .into_iter()
            .flat_map(move |network| match cursor {
                Some(cursor) => network.demand.range(cursor..),
                None => network.demand.range(..),
            })
            .copied()
    }

    /// Active-provider stock of one network, from `cursor` onward.
    pub(in crate::simulation) fn surplus_from(
        &self,
        network_id: u32,
        cursor: Option<(ItemId, EntityId)>,
    ) -> impl Iterator<Item = (ItemId, EntityId)> + '_ {
        let network = self.networks.get(network_id as usize);
        network
            .into_iter()
            .flat_map(move |network| match cursor {
                Some(cursor) => network.surplus.range(cursor..),
                None => network.surplus.range(..),
            })
            .copied()
    }

    /// Chests offering `item_id`, in the order supply is drawn from them.
    pub(in crate::simulation) fn supply_of(
        &self,
        network_id: u32,
        item_id: ItemId,
    ) -> impl Iterator<Item = (SupplyPriority, EntityId)> + '_ {
        self.networks
            .get(network_id as usize)
            .and_then(|network| network.supply.get(&item_id))
            .into_iter()
            .flatten()
            .copied()
    }

    pub(in crate::simulation) fn storage_chests(
        &self,
        network_id: u32,
    ) -> impl Iterator<Item = EntityId> + '_ {
        self.networks
            .get(network_id as usize)
            .into_iter()
            .flat_map(|network| network.storage_chests.iter())
            .copied()
    }

    pub(in crate::simulation) fn demand_cursor(
        &self,
        network_id: u32,
    ) -> Option<(DemandPriority, ItemId, EntityId)> {
        self.networks
            .get(network_id as usize)
            .and_then(|network| network.demand_cursor)
    }

    pub(in crate::simulation) fn set_demand_cursor(
        &mut self,
        network_id: u32,
        cursor: Option<(DemandPriority, ItemId, EntityId)>,
    ) {
        if let Some(network) = self.networks.get_mut(network_id as usize) {
            network.demand_cursor = cursor;
        }
    }

    pub(in crate::simulation) fn surplus_cursor(
        &self,
        network_id: u32,
    ) -> Option<(ItemId, EntityId)> {
        self.networks
            .get(network_id as usize)
            .and_then(|network| network.surplus_cursor)
    }

    pub(in crate::simulation) fn set_surplus_cursor(
        &mut self,
        network_id: u32,
        cursor: Option<(ItemId, EntityId)>,
    ) {
        if let Some(network) = self.networks.get_mut(network_id as usize) {
            network.surplus_cursor = cursor;
        }
    }

    fn add(&mut self, published: &PublishedChest) {
        let Some(network) = self.networks.get_mut(published.network_id as usize) else {
            return;
        };
        let entity_id = published.entity_id;
        if published.mode == LogisticChestMode::Storage {
            network.storage_chests.insert(entity_id);
        }
        for &(item_id, totals) in &published.contribution {
            network.contents.entry(item_id).or_default().add(totals);
            if totals.available > 0
                && let Some(priority) = supply_priority(published.mode)
            {
                network
                    .supply
                    .entry(item_id)
                    .or_default()
                    .insert((priority, entity_id));
                if published.mode == LogisticChestMode::ActiveProvider {
                    network.surplus.insert((item_id, entity_id));
                }
            }
            if totals.requested > 0
                && let Some(priority) = demand_priority(published.mode)
            {
                network.demand.insert((priority, item_id, entity_id));
            }
        }
    }

    fn remove(&mut self, published: &PublishedChest) {
        let Some(network) = self.networks.get_mut(published.network_id as usize) else {
            return;
        };
        let entity_id = published.entity_id;
        network.storage_chests.remove(&entity_id);
        for &(item_id, totals) in &published.contribution {
            if let Some(entry) = network.contents.get_mut(&item_id) {
                entry.subtract(totals);
                if entry.is_zero() {
                    network.contents.remove(&item_id);
                }
            }
            if let Some(priority) = supply_priority(published.mode)
                && let Some(candidates) = network.supply.get_mut(&item_id)
            {
                candidates.remove(&(priority, entity_id));
                if candidates.is_empty() {
                    network.supply.remove(&item_id);
                }
            }
            network.surplus.remove(&(item_id, entity_id));
            if let Some(priority) = demand_priority(published.mode) {
                network.demand.remove(&(priority, item_id, entity_id));
            }
        }
    }
}

fn supply_priority(mode: LogisticChestMode) -> Option<SupplyPriority> {
    match mode {
        LogisticChestMode::ActiveProvider => Some(SupplyPriority::ActiveProvider),
        LogisticChestMode::Storage => Some(SupplyPriority::Storage),
        LogisticChestMode::PassiveProvider => Some(SupplyPriority::PassiveProvider),
        LogisticChestMode::Buffer => Some(SupplyPriority::Buffer),
        LogisticChestMode::Requester => None,
    }
}

fn demand_priority(mode: LogisticChestMode) -> Option<DemandPriority> {
    match mode {
        LogisticChestMode::Requester => Some(DemandPriority::Requester),
        LogisticChestMode::Buffer => Some(DemandPriority::Buffer),
        LogisticChestMode::PassiveProvider
        | LogisticChestMode::ActiveProvider
        | LogisticChestMode::Storage => None,
    }
}

impl Simulation {
    /// Brings the logistic index back in step with the chests that changed.
    ///
    /// Runs inside the robot pass, after the topology has settled, so a chest
    /// is always placed into a network that currently exists.
    pub(in crate::simulation) fn refresh_logistic_index(&mut self) {
        let changed = self.entities.drain_changed_logistic_chests();
        if changed.is_empty() && !self.robots.logistic.rebuild_all {
            return;
        }
        let mut index = std::mem::take(&mut self.robots.logistic);
        if index.rebuild_all {
            index.rebuild_all = false;
            index
                .dirty
                .extend(self.entities.logistic_chests.keys().copied());
        }
        index.dirty.extend(changed);

        for entity_id in std::mem::take(&mut index.dirty) {
            if let Some(published) = index.published.remove(&entity_id) {
                index.remove(&published);
            }
            let Some((mode, contribution)) = self.logistic_chest_contribution(entity_id) else {
                continue;
            };
            let Some(network_id) = self.logistic_network_covering_entity(entity_id) else {
                continue;
            };
            let published = PublishedChest {
                entity_id,
                network_id,
                mode,
                contribution,
            };
            index.add(&published);
            index.published.insert(entity_id, published);
        }

        self.robots.logistic = index;
    }

    /// What one chest offers and asks of its network, or `None` when it is not
    /// a logistic chest at all.
    fn logistic_chest_contribution(
        &self,
        entity_id: EntityId,
    ) -> Option<(LogisticChestMode, ChestContribution)> {
        let state = self.entities.logistic_chests.get(&entity_id)?;
        let inventory = self.entities.entity_inventories.get(&entity_id)?;
        let mode = self
            .entities
            .placed_entity(entity_id)
            .and_then(|placed| self.world.prototypes.entity(placed.prototype_id))
            .and_then(|prototype| prototype.logistic_chest)?
            .mode;

        let mut contribution = ChestContribution::new();
        for stack in inventory.slots().iter().filter_map(|slot| slot.stack()) {
            let index = totals_index(&mut contribution, stack.item_id());
            let totals = &mut contribution[index].1;
            let count = u32::from(stack.count());
            totals.stored = totals.stored.saturating_add(count);
            if mode.supplies_network() {
                totals.available = totals.available.saturating_add(count);
            }
        }

        if mode.requests_items() {
            for (item_id, wanted) in state.requests.iter().filter_map(|request| request.demand()) {
                let index = totals_index(&mut contribution, item_id);
                contribution[index].1.requested =
                    contribution[index].1.requested.saturating_add(wanted);
            }
            // Only the shortfall is demand: the part of a request the chest
            // already holds is not work for the network. Held stock is netted
            // off the *summed* target rather than off each row, because two
            // rows naming the same item are one request for their total —
            // subtracting the same stock twice would understate the shortfall.
            for (_, totals) in &mut contribution {
                totals.requested = totals.requested.saturating_sub(totals.stored);
            }
        }

        contribution.retain(|(_, totals)| !totals.is_zero());
        Some((mode, contribution))
    }
}

/// Index of `item_id` in an ascending contribution list, inserting an empty
/// entry when the item is not present yet.
fn totals_index(contribution: &mut ChestContribution, item_id: ItemId) -> usize {
    match contribution.binary_search_by_key(&item_id, |(item, _)| *item) {
        Ok(index) => index,
        Err(index) => {
            contribution.insert(index, (item_id, LogisticItemTotals::default()));
            index
        }
    }
}
