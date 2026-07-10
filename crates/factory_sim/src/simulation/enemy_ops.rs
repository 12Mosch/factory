use super::*;
use factory_data::{EnemyBaseGenerationConfig, UnitPrototype};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Ticks between target rescans for units without a target.
const ENEMY_TARGET_RESCAN_TICKS: u64 = 120;
/// Ticks between path recomputations while a target is set.
const ENEMY_REPATH_INTERVAL_TICKS: u64 = 90;
/// Ticks between wander moves for idle guards.
const ENEMY_WANDER_INTERVAL_TICKS: u64 = 300;
/// Upper bound on A* node expansions per path request.
const ENEMY_PATHFIND_MAX_EXPANSIONS: usize = 600;
/// Targets farther than this (in tiles, Chebyshev) are approached greedily
/// instead of path-searched.
const ENEMY_PATHFIND_MAX_RANGE_TILES: i64 = 40;
/// How far around its footprint a spawner looks for a free tile to place a
/// freshly spawned unit.
const SPAWN_SEARCH_RINGS: i64 = 3;
/// Salt mixed into the world seed for spawner placement rolls so they are
/// independent of terrain and resource noise.
const SPAWNER_PLACEMENT_SALT: u64 = 0x656e_656d_795f_6261;

impl Simulation {
    pub fn enemies(&self) -> &EnemySubsystem {
        &self.enemies
    }

    /// Rolls spawner placement for every generated chunk that has not been
    /// seeded yet. Runs after any chunk generation opportunity; placement is
    /// a pure function of the world seed and chunk coordinate.
    pub(super) fn seed_enemy_spawners_in_new_chunks(&mut self) {
        let Some(config) = self.world.prototypes.world_generation.enemy_bases else {
            return;
        };
        // Seeded chunks only ever come from generated chunks, so equal sizes
        // mean there is nothing new.
        if self.enemies.seeded_chunks.len() == self.world.chunks.len() {
            return;
        }

        let new_chunks: Vec<ChunkCoord> = self
            .world
            .chunks
            .keys()
            .filter(|coord| !self.enemies.seeded_chunks.contains(coord))
            .copied()
            .collect();
        for coord in new_chunks {
            self.enemies.seeded_chunks.insert(coord);
            self.try_place_spawner_in_chunk(coord, &config);
        }
    }

    fn try_place_spawner_in_chunk(
        &mut self,
        coord: ChunkCoord,
        config: &EnemyBaseGenerationConfig,
    ) {
        let (min_x, min_y) = coord.min_tile();
        let center_x = min_x + i64::from(CHUNK_SIZE) / 2;
        let center_y = min_y + i64::from(CHUNK_SIZE) / 2;
        let min_distance = i64::from(config.min_distance_tiles);
        let center_distance_squared = i128::from(center_x) * i128::from(center_x)
            + i128::from(center_y) * i128::from(center_y);
        let min_distance_squared = i128::from(min_distance) * i128::from(min_distance);
        if center_distance_squared < min_distance_squared {
            return;
        }

        let roll = splitmix64(
            self.world.seed ^ SPAWNER_PLACEMENT_SALT ^ hash_world(self.world.seed, min_x, min_y),
        );
        if roll % 100 >= u64::from(config.frequency_percent) {
            return;
        }

        let Some(prototype) = self.world.prototypes.entity(config.spawner_entity) else {
            return;
        };
        // Keep the footprint fully inside the chunk so seeding one chunk
        // never depends on whether its neighbors exist yet.
        let margin = 2;
        let span_x = i64::from(CHUNK_SIZE) - 2 * margin - i64::from(prototype.size.x);
        let span_y = i64::from(CHUNK_SIZE) - 2 * margin - i64::from(prototype.size.y);
        if span_x <= 0 || span_y <= 0 {
            return;
        }
        let x = min_x + margin + ((roll >> 8) % span_x as u64) as i64;
        let y = min_y + margin + ((roll >> 24) % span_y as u64) as i64;

        // Placement validation rejects water, resources, and occupied tiles;
        // a failed roll simply leaves the chunk without a nest.
        let _ = placement::place(
            self,
            placement::EntityPlacementRequest {
                prototype_id: config.spawner_entity,
                x,
                y,
                direction: Direction::North,
            },
        );
    }

