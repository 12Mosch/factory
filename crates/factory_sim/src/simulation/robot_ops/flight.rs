//! The moving half of the robot subsystem: robots leaving a roboport, flying a
//! straight line, spending energy, charging, and docking again.
//!
//! Three properties shape everything here.
//!
//! * **Off-grid.** Robots are not placed entities. They never touch
//!   `OccupancyGrid` or `DenseEntityMap`, they fly over anything, and they do
//!   not collide with each other — the only ordering that matters is the
//!   [`crate::robots::RobotId`] order the store iterates in.
//! * **O(1) per robot per tick.** A step is one distance computation and one
//!   move. The searches that are not constant time (finding a roboport to
//!   charge at, adopting a new home) run on transitions, not every tick.
//! * **Energy is a roboport's energy.** A robot charges out of the roboport's
//!   buffer, never out of the electric network directly, so robot throughput is
//!   bounded by what [`Simulation::advance_robot_networks`] managed to buffer.

use crate::robots::{
    RoboportChargingState, Robot, RobotActivity, RobotDispatchError, RobotFlightSubsystem, RobotId,
};
use crate::simulation::*;

use super::types::RobotNetworkTopology;

/// Speed a robot with an empty buffer crawls at, in permille of its normal
/// speed.
///
/// A robot that ran dry still has to get somewhere: stalling it in mid-air
/// would strand it forever, since nothing else in the world can reach it. The
/// crawl is the escape hatch, and being slow is what makes running dry a
/// mistake worth avoiding rather than a free ride.
const ROBOT_EMPTY_SPEED_PERMILLE: u32 = 200;

impl Simulation {
    /// Advances every robot in flight by one tick.
    ///
    /// Pads are assigned before robots step so a robot that arrived last tick
    /// starts charging this tick rather than idling one tick per hop through
    /// the queue.
    pub(in crate::simulation) fn advance_robots(&mut self) {
        self.assign_charging_pads();
        self.step_robots();
        // Charging and docking both changed roboport state the network
        // snapshots summarize, so they settle again before anything reads them.
        self.refresh_robot_network_snapshots();
    }

    /// Robots currently in flight, in ascending id order.
    pub fn robots(&self) -> impl Iterator<Item = &Robot> {
        self.robot_flights.iter()
    }

    pub fn robot(&self, robot_id: RobotId) -> Option<&Robot> {
        self.robot_flights.get(robot_id)
    }

    pub fn robot_count(&self) -> usize {
        self.robot_flights.len()
    }

    /// Charging occupancy of one roboport, or `None` while nothing is charging
    /// or queued there.
    pub fn roboport_charging_state(&self, roboport: EntityId) -> Option<&RoboportChargingState> {
        self.robot_flights.charging_state(roboport)
    }

