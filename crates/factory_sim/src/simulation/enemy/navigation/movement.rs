use super::targeting::{acquire_target, is_attackable_kind};
use super::*;

/// Ticks between target rescans for units without a target.
const ENEMY_TARGET_RESCAN_TICKS: u64 = 120;
/// Ticks between path recomputations while a target is set.
const ENEMY_REPATH_INTERVAL_TICKS: u64 = 90;
/// Ticks between wander moves for idle guards.
const ENEMY_WANDER_INTERVAL_TICKS: u64 = 300;
/// Upper bound on A* node expansions per path request.
const ENEMY_PATHFIND_MAX_EXPANSIONS: usize = 600;
/// Distant targets use deterministic greedy movement unless terrain blocks it.
const ENEMY_PATHFIND_MAX_RANGE_TILES: i64 = 40;
/// A blocked long-range greedy step searches to a nearby forward edge.
/// The complete square fits within one request's expansion allowance, so a
/// missing route means the local frontier is genuinely unreachable rather
/// than merely unfinished when the work cap was reached.
const ENEMY_LONG_RANGE_DETOUR_TILES: i64 = 16;
#[cfg(test)]
const ENEMY_LONG_RANGE_DETOUR_MAX_EXPANSIONS: usize = 33 * 33;
/// Wander goals stay within a few tiles of their anchor, so a small bounded
/// window keeps every idle decision inside the shared navigation budget.
const ENEMY_WANDER_ROUTE_RANGE_TILES: i64 = 8;
const ENEMY_WANDER_ROUTE_MAX_EXPANSIONS: usize = 128;

impl Simulation {
    pub(in crate::simulation) fn advance_enemies(&mut self, commands: &mut CombatCommandBuffer) {
        self.enemy_navigation.begin_tick(
            self.entity_topology_revision,
            self.world.chunk_revision(),
            self.world.walkability_revision(),
        );
        let targets_invalidated = self.attack_targets.refresh(
            self.entity_topology_revision,
            &self.world,
            &self.entities,
            &self.enemies,
        );
        if targets_invalidated {
            for raid in self.enemies.raids.values_mut() {
                raid.target = None;
            }
            for unit in self.enemies.enemies.values_mut().filter(|unit| {
                matches!(
                    unit.mission,
                    EnemyMission::Staging(_) | EnemyMission::Raid(_)
                )
            }) {
                unit.target = None;
                unit.path.clear();
            }
        }

        let tick = self.tick;
        let Simulation {
            world,
            entities,
            enemies,
            attack_targets,
            enemy_navigation,
            ..
        } = self;
        attack_targets.retain_active_groups(enemies);
        enemy_navigation.retain_raids(&enemies.raids);
        enemy_navigation.retain_units(&enemies.enemies);
        for raid in enemies.raids.values_mut() {
            if raid
                .target
                .is_some_and(|target| !entities.placed_entities.contains_key(&target))
            {
                raid.target = None;
            }
            let origin = raid
                .members
                .iter()
                .filter_map(|id| enemies.enemies.get(id))
                .map(Enemy::tile)
                .next();
            if let Some(origin) = origin {
                raid.target = attack_targets.target_for_raid(
                    raid.id,
                    EntityFootprint::single_tile(origin.0, origin.1),
                    raid.target,
                );
            } else {
                raid.target = None;
            }
            for member in &raid.members {
                if let Some(unit) = enemies.enemies.get_mut(member) {
                    let local_blocker_active = unit.target.is_some_and(|target| {
                        Some(target) != raid.target
                            && entities
                                .placed_entities
                                .get(&target)
                                .is_some_and(|placed| is_attackable_kind(entities, placed))
                    });
                    if !local_blocker_active {
                        unit.target = raid.target;
                    }
                }
            }
        }
        for raid in enemies.raids.values() {
            if let Some(target) = raid.target
                && let Some(placed) = entities.placed_entities.get(&target)
            {
                enemy_navigation.sync_raid(raid.id, target, placed.footprint);
            }
        }
        enemy_navigation.advance_raid_fields(world, entities);
        // Expansion parties route toward their destination through the shared
        // budgeted tile-goal pathfinder, so they can detour around obstacles
        // instead of tunneling or stalling. Pushing `party.destination`
        // directly would let `follow_path` cross blocked intermediate tiles.
        // Retry policy: waiting routes leave the path empty and throttle
        // targetless units to one attempt per repath interval; the next
        // empty-path tick tries again, so parties resume when terrain or
        // budget pressure clears.
        for party in enemies.expansions.values() {
            for member in &party.members {
                let destination = party.destination;
                let Some(unit) = enemies.enemies.get_mut(member) else {
                    continue;
                };
                // The destination is authoritative, so an attack target here
                // can only come from combat retaliation (assigned by
                // `resolve_combat_commands` after movement) or stale pre-fix
                // state. Drop it before routing so the unit replans the
                // expansion route on this same tick: clearing after the
                // prefill would discard the fresh route, and the old combat
                // repath deadline would idle the unit — under repeated fire
                // that stalls the party indefinitely.
                if matches!(unit.mission, EnemyMission::Expansion(_)) && unit.target.is_some() {
                    unit.target = None;
                    unit.path.clear();
                    unit.next_decision_tick = unit.next_decision_tick.min(tick);
                }
                if !unit.path.is_empty() {
                    continue;
                }
                let from = unit.tile();
                if from == destination {
                    // Arrival marker: consumed without moving once the unit
                    // is centered, then `resolve_arrived_expansions` founds
                    // the colony.
                    unit.path.push_back(destination);
                    continue;
                }
                let targetless = unit.target.is_none();
                if targetless && tick < unit.next_decision_tick {
                    continue;
                }
                match plan_expansion_move(enemy_navigation, world, entities, from, destination) {
                    ExpansionRoute::Steps(steps) => {
                        debug_assert!(
                            !steps.is_empty(),
                            "expansion routing must return steps or wait"
                        );
                        unit.path = steps;
                    }
                    ExpansionRoute::Wait => {
                        if targetless {
                            unit.next_decision_tick =
                                tick + ENEMY_REPATH_INTERVAL_TICKS + unit.id.raw() % 16;
                        }
                    }
                }
            }
        }
        let newly_spotted: Vec<_> = self
            .enemies
            .expansions
            .iter()
            .filter_map(|(&id, party)| {
                (!party.spotted
                    && party.members.iter().any(|member| {
                        self.enemies.enemies.get(member).is_some_and(|unit| {
                            ChunkCoord::from_tile(unit.tile().0, unit.tile().1)
                                .is_some_and(|chunk| self.chart.revealed_chunks.contains(&chunk))
                        })
                    }))
                .then_some(id)
            })
            .collect();
        for id in newly_spotted {
            let destination = self.enemies.expansions.get_mut(&id).map(|party| {
                party.spotted = true;
                party.destination
            });
            if let Some((x, y)) = destination {
                self.emit_event(
                    ThreatEventKind::ExpansionSpotted,
                    ThreatLocation::Exact { x, y },
                );
            }
        }
        {
            let Simulation {
                world,
                entities,
                enemies,
                attack_targets,
                enemy_navigation,
                tick,
                ..
            } = self;
            let tick = *tick;
            let seed = world.seed;
            let mut context = EnemyStepContext {
                world,
                entities,
                attack_targets,
                navigation: enemy_navigation,
                seed,
                tick,
            };

            for enemy in enemies.enemies.values_mut() {
                step_enemy(&mut context, enemy, commands);
            }
        }
    }
}

/// Outcome of planning one expansion move: adjacent steps to follow now, or
/// an instruction to wait and retry on a later tick.
enum ExpansionRoute {
    Steps(VecDeque<(WorldTileCoord, WorldTileCoord)>),
    Wait,
}

