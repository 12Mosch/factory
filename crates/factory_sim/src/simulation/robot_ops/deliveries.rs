//! Matching logistic supply to logistic demand, and the robots that fly the
//! result.
//!
//! Three properties shape this pass.
//!
//! * **A delivery exists only while a robot flies it.** There is no queue of
//!   pending deliveries: matching and dispatch are one step, so the items
//!   promised out of one chest and into another can be read straight off the
//!   robots in flight. That ledger — [`LogisticReservations`] — is rebuilt from
//!   those robots every pass rather than maintained alongside them, which is
//!   what makes it impossible for it to drift.
//! * **Bounded per tick.** Matching, not flying, is where the frame time of a
//!   large network goes: every unmet request could be compared against every
//!   chest that offers the item. So each network is matched on one tick in
//!   [`DELIVERY_MATCH_INTERVAL_TICKS`] — staggered by network id, the way enemy
//!   decision ticks are staggered by unit id — and a pass examines a bounded
//!   prefix of the network's demand, resuming from a cursor next time so a
//!   network larger than one pass still works through all of it.
//! * **Deterministic order.** Demand is served requester before buffer and
//!   supply drawn active provider before storage before passive provider, with
//!   distance and then entity id breaking every remaining tie. Nothing here
//!   depends on iteration order of a hash container or on floating point.

use std::collections::BTreeMap;

use crate::robots::{LogisticDelivery, LogisticDeliveryStage, Robot, RobotId};
use crate::simulation::*;
use factory_data::{LogisticChestMode, RobotKind};

use super::dispatch::{commit_robot_dispatch, dispatching_roboport, network_can_dispatch};
use super::flight::{footprint_center_fixed, squared_distance};
use super::logistic_index::{DemandPriority, SupplyPriority};

/// Ticks between matching passes of one network. A network is matched when its
/// id falls in the tick's slot, so the cost of a world full of networks is
/// spread instead of landing on one tick.
const DELIVERY_MATCH_INTERVAL_TICKS: u64 = 8;
/// Deliveries one network may start in one pass. This is the per-network robot
/// limit: a network with a thousand unmet requests still launches at most this
/// many robots per pass, however many it has stationed.
const DELIVERIES_PER_NETWORK_PASS: usize = 8;
/// Demand entries one pass looks at before yielding the tick, whether or not
/// they turned into deliveries.
const DEMAND_EXAMINATION_BUDGET: usize = 32;
/// Active-provider stock entries one pass looks at when pushing surplus into
/// storage. Smaller than the demand budget because demand is the point of the
/// network and surplus is housekeeping.
const SURPLUS_EXAMINATION_BUDGET: usize = 8;
/// Chests offering an item that one demand entry compares before settling.
const SUPPLY_EXAMINATION_BUDGET: usize = 16;
/// Storage chests examined when looking for somewhere to put an item.
const STORAGE_EXAMINATION_BUDGET: usize = 16;

/// Items promised out of and into chests by the robots currently flying.
///
/// Without it, every pass would hand the same request to a fresh robot until
/// the first one arrived: the index reports what a chest *holds*, and a chest
/// does not lose its contents until a robot lands on it.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct LogisticReservations {
    /// Promised out of a chest, not yet collected.
    outbound: BTreeMap<(EntityId, ItemId), u32>,
    /// Promised into a chest, not yet delivered.
    inbound: BTreeMap<(EntityId, ItemId), u32>,
    /// Promised into each destination across every item id. Machine endpoints
    /// use this to reserve shared physical slots without scanning all flights.
    inbound_totals: BTreeMap<EntityId, u32>,
}

impl LogisticReservations {
    fn clear(&mut self) {
        self.outbound.clear();
        self.inbound.clear();
        self.inbound_totals.clear();
    }

    fn outbound(&self, chest: EntityId, item_id: ItemId) -> u32 {
        self.outbound.get(&(chest, item_id)).copied().unwrap_or(0)
    }

