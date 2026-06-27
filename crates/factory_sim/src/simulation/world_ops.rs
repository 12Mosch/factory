use super::*;

impl WorldSim {
    pub fn new(seed: u64, prototypes: PrototypeCatalog) -> Self {
        let chunks = generate_world_chunks(seed, &prototypes);
        Self {
            seed,
            prototypes,
            chunks,
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

    pub(super) fn tile_at_profiled<P: TickProfiler>(
        &self,
        x: i32,
        y: i32,
        profiler: &mut P,
    ) -> Option<&TileCell> {
        profiler.measure(ProfilePhase::ChunkLookup, || self.tile_at(x, y))
    }

    pub fn resource_hash(&self) -> u64 {
        let mut hasher = StableHasher::default();

        for chunk in self.chunks.values() {
            for (index, tile) in chunk.tiles.iter().enumerate() {
                let Some(resource) = tile.resource else {
                    continue;
                };

                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                let x = chunk.coord.x * CHUNK_SIZE + local_x;
                let y = chunk.coord.y * CHUNK_SIZE + local_y;

                x.hash(&mut hasher);
                y.hash(&mut hasher);
                resource.resource_item.hash(&mut hasher);
                resource.amount.hash(&mut hasher);
            }
        }

        hasher.finish()
    }

    pub fn mine_resource_at(&mut self, x: i32, y: i32, amount: u32) -> Option<MinedResource> {
        if amount == 0 {
            return None;
        }

        let ids = WorldPrototypeIds::from_catalog(&self.prototypes);
        let tile = self.tile_at_mut(x, y)?;
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
        let tile = profiler.measure(ProfilePhase::ChunkLookup, || self.tile_at_mut(x, y))?;
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
    ) -> Result<(), BuildError> {
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