/// Deterministic intermediate goal for long-distance travel: the destination
/// clamped into the search window around `from`. Keeps every search inside
/// the existing range instead of running a 200+ tile A*.
fn bounded_intermediate_goal(
    from: (WorldTileCoord, WorldTileCoord),
    destination: (WorldTileCoord, WorldTileCoord),
) -> (WorldTileCoord, WorldTileCoord) {
    (
        from.0
            .saturating_add((destination.0.saturating_sub(from.0)).clamp(
                -ENEMY_PATHFIND_MAX_RANGE_TILES,
                ENEMY_PATHFIND_MAX_RANGE_TILES,
            )),
        from.1
            .saturating_add((destination.1.saturating_sub(from.1)).clamp(
                -ENEMY_PATHFIND_MAX_RANGE_TILES,
                ENEMY_PATHFIND_MAX_RANGE_TILES,
            )),
    )
}

fn plan_expansion_move(
    navigation: &mut EnemyNavigation,
    world: &WorldSim,
    entities: &EntityStore,
    from: (WorldTileCoord, WorldTileCoord),
    destination: (WorldTileCoord, WorldTileCoord),
) -> ExpansionRoute {
    if from == destination {
        // Arrival marker: consumed without moving once the unit is centered,
        // then `resolve_arrived_expansions` founds the colony.
        return ExpansionRoute::Steps(VecDeque::from([destination]));
    }
    let in_range = EntityFootprint::single_tile(from.0, from.1)
        .chebyshev_distance_to(&EntityFootprint::single_tile(destination.0, destination.1))
        <= ENEMY_PATHFIND_MAX_RANGE_TILES;
    // Beyond routing range, head for the deterministic intermediate goal so
    // local barriers are detoured with the shared pathfinder instead of the
    // purely greedy fallback below.
    let goal = if in_range {
        destination
    } else {
        bounded_intermediate_goal(from, destination)
    };
    match navigation.request_tile_path(
        world,
        entities,
        from,
        goal,
        ENEMY_PATHFIND_MAX_RANGE_TILES,
        ENEMY_PATHFIND_MAX_EXPANSIONS,
    ) {
        PathRequest::Ready(Some(path)) if !path.is_empty() => ExpansionRoute::Steps(path),
        // The true goal is provably unreachable inside the window: wait for
        // terrain to change instead of marching toward it.
        PathRequest::Ready(_) if in_range => ExpansionRoute::Wait,
        // Otherwise the tick budget is spent or only the intermediate goal
        // failed: probe one validated greedy step toward the destination,
        // or wait when every adjacent step is blocked.
        _ => match expansion_step_toward(world, entities, from, destination) {
            Some(next) => ExpansionRoute::Steps(VecDeque::from([next])),
            None => ExpansionRoute::Wait,
        },
    }
}

struct EnemyStepContext<'a> {
    world: &'a WorldSim,
    entities: &'a EntityStore,
    attack_targets: &'a mut AttackTargetCache,
    navigation: &'a mut EnemyNavigation,
    seed: u64,
    tick: u64,
}

enum LongRangePlan {
    FollowPath,
    Done,
}

fn direction_toward(
    from: (WorldTileCoord, WorldTileCoord),
    target: (WorldTileCoord, WorldTileCoord),
) -> Direction {
    let dx = target.0.saturating_sub(from.0);
    let dy = target.1.saturating_sub(from.1);
    if dx.saturating_abs() >= dy.saturating_abs() {
        if dx >= 0 {
            Direction::East
        } else {
            Direction::West
        }
    } else if dy >= 0 {
        Direction::North
    } else {
        Direction::South
    }
}

fn step_in_direction(
    tile: (WorldTileCoord, WorldTileCoord),
    direction: Direction,
) -> Option<(WorldTileCoord, WorldTileCoord)> {
    let (dx, dy) = direction.tile_step();
    Some((tile.0.checked_add(dx)?, tile.1.checked_add(dy)?))
}

#[allow(clippy::too_many_arguments)]
fn plan_long_range_move(
    navigation: &mut EnemyNavigation,
    world: &WorldSim,
    entities: &EntityStore,
    tick: u64,
    enemy: &mut Enemy,
    target: EntityId,
    target_footprint: &EntityFootprint,
) -> LongRangePlan {
    let tile = enemy.tile();
    let destination = footprint_center_tile(target_footprint);
    let current_distance = enemy_footprint(enemy).manhattan_distance_to(target_footprint);
    let mut detour = navigation.long_range_detour(enemy.id);

    // Once the unit is closer than where it first met the obstacle, the
    // detour has cleared that obstacle. Ordinary greedy movement may safely
    // resume without walking back across the lateral progress it just made.
    if detour.is_some_and(|state| current_distance < state.hit_distance) {
        navigation.clear_long_range_detour(enemy.id);
        detour = None;
    }

    if detour.is_none() {
        // Preserve the zero-search-cost straight-line behavior in open
        // terrain and the existing behavior of attacking structural blockers.
        if greedy_step(world, entities, enemy, target, target_footprint) {
            enemy.next_decision_tick = tick + ENEMY_REPATH_INTERVAL_TICKS + enemy.id.raw() % 16;
            return LongRangePlan::Done;
        }

        let forward = direction_toward(tile, destination);
        let clockwise = forward.rotate_clockwise();
        let heading = if enemy.id.raw().is_multiple_of(2) {
            clockwise
        } else {
            clockwise.opposite()
        };
        let state = LongRangeDetour {
            target,
            forward,
            heading,
            wall_on_clockwise_side: heading.rotate_clockwise() == forward,
            hit_distance: current_distance,
        };
        navigation.set_long_range_detour(enemy.id, state);
        detour = Some(state);
    }

    let detour = detour.expect("long-range detour must be initialized");
    match navigation.request_frontier_path(
        world,
        entities,
        tile,
        destination,
        detour.forward,
        ENEMY_LONG_RANGE_DETOUR_TILES,
    ) {
        PathRequest::Ready(Some(path)) => {
            debug_assert!(
                !path.is_empty(),
                "a long-range frontier goal must produce movement steps"
            );
            enemy.path = path;
            enemy.next_decision_tick = tick + ENEMY_REPATH_INTERVAL_TICKS + enemy.id.raw() % 16;
            LongRangePlan::FollowPath
        }
        PathRequest::Ready(None) => {
            // A complete local search proved that the forward edge cannot be
            // reached from this window. Shift the next window along the
            // obstacle. Repeating this bounded step eventually exposes the
            // end of every finite wall, without increasing per-tick work.
            let toward_wall = if detour.wall_on_clockwise_side {
                detour.heading.rotate_clockwise()
            } else {
                detour.heading.rotate_clockwise().opposite()
            };
            let away_from_wall = toward_wall.opposite();
            for heading in [
                toward_wall,
                detour.heading,
                away_from_wall,
                detour.heading.opposite(),
            ] {
                let Some(next) = step_in_direction(tile, heading) else {
                    continue;
                };
                if tile_open_for_enemy(world, entities, next.0, next.1, Some(target)) {
                    enemy.path.push_back(next);
                    if heading != detour.heading {
                        navigation
                            .set_long_range_detour(enemy.id, LongRangeDetour { heading, ..detour });
                    }
                    enemy.next_decision_tick =
                        tick + ENEMY_REPATH_INTERVAL_TICKS + enemy.id.raw() % 16;
                    return LongRangePlan::FollowPath;
                }
            }
            enemy.next_decision_tick = tick + ENEMY_REPATH_INTERVAL_TICKS + enemy.id.raw() % 16;
            LongRangePlan::Done
        }
        PathRequest::Deferred => LongRangePlan::Done,
    }
}