    fn inbound(&self, chest: EntityId, item_id: ItemId) -> u32 {
        self.inbound.get(&(chest, item_id)).copied().unwrap_or(0)
    }

    /// All items promised to one destination.
    ///
    /// Most inventories reserve capacity per item, but a rocket cargo slot
    /// accepts several alternative payload ids while still holding only one
    /// item total. Summing here keeps those alternatives from each reserving
    /// the same physical slot.
    fn total_inbound(&self, destination: EntityId) -> u32 {
        self.inbound_totals.get(&destination).copied().unwrap_or(0)
    }

    fn reserve(&mut self, delivery: &LogisticDelivery) {
        let count = u32::from(delivery.count);
        if delivery.stage == LogisticDeliveryStage::Pickup {
            *self
                .outbound
                .entry((delivery.source, delivery.item_id))
                .or_default() += count;
        }
        *self
            .inbound
            .entry((delivery.destination, delivery.item_id))
            .or_default() += count;
        *self.inbound_totals.entry(delivery.destination).or_default() += count;
    }
}

impl Simulation {
    /// Drops deliveries whose chests no longer make sense, before the robots
    /// holding them move.
    ///
    /// A robot on the pickup leg whose source is gone has nothing to fetch and
    /// turns around empty. One on the drop-off leg is already carrying the
    /// items, so it is re-aimed at a storage chest instead — and only when the
    /// network has none does it carry them home to the roboport, which is the
    /// last place items can be put down without vanishing.
    pub(in crate::simulation) fn reconcile_logistic_deliveries(&mut self) {
        if self.robot_flights.robots.is_empty() {
            return;
        }
        let stale = self
            .robot_flights
            .robots
            .iter()
            .filter_map(|(robot_id, robot)| {
                let delivery = robot.delivery?;
                let usable = match delivery.stage {
                    LogisticDeliveryStage::Pickup => {
                        self.delivery_robot_network_covers(robot, delivery.source)
                            && self.delivery_source_is_usable(delivery.source)
                    }
                    LogisticDeliveryStage::Dropoff => {
                        self.delivery_robot_network_covers(robot, delivery.destination)
                            && self.delivery_destination_is_usable(
                                delivery.destination,
                                delivery.item_id,
                            )
                    }
                };
                (!usable).then_some(*robot_id)
            })
            .collect::<Vec<_>>();
        for robot_id in stale {
            self.abandon_delivery(robot_id);
        }
    }

    /// Resolves a logistic robot that reached its errand: collect at the source,
    /// or hand over at the destination.
    pub(in crate::simulation) fn resolve_delivery_arrival(&mut self, robot_id: RobotId) {
        let Some(delivery) = self
            .robot_flights
            .robots
            .get(&robot_id)
            .and_then(|robot| robot.delivery)
        else {
            return;
        };
        match delivery.stage {
            LogisticDeliveryStage::Pickup => self.collect_delivery(robot_id, delivery),
            LogisticDeliveryStage::Dropoff => self.hand_over_delivery(robot_id, delivery),
        }
    }

    /// Starts deliveries for the networks whose turn it is this tick.
    pub(in crate::simulation) fn dispatch_logistic_deliveries(&mut self) {
        let network_count = self.robots.topology_networks.len();
        if network_count == 0 {
            return;
        }
        self.rebuild_delivery_reservations();

        let slot = self.tick % DELIVERY_MATCH_INTERVAL_TICKS;
        let mut network_index = slot as usize;
        while network_index < network_count {
            self.run_delivery_pass(network_index as u32);
            network_index += DELIVERY_MATCH_INTERVAL_TICKS as usize;
        }
    }

    /// Reads the promised-item ledger off the robots in flight.
    ///
    /// One pass over the flight store, which the tick already pays for once in
    /// [`Simulation::advance_robots`], in exchange for a ledger that cannot
    /// disagree with the robots it describes — including after a load, where
    /// nothing but the robots survives.
    fn rebuild_delivery_reservations(&mut self) {
        let mut reservations = std::mem::take(&mut self.robots.delivery_reservations);
        reservations.clear();
        for robot in self.robot_flights.robots.values() {
            if let Some(delivery) = &robot.delivery {
                reservations.reserve(delivery);
            }
        }
        self.robots.delivery_reservations = reservations;
    }

