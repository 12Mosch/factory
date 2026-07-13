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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NavigationGridRevision {
    entity_topology: u64,
    world_chunks: u64,
}

/// Derived navigation state shared by all enemy units. It is omitted from
/// saves and rebuilt deterministically from durable simulation state.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct EnemyNavigation {
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

    fn raid_route(&self, raid_id: RaidId, from: (WorldTileCoord, WorldTileCoord)) -> RaidRoute {
        self.raid_fields
            .get(&raid_id)
            .map_or(RaidRoute::Pending, |field| field.route_from(from))
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