fn step_enemy(
    context: &mut EnemyStepContext<'_>,
    enemy: &mut Enemy,
    commands: &mut CombatCommandBuffer,
) {
    let world = context.world;
    let entities = context.entities;
    let seed = context.seed;
    let tick = context.tick;
    // Expansion parties keep their destination authoritative: they never run
    // ordinary global attack targeting, so a player structure must not divert
    // them into a de facto raid. Target hygiene and route planning both live
    // in the expansion prefill in `advance_enemies`; here the unit only
    // follows its route (`follow_path` no-ops on an empty path while the
    // prefill throttles the next attempt).
    if matches!(enemy.mission, EnemyMission::Expansion(_)) {
        context.navigation.clear_long_range_detour(enemy.id);
        follow_path(world, entities, enemy, None);
        return;
    }
    if context
        .navigation
        .long_range_detour(enemy.id)
        .is_some_and(|detour| Some(detour.target) != enemy.target)
    {
        context.navigation.clear_long_range_detour(enemy.id);
    }
    // Drop targets that no longer exist.
    if let Some(target) = enemy.target
        && !entities.placed_entities.contains_key(&target)
    {
        enemy.target = None;
        enemy.path.clear();
    }

    if enemy.target.is_none() && tick >= enemy.next_decision_tick {
        enemy.target = if let EnemyMission::Staging(base_id) = enemy.mission {
            context
                .attack_targets
                .target_for_base(base_id, enemy_footprint(enemy))
        } else {
            acquire_target(entities, &context.attack_targets.index, enemy)
        };
        if enemy.target.is_some() {
            enemy.path.clear();
            enemy.next_decision_tick = tick;
        } else {
            enemy.next_decision_tick = tick + ENEMY_TARGET_RESCAN_TICKS + enemy.id.raw() % 16;
        }
    }

    let Some(target) = enemy.target else {
        context.navigation.clear_long_range_detour(enemy.id);
        wander(world, entities, context.navigation, seed, tick, enemy);
        return;
    };
    let Some(target_footprint) = entities
        .placed_entities
        .get(&target)
        .map(|placed| placed.footprint)
    else {
        return;
    };

    // Attack when standing next to (or on the edge of) the target.
    let tile = enemy.tile();
    if enemy_footprint(enemy).chebyshev_distance_to(&target_footprint)
        <= i64::from(enemy.attack.delivery.range_tiles())
    {
        context.navigation.clear_long_range_detour(enemy.id);
        enemy.path.clear();
        if tick >= enemy.next_attack_tick {
            commands.attack(
                CombatSource {
                    owner: CombatantId::Enemy(enemy.id),
                    faction: enemy.faction(),
                },
                CombatantId::Entity(target),
                enemy.attack,
            );
            enemy.next_attack_tick = tick + u64::from(enemy.attack.cooldown_ticks);
        }
        return;
    }

    // Recompute the path when it ran out, was invalidated, or grew stale.
    let next_waypoint_blocked = enemy
        .path
        .front()
        .is_some_and(|&(x, y)| !tile_open_for_enemy(world, entities, x, y, Some(target)));

    if let EnemyMission::Raid(raid_id) = enemy.mission {
        context.navigation.clear_long_range_detour(enemy.id);
        if next_waypoint_blocked {
            enemy.path.clear();
        }
        if enemy.path.is_empty() {
            match context.navigation.raid_route(raid_id, tile) {
                RaidRoute::Step(next) => enemy.path.push_back(next),
                RaidRoute::AtGoal | RaidRoute::Pending => {
                    return;
                }
                RaidRoute::OutsideField | RaidRoute::Unreachable => {
                    greedy_step(world, entities, enemy, target, &target_footprint);
                    return;
                }
            }
        }
        let target = enemy.target;
        follow_path(world, entities, enemy, target);
        return;
    }

    if (enemy.path.is_empty() || next_waypoint_blocked) && tick >= enemy.next_decision_tick {
        enemy.path.clear();
        let target_distance = enemy_footprint(enemy).chebyshev_distance_to(&target_footprint);
        if target_distance <= ENEMY_PATHFIND_MAX_RANGE_TILES {
            context.navigation.clear_long_range_detour(enemy.id);
            match context.navigation.request_path(
                world,
                entities,
                tile,
                target,
                &target_footprint,
                ENEMY_PATHFIND_MAX_RANGE_TILES,
                ENEMY_PATHFIND_MAX_EXPANSIONS,
            ) {
                PathRequest::Ready(path) => {
                    enemy.next_decision_tick =
                        tick + ENEMY_REPATH_INTERVAL_TICKS + enemy.id.raw() % 16;
                    if let Some(path) = path {
                        enemy.path = path;
                    }
                }
                PathRequest::Deferred => return,
            }
        } else {
            match plan_long_range_move(
                context.navigation,
                world,
                entities,
                tick,
                enemy,
                target,
                &target_footprint,
            ) {
                LongRangePlan::FollowPath => {}
                LongRangePlan::Done => return,
            }
        }
        if enemy.path.is_empty() {
            // No route: walk straight at the target and gnaw through the
            // first structure in the way.
            greedy_step(world, entities, enemy, target, &target_footprint);
            return;
        }
    }

    let target = enemy.target;
    follow_path(world, entities, enemy, target);
}

/// Idle guards drift around their home spawner so nests look alive. Goals are
/// routed through the shared budgeted tile-goal pathfinder, so wandering
/// detours around blocked tiles instead of crossing them.
fn wander(
    world: &WorldSim,
    entities: &EntityStore,
    navigation: &mut EnemyNavigation,
    seed: u64,
    tick: u64,
    enemy: &mut Enemy,
) {
    if !enemy.path.is_empty() {
        follow_path(world, entities, enemy, None);
        return;
    }
    if tick < enemy.next_decision_tick {
        return;
    }

    let anchor = enemy
        .home_spawner
        .and_then(|spawner| entities.placed_entities.get(&spawner))
        .map(|placed| footprint_center_tile(&placed.footprint))
        .unwrap_or_else(|| enemy.tile());
    let roll = splitmix64(seed ^ enemy.id.raw().wrapping_mul(0x9e37_79b9) ^ tick);
    let dx = ((roll & 0x7) as i64) - 3;
    let dy = (((roll >> 3) & 0x7) as i64) - 3;
    let goal = (anchor.0 + dx, anchor.1 + dy);
    let from = enemy.tile();
    if from == goal || !tile_open_for_enemy(world, entities, goal.0, goal.1, None) {
        enemy.next_decision_tick = tick + ENEMY_WANDER_INTERVAL_TICKS + enemy.id.raw() % 64;
        return;
    }
    // Both a route and the lack of one sleep a full wander interval; the
    // unit simply waits when no walkable route exists.
    match navigation.request_tile_path(
        world,
        entities,
        from,
        goal,
        ENEMY_WANDER_ROUTE_RANGE_TILES,
        ENEMY_WANDER_ROUTE_MAX_EXPANSIONS,
    ) {
        PathRequest::Ready(Some(path)) => {
            debug_assert!(
                !path.is_empty(),
                "tile routing from outside the goal must return steps"
            );
            enemy.path = path;
            enemy.next_decision_tick = tick + ENEMY_WANDER_INTERVAL_TICKS + enemy.id.raw() % 64;
        }
        PathRequest::Ready(None) => {
            enemy.next_decision_tick = tick + ENEMY_WANDER_INTERVAL_TICKS + enemy.id.raw() % 64;
        }
        PathRequest::Deferred => {
            // Budget exhausted: retry soon instead of sleeping a full wander
            // interval so idle motion isn't starved under load.
            enemy.next_decision_tick = tick + ENEMY_REPATH_INTERVAL_TICKS + enemy.id.raw() % 16;
        }
    }
}