    fn run_delivery_pass(&mut self, network_id: u32) {
        let Some(network) = self.robots.topology_networks.get(network_id as usize) else {
            return;
        };
        let mut members = std::mem::take(&mut self.robots.delivery_member_scratch);
        members.clear();
        members.extend(network.roboports.iter().map(|node| node.entity_id));

        if network_can_dispatch(self, &members, RobotKind::Logistic) {
            let started = self.serve_network_demand(network_id, &members);
            if started < DELIVERIES_PER_NETWORK_PASS {
                self.push_network_surplus(
                    network_id,
                    &members,
                    DELIVERIES_PER_NETWORK_PASS - started,
                );
            }
        }

        self.robots.delivery_member_scratch = members;
    }

    /// Works a bounded prefix of the network's unmet demand, resuming where the
    /// previous pass stopped.
    fn serve_network_demand(&mut self, network_id: u32, members: &[EntityId]) -> usize {
        let cursor = self.robots.logistic.demand_cursor(network_id);
        let mut entries = std::mem::take(&mut self.robots.delivery_demand_scratch);
        entries.clear();
        entries.extend(
            self.robots
                .logistic
                .demand_from(network_id, cursor)
                .take(DEMAND_EXAMINATION_BUDGET),
        );
        // A cursor that reached the end of the set wraps: the entries before it
        // have waited longest and would otherwise never be reached again.
        if entries.len() < DEMAND_EXAMINATION_BUDGET {
            let seen = entries.len();
            entries.extend(
                self.robots
                    .logistic
                    .demand_from(network_id, None)
                    .filter(|entry| cursor.is_none_or(|cursor| *entry < cursor))
                    .take(DEMAND_EXAMINATION_BUDGET - seen),
            );
        }
        // The cursor follows the *last examined* entry, so it is read before the
        // sort below puts the wrapped entries back in front of the ones they
        // came after. Sorting is what keeps a wrapped pass serving requesters
        // before buffers rather than in the order the two segments were read.
        let next_cursor = entries.last().map(|entry| next_demand_cursor(*entry));
        entries.sort_unstable();

        let mut started = 0;
        for (priority, item_id, destination) in entries.iter().copied() {
            if started >= DELIVERIES_PER_NETWORK_PASS {
                break;
            }
            if self.try_start_delivery(network_id, members, priority, item_id, destination) {
                started += 1;
            }
        }

        self.robots.delivery_demand_scratch = entries;
        self.robots
            .logistic
            .set_demand_cursor(network_id, next_cursor);
        started
    }

    /// Moves what active providers hold into storage, whether or not anything
    /// asked for it. This is the whole difference between an active and a
    /// passive provider, and it is what keeps a production line from backing up
    /// into the chest at the end of it.
    fn push_network_surplus(&mut self, network_id: u32, members: &[EntityId], budget: usize) {
        let cursor = self.robots.logistic.surplus_cursor(network_id);
        let mut entries = std::mem::take(&mut self.robots.delivery_surplus_scratch);
        entries.clear();
        entries.extend(
            self.robots
                .logistic
                .surplus_from(network_id, cursor)
                .take(SURPLUS_EXAMINATION_BUDGET),
        );
        if entries.len() < SURPLUS_EXAMINATION_BUDGET {
            let seen = entries.len();
            entries.extend(
                self.robots
                    .logistic
                    .surplus_from(network_id, None)
                    .filter(|entry| cursor.is_none_or(|cursor| *entry < cursor))
                    .take(SURPLUS_EXAMINATION_BUDGET - seen),
            );
        }
        let next_cursor = entries.last().map(|entry| next_surplus_cursor(*entry));

        let mut started = 0;
        for (item_id, source) in entries.iter().copied() {
            if started >= budget {
                break;
            }
            if self.try_start_surplus_delivery(network_id, members, item_id, source) {
                started += 1;
            }
        }

        self.robots.delivery_surplus_scratch = entries;
        self.robots
            .logistic
            .set_surplus_cursor(network_id, next_cursor);
    }

