use super::super::*;
use crate::rolling_stock::{RollingStock, TRAIN_VELOCITY_SCALE};

/// Checks that every piece of rolling stock is on the rails and that the trains
/// and their stock agree about who belongs to whom.
///
/// The two halves reference each other — a piece names its train, and that
/// train's list names the piece — so both directions are checked here. The rest
/// of the simulation treats "this stock is on a live edge at an in-range
/// distance" as a fact: the tick walks the graph from that position without
/// re-checking it, and the renderer derives a world point from it. A save that
/// broke the invariant would produce a train standing on nothing, and nothing
/// later would report it.
pub(super) fn validate_rolling_stock(sim: &Simulation) -> Result<(), SimValidationError> {
    for (stop_id, stop) in &sim.rolling_stock.stops {
        if stop.id != *stop_id
            || stop_id.raw() > sim.rolling_stock.next_stop_id
            || stop.name.trim().is_empty()
            || stop.train_limit == 0
            || sim
                .rail_piece_geometry(stop.target.edge)
                .is_none_or(|geometry| {
                    !(0..=geometry.length_fixed).contains(&stop.target.distance_fixed)
                })
        {
            return Err(SimValidationError::InvalidTrainStop { stop_id: *stop_id });
        }
    }
    for (stock_id, stock) in &sim.rolling_stock.stock {
        let invalid = || SimValidationError::InvalidRollingStock {
            stock_id: *stock_id,
        };
        if stock.id != *stock_id || stock_id.raw() > sim.rolling_stock.next_stock_id {
            return Err(invalid());
        }
        let prototype = sim
            .world
            .prototypes
            .entity(stock.prototype_id)
            .ok_or_else(invalid)?;
        // Rolling-stock metadata is what the motion model divides by, so a
        // piece whose prototype does not declare it could never be stepped.
        prototype.rolling_stock.ok_or_else(invalid)?;

        // On the rails: the edge is a live rail piece, and the distance is
        // somewhere on it rather than past either end.
        let geometry = sim
            .rail_piece_geometry(stock.position.edge)
            .ok_or_else(invalid)?;
        if !(0..=geometry.length_fixed).contains(&stock.position.distance_fixed) {
            return Err(invalid());
        }

        // Cargo exists exactly where the prototype declares it, with the shape
        // it declares. A mismatch means the save and the catalog disagree about
        // what this wagon is.
        let declared_slots = prototype.inventory_slot_count;
        match (&stock.inventory, declared_slots) {
            (Some(inventory), Some(slot_count)) if inventory.slots().len() == slot_count => {
                super::inventory::validate_inventory(&sim.world.prototypes, inventory)?;
            }
            (None, None) => {}
            _ => return Err(invalid()),
        }
        if stock.fluid_boxes.len() != prototype.fluid_boxes.len() {
            return Err(invalid());
        }
        for (state, declared) in stock.fluid_boxes.iter().zip(&prototype.fluid_boxes) {
            let holds_unknown_fluid = state
                .fluid_id
                .is_some_and(|fluid_id| sim.world.prototypes.fluid(fluid_id).is_none());
            if holds_unknown_fluid
                || state.amount_milliunits > declared.capacity_milliunits
                || (state.fluid_id.is_none() && state.amount_milliunits != 0)
            {
                return Err(invalid());
            }
        }
        if stock.energy.is_some() != prototype.burner.is_some() {
            return Err(invalid());
        }
        if let Some(stack) = stock
            .energy
            .as_ref()
            .and_then(|energy| energy.fuel_slot.stack())
            && ItemStack::new(&sim.world.prototypes, stack.item_id(), stack.count()).is_err()
        {
            return Err(invalid());
        }

        if sim
            .rolling_stock
            .trains
            .get(&stock.train)
            .is_none_or(|train| !train.stock.contains(stock_id))
        {
            return Err(invalid());
        }
    }

    // Who holds each block, built up as the trains are walked. A claim is
    // exclusive, and nothing later re-checks it: the signalling pass seeds itself
    // from these and then lets a train run to the far end of what it holds, so a
    // save with two trains on one claim is two trains driving at each other with
    // nothing to stop them.
    let mut claimants = BTreeMap::<EntityId, TrainId>::new();

    for (train_id, train) in &sim.rolling_stock.trains {
        let invalid = || SimValidationError::InvalidTrain {
            train_id: *train_id,
        };
        if train.id != *train_id
            || train_id.raw() > sim.rolling_stock.next_train_id
            // A train with no stock is a train that should have been dropped
            // when its last piece was mined; nothing could ever remove it later.
            || train.stock.is_empty()
        {
            return Err(invalid());
        }
        if (train.schedule.entries.is_empty() && train.schedule.current != 0)
            || (!train.schedule.entries.is_empty()
                && train.schedule.current >= train.schedule.entries.len())
            || train
                .schedule
                .entries
                .iter()
                .any(|entry| entry.stop_name.trim().is_empty())
            || train
                .scheduled_stop
                .is_some_and(|stop| !sim.rolling_stock.stops.contains_key(&stop))
        {
            return Err(invalid());
        }
        // Wait state only means something about a train standing at a stop it
        // claimed. An arrival tick without a claim is a train timing a wait at
        // nothing; wait state without an arrival is a clock nothing started; a
        // tick in the future is state no tick could have written; and activity
        // before arrival would date the inactivity clock to another visit. All
        // four are read straight by the schedule pass, which departs a train on
        // the strength of them.
        let arrival = train.schedule_arrival_tick;
        let activity = train.schedule_last_activity_tick;
        if arrival.is_some() && train.scheduled_stop.is_none()
            || arrival.is_none() && (activity.is_some() || train.schedule_activity_cargo.is_some())
            || activity < arrival
            || [arrival, activity]
                .into_iter()
                .flatten()
                .any(|recorded| recorded > sim.tick_count())
        {
            return Err(invalid());
        }
        // The remainder is the sub-unit part of a tick's travel, so a value at
        // or beyond a whole unit is travel the train was owed and never paid.
        if !(0..TRAIN_VELOCITY_SCALE).contains(&train.travel_remainder) {
            return Err(invalid());
        }

        let mut seen = BTreeSet::new();
        for stock_id in &train.stock {
            if !seen.insert(*stock_id)
                || sim
                    .rolling_stock
                    .stock
                    .get(stock_id)
                    .is_none_or(|stock: &RollingStock| stock.train != *train_id)
            {
                return Err(invalid());
            }
        }

        // Where a train is going and the plan it is driving both name rails, and
        // both are followed without being re-checked: the step spends the leg's
        // distance and the routing pass re-plans against the destination. A save
        // naming track that is not there would drive a train toward nothing.
        // The mark is checked the same way a piece of stock's own position is,
        // and for the same reason: a route is measured to it, so a mark past the
        // end of its rail would produce a leg that drives a train off the track
        // — or, on the rail the train is already standing on, a negative
        // distance no command could ask for.
        if let Some(destination) = train.destination {
            let geometry = sim
                .rail_piece_geometry(destination.edge)
                .ok_or_else(invalid)?;
            if !(0..=geometry.length_fixed).contains(&destination.distance_fixed) {
                return Err(invalid());
            }
        }
        if let Some(route) = &train.route {
            // A route with no destination is a plan nobody asked for, and a route
            // with no legs is one that should have been retired the tick it ran
            // out. Neither can be produced by the routing pass.
            if train.destination.is_none() || route.legs.is_empty() || route.edges.is_empty() {
                return Err(invalid());
            }
            if route
                .edges
                .iter()
                .any(|edge| sim.rail_piece_geometry(*edge).is_none())
            {
                return Err(invalid());
            }
            // A route ends on the rail the train was sent to. Without this a
            // save could hold a plan that runs somewhere else entirely — a
            // single zero-distance leg on the rail underneath, say — which the
            // first tick would retire as an arrival, clearing a destination the
            // train never went anywhere near.
            if route.edges.last() != train.destination.map(|target| target.edge).as_ref() {
                return Err(invalid());
            }
            // Legs are distances still to run, and consecutive legs always
            // disagree about direction — a leg boundary is a reversal, so two in
            // a row driving the same way would be one leg written twice.
            if route.legs.iter().any(|leg| leg.distance_fixed < 0)
                || route
                    .legs
                    .iter()
                    .zip(route.legs.iter().skip(1))
                    .any(|(leg, next)| leg.forward == next.forward)
            {
                return Err(invalid());
            }
            // What ties the plan to the train that is driving it is the distance
            // still to run: it is at least the straight line from the train to
            // the mark, because track between two points is never shorter than
            // the line between them. That is what rejects a plan claiming a train
            // standing somewhere else is already there — a single zero-distance
            // leg, which the first tick would retire as an arrival without the
            // train having moved at all.
            //
            // Nothing stronger than a distance holds. The tempting checks — that
            // the route's rails run under the train, or that what is left to run
            // fits on the rails the route names, each crossed once — are true of
            // a plan as it was found and false a tick later. A train sent
            // somewhere behind itself keeps rolling the old way while it brakes,
            // off the end of the track its plan listed, and every unit of that is
            // a unit added to the leg it has yet to turn onto. How far it gets is
            // the motion model's answer rather than the route's, so nothing here
            // can bound it without re-deriving the tick that produced it — and a
            // check that rejects what the simulation itself does is a debug
            // build panicking in the middle of an ordinary manoeuvre.
            let remaining = route.legs.iter().map(|leg| leg.distance_fixed).sum::<i64>();
            if remaining < straight_line_to_mark(sim, train)? {
                return Err(invalid());
            }
        }
        // A train that has stopped asking is a train with somewhere to be and no
        // plan for getting there. Either half missing would leave a mark nothing
        // ever clears, on a train nothing is waiting to plan for.
        if train.route_search_exhausted_at.is_some()
            && (train.destination.is_none() || train.route.is_some())
        {
            return Err(invalid());
        }

        // What a train holds has to be blocks that exist, held by nobody else,
        // and named once each. All three are facts the signalling pass assumes:
        // it seeds its index from these claims without re-deriving them, which is
        // the whole point of saving them, so a save that broke any of the three
        // would put two trains into one block or hand a train a stretch of
        // railway that is no longer there.
        //
        // Ascending is checked rather than merely "no duplicates", because that
        // is the canonical form the pass writes and a save in any other order
        // came from somewhere else.
        //
        // While the graph is dirty the answer is stricter still: invalidating it
        // gives every claim back, so a world mid-placement holds none at all.
        // Checking that here rather than skipping the block checks is what keeps
        // the invariant a statement about every world and not only about
        // conveniently-timed ones.
        if sim.rails.graph_dirty {
            if !train.reserved_blocks.is_empty() {
                return Err(invalid());
            }
        } else {
            for (index, block) in train.reserved_blocks.iter().enumerate() {
                if sim.rails.blocks.block(*block).is_none()
                    || index > 0 && train.reserved_blocks[index - 1] >= *block
                    || claimants.insert(*block, *train_id).is_some()
                {
                    return Err(invalid());
                }
            }
        }

        // A train may not exceed the top speed of its slowest piece; the step
        // clamps to it every tick, so a value above it could only come from a
        // hand-edited save or a catalog the world no longer matches.
        let max_speed = train
            .stock
            .iter()
            .filter_map(|stock_id| {
                let stock = sim.rolling_stock.get(*stock_id)?;
                let rolling_stock = sim
                    .world
                    .prototypes
                    .entity(stock.prototype_id)?
                    .rolling_stock?;
                Some(i64::from(rolling_stock.max_speed_fixed_per_tick) * TRAIN_VELOCITY_SCALE)
            })
            .min()
            .unwrap_or(0);
        if train.velocity.unsigned_abs() > max_speed.unsigned_abs() {
            return Err(invalid());
        }
    }

    Ok(())
}