/// Single validated adjacent step toward a tile destination. Long-range
/// fallback for expansion parties beyond routing range (the same fallback
/// distant combat units use): only 4-connected moves are returned, and the
/// returned tile is verified open, so callers never insert a distant waypoint
/// that would tunnel across blocked terrain. Returns `None` when every
/// adjacent step toward the destination is blocked; the unit waits instead
/// of crossing.
fn expansion_step_toward(
    world: &WorldSim,
    entities: &EntityStore,
    from: (WorldTileCoord, WorldTileCoord),
    destination: (WorldTileCoord, WorldTileCoord),
) -> Option<(WorldTileCoord, WorldTileCoord)> {
    if from == destination {
        return Some(destination);
    }
    let dx = destination.0.saturating_sub(from.0);
    let dy = destination.1.saturating_sub(from.1);
    let mut steps = [(0, 0); 2];
    if dx.abs() >= dy.abs() {
        steps[0] = (dx.signum(), 0);
        steps[1] = (0, dy.signum());
    } else {
        steps[0] = (0, dy.signum());
        steps[1] = (dx.signum(), 0);
    }
    for (step_x, step_y) in steps {
        if step_x == 0 && step_y == 0 {
            continue;
        }
        let (Some(next_x), Some(next_y)) = (from.0.checked_add(step_x), from.1.checked_add(step_y))
        else {
            continue;
        };
        if tile_open_for_enemy(world, entities, next_x, next_y, None) {
            return Some((next_x, next_y));
        }
    }
    None
}

/// Advances the unit along its waypoints by one tick's movement budget.
///
/// Waypoint contract: `enemy.path` holds 4-connected adjacent tile centers,
/// and a waypoint completes only when the unit reaches its exact center.
/// Per-leg movement is then axis-aligned and stays exact in fixed-point
/// integers. `Enemy::tile()` flips at tile boundaries, half a tile early,
/// so completion must never key off the current tile.
///
/// The contract is mechanically upheld here: a non-adjacent or blocked front
/// waypoint discards the path without moving, so a stale distant waypoint
/// can never tunnel across water or other blocked terrain. A front waypoint
/// equal to the current tile is a mid-leg state (or a same-tile arrival
/// marker), not completion: the unit keeps moving toward its center.
fn follow_path(
    world: &WorldSim,
    entities: &EntityStore,
    enemy: &mut Enemy,
    target: Option<EntityId>,
) {
    let mut budget = i64::from(enemy.speed_fixed_per_tick);
    while budget > 0 {
        let Some(&waypoint) = enemy.path.front() else {
            return;
        };
        let current = enemy.tile();
        let adjacency = (waypoint.0.saturating_sub(current.0)).abs()
            + (waypoint.1.saturating_sub(current.1)).abs();
        if adjacency > 1 {
            enemy.path.clear();
            return;
        }
        if adjacency == 1 && !tile_open_for_enemy(world, entities, waypoint.0, waypoint.1, target) {
            enemy.path.clear();
            return;
        }
        let (waypoint_x, waypoint_y) = waypoint;
        let goal_x = tile_center_fixed(waypoint_x);
        let goal_y = tile_center_fixed(waypoint_y);

        let dx = goal_x - enemy.x;
        let step_x = dx.signum() * dx.abs().min(budget);
        enemy.x += step_x;
        budget -= step_x.abs();

        let dy = goal_y - enemy.y;
        let step_y = dy.signum() * dy.abs().min(budget);
        enemy.y += step_y;
        budget -= step_y.abs();

        if enemy.x == goal_x && enemy.y == goal_y {
            enemy.path.pop_front();
        } else {
            return;
        }
    }
}

/// Fallback movement when no path exists: step toward the target, and when a
/// structure blocks the step, attack it instead (walls become chew targets).
/// Returns whether the unit moved or selected a blocking structure.
fn greedy_step(
    world: &WorldSim,
    entities: &EntityStore,
    enemy: &mut Enemy,
    target: EntityId,
    target_footprint: &EntityFootprint,
) -> bool {
    let (tile_x, tile_y) = enemy.tile();
    let (goal_x, goal_y) = footprint_center_tile(target_footprint);
    let dx = goal_x - tile_x;
    let dy = goal_y - tile_y;

    let mut steps = [(0, 0); 2];
    if dx.abs() >= dy.abs() {
        steps[0] = (dx.signum(), 0);
        steps[1] = (0, dy.signum());
    } else {
        steps[0] = (0, dy.signum());
        steps[1] = (dx.signum(), 0);
    }

    for (step_x, step_y) in steps {
        if step_x == 0 && step_y == 0 {
            continue;
        }
        let next = (tile_x + step_x, tile_y + step_y);
        if tile_open_for_enemy(world, entities, next.0, next.1, Some(target)) {
            enemy.path.push_back(next);
            follow_path(world, entities, enemy, Some(target));
            return true;
        }
        // Blocked by a structure: switch targets and chew through it.
        if let Some(blocker) = entities.occupancy.entity_at(next.0, next.1)
            && blocker != target
            && entities
                .placed_entities
                .get(&blocker)
                .is_some_and(|placed| is_attackable_kind(entities, placed))
        {
            enemy.target = Some(blocker);
            enemy.path.clear();
            return true;
        }
    }
    false
}

/// A tile a unit may stand on: generated, walkable terrain, and free of
/// structures other than the unit's own target.
pub(super) fn tile_open_for_enemy(
    world: &WorldSim,
    entities: &EntityStore,
    x: WorldTileCoord,
    y: WorldTileCoord,
    target: Option<EntityId>,
) -> bool {
    let Some(tile) = world.tile_at(x, y) else {
        return false;
    };
    if !tile.collision.walkable {
        return false;
    }
    match entities.occupancy.entity_at(x, y) {
        None => true,
        Some(occupant) => Some(occupant) == target,
    }
}

fn enemy_footprint(enemy: &Enemy) -> EntityFootprint {
    let (x, y) = enemy.tile();
    EntityFootprint::single_tile(x, y)
}

#[cfg(test)]
mod movement_regression_tests {
    use super::*;

    fn water_id(sim: &Simulation) -> factory_data::TileId {
        factory_data::BasePrototypeIds::from_catalog(&sim.world.prototypes)
            .tiles
            .water
    }

    fn corridor_is_clear(sim: &Simulation, x: WorldTileCoord, y: WorldTileCoord) -> bool {
        sim.world.tile_at(x, y).is_some_and(|tile| {
            tile.collision.walkable
                && tile.resource.is_none()
                && sim.entities.occupancy.entity_at(x, y).is_none()
        })
    }

    /// Horizontal 7-tile run with room for a 5-tall water wall through its
    /// middle. Returns `(start, destination, wall_x, base_y)`.
    type WallGeometry = (
        (WorldTileCoord, WorldTileCoord),
        (WorldTileCoord, WorldTileCoord),
        WorldTileCoord,
        WorldTileCoord,
    );
    fn clear_run_with_wall_room(sim: &Simulation) -> Option<WallGeometry> {
        for chunk in sim.world.chunks.values() {
            for (index, _) in chunk.tiles.iter().enumerate() {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                let (x, y) = chunk.coord.tile_at(local_x, local_y);
                if !(0..6).all(|dx| corridor_is_clear(sim, x + dx, y)) {
                    continue;
                }
                let wall_x = x + 3;
                if (-2..=2).any(|dy| sim.world.tile_at(wall_x, y + dy).is_none()) {
                    continue;
                }
                return Some(((x, y), (x + 6, y), wall_x, y));
            }
        }
        None
    }