    /// Matches one unmet request against the network's supply and sends a robot
    /// if anything can serve it.
    fn try_start_delivery(
        &mut self,
        network_id: u32,
        members: &[EntityId],
        priority: DemandPriority,
        item_id: ItemId,
        destination: EntityId,
    ) -> bool {
        let Some(wanted) = self.delivery_intake(destination, item_id) else {
            return false;
        };
        let requested = self
            .robots
            .logistic
            .endpoint_totals(destination, item_id)
            .requested
            .saturating_sub(
                self.robots
                    .delivery_reservations
                    .inbound(destination, item_id),
            );
        let wanted = wanted.min(requested);
        if wanted == 0 {
            return false;
        }

        let Some((source, available)) =
            self.pick_supply_source(network_id, item_id, destination, priority)
        else {
            return false;
        };
        self.start_delivery(members, item_id, source, destination, wanted.min(available))
    }

    /// Sends what one active provider holds to a storage chest.
    fn try_start_surplus_delivery(
        &mut self,
        network_id: u32,
        members: &[EntityId],
        item_id: ItemId,
        source: EntityId,
    ) -> bool {
        if self.robots.logistic.mode_of(source) != Some(LogisticChestMode::ActiveProvider) {
            return false;
        }
        let available = self
            .robots
            .logistic
            .endpoint_totals(source, item_id)
            .available
            .saturating_sub(self.robots.delivery_reservations.outbound(source, item_id));
        if available == 0 {
            return false;
        }
        let Some(destination) = self.pick_storage_target(network_id, item_id, source) else {
            return false;
        };
        let Some(room) = self.delivery_intake(destination, item_id) else {
            return false;
        };
        self.start_delivery(members, item_id, source, destination, available.min(room))
    }

    /// Commits one delivery: a robot from the roboport nearest the source, and
    /// the reservations that keep the next match from promising the same items
    /// twice.
    fn start_delivery(
        &mut self,
        members: &[EntityId],
        item_id: ItemId,
        source: EntityId,
        destination: EntityId,
        count: u32,
    ) -> bool {
        let stack_size = self
            .world
            .prototypes
            .item(item_id)
            .map_or(0, |item| u32::from(item.stack_size));
        // One trip carries one stack, so stack size is the throughput knob a
        // logistic robot has rather than a cargo number of its own.
        let count = u16::try_from(count.min(stack_size)).unwrap_or(u16::MAX);
        if count == 0 {
            return false;
        }
        let Some(target) = footprint_center_fixed(&self.entities, source) else {
            return false;
        };
        let Some(dispatch) = dispatching_roboport(self, members, target, RobotKind::Logistic)
        else {
            return false;
        };

        let delivery = LogisticDelivery {
            item_id,
            source,
            destination,
            count,
            stage: LogisticDeliveryStage::Pickup,
        };
        commit_robot_dispatch(self, dispatch, target, |robot| {
            robot.delivery = Some(delivery);
        });
        self.robots.delivery_reservations.reserve(&delivery);
        true
    }

