use super::*;

impl WorldSim {
    const RESOURCE_DIRTY_HISTORY_LIMIT: usize = 4096;

    pub fn new(seed: u64, prototypes: PrototypeCatalog) -> Self {
        let chunks = generate_world_chunks(seed, &prototypes);
        Self {
            seed,
            prototypes,
            chunks,
            chunk_revision: 0,
            resource_revision: 0,
            resource_dirty_tiles: VecDeque::new(),
        }
    }

    pub fn new_seeded(seed: u64) -> Self {
        Self::new(
            seed,
            PrototypeCatalog::load_base().expect("base prototype catalog should load"),
        )
    }

    pub fn tile_at(&self, x: i32, y: i32) -> Option<&TileCell> {
        let (coord, index) = chunk_coord_and_tile_index(x, y);

        self.chunks
            .get(&coord)
            .and_then(|chunk| chunk.tiles.get(index))
    }

    pub fn ensure_chunk_generated(&mut self, coord: ChunkCoord) -> bool {
        if self.chunks.contains_key(&coord) {
            return false;
        }

        let ids = WorldPrototypeIds::from_catalog(&self.prototypes);
        let chunk = generate_chunk(self.seed, coord, ids);
        self.chunks.insert(coord, chunk);
        self.chunk_revision = self.chunk_revision.wrapping_add(1);
        true
    }

    pub fn ensure_chunks_around_chunk(&mut self, center: ChunkCoord, radius: i32) -> usize {
        let radius = radius.max(0);
        let mut generated = 0;

        for y in center.y - radius..=center.y + radius {
            for x in center.x - radius..=center.x + radius {
                if self.ensure_chunk_generated(ChunkCoord { x, y }) {
                    generated += 1;
                }
            }
        }

        generated
    }

    pub fn chunk_revision(&self) -> u64 {
        self.chunk_revision
    }

    pub fn generated_chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub(super) fn tile_at_profiled<P: TickProfiler>(
        &self,
        x: i32,
        y: i32,
        profiler: &mut P,
    ) -> Option<&TileCell> {
        profiler.measure(ProfilePhase::ChunkLookup, || self.tile_at(x, y))
    }

    pub fn resource_revision(&self) -> u64 {
        self.resource_revision
    }

    pub fn resource_dirty_tiles_since(
        &self,
        revision: u64,
    ) -> Option<impl Iterator<Item = ResourceTileChange> + '_> {
        if revision > self.resource_revision {
            return None;
        }

        if revision < self.resource_revision
            && self
                .resource_dirty_tiles
                .front()
                .is_none_or(|change| change.revision > revision.saturating_add(1))
        {
            return None;
        }