    /// Like [`clear_run_with_wall_room`], but the parallel row above the run
    /// is also clear, so a single-tile blocker still leaves a walkable
    /// detour for routing tests.
    fn clear_run_with_open_detour(sim: &Simulation) -> Option<WallGeometry> {
        for chunk in sim.world.chunks.values() {
            for (index, _) in chunk.tiles.iter().enumerate() {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                let (x, y) = chunk.coord.tile_at(local_x, local_y);
                if !(0..6).all(|dx| corridor_is_clear(sim, x + dx, y))
                    || !(0..6).all(|dx| corridor_is_clear(sim, x + dx, y + 1))
                {
                    continue;
                }
                let wall_x = x + 3;
                if (-2..=2).any(|dy| sim.world.tile_at(wall_x, y + dy).is_none()) {
                    continue;
                }
                return Some(((x, y), (x + 6, y), wall_x, y));
            }
        }
        None
    }

    /// Long straight corridor two tiles wide and longer than routing range,
    /// built synthetically so the test never depends on world generation:
    /// every tile is cleared of entities and paved walkable except one
    /// flooded blocker next to the start. Returns `(start, destination)`
    /// with `distance > ENEMY_PATHFIND_MAX_RANGE_TILES`.
    fn long_corridor_with_blocker(
        sim: &mut Simulation,
    ) -> (
        (WorldTileCoord, WorldTileCoord),
        (WorldTileCoord, WorldTileCoord),
    ) {
        const LENGTH: i64 = 55;
        let concrete = factory_data::BasePrototypeIds::from_catalog(&sim.world.prototypes)
            .tiles
            .concrete;
        let anchor = sim
            .world
            .chunks
            .values()
            .flat_map(|chunk| {
                chunk.tiles.iter().enumerate().map(|(index, _)| {
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    chunk.coord.tile_at(local_x, local_y)
                })
            })
            .find(|&(x, y)| corridor_is_clear(sim, x, y))
            .expect("test world should contain a clear tile");
        for dx in 0..LENGTH {
            for dy in 0..2 {
                let coord = ChunkCoord::from_tile(anchor.0 + dx, anchor.1 + dy)
                    .expect("corridor tiles should be in the chunk plane");
                sim.ensure_chunk_generated(coord);
            }
        }
        for dx in 0..LENGTH {
            for dy in 0..2 {
                let (x, y) = (anchor.0 + dx, anchor.1 + dy);
                if let Some(occupant) = sim.entities.occupancy.entity_at(x, y) {
                    crate::entity_mutation::remove(sim, occupant)
                        .expect("corridor entity should be removable");
                }
                let paved = sim
                    .world
                    .tile_at(x, y)
                    .is_some_and(|tile| tile.tile_id == concrete);
                if !paved {
                    sim.world
                        .set_tile(x, y, concrete)
                        .expect("corridor tile should accept paving");
                }
                assert!(
                    tile_open_for_enemy(&sim.world, &sim.entities, x, y, None),
                    "corridor tile should be open after clearing"
                );
            }
        }
        let blocker = (anchor.0 + 1, anchor.1);
        flood_tile(sim, blocker.0, blocker.1);
        ((anchor.0, anchor.1), (anchor.0 + 50, anchor.1))
    }

    /// Wide deterministic ground with a wall extending beyond the first
    /// 16-tile local detour window. The route remains open one row beyond it.
    fn long_corridor_with_tall_wall(
        sim: &mut Simulation,
    ) -> (
        (WorldTileCoord, WorldTileCoord),
        (WorldTileCoord, WorldTileCoord),
    ) {
        const MIN_DX: i64 = -12;
        const MAX_DX: i64 = 54;
        const HALF_HEIGHT: i64 = 20;
        const WALL_HALF_HEIGHT: i64 = 16;
        let concrete = factory_data::BasePrototypeIds::from_catalog(&sim.world.prototypes)
            .tiles
            .concrete;
        let anchor = sim
            .world
            .chunks
            .values()
            .flat_map(|chunk| {
                chunk.tiles.iter().enumerate().map(|(index, _)| {
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    chunk.coord.tile_at(local_x, local_y)
                })
            })
            .find(|&(x, y)| corridor_is_clear(sim, x, y))
            .expect("test world should contain a clear tile");

        for dx in MIN_DX..=MAX_DX {
            for dy in -HALF_HEIGHT..=HALF_HEIGHT {
                let tile = (anchor.0 + dx, anchor.1 + dy);
                let chunk = ChunkCoord::from_tile(tile.0, tile.1)
                    .expect("test rectangle should be in the chunk plane");
                sim.ensure_chunk_generated(chunk);
                if let Some(occupant) = sim.entities.occupancy.entity_at(tile.0, tile.1) {
                    crate::entity_mutation::remove(sim, occupant)
                        .expect("rectangle entity should be removable");
                }
                if !sim
                    .world
                    .tile_at(tile.0, tile.1)
                    .is_some_and(|world_tile| world_tile.tile_id == concrete)
                {
                    sim.world
                        .set_tile(tile.0, tile.1, concrete)
                        .expect("test rectangle should accept paving");
                }
            }
        }
        for dy in -WALL_HALF_HEIGHT..=WALL_HALF_HEIGHT {
            flood_tile(sim, anchor.0 + 1, anchor.1 + dy);
        }
        ((anchor.0, anchor.1), (anchor.0 + 50, anchor.1))
    }

    fn flood_tile(sim: &mut Simulation, x: WorldTileCoord, y: WorldTileCoord) {
        let already_blocked = sim
            .world
            .tile_at(x, y)
            .is_some_and(|tile| !tile.collision.walkable);
        if already_blocked {
            return;
        }
        sim.world
            .set_tile(x, y, water_id(sim))
            .expect("blocker tile should be generated ground before flooding");
    }

    fn build_water_wall(sim: &mut Simulation, wall_x: WorldTileCoord, base_y: WorldTileCoord) {
        let water = water_id(sim);
        for dy in -2..=2 {
            let (x, y) = (wall_x, base_y + dy);
            let already_blocked = sim
                .world
                .tile_at(x, y)
                .is_some_and(|tile| !tile.collision.walkable);
            if already_blocked {
                continue;
            }
            // Test corridor tiles are ground, so the rewrite must succeed.
            sim.world
                .set_tile(x, y, water)
                .expect("wall tile should be generated ground before flooding");
        }
    }

    fn test_enemy_at(x: WorldTileCoord, y: WorldTileCoord, speed: u32) -> Enemy {
        Enemy {
            id: EnemyId::new(7),
            x: tile_center_fixed(x),
            y: tile_center_fixed(y),
            health: HealthState::new(100, Faction::Enemy),
            attack: AttackDefinition::melee(Damage::physical(5), 60, 1),
            speed_fixed_per_tick: speed,
            aggro_radius_tiles: 0,
            mode: EnemyMode::Guard,
            mission: EnemyMission::Guard,
            home_spawner: None,
            target: None,
            path: VecDeque::new(),
            next_attack_tick: 0,
            next_decision_tick: 0,
        }
    }

    fn place_chest_at(sim: &mut Simulation, tile: (WorldTileCoord, WorldTileCoord)) -> EntityId {
        let chest = sim
            .world
            .prototypes
            .entities()
            .iter()
            .find(|prototype| prototype.name == "chest")
            .expect("base data should contain a chest")
            .id;
        crate::placement::place(
            sim,
            crate::placement::EntityPlacementRequest {
                prototype_id: chest,
                x: tile.0,
                y: tile.1,
                direction: Direction::North,
            },
        )
        .expect("the cleared corridor should accept a target chest")
    }