    /// Best chest to draw `item_id` from for `destination`.
    ///
    /// Candidates arrive in role-priority order, so the first viable priority
    /// tier is the one that wins; within it the nearest chest is chosen, ties
    /// going to the lowest entity id. The scan stops at
    /// [`SUPPLY_EXAMINATION_BUDGET`] candidates, which is what keeps a network
    /// with a thousand provider chests from costing a thousand comparisons for
    /// every request it has.
    fn pick_supply_source(
        &self,
        network_id: u32,
        item_id: ItemId,
        destination: EntityId,
        demand_priority: DemandPriority,
    ) -> Option<(EntityId, u32)> {
        let target = footprint_center_fixed(&self.entities, destination)?;
        let mut best: Option<(SupplyPriority, i128, EntityId, u32)> = None;
        for (priority, source) in self
            .robots
            .logistic
            .supply_of(network_id, item_id)
            .take(SUPPLY_EXAMINATION_BUDGET)
        {
            if let Some((best_priority, ..)) = best
                && priority > best_priority
            {
                break;
            }
            if source == destination {
                continue;
            }
            // A buffer stocks itself out of the network, so letting one buffer
            // fill another would have the two trading the same stack forever.
            if demand_priority == DemandPriority::Buffer && priority == SupplyPriority::Buffer {
                continue;
            }
            let available = self
                .robots
                .logistic
                .endpoint_totals(source, item_id)
                .available
                .saturating_sub(self.robots.delivery_reservations.outbound(source, item_id));
            if available == 0 {
                continue;
            }
            let Some(position) = footprint_center_fixed(&self.entities, source) else {
                continue;
            };
            let distance = squared_distance(position.0 - target.0, position.1 - target.1);
            let is_better = best.is_none_or(|(_, best_distance, best_source, _)| {
                distance < best_distance || (distance == best_distance && source < best_source)
            });
            if is_better {
                best = Some((priority, distance, source, available));
            }
        }
        best.map(|(_, _, source, available)| (source, available))
    }

    /// Storage chest of `network_id` best able to take `item_id` in this
    /// bounded, rotating examination window.
    ///
    /// A chest filtered to the item beats an unfiltered one: the filter is the
    /// player saying where this item belongs, and honoring it is what keeps a
    /// sorted storage area sorted. Within either tier the nearest candidate to
    /// `near` wins. Rotating the window ensures a large network eventually
    /// considers every storage chest without making one match scan them all.
    fn pick_storage_target(
        &mut self,
        network_id: u32,
        item_id: ItemId,
        near: EntityId,
    ) -> Option<EntityId> {
        let target = footprint_center_fixed(&self.entities, near)?;
        let cursor = self.robots.logistic.storage_cursor(network_id);
        let mut candidates = std::mem::take(&mut self.robots.delivery_storage_scratch);
        candidates.clear();
        candidates.extend(
            self.robots
                .logistic
                .storage_from(network_id, cursor)
                .take(STORAGE_EXAMINATION_BUDGET),
        );
        if cursor.is_some() && candidates.len() < STORAGE_EXAMINATION_BUDGET {
            let seen = candidates.len();
            candidates.extend(
                self.robots
                    .logistic
                    .storage_from(network_id, None)
                    .filter(|entity_id| cursor.is_none_or(|cursor| *entity_id < cursor))
                    .take(STORAGE_EXAMINATION_BUDGET - seen),
            );
        }
        let next_cursor = candidates.last().copied().map(next_entity_id);
        self.robots
            .logistic
            .set_storage_cursor(network_id, next_cursor);

        let mut best: Option<(bool, i128, EntityId)> = None;
        for chest in candidates.iter().copied() {
            if chest == near {
                continue;
            }
            let filter = self
                .entities
                .logistic_chests
                .get(&chest)
                .and_then(|state| state.storage_filter());
            if filter.is_some_and(|filter| filter != item_id) {
                continue;
            }
            if self.delivery_intake(chest, item_id).unwrap_or(0) == 0 {
                continue;
            }
            let Some(position) = footprint_center_fixed(&self.entities, chest) else {
                continue;
            };
            // `false` sorts before `true`, so an unfiltered chest loses to a
            // filtered one at any distance.
            let unfiltered = filter.is_none();
            let distance = squared_distance(position.0 - target.0, position.1 - target.1);
            let is_better = best.is_none_or(|(best_unfiltered, best_distance, best_chest)| {
                (unfiltered, distance, chest) < (best_unfiltered, best_distance, best_chest)
            });
            if is_better {
                best = Some((unfiltered, distance, chest));
            }
        }
        self.robots.delivery_storage_scratch = candidates;
        best.map(|(_, _, chest)| chest)
    }

