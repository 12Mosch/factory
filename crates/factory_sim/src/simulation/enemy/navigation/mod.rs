use super::*;
use crate::enemies::Raid;
use std::ops::Bound::{Excluded, Unbounded};

mod flow_field;
mod movement;
mod pathfinding;

#[cfg(test)]
use flow_field::RAID_FLOW_CELL_COUNT;
use flow_field::{RaidFlowField, RaidRoute};
use pathfinding::{PathRequest, PathSearchScratch};

/// Total path-search node expansions permitted in one simulation tick.
/// Raid flow fields consume at most half, reserving capacity for independent
/// unit requests while a field is being built.
const NAVIGATION_EXPANSIONS_PER_TICK: usize = 2_400;
const RAID_FLOW_EXPANSIONS_PER_TICK: usize = NAVIGATION_EXPANSIONS_PER_TICK / 2;
const RAID_FLOW_EXPANSION_QUANTUM: usize = 600;

const CELL_UNVISITED: u8 = 0;
const CELL_GOAL: u8 = 1;
const CELL_EAST: u8 = 2;
const CELL_WEST: u8 = 3;
const CELL_SOUTH: u8 = 4;
const CELL_NORTH: u8 = 5;

fn footprint_center_tile(footprint: &EntityFootprint) -> (WorldTileCoord, WorldTileCoord) {
    (
        footprint.x + i64::from(footprint.width) / 2,
        footprint.y + i64::from(footprint.height) / 2,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
struct NavigationGridRevision {
    entity_topology: u64,
    world_chunks: u64,
    /// Landfill rewrites walkability inside already-generated chunks, which
    /// the chunk revision alone does not observe. Keyed on walkability rather
    /// than terrain so paving a large area, which never changes collision,
    /// does not discard every flow field.
    world_walkability: u64,
}

/// Durable incremental navigation work shared by enemy units. Rebuilding it
/// consumes tick budgets, so fields and scheduling decisions must survive saves.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(in crate::simulation) struct EnemyNavigation {
    revision: Option<NavigationGridRevision>,
    raid_fields: BTreeMap<RaidId, RaidFlowField>,
    next_raid: Option<RaidId>,
    #[serde(skip)]
    path_scratch: PathSearchScratch,
    #[serde(skip)]
    remaining_expansions: usize,
    #[cfg(test)]
    #[serde(skip)]
    expansions_this_tick: usize,
    #[cfg(test)]
    #[serde(skip)]
    field_initializations_this_tick: usize,
}

// Scratch and the remaining budget reset at begin_tick; only work carried
// across ticks participates in equality and deterministic hashing.
impl PartialEq for EnemyNavigation {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision
            && self.raid_fields == other.raid_fields
            && self.next_raid == other.next_raid
    }
}
impl Eq for EnemyNavigation {}
impl Hash for EnemyNavigation {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.revision.hash(state);
        self.raid_fields.hash(state);
        self.next_raid.hash(state);
    }
}

impl EnemyNavigation {
    /// Copies incremental navigation work while resetting per-tick scratch.
    pub(in crate::simulation) fn clone_for_save(&self) -> Self {
        Self {
            revision: self.revision,
            raid_fields: self.raid_fields.clone(),
            next_raid: self.next_raid,
            path_scratch: PathSearchScratch::default(),
            remaining_expansions: 0,
            #[cfg(test)]
            expansions_this_tick: 0,
            #[cfg(test)]
            field_initializations_this_tick: 0,
        }
    }

    /// Validates durable revisions, scheduling cursor, and every raid field.
    pub(in crate::simulation) fn validate(
        &self,
        world: &WorldSim,
    ) -> Result<(), SimValidationError> {
        if (self.revision.is_none() && (!self.raid_fields.is_empty() || self.next_raid.is_some()))
            || self
                .next_raid
                .is_some_and(|id| !self.raid_fields.contains_key(&id))
            || self
                .raid_fields
                .values()
                .any(|field| !field.is_valid(world))
        {
            return Err(SimValidationError::InvalidEnemyNavigation);
        }
        Ok(())
    }