    fn step_test_enemy(sim: &Simulation, navigation: &mut EnemyNavigation, enemy: &mut Enemy) {
        let mut attack_targets = AttackTargetCache::default();
        let mut context = EnemyStepContext {
            world: &sim.world,
            entities: &sim.entities,
            attack_targets: &mut attack_targets,
            navigation,
            seed: sim.world.seed,
            tick: sim.tick,
        };
        step_enemy(&mut context, enemy, &mut CombatCommandBuffer::default());
    }

    #[test]
    fn expansion_step_stops_before_water_strip() {
        let mut sim = Simulation::new_test_world(123);
        let (start, destination, wall_x, base_y) =
            clear_run_with_wall_room(&sim).expect("test world should contain a clear run");
        build_water_wall(&mut sim, wall_x, base_y);
        assert!(corridor_is_clear(&sim, start.0, start.1));
        assert!(corridor_is_clear(&sim, destination.0, destination.1));
        assert!(
            !tile_open_for_enemy(&sim.world, &sim.entities, wall_x, base_y, None),
            "wall tile should block enemies"
        );

        let first = expansion_step_toward(&sim.world, &sim.entities, start, destination)
            .expect("first step toward destination should exist");
        assert_eq!(
            (first.0 - start.0).abs() + (first.1 - start.1).abs(),
            1,
            "expansion must advance one adjacent tile"
        );
        assert!(
            tile_open_for_enemy(&sim.world, &sim.entities, first.0, first.1, None),
            "expansion must only step onto open tiles"
        );
        assert_ne!(first, destination);

        let before_wall = (wall_x - 1, base_y);
        assert!(
            expansion_step_toward(&sim.world, &sim.entities, before_wall, destination).is_none(),
            "expansion must wait rather than step into the water strip"
        );
    }

    fn routed_path(
        sim: &mut Simulation,
        navigation: &mut EnemyNavigation,
        start: (WorldTileCoord, WorldTileCoord),
        goal: (WorldTileCoord, WorldTileCoord),
    ) -> Option<VecDeque<(WorldTileCoord, WorldTileCoord)>> {
        match navigation.request_tile_path(
            &sim.world,
            &sim.entities,
            start,
            goal,
            ENEMY_PATHFIND_MAX_RANGE_TILES,
            ENEMY_PATHFIND_MAX_EXPANSIONS,
        ) {
            PathRequest::Ready(path) => path,
            PathRequest::Deferred => panic!("test navigation budget should cover one request"),
        }
    }

    fn assert_valid_steps(
        sim: &Simulation,
        start: (WorldTileCoord, WorldTileCoord),
        path: &VecDeque<(WorldTileCoord, WorldTileCoord)>,
    ) {
        assert!(!path.is_empty(), "routing should return steps");
        let mut previous = start;
        for &step in path {
            assert_eq!(
                (step.0 - previous.0).abs() + (step.1 - previous.1).abs(),
                1,
                "every routed step must be 4-connected"
            );
            assert!(
                tile_open_for_enemy(&sim.world, &sim.entities, step.0, step.1, None),
                "no routed step may cross blocked terrain"
            );
            previous = step;
        }
    }

    fn assert_valid_detour(
        sim: &Simulation,
        start: (WorldTileCoord, WorldTileCoord),
        destination: (WorldTileCoord, WorldTileCoord),
        path: &VecDeque<(WorldTileCoord, WorldTileCoord)>,
    ) {
        assert_valid_steps(sim, start, path);
        assert_eq!(
            *path.back().expect("non-empty path has a goal"),
            destination
        );
    }

    #[test]
    fn tile_routing_detours_around_single_blocker() {
        let mut sim = Simulation::new_test_world(123);
        let (start, destination, wall_x, base_y) =
            clear_run_with_open_detour(&sim).expect("test world should contain a detour run");
        flood_tile(&mut sim, wall_x, base_y);

        let mut navigation = EnemyNavigation::default();
        navigation.begin_tick(0, 0, 0);
        let path = routed_path(&mut sim, &mut navigation, start, destination)
            .expect("a detour around one tile must exist");
        assert_valid_detour(&sim, start, destination, &path);
        assert!(
            !path.contains(&(wall_x, base_y)),
            "the detour must avoid the flooded tile"
        );
    }

    #[test]
    fn tile_routing_returns_none_for_blocked_goal() {
        // A destination that becomes blocked (or was never clear) yields no
        // route instead of a path through blocked terrain.
        let mut sim = Simulation::new_test_world(123);
        let (start, destination, _, _) =
            clear_run_with_wall_room(&sim).expect("test world should contain a clear run");
        flood_tile(&mut sim, destination.0, destination.1);

        let mut navigation = EnemyNavigation::default();
        navigation.begin_tick(0, 0, 0);
        assert!(
            routed_path(&mut sim, &mut navigation, start, destination).is_none(),
            "a blocked goal must yield no route"
        );
    }

    #[test]
    fn follow_path_discards_distant_waypoint_without_crossing() {
        let mut sim = Simulation::new_test_world(123);
        let (start, destination, wall_x, base_y) =
            clear_run_with_wall_room(&sim).expect("test world should contain a clear run");
        build_water_wall(&mut sim, wall_x, base_y);

        let mut enemy = test_enemy_at(start.0, start.1, 1024);
        enemy.path.push_back(destination);
        let before = (enemy.x, enemy.y);
        follow_path(&sim.world, &sim.entities, &mut enemy, None);
        assert_eq!((enemy.x, enemy.y), before, "distant waypoint must not move");
        assert!(enemy.path.is_empty(), "distant waypoint must be discarded");
        assert_eq!(enemy.tile(), start);
    }

    #[test]
    fn adjacent_waypoints_move_deterministically() {
        let sim = Simulation::new_test_world(123);
        let (start, _, _, _) =
            clear_run_with_wall_room(&sim).expect("test world should contain a clear run");
        let first = (start.0 + 1, start.1);
        let second = (start.0 + 2, start.1);
        assert!(corridor_is_clear(&sim, first.0, first.1));
        assert!(corridor_is_clear(&sim, second.0, second.1));

        let mut enemy = test_enemy_at(start.0, start.1, 1024);
        enemy.path.push_back(first);
        enemy.path.push_back(second);
        follow_path(&sim.world, &sim.entities, &mut enemy, None);
        assert_eq!(enemy.tile(), first);
        assert_eq!(enemy.path.len(), 1);
        follow_path(&sim.world, &sim.entities, &mut enemy, None);
        assert_eq!(enemy.tile(), second);
        assert!(enemy.path.is_empty());
    }

    #[test]
    fn waypoints_complete_at_exact_centers_with_sub_tile_speed() {
        // Production enemy speed (40 fixed units/tick) crosses the tile
        // boundary 512 units before the waypoint center. Completion must key
        // off the exact center, never `Enemy::tile()`.
        let sim = Simulation::new_test_world(123);
        let (start, _, _, _) =
            clear_run_with_wall_room(&sim).expect("test world should contain a clear run");
        let first = (start.0 + 1, start.1);
        let second = (start.0 + 2, start.1);

        let mut enemy = test_enemy_at(start.0, start.1, 40);
        enemy.path.push_back(first);
        enemy.path.push_back(second);
        let start_center_x = tile_center_fixed(start.0);
        let first_center_x = tile_center_fixed(first.0);
        let second_center_x = tile_center_fixed(second.0);
        let center_y = tile_center_fixed(start.1);

        for _ in 0..13 {
            follow_path(&sim.world, &sim.entities, &mut enemy, None);
        }
        // 520 units along: the tile already flipped to `first`, but the
        // center (1024) is not reached, so the waypoint must be retained.
        assert_eq!(enemy.x, start_center_x + 520);
        assert_eq!(enemy.tile(), first);
        assert_eq!(enemy.path.front(), Some(&first));

        for _ in 0..12 {
            follow_path(&sim.world, &sim.entities, &mut enemy, None);
        }
        assert_eq!(enemy.x, start_center_x + 1000);
        assert_eq!(enemy.path.front(), Some(&first));

        // The 26th tick reaches the first center exactly (24 units), pops it,
        // and spends the leftover budget (16 units) on the second leg.
        follow_path(&sim.world, &sim.entities, &mut enemy, None);
        assert_eq!((enemy.x, enemy.y), (first_center_x + 16, center_y));
        assert_eq!(enemy.path.front(), Some(&second));

        for _ in 0..25 {
            follow_path(&sim.world, &sim.entities, &mut enemy, None);
        }
        assert_eq!(enemy.x, start_center_x + 2040);
        assert_eq!(enemy.path.front(), Some(&second));

        follow_path(&sim.world, &sim.entities, &mut enemy, None);
        assert_eq!((enemy.x, enemy.y), (second_center_x, center_y));
        assert!(enemy.path.is_empty());
    }