    /// How much more of `item_id` an endpoint could take, after reservations.
    fn delivery_intake(&self, destination: EntityId, item_id: ItemId) -> Option<u32> {
        if let Some(capacity) = self.machine_delivery_capacity(destination, item_id) {
            return Some(
                capacity
                    .saturating_sub(self.robots.delivery_reservations.total_inbound(destination)),
            );
        }
        let capacity = self.chest_delivery_capacity(destination, item_id)?;
        Some(
            capacity.saturating_sub(
                self.robots
                    .delivery_reservations
                    .inbound(destination, item_id),
            ),
        )
    }

    fn chest_delivery_capacity(&self, chest: EntityId, item_id: ItemId) -> Option<u32> {
        let stack_size = self.world.prototypes.item(item_id)?.stack_size;
        Some(
            self.entities
                .entity_inventories
                .get(&chest)?
                .insert_capacity(item_id, stack_size),
        )
    }

    /// Capacity exposed by a machine delivery endpoint before reservations.
    pub(super) fn machine_delivery_capacity(
        &self,
        entity_id: EntityId,
        item_id: ItemId,
    ) -> Option<u32> {
        let silo = self.entities.rocket_silos.get(&entity_id)?;
        let accepts = silo.rocket_ready()
            && matches!(silo.launch_phase, RocketLaunchPhase::Idle)
            && silo
                .cargo_inventory
                .slots()
                .first()
                .is_some_and(|slot| slot.is_empty())
            && item_slot_policy_accepts(
                &self.world.prototypes,
                &self.research,
                &self.entities,
                ItemSlotPolicy::RocketCargo,
                ItemSlotOperation::InserterInsert,
                item_id,
            )
            && silo
                .cargo_inventory
                .can_insert(&self.world.prototypes, item_id, 1);
        Some(u32::from(accepts))
    }

    fn delivery_robot_network_covers(&self, robot: &Robot, endpoint: EntityId) -> bool {
        let Some(network_id) = robot
            .home_roboport
            .and_then(|home| self.robots.network_ids_by_entity.get(&home).copied())
        else {
            return false;
        };
        self.logistic_network_covering_entity(endpoint) == Some(network_id)
    }

    fn delivery_source_is_usable(&self, chest: EntityId) -> bool {
        let Some(mode) = self
            .entities
            .placed_entity(chest)
            .and_then(|placed| self.world.prototypes.entity(placed.prototype_id))
            .and_then(|prototype| prototype.logistic_chest)
            .map(|logistic_chest| logistic_chest.mode)
        else {
            return false;
        };
        mode.supplies_network()
    }

    /// Machine acceptance and mutable storage filters are rechecked in flight.
    fn delivery_destination_is_usable(&self, destination: EntityId, item_id: ItemId) -> bool {
        if self.machine_delivery_capacity(destination, item_id) == Some(1) {
            return true;
        }
        let Some(mode) = self
            .entities
            .placed_entity(destination)
            .and_then(|placed| self.world.prototypes.entity(placed.prototype_id))
            .and_then(|prototype| prototype.logistic_chest)
            .map(|logistic_chest| logistic_chest.mode)
        else {
            return false;
        };
        self.entities
            .logistic_chests
            .get(&destination)
            .is_some_and(|state| {
                mode != LogisticChestMode::Storage
                    || state
                        .storage_filter()
                        .is_none_or(|filter| filter == item_id)
            })
    }

