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
//! Membership follows the *logistic* squares, not the construction ones: a
//! chest belongs to the network whose roboports could reach it with a logistic
//! robot, which is the same rule that decides whether two roboports share a
//! network in the first place.

use std::collections::{BTreeMap, BTreeSet};

use crate::robots::LogisticItemTotals;
use crate::simulation::*;

/// One chest's contribution, in ascending item order.
///
/// A `Vec` rather than a map because it is only ever built once and replayed
/// twice — subtracted when it goes stale, added when it is fresh — and a chest
/// holds at most a few distinct items.
type ChestContribution = Vec<(ItemId, LogisticItemTotals)>;

/// Cached logistic contents of every robot network.
///
/// Derived entirely from the entity store and the settled topology, so it is
/// rebuilt on load rather than saved, and takes no part in the simulation hash.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct LogisticIndex {
    /// Per network, in `topology_networks` order.
    networks: Vec<BTreeMap<ItemId, LogisticItemTotals>>,
    /// What each indexed chest last contributed and which network it was
    /// counted into, so an update is a subtract followed by an add rather than
    /// a rescan of the network.
    published: BTreeMap<EntityId, (u32, ChestContribution)>,
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
        self.networks.resize(network_count, BTreeMap::new());
        self.published.clear();
        self.dirty.clear();
        self.rebuild_all = true;
    }

    pub(in crate::simulation) fn contents(
        &self,
        network_id: u32,
    ) -> Option<&BTreeMap<ItemId, LogisticItemTotals>> {
        self.networks.get(network_id as usize)
    }

    /// Network a chest is currently counted into, or `None` when no roboport
    /// reaches it.
    pub(in crate::simulation) fn network_of(&self, entity_id: EntityId) -> Option<u32> {
        self.published
            .get(&entity_id)
            .map(|(network_id, _)| *network_id)
    }

    fn add(&mut self, network_id: u32, contribution: &ChestContribution) {
        let Some(network) = self.networks.get_mut(network_id as usize) else {
            return;
        };
        for &(item_id, totals) in contribution {
            network.entry(item_id).or_default().add(totals);
        }
    }

    fn remove(&mut self, network_id: u32, contribution: &ChestContribution) {
        let Some(network) = self.networks.get_mut(network_id as usize) else {
            return;
        };
        for &(item_id, totals) in contribution {
            let Some(entry) = network.get_mut(&item_id) else {
                continue;
            };
            entry.subtract(totals);
            if entry.is_zero() {
                network.remove(&item_id);
            }
        }
    }
}

impl Simulation {
    /// Brings the logistic index back in step with the chests that changed.
    ///
    /// Runs inside the robot pass, after the topology has settled, so a chest
    /// is always placed into a network that currently exists.
    pub(in crate::simulation) fn refresh_logistic_index(&mut self) {
        let changed = self.entities.drain_changed_logistic_chests();
        let mut index = std::mem::take(&mut self.robots.logistic);
        if index.rebuild_all {
            index.rebuild_all = false;
            index
                .dirty
                .extend(self.entities.logistic_chests.keys().copied());
        }
        index.dirty.extend(changed);

        for entity_id in std::mem::take(&mut index.dirty) {
            if let Some((network_id, contribution)) = index.published.remove(&entity_id) {
                index.remove(network_id, &contribution);
            }
            let Some(contribution) = self.logistic_chest_contribution(entity_id) else {
                continue;
            };
            let Some(network_id) = self.logistic_network_covering_entity(entity_id) else {
                continue;
            };
            index.add(network_id, &contribution);
            index
                .published
                .insert(entity_id, (network_id, contribution));
        }

        self.robots.logistic = index;
    }

    /// What one chest offers and asks of its network, or `None` when it is not
    /// a logistic chest at all.
    fn logistic_chest_contribution(&self, entity_id: EntityId) -> Option<ChestContribution> {
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
                let totals = &mut contribution[index].1;
                // Only the shortfall is demand: the part of a request the chest
                // already holds is not work for the network.
                let held = totals.stored;
                totals.requested = totals.requested.saturating_add(wanted.saturating_sub(held));
            }
        }

        contribution.retain(|(_, totals)| !totals.is_zero());
        Some(contribution)
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