    /// Sends one stationed robot from `roboport` to the tile `(x, y)`, from
    /// which it returns on its own.
    ///
    /// The robot leaves fully charged, paid for out of the roboport's buffer:
    /// that is what ties dispatch to the electric network, and what makes an
    /// under-powered network refuse to send robots instead of sending ones that
    /// strand themselves immediately.
    pub fn dispatch_robot(
        &mut self,
        roboport: EntityId,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Result<RobotId, RobotDispatchError> {
        if self.entities.placed_entity(roboport).is_none() {
            return Err(RobotDispatchError::MissingEntity(roboport));
        }
        let state = self
            .entities
            .roboports
            .get(&roboport)
            .ok_or(RobotDispatchError::NotRoboport(roboport))?;
        let item_id = first_stationed_robot(&self.world.prototypes, state)
            .ok_or(RobotDispatchError::NoRobotAvailable)?;
        let profile = self
            .world
            .prototypes
            .item(item_id)
            .and_then(|item| item.robot)
            .ok_or(RobotDispatchError::InvalidRobot(item_id))?;
        if state.charge_energy_joules < profile.energy_capacity_joules {
            return Err(RobotDispatchError::InsufficientCharge {
                required_joules: profile.energy_capacity_joules,
                available_joules: state.charge_energy_joules,
            });
        }
        let dock = roboport_dock_position(&self.entities, roboport)
            .ok_or(RobotDispatchError::MissingEntity(roboport))?;

        let state = self
            .entities
            .roboports
            .get_mut(&roboport)
            .expect("the roboport was just read");
        state
            .robots
            .remove(item_id, 1)
            .expect("the robot stack was just read from these slots");
        state.charge_energy_joules -= profile.energy_capacity_joules;
        self.robots.mark_roboport_dirty(roboport);

        let id = self.robot_flights.allocate_id();
        self.robot_flights.robots.insert(
            id,
            Robot {
                id,
                item_id,
                x: dock.0,
                y: dock.1,
                energy_joules: profile.energy_capacity_joules,
                home_roboport: Some(roboport),
                errand: Some((tile_center_fixed(x), tile_center_fixed(y))),
                activity: RobotActivity::Flying,
            },
        );
        Ok(id)
    }

    /// Drops flight state that points at roboports which no longer exist.
    ///
    /// Called from the robot-network invalidation, which is exactly the moment
    /// a roboport can appear or disappear. Doing it there rather than lazily in
    /// the tick keeps the state valid between ticks too, so a world saved right
    /// after a roboport was blown up still loads.
    pub(in crate::simulation) fn prune_robot_flight_state(&mut self) {
        if self.robot_flights.robots.is_empty() && self.robot_flights.charging.is_empty() {
            return;
        }
        let Simulation {
            robot_flights,
            entities,
            ..
        } = self;
        robot_flights
            .charging
            .retain(|roboport, _| entities.roboports.contains_key(roboport));
        for robot in robot_flights.robots.values_mut() {
            if robot
                .home_roboport
                .is_some_and(|entity_id| !entities.roboports.contains_key(&entity_id))
            {
                robot.home_roboport = None;
            }
            if robot
                .activity
                .roboport()
                .is_some_and(|entity_id| !entities.roboports.contains_key(&entity_id))
            {
                robot.activity = RobotActivity::Flying;
            }
        }
    }

    /// Moves queued robots onto free charging pads, oldest arrival first.
    fn assign_charging_pads(&mut self) {
        if self.robot_flights.charging.is_empty() {
            return;
        }
        let Simulation {
            robot_flights,
            entities,
            world,
            ..
        } = self;
        let RobotFlightSubsystem {
            robots, charging, ..
        } = robot_flights;

        charging.retain(|roboport, state| {
            // Robots leave a pad or a queue by changing activity (they charged
            // up, were released by a prune, or were re-homed), so the entry
            // that still names them is stale rather than authoritative.
            state.charging.retain(|robot_id| {
                robots
                    .get(robot_id)
                    .is_some_and(|robot| robot.activity == RobotActivity::Charging(*roboport))
            });
            state.queue.retain(|robot_id| {
                robots
                    .get(robot_id)
                    .is_some_and(|robot| robot.activity == RobotActivity::Queued(*roboport))
            });

            let pad_count = usize::from(
                roboport_prototype(&world.prototypes, entities, *roboport)
                    .map_or(0, |roboport| roboport.charging_pad_count),
            );
            while state.charging.len() < pad_count {
                let Some(robot_id) = state.queue.pop_front() else {
                    break;
                };
                let Some(robot) = robots.get_mut(&robot_id) else {
                    continue;
                };
                robot.activity = RobotActivity::Charging(*roboport);
                state.charging.insert(robot_id);
            }
            !state.is_empty()
        });
    }

    fn step_robots(&mut self) {
        if self.robot_flights.robots.is_empty() {
            return;
        }
        let Simulation {
            robot_flights,
            entities,
            world,
            robots: networks,
            ..
        } = self;
        let RobotFlightSubsystem {
            robots, charging, ..
        } = robot_flights;
        let mut context = RobotStepContext {
            catalog: &world.prototypes,
            entities,
            networks,
            charging,
        };
        robots.retain(|_, robot| step_robot(&mut context, robot));
    }
}

struct RobotStepContext<'a> {
    catalog: &'a PrototypeCatalog,
    entities: &'a mut EntityStore,
    networks: &'a mut RobotSubsystem,
    charging: &'a mut BTreeMap<EntityId, RoboportChargingState>,
}