    fn collect_delivery(&mut self, robot_id: RobotId, delivery: LogisticDelivery) {
        let Some(taken) = self.withdraw_for_delivery(delivery) else {
            // The source was emptied while the robot was on its way. Nothing
            // was picked up, so there is nothing to put down: it flies home.
            if let Some(robot) = self.robot_flights.robots.get_mut(&robot_id) {
                robot.delivery = None;
                robot.errand = None;
            }
            return;
        };

        let errand = footprint_center_fixed(&self.entities, delivery.destination);
        if let Some(robot) = self.robot_flights.robots.get_mut(&robot_id) {
            robot.cargo.push(taken);
            robot.delivery = Some(LogisticDelivery {
                count: taken.count(),
                stage: LogisticDeliveryStage::Dropoff,
                ..delivery
            });
            robot.errand = errand;
        }
        if errand.is_none() {
            // The destination went away in the same tick the pickup landed, so
            // the items are already aboard and need somewhere else to go.
            self.abandon_delivery(robot_id);
        }
    }

    /// Takes as much of the promised amount as the source actually still holds.
    fn withdraw_for_delivery(&mut self, delivery: LogisticDelivery) -> Option<ItemStack> {
        let Simulation {
            world, entities, ..
        } = self;
        let inventory = entities.chest_inventory_mut(delivery.source)?;
        let available = u16::try_from(inventory.count(delivery.item_id)).unwrap_or(u16::MAX);
        let count = delivery.count.min(available);
        if count == 0 {
            return None;
        }
        inventory
            .remove(delivery.item_id, count)
            .expect("the withdrawal was clamped to what the chest holds");
        ItemStack::new(&world.prototypes, delivery.item_id, count).ok()
    }

    fn hand_over_delivery(&mut self, robot_id: RobotId, delivery: LogisticDelivery) {
        let Some(mut robot) = self.robot_flights.robots.remove(&robot_id) else {
            return;
        };
        self.deposit_cargo_into_destination(&mut robot, delivery.destination, delivery.item_id);
        let carried_on = !robot.cargo.is_empty();
        robot.delivery = None;
        robot.errand = None;
        self.robot_flights.robots.insert(robot_id, robot);

        // Anything the destination could not take goes to storage rather than
        // back to the roboport: a roboport holds construction material, and a
        // stack of iron parked there is a stack the network can no longer see.
        if carried_on {
            self.divert_cargo_to_storage(robot_id, delivery.source);
        }
    }

    fn deposit_cargo_into_destination(
        &mut self,
        robot: &mut Robot,
        destination: EntityId,
        item_id: ItemId,
    ) {
        if self.entities.rocket_silos.contains_key(&destination) {
            self.deposit_cargo_into_machine(robot, destination, item_id);
        } else {
            self.deposit_cargo_into_chest(robot, destination, item_id);
        }
    }

    /// Deposits through the same cargo-slot policy used by player and inserter
    /// transfers. Rejected or excess cargo remains aboard for diversion.
    fn deposit_cargo_into_machine(
        &mut self,
        robot: &mut Robot,
        entity_id: EntityId,
        item_id: ItemId,
    ) {
        if self.machine_delivery_capacity(entity_id, item_id) != Some(1) {
            return;
        }
        let mut remaining_cargo = Vec::new();
        let mut room = 1_u16;
        for stack in robot.cargo.drain(..) {
            if stack.item_id() != item_id || room == 0 {
                remaining_cargo.push(stack);
                continue;
            }
            let accepted = stack.count().min(room);
            let inserted = self
                .entities
                .rocket_silo_state_mut(entity_id)
                .ok()
                .is_some_and(|silo| {
                    silo.cargo_inventory
                        .insert(&self.world.prototypes, item_id, accepted)
                        .is_ok()
                });
            if !inserted {
                remaining_cargo.push(stack);
                continue;
            }
            room -= accepted;
            if let Some(leftover) = stack
                .count()
                .checked_sub(accepted)
                .filter(|count| *count > 0)
            {
                remaining_cargo.push(
                    ItemStack::new(&self.world.prototypes, item_id, leftover)
                        .expect("a leftover preserves a validated stack"),
                );
            }
        }
        robot.cargo = remaining_cargo;
    }