/// The straight line between a train's leading piece and the mark it was sent
/// to, in fixed-point units, rounded down.
///
/// A lower bound on the track still to run, in the same sense the route search's
/// own estimate is one: rails run between their ends rather than through the
/// ground, so no route can be shorter than the line it spans. Rounded down, so
/// the bound stays a bound.
fn straight_line_to_mark(
    sim: &Simulation,
    train: &crate::rolling_stock::Train,
) -> Result<i64, SimValidationError> {
    let invalid = || SimValidationError::InvalidTrain { train_id: train.id };
    let destination = train.destination.ok_or_else(invalid)?;
    let standing_on = train
        .stock
        .first()
        .and_then(|stock_id| sim.rolling_stock.get(*stock_id))
        .map(|stock| stock.position)
        .ok_or_else(invalid)?;
    let from = rolling_stock_ops::world_point(sim, standing_on).ok_or_else(invalid)?;
    let to = rolling_stock_ops::world_point(
        sim,
        RailPosition::new(destination.edge, destination.distance_fixed, true),
    )
    .ok_or_else(invalid)?;

    let dx = i128::from(from.x) - i128::from(to.x);
    let dy = i128::from(from.y) - i128::from(to.y);
    Ok((dx * dx + dy * dy).isqrt() as i64)
}
