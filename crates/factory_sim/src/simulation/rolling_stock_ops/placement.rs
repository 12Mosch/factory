//! Putting rolling stock on the track, coupling it into trains, and taking it
//! off again.
//!
//! Rolling stock does not go through the tile placement path, and that is not
//! an omission: a wagon sits between tiles, so there is no footprint to reserve
//! and no occupancy to check. What takes their place is the along-track
//! measurement in [`stock_offsets_within`] — one primitive that answers both of
//! the questions placement has, "does this overlap something?" and "does this
//! couple to something?", from one walk of the graph. Two separate answers is
//! how a wagon ends up coupled to a train it is also standing inside.
//!
//! Everything here runs on a player action rather than on a tick, so the walks
//! it does are allowed to be proportional to the track around the click. The
//! per-tick path in the parent module never calls any of it.

use crate::rolling_stock::{
    RailPosition, RollingStock, RollingStockId, RollingStockMiningError,
    RollingStockPlacementError, Train, TrainId, TrainThrottle,
};
use crate::simulation::rail_ops::RailGraph;
use crate::simulation::*;
use factory_data::EntityPrototypeId;
use std::collections::BTreeMap;

use super::{coupled_spacing_fixed, travel};

/// How far around a click placement measures the track.
///
/// This is the correctness radius, not the convenience one: everything within
/// it is priced so overlap and coupling can be decided exactly, so it has to
/// comfortably exceed the longest piece of stock plus the longest one it could
/// be laid against. Being generous costs a slightly longer walk on a player
/// action and nothing on a tick.
const COUPLING_SEARCH_FIXED: i64 = 32 * crate::POSITION_SCALE;

/// How far a click will reach to attach to an existing train.
///
/// Deliberately much shorter than the search radius above. Snapping is a
/// convenience — click beside the train and the wagon lands coupled — and a
/// convenience that reached across half a siding would make it impossible to
/// park a second train on the same stretch of track.
const COUPLING_SNAP_FIXED: i64 = 10 * crate::POSITION_SCALE;

/// One rail edge the search reached, and how to turn a distance along it into
/// an offset from where the search started.
///
/// A point at `distance_fixed` maps to `entry_offset + direction * (distance -
/// entry_distance)`. Keeping the mapping rather than a per-point answer is what
/// lets one walk price every piece of stock on the reachable track.
#[derive(Clone, Copy, Debug)]
struct EdgeReach {
    entry_offset: i64,
    entry_distance: i64,
    direction: i64,
}

impl EdgeReach {
    fn offset_of(&self, distance_fixed: i64) -> i64 {
        self.entry_offset + self.direction * (distance_fixed - self.entry_distance)
    }
}

