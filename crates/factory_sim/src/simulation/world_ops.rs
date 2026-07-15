use super::*;

impl WorldSim {
    const RESOURCE_DIRTY_HISTORY_LIMIT: usize = 4096;

    pub fn new(seed: u64, prototypes: PrototypeCatalog) -> Self {
        let generator = WorldGenerator::from_catalog(&prototypes);
        let chunks = generate_world_chunks(seed, &prototypes, &generator);
        Self {
            seed,
            prototypes,
            chunks,
            generator,
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

    pub(crate) fn from_snapshot(
        seed: u64,
        prototypes: PrototypeCatalog,
        mut chunks: BTreeMap<ChunkCoord, Chunk>,
    ) -> Self {
        let generator = WorldGenerator::from_catalog(&prototypes);
        for chunk in chunks.values_mut() {
            chunk.pollution_absorption_per_minute_milli = chunk
                .tiles
                .iter()
                .map(|tile| {
                    generator
                        .tile_pollution_absorption_per_minute_milli
                        .get(tile.tile_id.index())
                        .copied()
                        .unwrap_or(0)
                })
                .sum();
        }

        Self {
            seed,
            prototypes,
            chunks,
            generator,
            chunk_revision: 0,
            resource_revision: 0,
            resource_dirty_tiles: VecDeque::new(),
        }
    }

    pub fn tile_at<X: Into<WorldTileCoord>, Y: Into<WorldTileCoord>>(
        &self,
        x: X,
        y: Y,
    ) -> Option<&TileCell> {
        let x = x.into();
        let y = y.into();
        let (coord, index) = chunk_coord_and_tile_index(x, y)?;

        self.chunks
            .get(&coord)
            .and_then(|chunk| chunk.tiles.get(index))
    }

    pub fn ensure_chunk_generated(&mut self, coord: ChunkCoord) -> bool {
        self.generate_missing_chunk(coord)
    }

    /// Generates every missing coordinate in iteration order and returns the
    /// coordinates actually inserted. Duplicate and already-generated
    /// coordinates are omitted from the result.
    pub fn ensure_chunks_generated(
        &mut self,
        coords: impl IntoIterator<Item = ChunkCoord>,
    ) -> Vec<ChunkCoord> {
        let mut generated = Vec::new();
        for coord in coords {
            if self.generate_missing_chunk(coord) {
                generated.push(coord);
            }
        }
        generated
    }

    fn generate_missing_chunk(&mut self, coord: ChunkCoord) -> bool {
        let std::collections::btree_map::Entry::Vacant(entry) = self.chunks.entry(coord) else {
            return false;
        };

        entry.insert(generate_chunk(self.seed, coord, &self.generator));
        self.chunk_revision = self.chunk_revision.wrapping_add(1);
        true
    }

    pub fn ensure_chunks_around_chunk(
        &mut self,
        center: ChunkCoord,
        radius: i32,
    ) -> Result<usize, ChunkNeighborhoodError> {
        let (min_x, max_x, min_y, max_y) = chunk_neighborhood_bounds(center, radius)?;
        let coords =
            (min_y..=max_y).flat_map(|y| (min_x..=max_x).map(move |x| ChunkCoord { x, y }));
        Ok(self.ensure_chunks_generated(coords).len())
    }

    pub fn chunk_revision(&self) -> u64 {
        self.chunk_revision
    }

    pub fn generated_chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub(super) fn tile_at_profiled<P: TickProfiler>(
        &self,
        x: WorldTileCoord,
        y: WorldTileCoord,
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

    pub fn mine_resource_at<X: Into<WorldTileCoord>, Y: Into<WorldTileCoord>>(
        &mut self,
        x: X,
        y: Y,
        amount: u32,
    ) -> Option<MinedResource> {
        let x = x.into();
        let y = y.into();
        if amount == 0 {
            return None;
        }

        let (mined, resource) = {
            let prototypes = &self.prototypes;
            let (coord, index) = chunk_coord_and_tile_index(x, y)?;
            let tile = self
                .chunks
                .get_mut(&coord)
                .and_then(|chunk| chunk.tiles.get_mut(index))?;
            let mined = mine_resource_from_tile(tile, prototypes, amount)?;
            (mined, tile.resource)
        };
        self.record_resource_tile_change(x, y, resource);

        Some(mined)
    }

    pub(super) fn mine_resource_at_profiled<P: TickProfiler>(
        &mut self,
        x: WorldTileCoord,
        y: WorldTileCoord,
        amount: u32,
        profiler: &mut P,
    ) -> Option<MinedResource> {
        if amount == 0 {
            return None;
        }

        let (mined, resource) = {
            let prototypes = &self.prototypes;
            let chunks = &mut self.chunks;
            let tile = profiler.measure(ProfilePhase::ChunkLookup, move || {
                let (coord, index) = chunk_coord_and_tile_index(x, y)?;
                chunks
                    .get_mut(&coord)
                    .and_then(|chunk| chunk.tiles.get_mut(index))
            })?;
            let mined = mine_resource_from_tile(tile, prototypes, amount)?;
            (mined, tile.resource)
        };
        self.record_resource_tile_change(x, y, resource);

        Some(mined)
    }

    pub fn can_build_on_tile<X: Into<WorldTileCoord>, Y: Into<WorldTileCoord>>(
        &self,
        x: X,
        y: Y,
    ) -> Result<(), BuildError> {
        let x = x.into();
        let y = y.into();
        let tile = self
            .tile_at(x, y)
            .ok_or(BuildError::OutsideGeneratedChunks { x, y })?;

        if tile.collision.buildable {
            Ok(())
        } else {
            Err(BuildError::TileBlocked { x, y })
        }
    }

    pub fn entity_footprint<X: Into<WorldTileCoord>, Y: Into<WorldTileCoord>>(
        &self,
        prototype_id: EntityPrototypeId,
        x: X,
        y: Y,
        direction: Direction,
    ) -> Result<EntityFootprint, BuildError> {
        let x = x.into();
        let y = y.into();
        let prototype = self
            .prototypes
            .entity(prototype_id)
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

        if prototype.entity_kind == EntityKind::Pumpjack
            && let Some(pumpjack) = prototype.pumpjack.as_ref()
        {
            self.validate_walkable_footprint(footprint)?;
            let covers_resource = footprint.tiles().into_iter().any(|(x, y)| {
                self.tile_at(x, y)
                    .and_then(|tile| tile.resource)
                    .is_some_and(|resource| resource.resource_item == pumpjack.resource_item)
            });
            if !covers_resource {
                return Err(BuildError::TileBlocked {
                    x: footprint.x,
                    y: footprint.y,
                });
            }

            return Ok(());
        }

        if prototype.entity_kind != EntityKind::MiningDrill || prototype.mining_drill.is_none() {
            return self.validate_entity_footprint(footprint);
        }

        self.validate_walkable_footprint(footprint)?;

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

    /// Footprint check for machines allowed to sit on resource tiles: every
    /// tile must exist and be walkable, but need not be buildable.
    fn validate_walkable_footprint(&self, footprint: &EntityFootprint) -> Result<(), BuildError> {
        footprint.validate()?;
        for (x, y) in footprint.tiles() {
            let tile = self
                .tile_at(x, y)
                .ok_or(BuildError::OutsideGeneratedChunks { x, y })?;
            if !tile.collision.walkable {
                return Err(BuildError::TileBlocked { x, y });
            }
        }

        Ok(())
    }

    fn record_resource_tile_change(
        &mut self,
        x: WorldTileCoord,
        y: WorldTileCoord,
        resource: Option<ResourceCell>,
    ) {
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

pub(super) fn chunk_neighborhood_bounds(
    center: ChunkCoord,
    radius: i32,
) -> Result<(i32, i32, i32, i32), ChunkNeighborhoodError> {
    let radius = i64::from(radius.max(0));
    let min_x = i64::from(center.x) - radius;
    let max_x = i64::from(center.x) + radius;
    let min_y = i64::from(center.y) - radius;
    let max_y = i64::from(center.y) + radius;
    if min_x < i64::from(i32::MIN)
        || max_x > i64::from(i32::MAX)
        || min_y < i64::from(i32::MIN)
        || max_y > i64::from(i32::MAX)
    {
        return Err(ChunkNeighborhoodError::OutOfChunkCoordinateRange {
            center,
            radius: radius as i32,
        });
    }

    Ok((min_x as i32, max_x as i32, min_y as i32, max_y as i32))
}

#[derive(Clone, Copy)]
pub(super) enum ChunkGenerationPriority {
    Required,
    Chart,
    Prefetch,
}

impl Simulation {
    pub(super) fn request_chunk_generation(
        &mut self,
        coord: ChunkCoord,
        priority: ChunkGenerationPriority,
    ) {
        if self.world.chunks.contains_key(&coord) {
            return;
        }

        match priority {
            ChunkGenerationPriority::Required => {
                self.chunk_generation_queue.chart.remove(&coord);
                self.chunk_generation_queue.prefetch.remove(&coord);
                self.chunk_generation_queue.required.insert(coord);
            }
            ChunkGenerationPriority::Chart => {
                if !self.chunk_generation_queue.required.contains(&coord)
                    && !self.chunk_generation_queue.prefetch.contains(&coord)
                {
                    self.chunk_generation_queue.chart.insert(coord);
                }
            }
            ChunkGenerationPriority::Prefetch => {
                if !self.chunk_generation_queue.required.contains(&coord) {
                    self.chunk_generation_queue.chart.remove(&coord);
                    self.chunk_generation_queue.prefetch.insert(coord);
                }
            }
        }
    }

    pub(super) fn process_chunk_generation_queue(&mut self, budget: usize) -> usize {
        let mut generated = 0;
        while generated < budget {
            let coord = self
                .chunk_generation_queue
                .required
                .pop_first()
                .or_else(|| self.chunk_generation_queue.prefetch.pop_first())
                .or_else(|| self.chunk_generation_queue.chart.pop_first());
            let Some(coord) = coord else {
                break;
            };
            if self.world.ensure_chunk_generated(coord) {
                generated += 1;
            }
        }

        if generated != 0 {
            self.seed_enemy_spawners_in_new_chunks();
        }
        generated
    }

    pub(super) fn remove_chunk_generation_request(&mut self, coord: ChunkCoord) {
        self.chunk_generation_queue.required.remove(&coord);
        self.chunk_generation_queue.chart.remove(&coord);
        self.chunk_generation_queue.prefetch.remove(&coord);
    }
}

pub(super) fn is_water_like_tile(tile: &TileCell) -> bool {
    !tile.collision.walkable && !tile.collision.buildable
}

pub(super) fn offshore_pump_water_tiles(
    footprint: &EntityFootprint,
    direction: Direction,
) -> Vec<(WorldTileCoord, WorldTileCoord)> {
    match direction {
        Direction::North => (footprint.x..footprint.x + i64::from(footprint.width))
            .map(|x| (x, footprint.y - 1))
            .collect(),
        Direction::East => (footprint.y..footprint.y + i64::from(footprint.height))
            .map(|y| (footprint.x + i64::from(footprint.width), y))
            .collect(),
        Direction::South => (footprint.x..footprint.x + i64::from(footprint.width))
            .map(|x| (x, footprint.y + i64::from(footprint.height)))
            .collect(),
        Direction::West => (footprint.y..footprint.y + i64::from(footprint.height))
            .map(|y| (footprint.x - 1, y))
            .collect(),
    }
}

fn mine_resource_from_tile(
    tile: &mut TileCell,
    prototypes: &PrototypeCatalog,
    amount: u32,
) -> Option<MinedResource> {
    // Fluid resources (e.g. crude oil) are extracted by pumpjacks, never mined.
    if !tile.collision.minable {
        return None;
    }

    let resource = tile.resource.as_mut()?;
    let mined_amount = amount.min(resource.amount);
    let mined = MinedResource {
        resource_item: resource.resource_item,
        amount: mined_amount,
    };

    resource.amount -= mined_amount;
    if resource.amount == 0 {
        tile.resource = None;
        tile.collision = tile_collision(prototypes, tile.tile_id);
    }

    Some(mined)
}

pub(super) fn chunk_coord_and_tile_index(
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> Option<(ChunkCoord, usize)> {
    let coord = ChunkCoord::from_tile(x, y)?;
    let size = i64::from(CHUNK_SIZE);
    let local_x = x.rem_euclid(size) as usize;
    let local_y = y.rem_euclid(size) as usize;
    let index = local_y * CHUNK_SIZE as usize + local_x;

    Some((coord, index))
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

pub(super) fn world_tile_bounds(
    world: &WorldSim,
) -> Option<(
    WorldTileCoord,
    WorldTileCoord,
    WorldTileCoord,
    WorldTileCoord,
)> {
    let min_chunk_x = world.chunks.keys().map(|coord| coord.x).min()?;
    let max_chunk_x = world.chunks.keys().map(|coord| coord.x).max()?;
    let min_chunk_y = world.chunks.keys().map(|coord| coord.y).min()?;
    let max_chunk_y = world.chunks.keys().map(|coord| coord.y).max()?;

    Some((
        ChunkCoord {
            x: min_chunk_x,
            y: min_chunk_y,
        }
        .min_tile()
        .0,
        ChunkCoord {
            x: max_chunk_x,
            y: max_chunk_y,
        }
        .min_tile()
        .0 + i64::from(CHUNK_SIZE - 1),
        ChunkCoord {
            x: min_chunk_x,
            y: min_chunk_y,
        }
        .min_tile()
        .1,
        ChunkCoord {
            x: max_chunk_x,
            y: max_chunk_y,
        }
        .min_tile()
        .1 + i64::from(CHUNK_SIZE - 1),
    ))
}

pub(super) fn player_can_occupy_tile(
    world: &WorldSim,
    occupancy: &OccupancyGrid,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> bool {
    let Some(tile) = world.tile_at(x, y) else {
        return false;
    };

    tile.collision.walkable && occupancy.entity_at(x, y).is_none()
}

pub(super) fn tiles_to_fixed(tiles: f32) -> i64 {
    (tiles * PLAYER_POSITION_SCALE as f32).round() as i64
}

pub(super) fn fixed_to_tile(value: i64) -> WorldTileCoord {
    value.div_euclid(PLAYER_POSITION_SCALE)
}

pub(super) fn tile_center_fixed(tile: WorldTileCoord) -> i64 {
    tile * PLAYER_POSITION_SCALE + PLAYER_POSITION_SCALE / 2
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkNeighborhoodError {
    OutOfChunkCoordinateRange { center: ChunkCoord, radius: i32 },
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