    pub(super) fn begin_tick(
        &mut self,
        entity_topology: u64,
        world_chunks: u64,
        world_walkability: u64,
    ) {
        let revision = NavigationGridRevision {
            entity_topology,
            world_chunks,
            world_walkability,
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

    fn raid_route(&self, raid_id: RaidId, from: (WorldTileCoord, WorldTileCoord)) -> RaidRoute {
        self.raid_fields
            .get(&raid_id)
            .map_or(RaidRoute::Pending, |field| field.route_from(from))
    }

    #[allow(clippy::too_many_arguments)]
    /// Budgeted tile-goal routing shared by expansion and wander movement, so
    /// both reuse the same A* scratch buffers and per-tick expansion budget
    /// as combat pathing instead of maintaining separate search machinery.
    fn request_tile_path(
        &mut self,
        world: &WorldSim,
        entities: &EntityStore,
        start: (WorldTileCoord, WorldTileCoord),
        goal: (WorldTileCoord, WorldTileCoord),
        max_range: i64,
        max_expansions: usize,
    ) -> PathRequest {
        if self.remaining_expansions < max_expansions {
            return PathRequest::Deferred;
        }

        let (path, expansions) = self.path_scratch.find_path_to_tile(
            world,
            entities,
            start,
            goal,
            max_range,
            max_expansions,
        );
        self.charge(expansions);
        PathRequest::Ready(path)
    }

    #[allow(clippy::too_many_arguments)]
    /// Budgeted routing to the reachable forward edge of a search window.
    /// The request reserves enough work to visit the complete window, making
    /// `Ready(None)` a proven local disconnection rather than an exhausted search.
    fn request_frontier_path(
        &mut self,
        world: &WorldSim,
        entities: &EntityStore,
        start: (WorldTileCoord, WorldTileCoord),
        target: (WorldTileCoord, WorldTileCoord),
        max_range: i64,
    ) -> PathRequest {
        let diameter = (max_range as usize) * 2 + 1;
        let max_expansions = diameter * diameter;
        if self.remaining_expansions < max_expansions {
            return PathRequest::Deferred;
        }

        let (path, expansions) = self
            .path_scratch
            .find_path_to_frontier(world, entities, start, target, max_range);
        self.charge(expansions);
        PathRequest::Ready(path)
    }

    #[allow(clippy::too_many_arguments)]
    fn request_path(
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds several simultaneous raids sharing one durable structure target.
    fn multiple_raid_world(count: usize) -> Simulation {
        let mut sim = Simulation::new_test_world(293);
        let chest = sim
            .world
            .prototypes
            .entities()
            .iter()
            .find(|prototype| prototype.name == "chest")
            .unwrap()
            .id;
        let (x, y) = sim
            .world
            .chunks
            .values()
            .flat_map(|chunk| {
                chunk
                    .tiles
                    .iter()
                    .enumerate()
                    .filter(|(_, tile)| tile.collision.buildable && tile.resource.is_none())
                    .map(|(index, _)| {
                        chunk
                            .coord
                            .tile_at(index as i32 % CHUNK_SIZE, index as i32 / CHUNK_SIZE)
                    })
            })
            .find(|(x, y)| sim.entities.occupancy.entity_at(*x, *y).is_none())
            .unwrap();
        let target = crate::placement::place(
            &mut sim,
            crate::placement::EntityPlacementRequest {
                prototype_id: chest,
                x,
                y,
                direction: Direction::North,
            },
        )
        .unwrap();
        for index in 0..count {
            let offset = 12 + (index as i64 % 8) * 2;
            let member =
                crate::simulation::tests::combat::spawn_test_enemy_at(&mut sim, x + offset, y);
            let id = sim.enemies.allocate_raid_id();
            sim.enemies.enemies.get_mut(&member).unwrap().mission = EnemyMission::Raid(id);
            sim.enemies.raids.insert(
                id,
                Raid {
                    id,
                    base_id: EnemyBaseId::new(1),
                    members: BTreeSet::from([member]),
                    target: Some(target),
                    launched_tick: 0,
                },
            );
        }
        sim
    }

    /// Partially initialized fields must preserve their exact budgeted progress.
    #[test]
    fn partially_built_multiple_raid_fields_continue_after_save() {
        let mut sim = multiple_raid_world(2);
        sim.tick();
        assert_eq!(sim.enemy_navigation.raid_fields.len(), 2);
        assert_eq!(
            sim.enemy_navigation
                .raid_fields
                .values()
                .filter(|f| f.initialized)
                .count(),
            1
        );
        assert_eq!(sim.enemy_navigation.field_initializations_this_tick, 1);
        super::super::super::save::assert_save_continuation(&mut sim, 40, &[]);
    }

    /// Warm fields must retain their routes rather than rebuild after loading.
    #[test]
    fn warm_multiple_raid_fields_continue_after_save() {
        let mut sim = multiple_raid_world(2);
        for _ in 0..6 {
            sim.tick();
        }
        assert_eq!(sim.enemy_navigation.raid_fields.len(), 2);
        assert!(
            sim.enemy_navigation
                .raid_fields
                .values()
                .all(|f| f.initialized)
        );
        super::super::super::save::assert_save_continuation(&mut sim, 40, &[]);
    }

    /// Exhausted per-tick work and round-robin ordering resume after loading.
    #[test]
    fn exhausted_navigation_budget_and_cursor_survive_save() {
        let mut sim = multiple_raid_world(20);
        sim.tick();
        sim.tick();
        assert_eq!(
            sim.enemy_navigation.expansions_this_tick,
            RAID_FLOW_EXPANSIONS_PER_TICK
        );
        assert!(
            sim.enemy_navigation
                .raid_fields
                .values()
                .any(|field| !field.initialized)
        );
        super::super::super::save::assert_save_continuation(&mut sim, 20, &[]);
    }

    /// A revision change after the enemy pass remains pending through a save.
    #[test]
    fn pending_navigation_invalidation_survives_save() {
        let mut sim = multiple_raid_world(2);
        for _ in 0..6 {
            sim.tick();
        }
        // Chunk generation after the enemy pass leaves old fields pending an
        // invalidation. Loading must neither acknowledge it nor lose it.
        sim.ensure_chunk_generated(ChunkCoord { x: 50, y: 50 });
        sim.request_chunk_generation(
            ChunkCoord { x: 51, y: 50 },
            ChunkGenerationPriority::Required,
        );
        let commands = [
            (
                2,
                SimCommand::MovePlayer {
                    direction_x: 1.0,
                    direction_y: 0.0,
                    delta_seconds: 1.0 / 60.0,
                },
            ),
            (
                2,
                SimCommand::MovePlayer {
                    direction_x: 0.0,
                    direction_y: 1.0,
                    delta_seconds: 1.0 / 60.0,
                },
            ),
        ];
        super::super::super::save::assert_save_continuation(&mut sim, 20, &commands);
    }

    /// Malformed direction storage is rejected before route reconstruction.
    #[test]
    fn malformed_navigation_is_rejected_before_reconstruction() {
        let mut sim = multiple_raid_world(2);
        sim.tick();
        // An initialized field with no cells used to reach indexed movement.
        sim.enemy_navigation
            .raid_fields
            .values_mut()
            .find(|f| f.initialized)
            .unwrap()
            .corrupt_directions_for_test();
        assert!(matches!(
            load_from_bytes(&save_to_bytes(&sim).unwrap()),
            Err(SaveLoadError::InvalidSimulationState(
                SimValidationError::InvalidEnemyNavigation
            ))
        ));
    }

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

        navigation.begin_tick(0, 0, 0);
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

        navigation.begin_tick(3, 5, 0);
        navigation.sync_raid(raid_id, target, footprint);
        navigation.advance_raid_fields(&sim.world, &sim.entities);
        assert!(navigation.raid_fields[&raid_id].initialized);

        navigation.begin_tick(4, 5, 0);
        assert!(navigation.raid_fields.is_empty());
    }

    #[test]
    fn navigation_revision_discards_fields_when_walkability_changes() {
        let sim = Simulation::new_test_world(11);
        let target = EntityId::new(1);
        let footprint = test_footprint(0, 0);
        let mut navigation = EnemyNavigation::default();
        let raid_id = RaidId::new(1);

        navigation.begin_tick(3, 5, 7);
        navigation.sync_raid(raid_id, target, footprint);
        navigation.advance_raid_fields(&sim.world, &sim.entities);
        assert!(navigation.raid_fields[&raid_id].initialized);

        // Landfill changes walkability inside chunks that already exist, so
        // the flow fields must be rebuilt even though nothing else moved.
        navigation.begin_tick(3, 5, 8);
        assert!(navigation.raid_fields.is_empty());
    }

    #[test]
    fn paving_terrain_does_not_discard_navigation_fields() {
        let mut sim = Simulation::new_test_world(11);
        let concrete = factory_data::BasePrototypeIds::from_catalog(&sim.world.prototypes)
            .tiles
            .concrete;
        let target = EntityId::new(1);
        let footprint = test_footprint(0, 0);
        let mut navigation = EnemyNavigation::default();
        let raid_id = RaidId::new(1);

        navigation.begin_tick(3, 5, sim.world.walkability_revision());
        navigation.sync_raid(raid_id, target, footprint);
        navigation.advance_raid_fields(&sim.world, &sim.entities);
        assert!(navigation.raid_fields[&raid_id].initialized);

        // Paving walkable ground only changes appearance and walking speed, so
        // bulk paving must not churn every raid's flow field.
        let paved = sim
            .world
            .chunks
            .values()
            .flat_map(|chunk| {
                chunk.tiles.iter().enumerate().filter_map(|(index, tile)| {
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    (tile.collision.walkable
                        && tile.collision.buildable
                        && tile.resource.is_none()
                        && tile.tile_id != concrete)
                        .then(|| chunk.coord.tile_at(local_x, local_y))
                })
            })
            .take(32)
            .collect::<Vec<_>>();
        assert!(!paved.is_empty(), "test world should contain plain ground");
        let before_walkability = sim.world.walkability_revision();
        for (x, y) in paved {
            sim.world
                .set_tile(x, y, concrete)
                .expect("plain ground should accept concrete");
        }

        assert_eq!(sim.world.walkability_revision(), before_walkability);
        assert!(sim.world.terrain_revision() > 0, "paving is still recorded");
        navigation.begin_tick(3, 5, sim.world.walkability_revision());
        assert!(navigation.raid_fields.contains_key(&raid_id));
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
        while EntityFootprint::single_tile(current.0, current.1).chebyshev_distance_to(&footprint)
            > 1
        {
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
        navigation.begin_tick(0, 0, 0);
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