/// Advances one robot. Returns `false` when the robot docked and should leave
/// the flight store.
fn step_robot(context: &mut RobotStepContext<'_>, robot: &mut Robot) -> bool {
    let Some(profile) = context
        .catalog
        .item(robot.item_id)
        .and_then(|item| item.robot)
    else {
        // No flight profile means no rule for how this robot moves; leaving it
        // parked is the only safe answer, and validation rejects the state.
        return true;
    };

    adopt_home_roboport(context, robot);

    if let RobotActivity::Charging(roboport) = robot.activity {
        if !charge_robot(context, robot, roboport, profile.energy_capacity_joules) {
            release_pad(context, robot, roboport);
        }
        return true;
    }
    if matches!(robot.activity, RobotActivity::Queued(_)) {
        // Hovering in a queue costs nothing: a robot that drained while waiting
        // for a pad could never reach one.
        return true;
    }

    let joules_per_tick = joules_per_tick(profile.flight_energy_usage_watts);
    let has_energy = robot.energy_joules >= joules_per_tick;
    if !has_energy && matches!(robot.activity, RobotActivity::Flying) {
        // Ran dry mid-errand: divert to the nearest roboport that can charge
        // it, before picking a target so this tick already crawls that way. The
        // errand is kept, so the robot resumes it once full.
        if let Some(roboport) = nearest_charging_roboport(context, robot) {
            robot.activity = RobotActivity::SeekingCharge(roboport);
        }
    }

    let Some(target) = flight_target(context, robot) else {
        // Nowhere to be — no errand, and no roboport left in the world to
        // return to. Hovering keeps the robot recoverable once one is built.
        return true;
    };

    // A robot already standing on its target spends nothing: it is docking,
    // queueing, or waiting out a full inventory, not flying.
    let budget = if (robot.x, robot.y) == target {
        0
    } else if has_energy {
        robot.energy_joules -= joules_per_tick;
        i64::from(profile.speed_fixed_per_tick)
    } else {
        crawl_speed(profile.speed_fixed_per_tick)
    };
    fly(context, robot, target, budget)
}

/// Moves `robot` toward `target` and resolves what arriving there means.
fn fly(
    context: &mut RobotStepContext<'_>,
    robot: &mut Robot,
    target: (i64, i64),
    budget: i64,
) -> bool {
    let (x, y) = step_toward(robot.x, robot.y, target, budget);
    robot.x = x;
    robot.y = y;
    if (x, y) != target {
        return true;
    }

    match robot.activity {
        RobotActivity::SeekingCharge(roboport) => {
            enqueue_for_charge(context, robot, roboport);
            true
        }
        RobotActivity::Flying if robot.errand.is_some() => {
            // The errand target is reached; this issue has no work to do there,
            // so the robot turns around. Jobs will hook in here.
            robot.errand = None;
            true
        }
        RobotActivity::Flying => arrive_home(context, robot),
        RobotActivity::Queued(_) | RobotActivity::Charging(_) => true,
    }
}

/// Handles a robot that reached its home roboport: top up first, then dock.
fn arrive_home(context: &mut RobotStepContext<'_>, robot: &mut Robot) -> bool {
    let Some(roboport) = robot.home_roboport else {
        return true;
    };
    let capacity = context
        .catalog
        .item(robot.item_id)
        .and_then(|item| item.robot)
        .map_or(0, |profile| profile.energy_capacity_joules);
    if robot.energy_joules < capacity {
        enqueue_for_charge(context, robot, roboport);
        return true;
    }

    // Docking turns the unit back into the item it came from. A full roboport
    // cannot take it, so the robot hovers and retries rather than evaporating.
    let Some(state) = context.entities.roboports.get_mut(&roboport) else {
        return true;
    };
    if state
        .robots
        .insert(context.catalog, robot.item_id, 1)
        .is_err()
    {
        return true;
    }
    context.networks.mark_roboport_dirty(roboport);
    false
}

/// Joins the charging queue of `roboport`, or takes a pad directly when one is
/// free.
fn enqueue_for_charge(context: &mut RobotStepContext<'_>, robot: &mut Robot, roboport: EntityId) {
    let pad_count = usize::from(
        roboport_prototype(context.catalog, context.entities, roboport)
            .map_or(0, |prototype| prototype.charging_pad_count),
    );
    let state = context.charging.entry(roboport).or_default();
    if state.charging.contains(&robot.id) || state.queue.contains(&robot.id) {
        return;
    }
    if state.charging.len() < pad_count {
        state.charging.insert(robot.id);
        robot.activity = RobotActivity::Charging(roboport);
    } else {
        state.queue.push_back(robot.id);
        robot.activity = RobotActivity::Queued(roboport);
    }
}

