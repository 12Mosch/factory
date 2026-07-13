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
/// Distant targets use deterministic greedy movement instead of A*.
const ENEMY_PATHFIND_MAX_RANGE_TILES: i64 = 40;

impl Simulation {
    pub(in crate::simulation) fn advance_enemies(&mut self, commands: &mut CombatCommandBuffer) {
        self.enemy_navigation
            .begin_tick(self.entity_topology_revision, self.world.chunk_revision());
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
        for party in enemies.expansions.values() {
            for member in &party.members {
                if let Some(unit) = enemies.enemies.get_mut(member)
                    && unit.path.is_empty()
                {
                    unit.path.push_back(party.destination);
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

struct EnemyStepContext<'a> {
    world: &'a WorldSim,
    entities: &'a EntityStore,
    attack_targets: &'a mut AttackTargetCache,
    navigation: &'a mut EnemyNavigation,
    seed: u64,
    tick: u64,
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
        wander(world, entities, seed, tick, enemy);
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
        follow_path(enemy);
        return;
    }

    if (enemy.path.is_empty() || next_waypoint_blocked) && tick >= enemy.next_decision_tick {
        enemy.path.clear();
        if enemy_footprint(enemy).chebyshev_distance_to(&target_footprint)
            <= ENEMY_PATHFIND_MAX_RANGE_TILES
        {
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
            enemy.next_decision_tick = tick + ENEMY_REPATH_INTERVAL_TICKS + enemy.id.raw() % 16;
        }
        if enemy.path.is_empty() {
            // No route: walk straight at the target and gnaw through the
            // first structure in the way.
            greedy_step(world, entities, enemy, target, &target_footprint);
            return;
        }
    }

    follow_path(enemy);
}

/// Idle guards drift around their home spawner so nests look alive.
fn wander(world: &WorldSim, entities: &EntityStore, seed: u64, tick: u64, enemy: &mut Enemy) {
    if !enemy.path.is_empty() {
        follow_path(enemy);
        return;
    }
    if tick < enemy.next_decision_tick {
        return;
    }
    enemy.next_decision_tick = tick + ENEMY_WANDER_INTERVAL_TICKS + enemy.id.raw() % 64;

    let anchor = enemy
        .home_spawner
        .and_then(|spawner| entities.placed_entities.get(&spawner))
        .map(|placed| footprint_center_tile(&placed.footprint))
        .unwrap_or_else(|| enemy.tile());
    let roll = splitmix64(seed ^ enemy.id.raw().wrapping_mul(0x9e37_79b9) ^ tick);
    let dx = ((roll & 0x7) as i64) - 3;
    let dy = (((roll >> 3) & 0x7) as i64) - 3;
    let goal = (anchor.0 + dx, anchor.1 + dy);
    if tile_open_for_enemy(world, entities, goal.0, goal.1, None) {
        enemy.path.push_back(goal);
    }
}

/// Advances the unit along its waypoints by one tick's movement budget.
/// Waypoints are 4-connected tile centers, so per-leg movement is
/// axis-aligned and stays exact in fixed-point integers.
fn follow_path(enemy: &mut Enemy) {
    let mut budget = i64::from(enemy.speed_fixed_per_tick);
    while budget > 0 {
        let Some(&(waypoint_x, waypoint_y)) = enemy.path.front() else {
            return;
        };
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
fn greedy_step(
    world: &WorldSim,
    entities: &EntityStore,
    enemy: &mut Enemy,
    target: EntityId,
    target_footprint: &EntityFootprint,
) {
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
            follow_path(enemy);
            return;
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
            return;
        }
    }
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