    /// Spawner behavior: drain pollution from the local chunk, convert it
    /// into attacking units, and keep a small idle guard detail alive.
    pub(super) fn advance_enemy_spawners(&mut self) {
        struct SpawnRequest {
            spawner_id: EntityId,
            unit: UnitPrototype,
            mode: EnemyMode,
        }

        let mut alive_by_spawner = BTreeMap::<EntityId, u32>::new();
        let mut guards_by_spawner = BTreeMap::<EntityId, u32>::new();
        for enemy in self.enemies.enemies.values() {
            if let Some(spawner) = enemy.home_spawner {
                *alive_by_spawner.entry(spawner).or_default() += 1;
                if enemy.mode == EnemyMode::Guard {
                    *guards_by_spawner.entry(spawner).or_default() += 1;
                }
            }
        }

        let mut requests: Vec<SpawnRequest> = Vec::new();
        let tick = self.tick;
        let Simulation {
            world,
            entities,
            pollution,
            ..
        } = self;
        for (&spawner_id, state) in entities.enemy_spawners.iter_mut() {
            let Some(placed) = entities.placed_entities.get(&spawner_id) else {
                continue;
            };
            let Some(config) = world
                .prototypes
                .entity(placed.prototype_id)
                .and_then(|prototype| prototype.enemy_spawner.as_ref())
            else {
                continue;
            };
            let Some(coord) = ChunkCoord::from_tile(placed.x, placed.y) else {
                continue;
            };

            let absorbed = pollution.remove_micro(
                coord,
                u64::from(config.pollution_absorption_per_tick_milli) * 1000,
            );
            state.absorbed_pollution_micro += absorbed;

            let alive = alive_by_spawner.get(&spawner_id).copied().unwrap_or(0);
            let guards = guards_by_spawner.get(&spawner_id).copied().unwrap_or(0);
            let spawn_cost = u64::from(config.unit_spawn_pollution_cost_milli) * 1000;

            if spawn_cost > 0
                && state.absorbed_pollution_micro >= spawn_cost
                && alive < config.max_alive_units
            {
                state.absorbed_pollution_micro -= spawn_cost;
                requests.push(SpawnRequest {
                    spawner_id,
                    unit: config.unit,
                    mode: EnemyMode::Attack,
                });
            } else if tick >= state.next_free_spawn_tick {
                state.next_free_spawn_tick = tick + u64::from(config.free_spawn_interval_ticks);
                if guards < config.guard_units && alive < config.max_alive_units {
                    requests.push(SpawnRequest {
                        spawner_id,
                        unit: config.unit,
                        mode: EnemyMode::Guard,
                    });
                }
            }
        }

        for request in requests {
            self.spawn_enemy_near_spawner(request.spawner_id, &request.unit, request.mode);
        }
    }

    fn spawn_enemy_near_spawner(
        &mut self,
        spawner_id: EntityId,
        unit: &UnitPrototype,
        mode: EnemyMode,
    ) {
        let Some(placed) = self.entities.placed_entities.get(&spawner_id) else {
            return;
        };
        let footprint = placed.footprint;
        let Some((tile_x, tile_y)) = free_tile_around_footprint(
            &self.world,
            &self.entities.occupancy,
            &footprint,
            SPAWN_SEARCH_RINGS,
        ) else {
            return;
        };

        let id = self.enemies.allocate_id();
        let stagger = id.raw() % 16;
        self.enemies.enemies.insert(
            id,
            Enemy {
                id,
                x: tile_center_fixed(tile_x),
                y: tile_center_fixed(tile_y),
                health: unit.max_health,
                max_health: unit.max_health,
                damage: unit.damage,
                attack_cooldown_ticks: unit.attack_cooldown_ticks,
                speed_fixed_per_tick: unit.speed_fixed_per_tick,
                aggro_radius_tiles: unit.aggro_radius_tiles,
                mode,
                home_spawner: Some(spawner_id),
                target: None,
                path: VecDeque::new(),
                next_attack_tick: 0,
                next_decision_tick: self.tick + stagger,
            },
        );
    }

