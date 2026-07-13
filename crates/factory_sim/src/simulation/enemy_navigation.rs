use super::enemy_ops::{
    chebyshev_distance_to_footprint, manhattan_distance_to_footprint, tile_open_for_enemy,
};
use super::*;
use crate::enemies::Raid;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::ops::Bound::{Excluded, Unbounded};

/// Total path-search node expansions permitted in one simulation tick.
/// Raid flow fields consume at most half, reserving capacity for independent
/// unit requests while a field is being built.
const NAVIGATION_EXPANSIONS_PER_TICK: usize = 2_400;
const RAID_FLOW_EXPANSIONS_PER_TICK: usize = NAVIGATION_EXPANSIONS_PER_TICK / 2;
const RAID_FLOW_EXPANSION_QUANTUM: usize = 600;
const RAID_FLOW_RADIUS_TILES: i64 = 96;
const RAID_FLOW_DIAMETER: usize = (RAID_FLOW_RADIUS_TILES as usize) * 2 + 1;
const RAID_FLOW_CELL_COUNT: usize = RAID_FLOW_DIAMETER * RAID_FLOW_DIAMETER;

const CELL_UNVISITED: u8 = 0;
const CELL_GOAL: u8 = 1;
const CELL_EAST: u8 = 2;
const CELL_WEST: u8 = 3;
const CELL_SOUTH: u8 = 4;
const CELL_NORTH: u8 = 5;

type Tile = (WorldTileCoord, WorldTileCoord);
type OpenNode = Reverse<(i64, i64, Tile)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NavigationGridRevision {
    entity_topology: u64,
    world_chunks: u64,
}

#[derive(Clone, Debug, Default)]
struct PathSearchScratch {
    open: BinaryHeap<OpenNode>,
    best_g: Vec<u16>,
    came_from: Vec<u8>,
}

#[derive(Clone, Debug)]
struct RaidFlowField {
    target: EntityId,
    target_footprint: EntityFootprint,
    min_x: WorldTileCoord,
    min_y: WorldTileCoord,
    directions: Vec<u8>,
    frontier: VecDeque<(WorldTileCoord, WorldTileCoord)>,
    initialized: bool,
}

impl RaidFlowField {
    fn pending(target: EntityId, target_footprint: EntityFootprint) -> Self {
        let (center_x, center_y) = footprint_center_tile(&target_footprint);
        Self {
            target,
            target_footprint,
            min_x: center_x.saturating_sub(RAID_FLOW_RADIUS_TILES),
            min_y: center_y.saturating_sub(RAID_FLOW_RADIUS_TILES),
            directions: Vec::new(),
            frontier: VecDeque::new(),
            initialized: false,
        }
    }

    fn matches(&self, target: EntityId, target_footprint: EntityFootprint) -> bool {
        self.target == target && self.target_footprint == target_footprint
    }