/// Draws one tick of charge out of the roboport's buffer. Returns whether the
/// robot is still charging afterwards.
fn charge_robot(
    context: &mut RobotStepContext<'_>,
    robot: &mut Robot,
    roboport: EntityId,
    capacity_joules: u64,
) -> bool {
    let Some(prototype) = roboport_prototype(context.catalog, context.entities, roboport) else {
        return false;
    };
    let Some(state) = context.entities.roboports.get_mut(&roboport) else {
        return false;
    };
    let transferred = joules_per_tick(prototype.charging_pad_watts)
        .min(state.charge_energy_joules)
        .min(capacity_joules.saturating_sub(robot.energy_joules));
    if transferred > 0 {
        state.charge_energy_joules -= transferred;
        robot.energy_joules += transferred;
        context.networks.mark_roboport_dirty(roboport);
    }
    // An empty buffer holds the robot on the pad: it is the only place it can
    // ever be filled, and freeing the pad would just hand it to another robot
    // with the same problem.
    robot.energy_joules < capacity_joules
}

fn release_pad(context: &mut RobotStepContext<'_>, robot: &mut Robot, roboport: EntityId) {
    if let Some(state) = context.charging.get_mut(&roboport) {
        state.charging.remove(&robot.id);
    }
    robot.activity = RobotActivity::Flying;
}

/// Adopts a home roboport when the robot has none, which happens when the one
/// it came from was destroyed.
fn adopt_home_roboport(context: &RobotStepContext<'_>, robot: &mut Robot) {
    if robot.home_roboport.is_some() {
        return;
    }
    robot.home_roboport = nearest_roboport(
        context.entities,
        context.entities.roboports.keys().copied(),
        robot.x,
        robot.y,
    );
}

/// Where the robot is flying: the errand target, otherwise its home dock.
fn flight_target(context: &RobotStepContext<'_>, robot: &Robot) -> Option<(i64, i64)> {
    match robot.activity {
        RobotActivity::SeekingCharge(roboport) => {
            roboport_dock_position(context.entities, roboport)
        }
        _ => robot.errand.or_else(|| {
            robot
                .home_roboport
                .and_then(|roboport| roboport_dock_position(context.entities, roboport))
        }),
    }
}

/// Nearest roboport a stranded robot can charge at.
///
/// Restricted to the robot's own network when it has one: a robot belongs to
/// the network it was dispatched from, and charging at an unrelated roboport
/// across the map would let it teleport between networks over time. Runs on the
/// tick a robot runs dry, not every tick.
fn nearest_charging_roboport(context: &RobotStepContext<'_>, robot: &Robot) -> Option<EntityId> {
    let network = robot
        .home_roboport
        .and_then(|home| context.networks.network_ids_by_entity.get(&home).copied())
        .and_then(|network_id| context.networks.topology_networks.get(network_id as usize));
    match network {
        Some(network) => nearest_roboport(
            context.entities,
            network_member_ids(network),
            robot.x,
            robot.y,
        ),
        None => nearest_roboport(
            context.entities,
            context.entities.roboports.keys().copied(),
            robot.x,
            robot.y,
        ),
    }
}

fn network_member_ids(network: &RobotNetworkTopology) -> impl Iterator<Item = EntityId> + '_ {
    network.roboports.iter().map(|roboport| roboport.entity_id)
}

/// Closest roboport to `(x, y)`, ties going to the lowest entity id so the
/// answer never depends on iteration order.
fn nearest_roboport(
    entities: &EntityStore,
    candidates: impl Iterator<Item = EntityId>,
    x: i64,
    y: i64,
) -> Option<EntityId> {
    let mut best: Option<(i128, EntityId)> = None;
    for entity_id in candidates {
        let Some((dock_x, dock_y)) = roboport_dock_position(entities, entity_id) else {
            continue;
        };
        let distance = squared_distance(dock_x - x, dock_y - y);
        let is_better = best.is_none_or(|(best_distance, best_id)| {
            distance < best_distance || (distance == best_distance && entity_id < best_id)
        });
        if is_better {
            best = Some((distance, entity_id));
        }
    }
    best.map(|(_, entity_id)| entity_id)
}

/// Item id of the first robot stationed in a roboport, by slot order.
fn first_stationed_robot(catalog: &PrototypeCatalog, state: &RoboportState) -> Option<ItemId> {
    state
        .robots
        .slots()
        .iter()
        .filter_map(|slot| slot.stack())
        .map(|stack| stack.item_id())
        .find(|item_id| {
            catalog
                .item(*item_id)
                .is_some_and(|item| item.robot.is_some())
        })
}