    /// Unit AI: validate or acquire a target, path toward it, and attack
    /// whatever stands adjacent. Damage is collected first and applied after
    /// the loop so unit order cannot observe half-applied destruction.
    pub(super) fn advance_enemies(&mut self) {
        let mut attacks: Vec<(EntityId, u32)> = Vec::new();
        {
            let Simulation {
                world,
                entities,
                enemies,
                tick,
                ..
            } = self;
            let tick = *tick;
            let seed = world.seed;

            for enemy in enemies.enemies.values_mut() {
                step_enemy(world, entities, seed, tick, enemy, &mut attacks);
            }
        }

        for (entity_id, damage) in attacks {
            self.damage_entity(entity_id, damage);
        }
    }
}

fn step_enemy(
    world: &WorldSim,
    entities: &EntityStore,
    seed: u64,
    tick: u64,
    enemy: &mut Enemy,
    attacks: &mut Vec<(EntityId, u32)>,
) {
    // Drop targets that no longer exist.
    if let Some(target) = enemy.target
        && !entities.placed_entities.contains_key(&target)
    {
        enemy.target = None;
        enemy.path.clear();
    }

    if enemy.target.is_none() && tick >= enemy.next_decision_tick {
        enemy.target = acquire_target(world, entities, enemy);
        enemy.next_decision_tick = tick + ENEMY_TARGET_RESCAN_TICKS + enemy.id.raw() % 16;
        if enemy.target.is_some() {
            enemy.path.clear();
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
    if chebyshev_distance_to_footprint(tile, &target_footprint) <= 1 {
        enemy.path.clear();
        if tick >= enemy.next_attack_tick {
            attacks.push((target, enemy.damage));
            enemy.next_attack_tick = tick + u64::from(enemy.attack_cooldown_ticks);
        }
        return;
    }

    // Recompute the path when it ran out, was invalidated, or grew stale.
    let next_waypoint_blocked = enemy
        .path
        .front()
        .is_some_and(|&(x, y)| !tile_open_for_enemy(world, entities, x, y, Some(target)));
    if (enemy.path.is_empty() || next_waypoint_blocked) && tick >= enemy.next_decision_tick {
        enemy.next_decision_tick = tick + ENEMY_REPATH_INTERVAL_TICKS + enemy.id.raw() % 16;
        enemy.path.clear();
        if chebyshev_distance_to_footprint(tile, &target_footprint)
            <= ENEMY_PATHFIND_MAX_RANGE_TILES
            && let Some(path) = find_path(world, entities, tile, target, &target_footprint)
        {
            enemy.path = path;
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

/// Chooses what a unit fights: guards react to player structures near them,
/// attackers march on the closest structure anywhere in the world.
fn acquire_target(world: &WorldSim, entities: &EntityStore, enemy: &Enemy) -> Option<EntityId> {
    let (tile_x, tile_y) = enemy.tile();
    match enemy.mode {
        EnemyMode::Guard => {
            let radius = i64::from(enemy.aggro_radius_tiles);
            let candidates = entities.occupancy.entity_ids_in_tile_rect(
                tile_x - radius,
                tile_x + radius,
                tile_y - radius,
                tile_y + radius,
            );
            nearest_attackable(world, entities, (tile_x, tile_y), candidates.into_iter())
        }
        EnemyMode::Attack => {
            nearest_attackable_in_expanding_ranges(world, entities, (tile_x, tile_y))
        }
    }
}

/// Nearest player structure among `candidates`; enemy-owned entities are
/// never targets. Ties resolve to the lowest entity id because candidates
/// iterate in ascending id order.
fn nearest_attackable(
    world: &WorldSim,
    entities: &EntityStore,
    from: (WorldTileCoord, WorldTileCoord),
    candidates: impl Iterator<Item = EntityId>,
) -> Option<EntityId> {
    nearest_attackable_with_distance(world, entities, from, candidates)
        .map(|(_, entity_id)| entity_id)
}

fn nearest_attackable_with_distance(
    world: &WorldSim,
    entities: &EntityStore,
    from: (WorldTileCoord, WorldTileCoord),
    candidates: impl Iterator<Item = EntityId>,
) -> Option<(i128, EntityId)> {
    let mut best: Option<(i128, EntityId)> = None;
    for entity_id in candidates {
        let Some(placed) = entities.placed_entities.get(&entity_id) else {
            continue;
        };
        if !is_attackable_kind(world, placed) {
            continue;
        }
        let (center_x, center_y) = footprint_center_tile(&placed.footprint);
        let dx = center_x - from.0;
        let dy = center_y - from.1;
        let distance = i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy);
        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, entity_id));
        }
    }
    best
}

/// Finds the nearest player structure through the occupancy grid, doubling
/// the searched square until it proves no unseen footprint can be nearer.
fn nearest_attackable_in_expanding_ranges(
    world: &WorldSim,
    entities: &EntityStore,
    from: (WorldTileCoord, WorldTileCoord),
) -> Option<EntityId> {
    let mut radius = i64::from(CHUNK_SIZE);
    let mut candidates = BTreeSet::new();

    loop {
        candidates.extend(entities.occupancy.entity_ids_in_tile_rect(
            from.0.saturating_sub(radius),
            from.0.saturating_add(radius),
            from.1.saturating_sub(radius),
            from.1.saturating_add(radius),
        ));
        let best =
            nearest_attackable_with_distance(world, entities, from, candidates.iter().copied());
        if let Some((distance, entity_id)) = best {
            let min_unseen_center_distance = radius.saturating_add(1);
            let unseen_distance_squared =
                i128::from(min_unseen_center_distance) * i128::from(min_unseen_center_distance);
            if distance < unseen_distance_squared {
                return Some(entity_id);
            }
        }

        if radius == i64::MAX {
            return best.map(|(_, entity_id)| entity_id);
        }
        radius = radius.saturating_mul(2);
    }
}

fn is_attackable_kind(world: &WorldSim, placed: &PlacedEntity) -> bool {
    world
        .prototypes
        .entity(placed.prototype_id)
        .is_some_and(|prototype| {
            !matches!(
                prototype.entity_kind,
                EntityKind::EnemySpawner | EntityKind::ResourcePatch
            )
        })
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
                .is_some_and(|placed| is_attackable_kind(world, placed))
        {
            enemy.target = Some(blocker);
            enemy.path.clear();
            return;
        }
    }
}

