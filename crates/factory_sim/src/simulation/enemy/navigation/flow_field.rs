use super::movement::tile_open_for_enemy;
use super::*;

const RAID_FLOW_RADIUS_TILES: i64 = 96;
const RAID_FLOW_DIAMETER: usize = (RAID_FLOW_RADIUS_TILES as usize) * 2 + 1;
pub(super) const RAID_FLOW_CELL_COUNT: usize = RAID_FLOW_DIAMETER * RAID_FLOW_DIAMETER;

#[derive(Clone, Debug)]
pub(super) struct RaidFlowField {
    target: EntityId,
    target_footprint: EntityFootprint,
    min_x: WorldTileCoord,
    min_y: WorldTileCoord,
    directions: Vec<u8>,
    frontier: VecDeque<(WorldTileCoord, WorldTileCoord)>,
    pub(super) initialized: bool,
}

impl RaidFlowField {
    pub(super) fn pending(target: EntityId, target_footprint: EntityFootprint) -> Self {
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

    pub(super) fn matches(&self, target: EntityId, target_footprint: EntityFootprint) -> bool {
        self.target == target && self.target_footprint == target_footprint
    }

    pub(super) fn initialize(&mut self, world: &WorldSim, entities: &EntityStore) {
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
                if EntityFootprint::single_tile(x, y).chebyshev_distance_to(&footprint) != 1
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

    pub(super) fn expand(
        &mut self,
        world: &WorldSim,
        entities: &EntityStore,
        limit: usize,
    ) -> usize {
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

    pub(super) fn route_from(&self, tile: (WorldTileCoord, WorldTileCoord)) -> RaidRoute {
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