fn roboport_prototype(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    roboport: EntityId,
) -> Option<factory_data::RoboportPrototype> {
    entities
        .placed_entity(roboport)
        .and_then(|placed| catalog.entity(placed.prototype_id))
        .and_then(|prototype| prototype.roboport)
}

/// Fixed-point center of a roboport's footprint: where robots leave from, dock,
/// and charge.
fn roboport_dock_position(entities: &EntityStore, roboport: EntityId) -> Option<(i64, i64)> {
    let footprint = entities.placed_entity(roboport)?.footprint;
    Some((
        footprint.x * POSITION_SCALE + i64::from(footprint.width) * POSITION_SCALE / 2,
        footprint.y * POSITION_SCALE + i64::from(footprint.height) * POSITION_SCALE / 2,
    ))
}

/// One tick of straight-line flight, snapping onto the target when the
/// remaining distance fits inside the budget.
///
/// Integer throughout: the direction is scaled by the budget and divided by the
/// exact distance, so two machines that run the same tick land on the same
/// position rather than on two roundings of the same float.
fn step_toward(x: i64, y: i64, target: (i64, i64), budget: i64) -> (i64, i64) {
    let dx = target.0 - x;
    let dy = target.1 - y;
    let distance = squared_distance(dx, dy).isqrt();
    if distance <= i128::from(budget) {
        return target;
    }

    let step_x = (i128::from(dx) * i128::from(budget) / distance) as i64;
    let step_y = (i128::from(dy) * i128::from(budget) / distance) as i64;
    if step_x == 0 && step_y == 0 {
        // Truncation toward zero can cancel both components on a near-diagonal
        // approach. Nudging the dominant axis keeps every tick a real step, so
        // a robot can never hover one unit short of its target forever.
        return if dx.abs() >= dy.abs() {
            (x + dx.signum(), y)
        } else {
            (x, y + dy.signum())
        };
    }
    (x + step_x, y + step_y)
}

fn squared_distance(dx: i64, dy: i64) -> i128 {
    i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy)
}

/// Energy one tick of a continuous draw costs. Rounds up to a joule so a draw
/// below one joule per tick still costs something instead of being free.
fn joules_per_tick(watts: u64) -> u64 {
    (watts / SIMULATION_TICKS_PER_SECOND).max(1)
}

fn crawl_speed(speed_fixed_per_tick: u32) -> i64 {
    // Widened before scaling: the product overflows a `u32` for fast enough
    // prototypes, and a robot that crawled at zero would never reach a pad.
    (i64::from(speed_fixed_per_tick) * i64::from(ROBOT_EMPTY_SPEED_PERMILLE) / 1_000).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_never_overshoots_and_snaps_on_arrival() {
        assert_eq!(step_toward(0, 0, (100, 0), 30), (30, 0));
        assert_eq!(step_toward(0, 0, (100, 0), 100), (100, 0));
        assert_eq!(step_toward(0, 0, (100, 0), 250), (100, 0));
        assert_eq!(step_toward(50, -20, (50, -20), 60), (50, -20));
    }

    /// A diagonal leg must not travel further per tick than an axis-aligned
    /// one; the budget is a distance, not a per-axis allowance.
    #[test]
    fn a_diagonal_step_spends_the_same_budget_as_a_straight_one() {
        let (x, y) = step_toward(0, 0, (10_000, 10_000), 100);
        let travelled = squared_distance(x, y).isqrt();
        // Truncating each axis toward zero can lose up to one unit per axis,
        // which is the only slack allowed here.
        assert!(travelled <= 100, "diagonal step travelled {travelled}");
        assert!(travelled >= 98, "diagonal step travelled {travelled}");
    }

    /// The dominant-axis nudge: without it a one-unit budget on a near-diagonal
    /// approach truncates both components to zero and the robot never arrives.
    #[test]
    fn a_tiny_budget_still_makes_progress() {
        let mut position = (0, 0);
        for _ in 0..8 {
            position = step_toward(position.0, position.1, (3, 4), 1);
        }
        assert_eq!(position, (3, 4));
    }

    #[test]
    fn an_empty_robot_still_crawls_at_least_one_unit() {
        assert_eq!(crawl_speed(60), 12);
        assert_eq!(crawl_speed(1), 1);
    }
}