/// A tile a unit may stand on: generated, walkable terrain, and free of
/// structures other than the unit's own target.
fn tile_open_for_enemy(
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

/// Bounded deterministic A* over 4-connected tiles toward any tile adjacent
/// to the target footprint. Returns waypoints excluding the start tile.
fn find_path(
    world: &WorldSim,
    entities: &EntityStore,
    start: (WorldTileCoord, WorldTileCoord),
    target: EntityId,
    target_footprint: &EntityFootprint,
) -> Option<VecDeque<(WorldTileCoord, WorldTileCoord)>> {
    type Tile = (WorldTileCoord, WorldTileCoord);

    let heuristic = |tile: Tile| -> i64 { manhattan_distance_to_footprint(tile, target_footprint) };

    let mut open: BinaryHeap<Reverse<(i64, i64, Tile)>> = BinaryHeap::new();
    let mut best_g: BTreeMap<Tile, i64> = BTreeMap::new();
    let mut came_from: BTreeMap<Tile, Tile> = BTreeMap::new();

    best_g.insert(start, 0);
    open.push(Reverse((heuristic(start), 0, start)));
    let mut expansions = 0;

    while let Some(Reverse((_, g, tile))) = open.pop() {
        if best_g.get(&tile).copied().is_some_and(|best| g > best) {
            continue;
        }
        if chebyshev_distance_to_footprint(tile, target_footprint) <= 1 {
            let mut path = VecDeque::new();
            let mut current = tile;
            while current != start {
                path.push_front(current);
                current = came_from[&current];
            }
            return Some(path);
        }
        expansions += 1;
        if expansions > ENEMY_PATHFIND_MAX_EXPANSIONS {
            return None;
        }

        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = (tile.0 + dx, tile.1 + dy);
            if (next.0 - start.0).abs() > ENEMY_PATHFIND_MAX_RANGE_TILES
                || (next.1 - start.1).abs() > ENEMY_PATHFIND_MAX_RANGE_TILES
            {
                continue;
            }
            if !tile_open_for_enemy(world, entities, next.0, next.1, Some(target)) {
                continue;
            }
            let next_g = g + 1;
            if best_g.get(&next).copied().is_none_or(|best| next_g < best) {
                best_g.insert(next, next_g);
                came_from.insert(next, tile);
                open.push(Reverse((next_g + heuristic(next), next_g, next)));
            }
        }
    }

    None
}