    fn expansion_member(
        sim: &mut Simulation,
        start: (WorldTileCoord, WorldTileCoord),
        destination: (WorldTileCoord, WorldTileCoord),
    ) -> EnemyId {
        let enemy_id = sim.enemies.allocate_id();
        let expansion_id = sim.enemies.allocate_expansion_id();
        sim.enemies.enemies.insert(
            enemy_id,
            Enemy {
                id: enemy_id,
                x: tile_center_fixed(start.0),
                y: tile_center_fixed(start.1),
                health: HealthState::new(100, Faction::Enemy),
                attack: AttackDefinition::melee(Damage::physical(5), 60, 1),
                speed_fixed_per_tick: 40,
                aggro_radius_tiles: 0,
                mode: EnemyMode::Attack,
                mission: EnemyMission::Expansion(expansion_id),
                home_spawner: None,
                target: None,
                path: VecDeque::new(),
                next_attack_tick: 0,
                next_decision_tick: 0,
            },
        );
        sim.enemies.expansions.insert(
            expansion_id,
            Expansion {
                id: expansion_id,
                base_id: EnemyBaseId::new(1),
                members: BTreeSet::from([enemy_id]),
                destination,
                spotted: true,
                spawner_prototype: factory_data::entity_prototype_id_by_name(
                    &sim.world.prototypes,
                    "biter_spawner",
                ),
            },
        );
        enemy_id
    }

    #[test]
    fn long_range_expansion_detours_around_nearby_blocker() {
        // Distance 50 exceeds the 40-tile routing range, with a blocker that
        // the purely greedy fallback cannot pass: it only tries
        // Manhattan-reducing steps, and the only open first step detours.
        let mut sim = Simulation::new_test_world(123);
        let (start, destination) = long_corridor_with_blocker(&mut sim);
        assert!(
            (destination.0 - start.0).abs() > ENEMY_PATHFIND_MAX_RANGE_TILES,
            "test geometry must exceed routing range"
        );
        assert!(
            expansion_step_toward(&sim.world, &sim.entities, start, destination).is_none(),
            "the greedy fallback alone must stall at this blocker"
        );

        let mut navigation = EnemyNavigation::default();
        navigation.begin_tick(0, 0, 0);
        let steps = match plan_expansion_move(
            &mut navigation,
            &sim.world,
            &sim.entities,
            start,
            destination,
        ) {
            ExpansionRoute::Steps(steps) => steps,
            ExpansionRoute::Wait => panic!("a detour around one tile must exist"),
        };
        let blocker = (start.0 + 1, start.1);
        let intermediate = (start.0 + ENEMY_PATHFIND_MAX_RANGE_TILES, start.1);
        let front = *steps.front().expect("routing must return steps");
        assert_eq!(
            (front.0 - start.0).abs() + (front.1 - start.1).abs(),
            1,
            "the first routed step must be adjacent"
        );
        let mut previous = start;
        for &step in &steps {
            assert_eq!(
                (step.0 - previous.0).abs() + (step.1 - previous.1).abs(),
                1,
                "every routed step must be 4-connected"
            );
            assert!(
                tile_open_for_enemy(&sim.world, &sim.entities, step.0, step.1, None),
                "no routed step may cross blocked terrain"
            );
            previous = step;
        }
        assert!(
            !steps.contains(&blocker),
            "the long-range route must avoid the blocker"
        );
        assert_eq!(
            *steps.back().expect("routing must return steps"),
            intermediate,
            "beyond range, routing heads for the intermediate goal"
        );
    }

    #[test]
    fn distant_combat_enemy_detours_when_greedy_steps_are_blocked() {
        let mut sim = Simulation::new_test_world(123);
        let (start, destination) = long_corridor_with_blocker(&mut sim);
        let blocked_intermediate = bounded_intermediate_goal(start, destination);
        flood_tile(&mut sim, blocked_intermediate.0, blocked_intermediate.1);
        let target = place_chest_at(&mut sim, destination);
        let target_footprint = sim.entities.placed_entities[&target].footprint;
        assert!(
            EntityFootprint::single_tile(start.0, start.1).chebyshev_distance_to(&target_footprint)
                > ENEMY_PATHFIND_MAX_RANGE_TILES,
            "test target must exercise long-range navigation"
        );
        let mut exact_goal_navigation = EnemyNavigation::default();
        exact_goal_navigation.begin_tick(0, 0, 0);
        assert!(
            routed_path(
                &mut sim,
                &mut exact_goal_navigation,
                start,
                blocked_intermediate,
            )
            .is_none(),
            "the former exact intermediate strategy must fail in this geometry"
        );

        let mut enemy = test_enemy_at(start.0, start.1, 40);
        enemy.target = Some(target);
        let mut navigation = EnemyNavigation::default();
        navigation.begin_tick(0, 0, 0);

        step_test_enemy(&sim, &mut navigation, &mut enemy);

        assert_valid_steps(&sim, start, &enemy.path);
        let frontier = *enemy.path.back().expect("route has a frontier");
        assert_eq!(
            EntityFootprint::single_tile(start.0, start.1)
                .chebyshev_distance_to(&EntityFootprint::single_tile(frontier.0, frontier.1)),
            ENEMY_LONG_RANGE_DETOUR_TILES,
            "the bounded route must end at a reachable search frontier"
        );
        assert!(
            !enemy.path.contains(&(start.0 + 1, start.1)),
            "the route must avoid the flooded direct step"
        );
        assert!(
            enemy.y > tile_center_fixed(start.1),
            "the first movement must take the required lateral detour"
        );

        let mut reached_target = false;
        for tick in 1..=2_000 {
            sim.tick = tick;
            navigation.begin_tick(0, 0, 0);
            step_test_enemy(&sim, &mut navigation, &mut enemy);
            if enemy_footprint(&enemy).chebyshev_distance_to(&target_footprint)
                <= i64::from(enemy.attack.delivery.range_tiles())
            {
                reached_target = true;
                break;
            }
        }
        assert!(
            reached_target,
            "the frontier detour must pass the blocked intermediate and reach the target"
        );
    }