        Some(
            self.resource_dirty_tiles
                .iter()
                .copied()
                .filter(move |change| change.revision > revision),
        )
    }

    pub fn mine_resource_at(&mut self, x: i32, y: i32, amount: u32) -> Option<MinedResource> {
        if amount == 0 {
            return None;
        }

        let ids = WorldPrototypeIds::from_catalog(&self.prototypes);
        let (mined, resource) = {
            let tile = self.tile_at_mut(x, y)?;
            let mined = mine_resource_from_tile(tile, ids, amount)?;
            (mined, tile.resource)
        };
        self.record_resource_tile_change(x, y, resource);

        Some(mined)
    }

    pub(super) fn mine_resource_at_profiled<P: TickProfiler>(
        &mut self,
        x: i32,
        y: i32,
        amount: u32,
        profiler: &mut P,
    ) -> Option<MinedResource> {
        if amount == 0 {
            return None;
        }

        let ids = WorldPrototypeIds::from_catalog(&self.prototypes);
        let (mined, resource) = {
            let tile = profiler.measure(ProfilePhase::ChunkLookup, || self.tile_at_mut(x, y))?;
            let mined = mine_resource_from_tile(tile, ids, amount)?;
            (mined, tile.resource)
        };
        self.record_resource_tile_change(x, y, resource);

        Some(mined)
    }

    pub fn can_build_on_tile(&self, x: i32, y: i32) -> Result<(), BuildError> {
        let tile = self
            .tile_at(x, y)
            .ok_or(BuildError::OutsideGeneratedChunks { x, y })?;

        if tile.collision.buildable {
            Ok(())
        } else {
            Err(BuildError::TileBlocked { x, y })
        }
    }

    pub fn entity_footprint(
        &self,
        prototype_id: EntityPrototypeId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> Result<EntityFootprint, BuildError> {
        let prototype = self
            .prototypes
            .entities
            .get(prototype_id.index())
            .filter(|prototype| prototype.id == prototype_id)
            .ok_or(BuildError::MissingPrototype(prototype_id))?;

        Ok(EntityFootprint::from_size(
            x,
            y,
            prototype.size.x,
            prototype.size.y,
            direction,
        ))
    }

    pub fn validate_entity_footprint(&self, footprint: &EntityFootprint) -> Result<(), BuildError> {
        footprint.validate()?;

        for (x, y) in footprint.tiles() {
            self.can_build_on_tile(x, y)?;
        }

        Ok(())
    }

    pub(super) fn validate_entity_footprint_for_prototype(
        &self,
        prototype: &factory_data::EntityPrototype,
        footprint: &EntityFootprint,
        direction: Direction,
    ) -> Result<(), BuildError> {
        if prototype.entity_kind == EntityKind::OffshorePump && prototype.offshore_pump.is_some() {
            self.validate_entity_footprint(footprint)?;
            if offshore_pump_water_tiles(footprint, direction)
                .into_iter()
                .any(|(x, y)| self.tile_at(x, y).is_some_and(is_water_like_tile))
            {
                return Ok(());
            }

            return Err(BuildError::TileBlocked {
                x: footprint.x,
                y: footprint.y,
            });
        }

        if prototype.entity_kind != EntityKind::MiningDrill || prototype.mining_drill.is_none() {
            return self.validate_entity_footprint(footprint);
        }

        footprint.validate()?;
        for (x, y) in footprint.tiles() {
            let tile = self
                .tile_at(x, y)
                .ok_or(BuildError::OutsideGeneratedChunks { x, y })?;
            if !tile.collision.walkable {
                return Err(BuildError::TileBlocked { x, y });
            }
        }

        let mining_drill = prototype
            .mining_drill
            .as_ref()
            .expect("mining drill prototype should have mining metadata");
        if first_resource_in_mining_area(self, footprint, mining_drill).is_none() {
            return Err(BuildError::TileBlocked {
                x: footprint.x,
                y: footprint.y,
            });
        }

        Ok(())
    }

    pub(super) fn tile_at_mut(&mut self, x: i32, y: i32) -> Option<&mut TileCell> {
        let (coord, index) = chunk_coord_and_tile_index(x, y);

        self.chunks
            .get_mut(&coord)
            .and_then(|chunk| chunk.tiles.get_mut(index))
    }

    fn record_resource_tile_change(&mut self, x: i32, y: i32, resource: Option<ResourceCell>) {
        self.resource_revision = self.resource_revision.wrapping_add(1);
        self.resource_dirty_tiles.push_back(ResourceTileChange {
            revision: self.resource_revision,
            x,
            y,
            resource,
        });

        while self.resource_dirty_tiles.len() > Self::RESOURCE_DIRTY_HISTORY_LIMIT {
            self.resource_dirty_tiles.pop_front();
        }
    }
}

pub(super) fn is_water_like_tile(tile: &TileCell) -> bool {
    !tile.collision.walkable && !tile.collision.buildable
}

pub(super) fn offshore_pump_water_tiles(
    footprint: &EntityFootprint,
    direction: Direction,
) -> Vec<(i32, i32)> {
    match direction {
        Direction::North => (footprint.x..footprint.x + footprint.width)
            .map(|x| (x, footprint.y - 1))
            .collect(),
        Direction::East => (footprint.y..footprint.y + footprint.height)
            .map(|y| (footprint.x + footprint.width, y))
            .collect(),
        Direction::South => (footprint.x..footprint.x + footprint.width)
            .map(|x| (x, footprint.y + footprint.height))
            .collect(),
        Direction::West => (footprint.y..footprint.y + footprint.height)
            .map(|y| (footprint.x - 1, y))
            .collect(),
    }
}