fn footprint_center_tile(footprint: &EntityFootprint) -> (WorldTileCoord, WorldTileCoord) {
    (
        footprint.x + i64::from(footprint.width) / 2,
        footprint.y + i64::from(footprint.height) / 2,
    )
}

fn axis_distance_to_span(value: i64, span_start: i64, span_len: i32) -> i64 {
    if value < span_start {
        span_start - value
    } else if value >= span_start + i64::from(span_len) {
        value - (span_start + i64::from(span_len) - 1)
    } else {
        0
    }
}

pub(super) fn chebyshev_distance_to_footprint(
    tile: (WorldTileCoord, WorldTileCoord),
    footprint: &EntityFootprint,
) -> i64 {
    let dx = axis_distance_to_span(tile.0, footprint.x, footprint.width);
    let dy = axis_distance_to_span(tile.1, footprint.y, footprint.height);
    dx.max(dy)
}

fn manhattan_distance_to_footprint(
    tile: (WorldTileCoord, WorldTileCoord),
    footprint: &EntityFootprint,
) -> i64 {
    let dx = axis_distance_to_span(tile.0, footprint.x, footprint.width);
    let dy = axis_distance_to_span(tile.1, footprint.y, footprint.height);
    dx + dy
}

/// First free walkable tile in expanding rings around a footprint,
/// deterministic scan order.
fn free_tile_around_footprint(
    world: &WorldSim,
    occupancy: &OccupancyGrid,
    footprint: &EntityFootprint,
    max_rings: i64,
) -> Option<(WorldTileCoord, WorldTileCoord)> {
    for ring in 1..=max_rings {
        let min_x = footprint.x - ring;
        let max_x = footprint.x + i64::from(footprint.width) - 1 + ring;
        let min_y = footprint.y - ring;
        let max_y = footprint.y + i64::from(footprint.height) - 1 + ring;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let on_ring = x == min_x || x == max_x || y == min_y || y == max_y;
                if !on_ring {
                    continue;
                }
                let walkable = world
                    .tile_at(x, y)
                    .is_some_and(|tile| tile.collision.walkable);
                if walkable && occupancy.entity_at(x, y).is_none() {
                    return Some((x, y));
                }
            }
        }
    }
    None
}
