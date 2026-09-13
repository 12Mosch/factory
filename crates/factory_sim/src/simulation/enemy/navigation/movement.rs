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

type TilePos = (WorldTileCoord, WorldTileCoord);
/// Pending expansion move: unit, current tile, and party destination.
type ExpansionIntent = (EnemyId, TilePos, TilePos);

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
        // Expansion parties advance one validated adjacent tile at a time.
        // Pushing `party.destination` directly would let `follow_path`
        // tunnel across blocked intermediate terrain.
        let expansion_intents: Vec<ExpansionIntent> = enemies
            .expansions
            .values()
            .flat_map(|party| {
                party
                    .members
                    .iter()
                    .map(|member| (*member, party.destination))
            })
            .filter_map(|(member, destination)| {
                let unit = enemies.enemies.get(&member)?;
                unit.path
                    .is_empty()
                    .then(|| (member, unit.tile(), destination))
            })
            .collect();
        for (member, from, destination) in expansion_intents {
            if let Some(next) = expansion_step_toward(world, entities, from, destination)
                && let Some(unit) = enemies.enemies.get_mut(&member)
                && unit.path.is_empty()
            {
                unit.path.push_back(next);
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
        let target = enemy.target;
        follow_path(world, entities, enemy, target);
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

    let target = enemy.target;
    follow_path(world, entities, enemy, target);
}

/// Idle guards drift around their home spawner so nests look alive.
fn wander(world: &WorldSim, entities: &EntityStore, seed: u64, tick: u64, enemy: &mut Enemy) {
    if !enemy.path.is_empty() {
        follow_path(world, entities, enemy, None);
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
    if let Some(path) = wander_path_toward(world, entities, enemy.tile(), goal) {
        enemy.path = path;
    }
}

/// Single validated adjacent step toward a tile destination for expansion
/// parties. Only 4-connected moves are returned, and the returned tile is
/// verified open, so callers never insert a distant waypoint that would
/// tunnel across blocked terrain. Returns `None` when every adjacent step
/// toward the destination is blocked; the unit waits instead of crossing.
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

/// Validated adjacent-step path for idle wandering. The goal may be several
/// tiles away, so it is expanded into 4-connected steps with a deterministic
/// breadth-first search over walkable tiles. Returns `None` when the goal is
/// blocked or no walkable route exists; the unit waits instead of crossing.
fn wander_path_toward(
    world: &WorldSim,
    entities: &EntityStore,
    start: (WorldTileCoord, WorldTileCoord),
    goal: (WorldTileCoord, WorldTileCoord),
) -> Option<VecDeque<(WorldTileCoord, WorldTileCoord)>> {
    if start == goal {
        return Some(VecDeque::new());
    }
    if !tile_open_for_enemy(world, entities, goal.0, goal.1, None) {
        return None;
    }
    let min_x = start.0.min(goal.0).saturating_sub(1);
    let max_x = start.0.max(goal.0).saturating_add(1);
    let min_y = start.1.min(goal.1).saturating_sub(1);
    let max_y = start.1.max(goal.1).saturating_add(1);
    let mut queue = VecDeque::from([start]);
    let mut visited = BTreeSet::from([start]);
    let mut came_from: BTreeMap<
        (WorldTileCoord, WorldTileCoord),
        (WorldTileCoord, WorldTileCoord),
    > = BTreeMap::new();
    while let Some(tile) = queue.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (Some(next_x), Some(next_y)) = (tile.0.checked_add(dx), tile.1.checked_add(dy))
            else {
                continue;
            };
            if next_x < min_x || next_x > max_x || next_y < min_y || next_y > max_y {
                continue;
            }
            let next = (next_x, next_y);
            if !visited.insert(next) {
                continue;
            }
            if !tile_open_for_enemy(world, entities, next_x, next_y, None) {
                continue;
            }
            came_from.insert(next, tile);
            if next == goal {
                let mut path = VecDeque::new();
                let mut current = next;
                while current != start {
                    path.push_front(current);
                    current = came_from[&current];
                }
                return Some(path);
            }
            queue.push_back(next);
        }
    }
    None
}

/// Advances the unit along its waypoints by one tick's movement budget.
///
/// Waypoint contract: `enemy.path` holds 4-connected adjacent tile centers
/// (Manhattan distance exactly 1 from the unit's current tile to the front
/// waypoint, and between consecutive waypoints). Per-leg movement is then
/// axis-aligned and stays exact in fixed-point integers.
///
/// The contract is mechanically upheld here: a front waypoint equal to the
/// current tile is consumed, while a non-adjacent or blocked front waypoint
/// discards the path without moving, so a stale distant waypoint can never
/// tunnel across water or other blocked terrain.
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
        if waypoint == current {
            enemy.path.pop_front();
            continue;
        }
        if (waypoint.0.saturating_sub(current.0)).abs()
            + (waypoint.1.saturating_sub(current.1)).abs()
            != 1
        {
            enemy.path.clear();
            return;
        }
        if !tile_open_for_enemy(world, entities, waypoint.0, waypoint.1, target) {
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
            follow_path(world, entities, enemy, Some(target));
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
    type WallGeometry = (TilePos, TilePos, WorldTileCoord, WorldTileCoord);
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

    #[test]
    fn wander_path_does_not_cross_water_strip() {
        let mut sim = Simulation::new_test_world(123);
        let (start, destination, wall_x, base_y) =
            clear_run_with_wall_room(&sim).expect("test world should contain a clear run");
        build_water_wall(&mut sim, wall_x, base_y);

        assert!(
            wander_path_toward(&sim.world, &sim.entities, start, destination).is_none(),
            "no walkable wander route crosses the water wall"
        );

        let nearby = (start.0 + 1, start.1);
        let path = wander_path_toward(&sim.world, &sim.entities, start, nearby)
            .expect("adjacent open goal should route");
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], nearby);
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
}