    /// Inserts the robot's cargo of `item_id` into `chest`, keeping whatever
    /// does not fit.
    fn deposit_cargo_into_chest(&mut self, robot: &mut Robot, chest: EntityId, item_id: ItemId) {
        let Simulation {
            world, entities, ..
        } = self;
        let catalog = &world.prototypes;
        let Some(stack_size) = catalog.item(item_id).map(|item| item.stack_size) else {
            return;
        };
        let Some(inventory) = entities.chest_inventory_mut(chest) else {
            return;
        };
        let mut remaining_cargo = Vec::new();
        for stack in robot.cargo.drain(..) {
            if stack.item_id() != item_id {
                remaining_cargo.push(stack);
                continue;
            }
            let accepted = u16::try_from(
                inventory
                    .insert_capacity(item_id, stack_size)
                    .min(u32::from(stack.count())),
            )
            .unwrap_or(u16::MAX);
            if accepted > 0 {
                inventory
                    .insert(catalog, item_id, accepted)
                    .expect("the insertion was clamped to the chest's capacity");
            }
            if let Some(leftover) = stack
                .count()
                .checked_sub(accepted)
                .filter(|count| *count > 0)
            {
                remaining_cargo.push(
                    ItemStack::new(catalog, item_id, leftover)
                        .expect("a leftover preserves a validated stack"),
                );
            }
        }
        robot.cargo = remaining_cargo;
    }

    /// Re-aims a loaded robot at a storage chest.
    ///
    /// Leaving it without a delivery is the fallback, not a failure: a robot
    /// with no errand flies home and unloads into the roboport, which is where
    /// items go when the network has nowhere else to put them.
    fn divert_cargo_to_storage(&mut self, robot_id: RobotId, source: EntityId) {
        let Some((item_id, count, home)) =
            self.robot_flights.robots.get(&robot_id).and_then(|robot| {
                let stack = robot.cargo.first()?;
                Some((stack.item_id(), stack.count(), robot.home_roboport?))
            })
        else {
            return;
        };
        let Some(network_id) = self.robots.network_ids_by_entity.get(&home).copied() else {
            return;
        };
        let Some(destination) = self.pick_storage_target(network_id, item_id, home) else {
            return;
        };
        let Some(errand) = footprint_center_fixed(&self.entities, destination) else {
            return;
        };
        let Some(robot) = self.robot_flights.robots.get_mut(&robot_id) else {
            return;
        };
        robot.delivery = Some(LogisticDelivery {
            item_id,
            source,
            destination,
            count,
            stage: LogisticDeliveryStage::Dropoff,
        });
        robot.errand = Some(errand);
    }

    /// Drops a robot's delivery. A loaded robot tries storage first; only when
    /// that fails does it fly home, where cargo lands in the roboport material
    /// slots.
    fn abandon_delivery(&mut self, robot_id: RobotId) {
        let Some(robot) = self.robot_flights.robots.get_mut(&robot_id) else {
            return;
        };
        let Some(delivery) = robot.delivery.take() else {
            return;
        };
        robot.errand = None;
        if robot.cargo.is_empty() {
            return;
        }
        self.divert_cargo_to_storage(robot_id, delivery.source);
    }
}

/// The demand entry just past `entry`, as a cursor a later pass resumes from.
///
/// Entity ids are allocated upward and never reused within a world, so bumping
/// the id is the smallest step that cannot land back on the entry just handled.
fn next_demand_cursor(
    entry: (DemandPriority, ItemId, EntityId),
) -> (DemandPriority, ItemId, EntityId) {
    let (priority, item_id, entity_id) = entry;
    (priority, item_id, next_entity_id(entity_id))
}

fn next_surplus_cursor(entry: (ItemId, EntityId)) -> (ItemId, EntityId) {
    let (item_id, entity_id) = entry;
    (item_id, next_entity_id(entity_id))
}

fn next_entity_id(entity_id: EntityId) -> EntityId {
    EntityId::new(entity_id.raw().saturating_add(1))
}