    fn initialize(&mut self, world: &WorldSim, entities: &EntityStore) {
        self.directions.clear();
        self.directions.resize(RAID_FLOW_CELL_COUNT, CELL_UNVISITED);
        self.frontier.clear();

        let footprint = self.target_footprint;
        let min_x = footprint.x.saturating_sub(1).max(self.min_x);
        let max_x = footprint
            .x
            .saturating_add(i64::from(footprint.width))
            .min(self.max_x());
        let min_y = footprint.y.saturating_sub(1).max(self.min_y);
        let max_y = footprint
            .y
            .saturating_add(i64::from(footprint.height))
            .min(self.max_y());

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if chebyshev_distance_to_footprint((x, y), &footprint) != 1
                    || !tile_open_for_enemy(world, entities, x, y, None)
                {
                    continue;
                }
                let index = self
                    .index((x, y))
                    .expect("flow-field goal must be within its bounds");
                self.directions[index] = CELL_GOAL;
                self.frontier.push_back((x, y));
            }
        }
        self.initialized = true;
    }

    fn expand(&mut self, world: &WorldSim, entities: &EntityStore, limit: usize) -> usize {
        let mut expansions = 0;
        while expansions < limit {
            let Some(tile) = self.frontier.pop_front() else {
                break;
            };
            expansions += 1;

            for (dx, dy, direction) in [
                (1, 0, CELL_WEST),
                (-1, 0, CELL_EAST),
                (0, 1, CELL_NORTH),
                (0, -1, CELL_SOUTH),
            ] {
                let Some(next_x) = tile.0.checked_add(dx) else {
                    continue;
                };
                let Some(next_y) = tile.1.checked_add(dy) else {
                    continue;
                };
                let next = (next_x, next_y);
                let Some(index) = self.index(next) else {
                    continue;
                };
                if self.directions[index] != CELL_UNVISITED
                    || !tile_open_for_enemy(world, entities, next.0, next.1, None)
                {
                    continue;
                }
                self.directions[index] = direction;
                self.frontier.push_back(next);
            }
        }
        expansions
    }

    fn route_from(&self, tile: (WorldTileCoord, WorldTileCoord)) -> RaidRoute {
        let Some(index) = self.index(tile) else {
            return RaidRoute::OutsideField;
        };
        if !self.initialized || self.directions[index] == CELL_UNVISITED {
            return if self.initialized && self.frontier.is_empty() {
                RaidRoute::Unreachable
            } else {
                RaidRoute::Pending
            };
        }

        let step = match self.directions[index] {
            CELL_EAST => (tile.0.saturating_add(1), tile.1),
            CELL_WEST => (tile.0.saturating_sub(1), tile.1),
            CELL_SOUTH => (tile.0, tile.1.saturating_add(1)),
            CELL_NORTH => (tile.0, tile.1.saturating_sub(1)),
            CELL_GOAL => return RaidRoute::AtGoal,
            _ => unreachable!("flow-field cells contain known direction values"),
        };
        RaidRoute::Step(step)
    }

    fn index(&self, tile: (WorldTileCoord, WorldTileCoord)) -> Option<usize> {
        let x = tile.0.checked_sub(self.min_x)?;
        let y = tile.1.checked_sub(self.min_y)?;
        if x < 0 || y < 0 || x >= RAID_FLOW_DIAMETER as i64 || y >= RAID_FLOW_DIAMETER as i64 {
            return None;
        }
        Some(y as usize * RAID_FLOW_DIAMETER + x as usize)
    }

    fn max_x(&self) -> WorldTileCoord {
        self.min_x.saturating_add(RAID_FLOW_DIAMETER as i64 - 1)
    }

    fn max_y(&self) -> WorldTileCoord {
        self.min_y.saturating_add(RAID_FLOW_DIAMETER as i64 - 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RaidRoute {
    Step((WorldTileCoord, WorldTileCoord)),
    AtGoal,
    Pending,
    OutsideField,
    Unreachable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PathRequest {
    Ready(Option<VecDeque<(WorldTileCoord, WorldTileCoord)>>),
    Deferred,
}

/// Derived navigation state shared by all enemy units. It is omitted from
/// saves and rebuilt deterministically from durable simulation state.
#[derive(Clone, Debug, Default)]
pub(super) struct EnemyNavigation {
    revision: Option<NavigationGridRevision>,
    raid_fields: BTreeMap<RaidId, RaidFlowField>,
    next_raid: Option<RaidId>,
    path_scratch: PathSearchScratch,
    remaining_expansions: usize,
    #[cfg(test)]
    expansions_this_tick: usize,
    #[cfg(test)]
    field_initializations_this_tick: usize,
}

// Navigation is a derived cache, so its transient work queues and retained
// capacities do not participate in durable Simulation equality or hashing.
impl PartialEq for EnemyNavigation {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for EnemyNavigation {}

impl Hash for EnemyNavigation {
    fn hash<H: Hasher>(&self, _state: &mut H) {}
}

impl EnemyNavigation {
    pub(super) fn begin_tick(&mut self, entity_topology: u64, world_chunks: u64) {
        let revision = NavigationGridRevision {
            entity_topology,
            world_chunks,
        };
        if self.revision != Some(revision) {
            self.raid_fields.clear();
            self.next_raid = None;
            self.revision = Some(revision);
        }
        self.remaining_expansions = NAVIGATION_EXPANSIONS_PER_TICK;
        #[cfg(test)]
        {
            self.expansions_this_tick = 0;
            self.field_initializations_this_tick = 0;
        }
    }

    pub(super) fn sync_raid(
        &mut self,
        raid_id: RaidId,
        target: EntityId,
        target_footprint: EntityFootprint,
    ) {
        let field = self
            .raid_fields
            .entry(raid_id)
            .or_insert_with(|| RaidFlowField::pending(target, target_footprint));
        if !field.matches(target, target_footprint) {
            *field = RaidFlowField::pending(target, target_footprint);
        }
    }

    pub(super) fn retain_raids(&mut self, raids: &BTreeMap<RaidId, Raid>) {
        self.raid_fields
            .retain(|raid_id, _| raids.contains_key(raid_id));
        if self
            .next_raid
            .is_some_and(|raid_id| !self.raid_fields.contains_key(&raid_id))
        {
            self.next_raid = None;
        }
    }

    pub(super) fn advance_raid_fields(&mut self, world: &WorldSim, entities: &EntityStore) {
        let mut allowance = self.remaining_expansions.min(RAID_FLOW_EXPANSIONS_PER_TICK);
        let mut visited = 0;
        let field_count = self.raid_fields.len();
        let mut initialized_field = false;

        while allowance > 0 && visited < field_count {
            let next = self
                .next_raid
                .and_then(|cursor| {
                    self.raid_fields
                        .range((Excluded(cursor), Unbounded))
                        .next()
                        .map(|(&raid_id, _)| raid_id)
                })
                .or_else(|| self.raid_fields.keys().next().copied());
            let Some(raid_id) = next else {
                break;
            };
            self.next_raid = Some(raid_id);
            visited += 1;

            let field = self
                .raid_fields
                .get_mut(&raid_id)
                .expect("selected raid field must still exist");
            if !field.initialized {
                if initialized_field {
                    continue;
                }
                field.initialize(world, entities);
                initialized_field = true;
                #[cfg(test)]
                {
                    self.field_initializations_this_tick += 1;
                }
            }
            let used = field.expand(world, entities, allowance.min(RAID_FLOW_EXPANSION_QUANTUM));
            allowance -= used;
            self.charge(used);
        }
    }

    pub(super) fn raid_route(
        &self,
        raid_id: RaidId,
        from: (WorldTileCoord, WorldTileCoord),
    ) -> RaidRoute {
        self.raid_fields
            .get(&raid_id)
            .map_or(RaidRoute::Pending, |field| field.route_from(from))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn request_path(
        &mut self,
        world: &WorldSim,
        entities: &EntityStore,
        start: (WorldTileCoord, WorldTileCoord),
        target: EntityId,
        target_footprint: &EntityFootprint,
        max_range: i64,
        max_expansions: usize,
    ) -> PathRequest {
        if self.remaining_expansions < max_expansions {
            return PathRequest::Deferred;
        }

        let (path, expansions) = self.path_scratch.find_path(
            world,
            entities,
            start,
            target,
            target_footprint,
            max_range,
            max_expansions,
        );
        self.charge(expansions);
        PathRequest::Ready(path)
    }

    fn charge(&mut self, expansions: usize) {
        self.remaining_expansions = self.remaining_expansions.saturating_sub(expansions);
        #[cfg(test)]
        {
            self.expansions_this_tick += expansions;
        }
    }
}

impl PathSearchScratch {
    #[allow(clippy::too_many_arguments)]
    fn find_path(
        &mut self,
        world: &WorldSim,
        entities: &EntityStore,
        start: (WorldTileCoord, WorldTileCoord),
        target: EntityId,
        target_footprint: &EntityFootprint,
        max_range: i64,
        max_expansions: usize,
    ) -> (Option<VecDeque<(WorldTileCoord, WorldTileCoord)>>, usize) {
        let diameter = (max_range as usize) * 2 + 1;
        let cell_count = diameter * diameter;
        self.open.clear();
        self.best_g.clear();
        self.best_g.resize(cell_count, u16::MAX);
        self.came_from.clear();
        self.came_from.resize(cell_count, CELL_UNVISITED);

        let min_x = start.0.saturating_sub(max_range);
        let min_y = start.1.saturating_sub(max_range);
        let index = |tile: (WorldTileCoord, WorldTileCoord)| -> Option<usize> {
            let x = tile.0.checked_sub(min_x)?;
            let y = tile.1.checked_sub(min_y)?;
            if x < 0 || y < 0 || x >= diameter as i64 || y >= diameter as i64 {
                return None;
            }
            Some(y as usize * diameter + x as usize)
        };
        let heuristic = |tile| manhattan_distance_to_footprint(tile, target_footprint);

        let start_index = index(start).expect("start is centered in path scratch bounds");
        self.best_g[start_index] = 0;
        self.open.push(Reverse((heuristic(start), 0, start)));
        let mut expansions = 0;

        while let Some(Reverse((_, g, tile))) = self.open.pop() {
            let tile_index = index(tile).expect("open path tile must be within scratch bounds");
            if g > i64::from(self.best_g[tile_index]) {
                continue;
            }
            if chebyshev_distance_to_footprint(tile, target_footprint) <= 1 {
                let mut path = VecDeque::new();
                let mut current = tile;
                while current != start {
                    path.push_front(current);
                    let current_index = index(current)
                        .expect("path reconstruction tile must be within scratch bounds");
                    current = apply_direction(current, self.came_from[current_index]);
                }
                return (Some(path), expansions);
            }
            if expansions == max_expansions {
                return (None, expansions);
            }
            expansions += 1;

            for (dx, dy, return_direction) in [
                (1, 0, CELL_WEST),
                (-1, 0, CELL_EAST),
                (0, 1, CELL_NORTH),
                (0, -1, CELL_SOUTH),
            ] {
                let Some(next_x) = tile.0.checked_add(dx) else {
                    continue;
                };
                let Some(next_y) = tile.1.checked_add(dy) else {
                    continue;
                };
                let next = (next_x, next_y);
                let Some(next_index) = index(next) else {
                    continue;
                };
                if !tile_open_for_enemy(world, entities, next.0, next.1, Some(target)) {
                    continue;
                }
                let next_g = g + 1;
                if next_g < i64::from(self.best_g[next_index]) {
                    self.best_g[next_index] = next_g as u16;
                    self.came_from[next_index] = return_direction;
                    self.open
                        .push(Reverse((next_g + heuristic(next), next_g, next)));
                }
            }
        }

        (None, expansions)
    }
}

fn apply_direction(
    tile: (WorldTileCoord, WorldTileCoord),
    direction: u8,
) -> (WorldTileCoord, WorldTileCoord) {
    match direction {
        CELL_EAST => (tile.0.saturating_add(1), tile.1),
        CELL_WEST => (tile.0.saturating_sub(1), tile.1),
        CELL_SOUTH => (tile.0, tile.1.saturating_add(1)),
        CELL_NORTH => (tile.0, tile.1.saturating_sub(1)),
        _ => unreachable!("A* path cells always have a return direction"),
    }
}

fn footprint_center_tile(footprint: &EntityFootprint) -> (WorldTileCoord, WorldTileCoord) {
    (
        footprint.x + i64::from(footprint.width) / 2,
        footprint.y + i64::from(footprint.height) / 2,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_footprint(x: i64, y: i64) -> EntityFootprint {
        EntityFootprint {
            x,
            y,
            width: 1,
            height: 1,
        }
    }

    #[test]
    fn flow_field_bounds_navigation_work_per_tick() {
        let sim = Simulation::new_test_world(7);
        let target = EntityId::new(1);
        let footprint = test_footprint(0, 0);
        let mut navigation = EnemyNavigation::default();

        navigation.begin_tick(0, 0);
        for raid in 1..=20 {
            navigation.sync_raid(RaidId::new(raid), target, footprint);
        }
        navigation.advance_raid_fields(&sim.world, &sim.entities);

        assert!(navigation.expansions_this_tick <= NAVIGATION_EXPANSIONS_PER_TICK);
        assert_eq!(navigation.field_initializations_this_tick, 1);
        assert_eq!(navigation.raid_fields.len(), 20);
    }

    #[test]
    fn navigation_revision_discards_stale_raid_fields() {
        let sim = Simulation::new_test_world(11);
        let target = EntityId::new(1);
        let footprint = test_footprint(0, 0);
        let mut navigation = EnemyNavigation::default();
        let raid_id = RaidId::new(1);

        navigation.begin_tick(3, 5);
        navigation.sync_raid(raid_id, target, footprint);
        navigation.advance_raid_fields(&sim.world, &sim.entities);
        assert!(navigation.raid_fields[&raid_id].initialized);

        navigation.begin_tick(4, 5);
        assert!(navigation.raid_fields.is_empty());
    }

    #[test]
    fn flow_field_routes_every_member_around_the_same_barrier() {
        let mut sim = Simulation::new_test_world(19);
        let blocker = EntityId::new(99);
        for y in -3..=3 {
            sim.entities
                .occupancy
                .occupied_tiles
                .insert((0, y), blocker);
        }

        let footprint = test_footprint(5, 0);
        let mut field = RaidFlowField::pending(EntityId::new(1), footprint);
        field.initialize(&sim.world, &sim.entities);
        field.expand(&sim.world, &sim.entities, RAID_FLOW_CELL_COUNT);

        let start = (-5, 0);
        assert_eq!(field.route_from(start), field.route_from(start));
        let mut current = start;
        let mut steps = 0;
        while chebyshev_distance_to_footprint(current, &footprint) > 1 {
            let RaidRoute::Step(next) = field.route_from(current) else {
                panic!("the shared field should route around the barrier");
            };
            assert!(!sim.entities.occupancy.occupied_tiles.contains_key(&next));
            current = next;
            steps += 1;
            assert!(steps < 100, "route should not loop");
        }

        assert!(steps > 9, "the route must detour around the barrier");
    }

    #[test]
    fn independent_requests_defer_before_exceeding_the_tick_budget() {
        let sim = Simulation::new_test_world(23);
        let mut navigation = EnemyNavigation::default();
        navigation.begin_tick(0, 0);
        let footprint = test_footprint(39, 0);
        let mut completed = 0;

        while let PathRequest::Ready(_) = navigation.request_path(
            &sim.world,
            &sim.entities,
            (0, 0),
            EntityId::new(1),
            &footprint,
            40,
            600,
        ) {
            completed += 1;
        }

        assert!(completed > 0);
        assert!(navigation.expansions_this_tick <= NAVIGATION_EXPANSIONS_PER_TICK);
        assert_eq!(
            navigation.request_path(
                &sim.world,
                &sim.entities,
                (0, 0),
                EntityId::new(1),
                &footprint,
                40,
                600,
            ),
            PathRequest::Deferred
        );
    }
}