    #[test]
    fn distant_combat_enemy_shifts_search_past_a_wider_wall() {
        let mut sim = Simulation::new_test_world(123);
        let (start, destination) = long_corridor_with_tall_wall(&mut sim);
        let intermediate = bounded_intermediate_goal(start, destination);
        let mut full_range_navigation = EnemyNavigation::default();
        full_range_navigation.begin_tick(0, 0, 0);
        assert!(
            routed_path(&mut sim, &mut full_range_navigation, start, intermediate,).is_none(),
            "the full-range search must exhaust its request budget in this geometry"
        );
        assert_eq!(
            full_range_navigation.expansions_this_tick, ENEMY_PATHFIND_MAX_EXPANSIONS,
            "the regression must distinguish exhaustion from a completed search"
        );

        let target = place_chest_at(&mut sim, destination);
        let mut enemy = test_enemy_at(start.0, start.1, 40);
        enemy.target = Some(target);
        let mut navigation = EnemyNavigation::default();
        navigation.begin_tick(0, 0, 0);

        step_test_enemy(&sim, &mut navigation, &mut enemy);
        assert_valid_steps(&sim, start, &enemy.path);
        let detour = navigation
            .long_range_detour(enemy.id)
            .expect("the disconnected first window must start a durable detour");
        assert_eq!(detour.forward, Direction::East);
        assert!(
            enemy
                .path
                .front()
                .is_some_and(|step| step.0 == start.0 && step.1.abs_diff(start.1) == 1),
            "the first failed window must shift laterally instead of retrying in place"
        );
        assert!(
            navigation.expansions_this_tick <= ENEMY_LONG_RANGE_DETOUR_MAX_EXPANSIONS,
            "each local frontier search must have a provably complete work bound"
        );

        let mut crossed_wall = false;
        for tick in 1..=1_500 {
            sim.tick = tick;
            navigation.begin_tick(0, 0, 0);
            step_test_enemy(&sim, &mut navigation, &mut enemy);
            if enemy.tile().0 > start.0 + 1 {
                crossed_wall = true;
                break;
            }
        }
        assert!(
            crossed_wall,
            "successive bounded searches must shift around a wall wider than the first window: start={start:?}, destination={destination:?}, tile={:?}, next_decision={}, path={:?}",
            enemy.tile(),
            enemy.next_decision_tick,
            enemy.path
        );
    }

    #[test]
    fn unreachable_distant_combat_target_uses_bounded_search() {
        let mut sim = Simulation::new_test_world(123);
        let (start, destination) = long_corridor_with_blocker(&mut sim);
        for neighbor in [
            (start.0 - 1, start.1),
            (start.0, start.1 - 1),
            (start.0, start.1 + 1),
        ] {
            let chunk = ChunkCoord::from_tile(neighbor.0, neighbor.1)
                .expect("neighbor should be in the chunk plane");
            sim.ensure_chunk_generated(chunk);
            if let Some(occupant) = sim.entities.occupancy.entity_at(neighbor.0, neighbor.1) {
                crate::entity_mutation::remove(&mut sim, occupant)
                    .expect("neighbor entity should be removable");
            }
            flood_tile(&mut sim, neighbor.0, neighbor.1);
        }
        let target = place_chest_at(&mut sim, destination);
        let mut enemy = test_enemy_at(start.0, start.1, 40);
        enemy.target = Some(target);
        let before = (enemy.x, enemy.y);
        let mut navigation = EnemyNavigation::default();
        navigation.begin_tick(0, 0, 0);

        step_test_enemy(&sim, &mut navigation, &mut enemy);

        assert_eq!((enemy.x, enemy.y), before, "an enclosed unit must wait");
        assert!(enemy.path.is_empty());
        assert!(
            enemy.next_decision_tick > sim.tick,
            "retries must be throttled"
        );
        assert!(
            navigation.expansions_this_tick > 0,
            "the regression must exercise bounded routing"
        );
        assert!(
            navigation.expansions_this_tick <= ENEMY_LONG_RANGE_DETOUR_MAX_EXPANSIONS,
            "an unreachable target must stay within the per-request budget"
        );
    }

    #[test]
    fn unobstructed_distant_combat_keeps_zero_cost_greedy_step() {
        let mut sim = Simulation::new_test_world(123);
        let (start, destination) = long_corridor_with_blocker(&mut sim);
        let concrete = factory_data::BasePrototypeIds::from_catalog(&sim.world.prototypes)
            .tiles
            .concrete;
        sim.world
            .set_tile(start.0 + 1, start.1, concrete)
            .expect("the synthetic blocker should accept landfill");
        let target = place_chest_at(&mut sim, destination);
        let mut enemy = test_enemy_at(start.0, start.1, 40);
        enemy.target = Some(target);
        let before_x = enemy.x;
        let mut navigation = EnemyNavigation::default();
        navigation.begin_tick(0, 0, 0);

        step_test_enemy(&sim, &mut navigation, &mut enemy);

        assert!(
            enemy.x > before_x,
            "the enemy should move directly toward the target"
        );
        assert_eq!(enemy.y, tile_center_fixed(start.1));
        assert_eq!(
            navigation.expansions_this_tick, 0,
            "open long-range movement should not invoke A*"
        );
    }

    #[test]
    fn expansion_prefill_routes_adjacent_steps_instead_of_destination() {
        let mut sim = Simulation::new_test_world(123);
        let (start, destination, wall_x, base_y) =
            clear_run_with_open_detour(&sim).expect("test world should contain a detour run");
        flood_tile(&mut sim, wall_x, base_y);
        let enemy_id = expansion_member(&mut sim, start, destination);

        sim.advance_enemies(&mut CombatCommandBuffer::default());

        let unit = &sim.enemies.enemies[&enemy_id];
        assert!(
            !unit.path.is_empty(),
            "reachable expansion destination must produce a path"
        );
        assert_ne!(
            unit.path.front(),
            Some(&destination),
            "pre-fill must not push the distant destination directly"
        );
        assert_valid_detour(&sim, start, destination, &unit.path);
    }

    #[test]
    fn expansion_prefill_waits_and_throttles_when_unreachable() {
        // Flooding the destination itself guarantees no route regardless of
        // detours, mirroring a site that becomes blocked after dispatch.
        let mut sim = Simulation::new_test_world(123);
        let (start, destination, _, _) =
            clear_run_with_wall_room(&sim).expect("test world should contain a clear run");
        flood_tile(&mut sim, destination.0, destination.1);
        let tick = sim.tick;
        let enemy_id = expansion_member(&mut sim, start, destination);

        sim.advance_enemies(&mut CombatCommandBuffer::default());

        let unit = &sim.enemies.enemies[&enemy_id];
        assert!(unit.target.is_none(), "test setup should stay targetless");
        assert!(
            unit.path.is_empty(),
            "unreachable expansion destination must leave the path empty"
        );
        assert!(
            unit.next_decision_tick > tick,
            "failed expansion routing must throttle retries"
        );
        assert_eq!(unit.tile(), start, "the unit must not move");
    }

    #[test]
    fn expansion_prefill_reclaims_attack_target_without_stall() {
        // Combat resolution assigns a retaliation target and clears the path
        // without touching the repath deadline; pre-fix saves carry the same
        // shape. The prefill must drop the target before routing and replan
        // on the same tick, or the unit stalls while under fire.
        let mut sim = Simulation::new_test_world(123);
        let (start, destination, _, _) =
            clear_run_with_wall_room(&sim).expect("test world should contain a clear run");
        let enemy_id = expansion_member(&mut sim, start, destination);
        sim.tick = 10;
        {
            let unit = sim.enemies.enemies.get_mut(&enemy_id).unwrap();
            unit.target = Some(EntityId::new(4242));
            unit.next_decision_tick = 500;
        }

        sim.advance_enemies(&mut CombatCommandBuffer::default());

        let unit = &sim.enemies.enemies[&enemy_id];
        assert_eq!(
            unit.target, None,
            "the attack target must be dropped before routing"
        );
        assert!(
            !unit.path.is_empty(),
            "the expansion route must replan on the same tick"
        );
        assert_eq!(
            unit.next_decision_tick, sim.tick,
            "reclaiming a target must not inherit the old repath deadline"
        );
        assert_valid_detour(&sim, start, destination, &unit.path);
    }
}