fn mine_resource_from_tile(
    tile: &mut TileCell,
    ids: WorldPrototypeIds,
    amount: u32,
) -> Option<MinedResource> {
    let resource = tile.resource.as_mut()?;
    let mined_amount = amount.min(resource.amount);
    let mined = MinedResource {
        resource_item: resource.resource_item,
        amount: mined_amount,
    };

    resource.amount -= mined_amount;
    if resource.amount == 0 {
        tile.resource = None;
        tile.collision = collision_for_tile(tile.tile_id, ids);
    }

    Some(mined)
}

pub(super) fn chunk_coord_and_tile_index(x: i32, y: i32) -> (ChunkCoord, usize) {
    let coord = ChunkCoord {
        x: x.div_euclid(CHUNK_SIZE),
        y: y.div_euclid(CHUNK_SIZE),
    };
    let local_x = x.rem_euclid(CHUNK_SIZE) as usize;
    let local_y = y.rem_euclid(CHUNK_SIZE) as usize;
    let index = local_y * CHUNK_SIZE as usize + local_x;

    (coord, index)
}

pub(super) fn find_player_start(
    world: &WorldSim,
    occupancy: &OccupancyGrid,
) -> Option<PlayerState> {
    let (min_x, max_x, min_y, max_y) = world_tile_bounds(world)?;
    let max_radius = min_x
        .abs()
        .max(max_x.abs())
        .max(min_y.abs())
        .max(max_y.abs());

    for radius in 0..=max_radius {
        for y in -radius..=radius {
            if y < min_y || y > max_y {
                continue;
            }

            for x in -radius..=radius {
                if x < min_x || x > max_x {
                    continue;
                }

                if x.abs().max(y.abs()) != radius {
                    continue;
                }

                if player_can_occupy_tile(world, occupancy, x, y) {
                    return Some(PlayerState::centered_on_tile(x, y));
                }
            }
        }
    }

    None
}

pub(super) fn world_tile_bounds(world: &WorldSim) -> Option<(i32, i32, i32, i32)> {
    let min_chunk_x = world.chunks.keys().map(|coord| coord.x).min()?;
    let max_chunk_x = world.chunks.keys().map(|coord| coord.x).max()?;
    let min_chunk_y = world.chunks.keys().map(|coord| coord.y).min()?;
    let max_chunk_y = world.chunks.keys().map(|coord| coord.y).max()?;

    Some((
        min_chunk_x * CHUNK_SIZE,
        max_chunk_x * CHUNK_SIZE + CHUNK_SIZE - 1,
        min_chunk_y * CHUNK_SIZE,
        max_chunk_y * CHUNK_SIZE + CHUNK_SIZE - 1,
    ))
}

pub(super) fn player_can_occupy_tile(
    world: &WorldSim,
    occupancy: &OccupancyGrid,
    x: i32,
    y: i32,
) -> bool {
    let Some(tile) = world.tile_at(x, y) else {
        return false;
    };

    tile.collision.walkable && occupancy.entity_at(x, y).is_none()
}

pub(super) fn tiles_to_fixed(tiles: f32) -> i64 {
    (tiles * PLAYER_POSITION_SCALE as f32).round() as i64
}

pub(super) fn fixed_to_tile(value: i64) -> i32 {
    value.div_euclid(PLAYER_POSITION_SCALE) as i32
}

pub(super) fn tile_center_fixed(tile: i32) -> i64 {
    i64::from(tile) * PLAYER_POSITION_SCALE + PLAYER_POSITION_SCALE / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offshore_pump_water_tiles_returns_non_north_facing_edge_tiles() {
        let footprint = EntityFootprint {
            x: 10,
            y: 20,
            width: 2,
            height: 3,
        };
        let cases = [
            (Direction::East, vec![(12, 20), (12, 21), (12, 22)]),
            (Direction::South, vec![(10, 23), (11, 23)]),
            (Direction::West, vec![(9, 20), (9, 21), (9, 22)]),
        ];

        for (direction, expected) in cases {
            assert_eq!(offshore_pump_water_tiles(&footprint, direction), expected);
        }
    }
}