impl Simulation {
    /// Puts one piece of rolling stock on the rail under `(x, y)`, taking the
    /// build item out of the player inventory.
    ///
    /// The piece lands coupled to whatever train already occupies that stretch
    /// of track, on the side of it the click was on, which is what makes
    /// building a train a matter of clicking beside the one you have rather
    /// than measuring wagon lengths onto rail pieces.
    pub fn place_rolling_stock_from_player_inventory(
        &mut self,
        prototype_id: EntityPrototypeId,
        item_id: ItemId,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Result<RollingStockId, RollingStockPlacementError> {
        // The graph is a cache rebuilt once a tick, and a player may lay the
        // rail and put a wagon on it in the same frame. Ensuring it here means
        // the placement answers for the track that exists rather than for the
        // track the last tick saw.
        self.ensure_rail_graph();
        let position = self.validate_rolling_stock_placement(prototype_id, item_id, x, y)?;
        self.player_inventory
            .remove(item_id, 1)
            .expect("the build item was just counted in the player inventory");
        Ok(self.insert_rolling_stock(prototype_id, position))
    }

    /// Puts stock on the rail under `(x, y)` without a player, an item, or a
    /// technology gate.
    ///
    /// The rolling-stock counterpart of [`crate::placement::place`], and used
    /// for the same things: fixtures, scripted worlds, and tests that are about
    /// what a train does rather than about how it was bought.
    pub fn place_rolling_stock(
        &mut self,
        prototype_id: EntityPrototypeId,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Result<RollingStockId, RollingStockPlacementError> {
        if self
            .world
            .prototypes
            .entity(prototype_id)
            .is_none_or(|prototype| prototype.rolling_stock.is_none())
        {
            return Err(RollingStockPlacementError::NotRollingStock(prototype_id));
        }
        self.ensure_rail_graph();
        let position = self.rolling_stock_placement_position(prototype_id, x, y)?;
        Ok(self.insert_rolling_stock(prototype_id, position))
    }

    /// Everything [`Simulation::place_rolling_stock_from_player_inventory`]
    /// checks, without doing it, and the position the placement would use.
    ///
    /// The build cursor answers with this rather than re-deriving the rules, so
    /// a preview that says "yes" is a click that succeeds.
    pub fn validate_rolling_stock_placement(
        &self,
        prototype_id: EntityPrototypeId,
        item_id: ItemId,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Result<RailPosition, RollingStockPlacementError> {
        let prototype = self
            .world
            .prototypes
            .entity(prototype_id)
            .filter(|prototype| prototype.rolling_stock.is_some())
            .ok_or(RollingStockPlacementError::NotRollingStock(prototype_id))?;
        let build_item = prototype
            .build_item
            .ok_or(RollingStockPlacementError::MissingBuildItem(prototype_id))?;
        if build_item != item_id {
            return Err(RollingStockPlacementError::ItemDoesNotBuildStock {
                item_id,
                prototype_id,
            });
        }
        if !placement_validation_ops::entity_is_unlocked(self, prototype_id) {
            return Err(RollingStockPlacementError::Locked(prototype_id));
        }
        if self.player_inventory.count(item_id) == 0 {
            return Err(RollingStockPlacementError::InsufficientInventory { item_id });
        }

        self.rolling_stock_placement_position(prototype_id, x, y)
    }

    /// Takes one piece of rolling stock off the track, returning its build
    /// item, its fuel, and its cargo to the player.
    ///
    /// All or nothing: a wagon whose contents will not fit stays on the rails,
    /// because a half-recovered wagon would leave items on a train that no
    /// longer exists.
    pub fn mine_rolling_stock(
        &mut self,
        stock_id: RollingStockId,
    ) -> Result<(), RollingStockMiningError> {
        let stock = self
            .rolling_stock
            .get(stock_id)
            .ok_or(RollingStockMiningError::MissingStock(stock_id))?;
        let build_item = self
            .world
            .prototypes
            .entity(stock.prototype_id)
            .and_then(|prototype| prototype.build_item)
            .ok_or(RollingStockMiningError::MissingBuildItem(
                stock.prototype_id,
            ))?;

        let mut recovered = vec![
            ItemStack::new(&self.world.prototypes, build_item, 1)
                .map_err(|_| RollingStockMiningError::MissingBuildItem(stock.prototype_id))?,
        ];
        recovered.extend(
            stock
                .energy
                .as_ref()
                .and_then(|energy| energy.fuel_slot.stack()),
        );
        recovered.extend(
            stock
                .inventory
                .iter()
                .flat_map(|inventory| inventory.slots())
                .filter_map(|slot| slot.stack()),
        );

        let mut player_inventory = self.player_inventory.clone();
        for stack in recovered {
            player_inventory
                .insert_stack(&self.world.prototypes, stack)
                .map_err(|_| RollingStockMiningError::InsufficientInventory {
                    item_id: stack.item_id(),
                })?;
        }

        self.player_inventory = player_inventory;
        self.remove_rolling_stock(stock_id);
        Ok(())
    }

    /// Drops stock whose rail is gone.
    ///
    /// Called from the rail-graph invalidation, which is exactly the moment a
    /// rail can be mined or blown up. Doing it there rather than lazily in the
    /// tick keeps the state valid between ticks too, so a world saved right
    /// after a train's track was destroyed still loads. Mining the track out
    /// from under a train destroys the train with it — the same bargain
    /// deconstructing anything else with contents makes, and the reason the
    /// checks here are graph-free: the graph has already been discarded by the
    /// time this runs.
    pub(in crate::simulation) fn prune_rolling_stock(&mut self) {
        if self.rolling_stock.is_empty() {
            return;
        }
        let stranded = self
            .rolling_stock
            .iter()
            .filter(|stock| self.rail_piece_geometry(stock.position.edge).is_none())
            .map(|stock| stock.id)
            .collect::<Vec<_>>();
        for stock_id in stranded {
            self.remove_rolling_stock(stock_id);
        }
    }

    /// Where a new piece of `prototype_id` would stand if it were placed on the
    /// rail under `(x, y)`.
    fn rolling_stock_placement_position(
        &self,
        prototype_id: EntityPrototypeId,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Result<RailPosition, RollingStockPlacementError> {
        let length = self.stock_length(prototype_id).unwrap_or(0);
        let rail = self
            .entities
            .occupancy
            .entity_at(x, y)
            .filter(|entity_id| self.rail_piece_geometry(*entity_id).is_some())
            .ok_or(RollingStockPlacementError::NoRail)?;
        let edge = self
            .rails
            .graph
            .edge_for_entity(rail)
            .ok_or(RollingStockPlacementError::NoRail)?;

        // The middle of the clicked rail is the anchor. The snap below moves
        // the piece from there onto the end of whatever train is already here,
        // so the anchor only has to name the stretch of track, not the spot.
        let anchor = RailPosition::new(rail, edge.length_fixed / 2, true);
        let position = self.snapped_placement_position(anchor, length);

        let (back, front) = super::stock_ends(&self.rails.graph, position, length);
        if back.is_blocked() || front.is_blocked() {
            return Err(RollingStockPlacementError::TrackTooShort);
        }
        if let Some((stock_id, _)) = self
            .stock_offsets_within(position, COUPLING_SEARCH_FIXED)
            .into_iter()
            .find(|(stock_id, offset)| self.stock_overlaps(*stock_id, *offset, length))
        {
            return Err(RollingStockPlacementError::Occupied(stock_id));
        }

        Ok(position)
    }

    /// Moves a prospective placement onto the end of the train already on this
    /// stretch of track, if there is one.
    ///
    /// Which end depends on where the anchor fell: a click ahead of the train
    /// attaches in front of it and a click behind attaches behind, so a player
    /// builds a train by clicking along it in the direction they want it to
    /// grow.
    fn snapped_placement_position(&self, anchor: RailPosition, length: i64) -> RailPosition {
        let offsets = self.stock_offsets_within(anchor, COUPLING_SEARCH_FIXED);
        let Some(&(nearest, _)) = offsets
            .iter()
            .filter(|(_, offset)| offset.abs() <= COUPLING_SNAP_FIXED)
            .min_by_key(|(stock_id, offset)| (offset.abs(), *stock_id))
        else {
            return anchor;
        };
        let Some(train_id) = self.rolling_stock.get(nearest).map(|stock| stock.train) else {
            return anchor;
        };

        let members = offsets
            .into_iter()
            .filter(|(stock_id, _)| {
                self.rolling_stock
                    .get(*stock_id)
                    .is_some_and(|stock| stock.train == train_id)
            })
            .collect::<Vec<_>>();
        let Some(&(high, high_offset)) = members.iter().max_by_key(|(_, offset)| *offset) else {
            return anchor;
        };
        let &(low, low_offset) = members
            .iter()
            .min_by_key(|(_, offset)| *offset)
            .expect("the members list is non-empty");

        // Attach beyond whichever end of the train the anchor is nearer. For an
        // anchor outside the train that is simply the end facing it; for one
        // that fell between two of its pieces, the shorter way out.
        let attach_ahead = low_offset < 0 && high_offset.abs() <= low_offset.abs();
        let (neighbor, neighbor_offset, side) = if attach_ahead {
            (high, high_offset, 1)
        } else {
            (low, low_offset, -1)
        };
        let Some(neighbor_length) = self
            .rolling_stock
            .get(neighbor)
            .and_then(|stock| self.stock_length(stock.prototype_id))
        else {
            return anchor;
        };

        let offset = neighbor_offset + side * coupled_spacing_fixed(neighbor_length, length);
        let outcome = travel(&self.rails.graph, anchor, offset);
        if outcome.is_blocked() {
            return anchor;
        }
        self.aligned_with_train(outcome.position, train_id)
    }

    /// Turns a prospective position to face the same way the train it is
    /// joining does.
    ///
    /// Stock of one train must agree about which way is forwards, or a shared
    /// velocity would drive the pieces apart on the first tick.
    fn aligned_with_train(&self, position: RailPosition, train_id: TrainId) -> RailPosition {
        let Some(front_sign) = self.train_front_sign(position, train_id, COUPLING_SEARCH_FIXED)
        else {
            return position;
        };
        let reach = self.stock_offsets_reach(position, COUPLING_SEARCH_FIXED);
        let own_direction = reach
            .get(&position.edge)
            .map_or(1, |reach: &EdgeReach| reach.direction);
        let forward = own_direction * front_sign == 1;
        RailPosition {
            forward,
            ..position
        }
    }

    /// Which way along the offset axis the train's front points, measured from
    /// `origin`.
    ///
    /// The radius is the caller's: a placement asks about the piece it is
    /// snapping to and needs only the pairwise reach, while ordering a whole
    /// train has to walk the train's own extent or the answer comes from
    /// whichever member happened to fall inside a shorter walk.
    fn train_front_sign(
        &self,
        origin: RailPosition,
        train_id: TrainId,
        max_distance: i64,
    ) -> Option<i64> {
        let reach = self.stock_offsets_reach(origin, max_distance);
        let train = self.rolling_stock.train(train_id)?;
        train.stock.iter().find_map(|stock_id| {
            let stock = self.rolling_stock.get(*stock_id)?;
            let edge = reach.get(&stock.position.edge)?;
            Some(edge.direction * if stock.position.forward { 1 } else { -1 })
        })
    }

    /// Whether a piece of stock `offset` away along the track would be inside a
    /// body of `length` centred at the origin of that measurement.
    fn stock_overlaps(&self, stock_id: RollingStockId, offset: i64, length: i64) -> bool {
        let Some(other_length) = self
            .rolling_stock
            .get(stock_id)
            .and_then(|stock| self.stock_length(stock.prototype_id))
        else {
            return false;
        };
        offset.abs() * 2 < other_length + length
    }

    /// An upper bound on the along-track span of a train.
    ///
    /// Anything that measures a *whole train* — ordering it front to back,
    /// regrouping it after an uncoupling, asking how far it may move — has to
    /// walk at least this far, or the pieces past the end of the walk go
    /// unpriced. The pairwise [`COUPLING_SEARCH_FIXED`] is the wrong radius for
    /// those: it is about one piece and its neighbour, and a train of six
    /// wagons is already longer than it.
    pub(super) fn train_extent_fixed(&self, stock_ids: &[RollingStockId]) -> i64 {
        stock_ids
            .iter()
            .filter_map(|stock_id| {
                let stock = self.rolling_stock.get(*stock_id)?;
                self.stock_length(stock.prototype_id)
            })
            .map(|length| length + crate::rolling_stock::TRAIN_COUPLING_GAP_FIXED * 2)
            .sum()
    }

    /// How far a whole-train measurement has to walk: the train's own extent,
    /// plus the pairwise radius so the stock just past either end is priced too.
    fn train_walk_radius(&self, stock_ids: &[RollingStockId]) -> i64 {
        self.train_extent_fixed(stock_ids) + COUPLING_SEARCH_FIXED
    }

    /// How far this train may travel before it would run into stock that is not
    /// part of it.
    ///
    /// Placement refuses to put two pieces in the same place, and the tick has
    /// to keep that true: without this, two trains sharing a run pass straight
    /// through each other — a fast one through a slow one it caught, or two
    /// head-on. The measurement is the same along-track one placement uses, so
    /// the moving case and the placing case cannot disagree about what counts
    /// as touching.
    ///
    /// One walk and one pass over the world's stock per train, not per piece:
    /// every piece of the train is priced in the same frame by that single
    /// walk.
    pub(super) fn train_clearance_fixed(
        &self,
        train_id: TrainId,
        stock_ids: &[RollingStockId],
        travel_fixed: i64,
    ) -> i64 {
        if travel_fixed == 0 {
            return 0;
        }
        // Nothing to run into. Worth checking because the walk below is the one
        // piece of per-tick work that is proportional to the world's stock
        // rather than to the train, and a world with a single train — which is
        // most of them, for most of a game — should not pay for it at all.
        if self.rolling_stock.train_count() < 2 {
            return travel_fixed;
        }
        let Some(origin) = stock_ids
            .first()
            .and_then(|stock_id| self.rolling_stock.get(*stock_id))
            .map(|stock| stock.position)
        else {
            return travel_fixed;
        };

        let radius = self.train_walk_radius(stock_ids) + travel_fixed.abs();
        let priced = self.stock_offsets_within(origin, radius);
        let mine = priced
            .iter()
            .filter(|(stock_id, _)| {
                self.rolling_stock
                    .get(*stock_id)
                    .is_some_and(|stock| stock.train == train_id)
            })
            .filter_map(|(stock_id, offset)| {
                let stock = self.rolling_stock.get(*stock_id)?;
                Some((self.stock_length(stock.prototype_id)?, *offset))
            })
            .collect::<Vec<_>>();

        let sign = travel_fixed.signum();
        let mut allowed = travel_fixed;
        for (stock_id, offset) in &priced {
            let Some(stock) = self.rolling_stock.get(*stock_id) else {
                continue;
            };
            if stock.train == train_id {
                continue;
            }
            let Some(other_length) = self.stock_length(stock.prototype_id) else {
                continue;
            };
            for (length, own_offset) in &mine {
                let separation = offset - own_offset;
                // Only what lies ahead can be run into; anything behind is
                // opening up, and a piece level with ours cannot exist.
                if separation.signum() != sign {
                    continue;
                }
                let gap = separation.abs() - (length + other_length) / 2;
                let reachable =
                    sign * (gap - crate::rolling_stock::TRAIN_COUPLING_GAP_FIXED).max(0);
                if reachable.abs() < allowed.abs() {
                    allowed = reachable;
                }
            }
        }
        allowed
    }

    /// Signed along-track offsets from `origin` to every piece of stock within
    /// `max_distance` of it.
    ///
    /// One walk of the reachable track prices every piece on it, so the cost is
    /// the track around the origin rather than the world's stock count.
    fn stock_offsets_within(
        &self,
        origin: RailPosition,
        max_distance: i64,
    ) -> Vec<(RollingStockId, i64)> {
        let reach = self.stock_offsets_reach(origin, max_distance);
        if reach.is_empty() {
            return Vec::new();
        }
        self.rolling_stock
            .iter()
            .filter_map(|stock| {
                let edge = reach.get(&stock.position.edge)?;
                let offset = edge.offset_of(stock.position.distance_fixed);
                (offset.abs() <= max_distance).then_some((stock.id, offset))
            })
            .collect()
    }

    fn stock_offsets_reach(
        &self,
        origin: RailPosition,
        max_distance: i64,
    ) -> BTreeMap<EntityId, EdgeReach> {
        let mut reach = BTreeMap::new();
        walk_reach(&self.rails.graph, origin, max_distance, 1, &mut reach);
        walk_reach(
            &self.rails.graph,
            origin.reversed(),
            max_distance,
            -1,
            &mut reach,
        );
        reach
    }

    fn stock_length(&self, prototype_id: EntityPrototypeId) -> Option<i64> {
        Some(i64::from(
            self.world
                .prototypes
                .entity(prototype_id)?
                .rolling_stock?
                .length_fixed,
        ))
    }

    /// Adds a piece of stock to the world, coupling it into a neighbouring
    /// train or giving it one of its own.
    fn insert_rolling_stock(
        &mut self,
        prototype_id: EntityPrototypeId,
        position: RailPosition,
    ) -> RollingStockId {
        let prototype = &self.world.prototypes.entities[prototype_id.index()];
        let inventory = prototype
            .inventory_slot_count
            .map(Inventory::with_slot_count);
        let fluid_boxes = vec![FluidBoxState::default(); prototype.fluid_boxes.len()];
        let energy = prototype.burner.as_ref().map(|burner| BurnerEnergy {
            fuel_slot: ItemSlot::default(),
            energy_remaining_joules: 0.0,
            energy_usage_watts: burner.energy_usage_watts as f64,
        });

        let train_id = self
            .coupling_train_for(position, prototype_id)
            .unwrap_or_else(|| {
                let train_id = self.rolling_stock.allocate_train_id();
                self.rolling_stock.trains.insert(
                    train_id,
                    Train {
                        id: train_id,
                        stock: Vec::new(),
                        velocity: 0,
                        travel_remainder: 0,
                        throttle: TrainThrottle::Coast,
                        destination: None,
                        route: None,
                        route_search_exhausted_at: None,
                        schedule: Default::default(),
                        schedule_arrival_tick: None,
                        schedule_last_activity_tick: None,
                        scheduled_stop: None,
                        reserved_blocks: Vec::new(),
                    },
                );
                train_id
            });

        let stock_id = self.rolling_stock.allocate_stock_id();
        self.rolling_stock.stock.insert(
            stock_id,
            RollingStock {
                id: stock_id,
                prototype_id,
                train: train_id,
                position,
                inventory,
                fluid_boxes,
                energy,
            },
        );
        self.rolling_stock
            .trains
            .get_mut(&train_id)
            .expect("the train was just created or found")
            .stock
            .push(stock_id);
        self.reorder_train(train_id);
        self.discard_train_route(train_id);
        stock_id
    }

    /// The train a piece placed at `position` would couple into: the one
    /// holding a piece exactly a coupling apart from it.
    fn coupling_train_for(
        &self,
        position: RailPosition,
        prototype_id: EntityPrototypeId,
    ) -> Option<TrainId> {
        let length = self.stock_length(prototype_id)?;
        self.stock_offsets_within(position, COUPLING_SEARCH_FIXED)
            .into_iter()
            .filter_map(|(stock_id, offset)| {
                let stock = self.rolling_stock.get(stock_id)?;
                let other_length = self.stock_length(stock.prototype_id)?;
                let gap = offset.abs() - (length + other_length) / 2;
                (0..=crate::rolling_stock::TRAIN_COUPLING_GAP_FIXED * 2)
                    .contains(&gap)
                    .then_some((offset.abs(), stock.train))
            })
            .min()
            .map(|(_, train_id)| train_id)
    }

    /// Removes a piece of stock and repairs the train it left behind.
    fn remove_rolling_stock(&mut self, stock_id: RollingStockId) {
        let Some(stock) = self.rolling_stock.stock.remove(&stock_id) else {
            return;
        };
        let Some(train) = self.rolling_stock.trains.get_mut(&stock.train) else {
            return;
        };
        train.stock.retain(|id| *id != stock_id);
        if train.stock.is_empty() {
            self.rolling_stock.trains.remove(&stock.train);
            return;
        }
        self.split_train(stock.train);
    }

    /// Orders a train's stock front to back, which is the order the list
    /// promises and the order any per-train presentation reads it in.
    fn reorder_train(&mut self, train_id: TrainId) {
        let Some(train) = self.rolling_stock.train(train_id) else {
            return;
        };
        let Some(origin) = train
            .stock
            .first()
            .and_then(|stock_id| self.rolling_stock.get(*stock_id))
            .map(|stock| stock.position)
        else {
            return;
        };
        let radius = self.train_walk_radius(&train.stock);
        let Some(front_sign) = self.train_front_sign(origin, train_id, radius) else {
            return;
        };
        let offsets = self
            .stock_offsets_within(origin, radius)
            .into_iter()
            .collect::<BTreeMap<_, _>>();

        let train = self
            .rolling_stock
            .trains
            .get_mut(&train_id)
            .expect("the train was just read");
        // A member the walk could not price is not on this stretch of track at
        // all, so it has no place in a front-to-back order; it sorts to the back
        // rather than being given a fabricated offset of zero, which would drop
        // it into the middle of the train.
        train
            .stock
            .sort_by_key(|stock_id| match offsets.get(stock_id) {
                Some(offset) => (0, -front_sign * offset, *stock_id),
                None => (1, 0, *stock_id),
            });
    }

    /// Re-derives the trains a run of stock forms after a piece left it.
    ///
    /// Removing a wagon from the middle of a train leaves two trains, and the
    /// gap that separates them is the same coupling rule placement used. Groups
    /// after the first get fresh ids; the first keeps the old one, so a train
    /// the player is driving keeps its identity when the tail is uncoupled.
    fn split_train(&mut self, train_id: TrainId) {
        let Some(train) = self.rolling_stock.train(train_id) else {
            return;
        };
        let Some(origin) = train
            .stock
            .first()
            .and_then(|stock_id| self.rolling_stock.get(*stock_id))
            .map(|stock| stock.position)
        else {
            return;
        };
        let velocity = train.velocity;
        let throttle = train.throttle;
        let members = train.stock.clone();

        // The walk has to cover the whole train, not a pairwise radius: a train
        // longer than that would leave its far pieces unpriced, and grouping
        // them as though they all stood on the origin would either hold a
        // severed tail in the train or cut an intact one out of it.
        let offsets = self
            .stock_offsets_within(origin, self.train_walk_radius(&members))
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let mut ordered = members
            .iter()
            .filter_map(|stock_id| {
                let stock = self.rolling_stock.get(*stock_id)?;
                let length = self.stock_length(stock.prototype_id)?;
                Some((offsets.get(stock_id).copied()?, *stock_id, length))
            })
            .collect::<Vec<_>>();
        ordered.sort_by_key(|(offset, stock_id, _)| (*offset, *stock_id));

        let mut groups: Vec<Vec<RollingStockId>> = Vec::new();
        let mut previous: Option<(i64, i64)> = None;
        for (offset, stock_id, length) in ordered {
            let coupled = previous.is_some_and(|(previous_offset, previous_length)| {
                let gap = (offset - previous_offset).abs() - (previous_length + length) / 2;
                gap <= crate::rolling_stock::TRAIN_COUPLING_GAP_FIXED * 2
            });
            if coupled {
                groups
                    .last_mut()
                    .expect("a coupled piece follows an existing group")
                    .push(stock_id);
            } else {
                groups.push(vec![stock_id]);
            }
            previous = Some((offset, length));
        }
        // A member the walk could not reach at all is no longer connected to the
        // rest by track — the rail between them went — so it is its own train
        // rather than a coupling that happens to be unmeasurable.
        for stock_id in &members {
            if !offsets.contains_key(stock_id) {
                groups.push(vec![*stock_id]);
            }
        }

        for (index, group) in groups.into_iter().enumerate() {
            let group_id = if index == 0 {
                train_id
            } else {
                let group_id = self.rolling_stock.allocate_train_id();
                self.rolling_stock.trains.insert(
                    group_id,
                    Train {
                        id: group_id,
                        stock: Vec::new(),
                        velocity,
                        travel_remainder: 0,
                        throttle,
                        // A train that split in two is two trains with no plan:
                        // the route belonged to the run of stock that was one
                        // train when it was found, and neither half is that.
                        destination: None,
                        route: None,
                        route_search_exhausted_at: None,
                        schedule: Default::default(),
                        schedule_arrival_tick: None,
                        schedule_last_activity_tick: None,
                        scheduled_stop: None,
                        // Nor does either half hold what the whole train held:
                        // the signalling pass hands each of them the blocks it
                        // is standing in on the very next tick, and a claim
                        // ahead belonged to a train that no longer exists.
                        reserved_blocks: Vec::new(),
                    },
                );
                group_id
            };
            for stock_id in &group {
                if let Some(stock) = self.rolling_stock.stock.get_mut(stock_id) {
                    stock.train = group_id;
                }
            }
            self.rolling_stock
                .trains
                .get_mut(&group_id)
                .expect("the group's train was just created or kept")
                .stock = group;
            self.reorder_train(group_id);
            self.discard_train_route(group_id);
        }
    }
}

/// Walks outward from `origin` recording how to price each rail edge it
/// reaches, stopping at free ends, at `max_distance`, and at track it has
/// already priced — which is what keeps a closed loop from walking forever.
fn walk_reach(
    graph: &RailGraph,
    origin: RailPosition,
    max_distance: i64,
    sign: i64,
    reach: &mut BTreeMap<EntityId, EdgeReach>,
) {
    let mut current = origin;
    let mut travelled = 0_i64;
    loop {
        let Some(edge) = graph.edge_for_entity(current.edge) else {
            return;
        };
        let direction = sign * if current.forward { 1 } else { -1 };
        let entry = EdgeReach {
            entry_offset: sign * travelled,
            entry_distance: current.distance_fixed,
            direction,
        };
        // Track already priced by a closer route has nothing better to say, and
        // stopping there is also what keeps a closed loop from walking forever.
        // The two walks meet on the starting edge at offset zero, where both
        // price it identically, so that meeting is not a revisit.
        let superseded = reach
            .get(&current.edge)
            .is_some_and(|priced| priced.entry_offset.abs() <= entry.entry_offset.abs());
        if superseded && travelled > 0 {
            return;
        }
        reach.insert(current.edge, entry);

        let to_end = if current.forward {
            edge.length_fixed - current.distance_fixed
        } else {
            current.distance_fixed
        };
        travelled += to_end;
        if travelled >= max_distance {
            return;
        }
        let Some((next_index, arrival_end)) =
            graph.neighbor_end(edge, usize::from(current.forward))
        else {
            return;
        };
        let next = &graph.edges[next_index];
        let forward = arrival_end == 0;
        current = RailPosition {
            edge: next.entity_id,
            distance_fixed: if forward { 0 } else { next.length_fixed },
            forward,
        };
    }
}
