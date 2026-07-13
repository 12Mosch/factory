use super::movement::tile_open_for_enemy;
use super::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

type Tile = (WorldTileCoord, WorldTileCoord);
type OpenNode = Reverse<(i64, i64, Tile)>;

#[derive(Clone, Debug, Default)]
pub(super) struct PathSearchScratch {
    open: BinaryHeap<OpenNode>,
    best_g: Vec<u16>,
    came_from: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PathRequest {
    Ready(Option<VecDeque<(WorldTileCoord, WorldTileCoord)>>),
    Deferred,
}

impl PathSearchScratch {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn find_path(
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
        let heuristic = |tile: (WorldTileCoord, WorldTileCoord)| {
            EntityFootprint::single_tile(tile.0, tile.1).manhattan_distance_to(target_footprint)
        };

        let start_index = index(start).expect("start is centered in path scratch bounds");
        self.best_g[start_index] = 0;
        self.open.push(Reverse((heuristic(start), 0, start)));
        let mut expansions = 0;

        while let Some(Reverse((_, g, tile))) = self.open.pop() {
            let tile_index = index(tile).expect("open path tile must be within scratch bounds");
            if g > i64::from(self.best_g[tile_index]) {
                continue;
            }
            if EntityFootprint::single_tile(tile.0, tile.1).chebyshev_distance_to(target_footprint)
                <= 1
            {
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
