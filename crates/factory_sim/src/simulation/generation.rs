use super::*;
use factory_data::{BasePrototypeIds, item_id_by_name};

#[derive(Default)]
pub(super) struct StableHasher {
    hash: u64,
    initialized: bool,
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, bytes: &[u8]) {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        if !self.initialized {
            self.hash = FNV_OFFSET;
            self.initialized = true;
        }

        for byte in bytes {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(FNV_PRIME);
        }
    }
}

pub(super) fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

pub(super) fn generate_world_chunks(
    seed: u64,
    prototypes: &PrototypeCatalog,
) -> BTreeMap<ChunkCoord, Chunk> {
    generate_chunks(seed, prototypes, STARTING_MIN_CHUNK, STARTING_MAX_CHUNK)
}

pub(super) fn generate_chunks(
    seed: u64,
    prototypes: &PrototypeCatalog,
    min_chunk: i32,
    max_chunk: i32,
) -> BTreeMap<ChunkCoord, Chunk> {
    let ids = WorldPrototypeIds::from_catalog(prototypes);
    let mut chunks = BTreeMap::new();

    for chunk_y in min_chunk..=max_chunk {
        for chunk_x in min_chunk..=max_chunk {
            let coord = ChunkCoord {
                x: chunk_x,
                y: chunk_y,
            };
            chunks.insert(coord, generate_chunk(seed, coord, ids));
        }
    }

    chunks
}

pub(super) fn generate_chunk(seed: u64, coord: ChunkCoord, ids: WorldPrototypeIds) -> Chunk {
    let mut tiles = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);
    let bounds = TileBounds::for_chunk(coord);
    let centers = generate_resource_patch_centers(seed, ids, bounds);

    for local_y in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            let (x, y) = coord.tile_at(local_x, local_y);
            tiles.push(generate_tile(seed, x, y, ids, &centers));
        }
    }

    Chunk { coord, tiles }
}

pub(super) fn generate_tile(
    seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    ids: WorldPrototypeIds,
    centers: &[ResourcePatchCenter],
) -> TileCell {
    let (tile_id, mut collision) = generate_terrain(seed, x, y, ids);
    let resource = if tile_id == ids.water {
        None
    } else {
        resource_at_patch_tile(seed, x, y, centers)
    };

    if let Some(resource) = resource {
        // Fluid resources (crude oil) are extracted by pumpjacks, never mined.
        collision = TileCollision {
            walkable: true,
            buildable: false,
            minable: resource.resource_item != ids.crude_oil,
        };
    }

    TileCell {
        tile_id,
        collision,
        resource,
    }
}

pub(super) fn generate_terrain(
    seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    ids: WorldPrototypeIds,
) -> (TileId, TileCollision) {
    let terrain_hash = hash_world(seed, x, y);
    let terrain_roll = terrain_hash % 100;
    if terrain_roll < 10 {
        (
            ids.water,
            TileCollision {
                walkable: false,
                buildable: false,
                minable: false,
            },
        )
    } else if terrain_roll < 35 {
        (ids.dirt, ground_collision())
    } else {
        (ids.grass, ground_collision())
    }
}

pub(super) fn ground_collision() -> TileCollision {
    TileCollision {
        walkable: true,
        buildable: true,
        minable: false,
    }
}

pub(super) fn collision_for_tile(tile_id: TileId, ids: WorldPrototypeIds) -> TileCollision {
    if tile_id == ids.water {
        TileCollision {
            walkable: false,
            buildable: false,
            minable: false,
        }
    } else {
        ground_collision()
    }
}

pub(super) fn generate_resource_patch_centers(
    seed: u64,
    ids: WorldPrototypeIds,
    bounds: TileBounds,
) -> Vec<ResourcePatchCenter> {
    let configs = resource_patch_configs(ids);
    let starting_offsets = [(-22, -14), (18, -12), (-16, 20), (20, 18), (-26, 4)];
    let mut centers = Vec::new();

    for (index, config) in configs.iter().enumerate() {
        let (x, y) = starting_offsets[index];
        let center = ResourcePatchCenter {
            resource_item: config.resource_item,
            x: i64::from(x),
            y: i64::from(y),
            radius: config.radius,
            richness: config.richness,
        };
        if resource_patch_can_affect_bounds(center, bounds) {
            centers.push(center);
        }
    }

    let max_reach = configs
        .iter()
        .map(|config| config.radius)
        .max()
        .unwrap_or(0)
        + RESOURCE_PATCH_EDGE_NOISE
        + RESOURCE_PATCH_GRID_JITTER;
    let max_reach = i64::from(max_reach);
    let grid_size = i64::from(RESOURCE_PATCH_GRID_SIZE);
    let min_grid_x = (bounds.min_x - max_reach).div_euclid(grid_size);
    let max_grid_x = (bounds.max_x + max_reach).div_euclid(grid_size);
    let min_grid_y = (bounds.min_y - max_reach).div_euclid(grid_size);
    let max_grid_y = (bounds.max_y + max_reach).div_euclid(grid_size);

    for grid_y in min_grid_y..=max_grid_y {
        for grid_x in min_grid_x..=max_grid_x {
            for config in configs {
                let hash = hash_resource_center(seed, grid_x, grid_y, config.resource_item);
                if hash % 100 >= u64::from(config.frequency_percent) {
                    continue;
                }

                let jitter_x = ((hash >> 8) % (RESOURCE_PATCH_GRID_JITTER * 2 + 1) as u64) as i64
                    - i64::from(RESOURCE_PATCH_GRID_JITTER);
                let jitter_y = ((hash >> 16) % (RESOURCE_PATCH_GRID_JITTER * 2 + 1) as u64) as i64
                    - i64::from(RESOURCE_PATCH_GRID_JITTER);

                let center = ResourcePatchCenter {
                    resource_item: config.resource_item,
                    x: grid_x * grid_size + grid_size / 2 + jitter_x,
                    y: grid_y * grid_size + grid_size / 2 + jitter_y,
                    radius: config.radius,
                    richness: config.richness,
                };
                if resource_patch_can_affect_bounds(center, bounds) {
                    centers.push(center);
                }
            }
        }
    }

    centers
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TileBounds {
    pub(super) min_x: WorldTileCoord,
    pub(super) max_x: WorldTileCoord,
    pub(super) min_y: WorldTileCoord,
    pub(super) max_y: WorldTileCoord,
}

impl TileBounds {
    pub(super) fn for_chunk(coord: ChunkCoord) -> Self {
        let (min_x, min_y) = coord.min_tile();
        let max_offset = i64::from(CHUNK_SIZE - 1);
        Self {
            min_x,
            max_x: min_x + max_offset,
            min_y,
            max_y: min_y + max_offset,
        }
    }
}

fn resource_patch_can_affect_bounds(center: ResourcePatchCenter, bounds: TileBounds) -> bool {
    let closest_x = center.x.clamp(bounds.min_x, bounds.max_x);
    let closest_y = center.y.clamp(bounds.min_y, bounds.max_y);
    let dx = i128::from(center.x) - i128::from(closest_x);
    let dy = i128::from(center.y) - i128::from(closest_y);
    let reach = i128::from(center.radius + RESOURCE_PATCH_EDGE_NOISE);

    dx * dx + dy * dy <= reach * reach
}

pub(super) fn resource_at_patch_tile(
    seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    centers: &[ResourcePatchCenter],
) -> Option<ResourceCell> {
    let mut best: Option<ResourceCandidate> = None;

    for center in centers {
        let dx = i128::from(x) - i128::from(center.x);
        let dy = i128::from(y) - i128::from(center.y);
        let distance_sq = dx * dx + dy * dy;
        let radius = center.radius + resource_edge_noise(seed, x, y, center.resource_item);
        let radius_sq = i128::from(radius) * i128::from(radius);

        if distance_sq > radius_sq {
            continue;
        }

        let score = radius_sq - distance_sq;
        if best.is_none_or(|candidate| score > candidate.score) {
            best = Some(ResourceCandidate {
                center: *center,
                distance_sq,
                radius_sq,
                score,
            });
        }
    }

    best.map(|candidate| {
        let radius_sq =
            u32::try_from(candidate.radius_sq.max(1)).expect("resource radius is bounded");
        let distance_sq = u32::try_from(candidate.distance_sq.max(0))
            .expect("resource distance is bounded by radius");
        let falloff = (radius_sq - distance_sq).max(1);
        let base = candidate.center.richness / 3;
        let scaled = candidate.center.richness * falloff / radius_sq;
        let variation =
            (hash_world(seed ^ 0x1d17_5f2c_6b31_f011, x, y) % u64::from(base.max(1))) as u32;

        ResourceCell {
            resource_item: candidate.center.resource_item,
            amount: base + scaled + variation,
        }
    })
}

pub(super) fn resource_patch_configs(ids: WorldPrototypeIds) -> [ResourcePatchConfig; 5] {
    [
        ResourcePatchConfig {
            resource_item: ids.resources[0],
            frequency_percent: 68,
            radius: 9,
            richness: 700,
        },
        ResourcePatchConfig {
            resource_item: ids.resources[1],
            frequency_percent: 62,
            radius: 8,
            richness: 650,
        },
        ResourcePatchConfig {
            resource_item: ids.resources[2],
            frequency_percent: 55,
            radius: 10,
            richness: 800,
        },
        ResourcePatchConfig {
            resource_item: ids.resources[3],
            frequency_percent: 48,
            radius: 7,
            richness: 520,
        },
        ResourcePatchConfig {
            resource_item: ids.crude_oil,
            frequency_percent: 30,
            radius: 4,
            richness: 900,
        },
    ]
}

pub(super) fn resource_edge_noise(
    seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    resource_item: ItemId,
) -> i32 {
    let hash = hash_world(
        seed ^ 0x7b5d_1f25_8c92_f6a3 ^ u64::from(resource_item.raw()),
        x,
        y,
    );
    (hash % (RESOURCE_PATCH_EDGE_NOISE * 2 + 1) as u64) as i32 - RESOURCE_PATCH_EDGE_NOISE
}

pub(super) fn hash_resource_center(
    seed: u64,
    grid_x: WorldTileCoord,
    grid_y: WorldTileCoord,
    resource_item: ItemId,
) -> u64 {
    hash_world(
        seed ^ 0xa24b_aed4_963e_e407 ^ u64::from(resource_item.raw()).rotate_left(17),
        grid_x,
        grid_y,
    )
}

#[derive(Clone, Copy)]
pub(super) struct ResourcePatchConfig {
    resource_item: ItemId,
    frequency_percent: u8,
    radius: i32,
    richness: u32,
}

#[derive(Clone, Copy)]
pub(super) struct ResourcePatchCenter {
    resource_item: ItemId,
    x: WorldTileCoord,
    y: WorldTileCoord,
    radius: i32,
    richness: u32,
}

#[derive(Clone, Copy)]
pub(super) struct ResourceCandidate {
    center: ResourcePatchCenter,
    distance_sq: i128,
    radius_sq: i128,
    score: i128,
}

pub(super) fn hash_world(seed: u64, x: WorldTileCoord, y: WorldTileCoord) -> u64 {
    let x_bits = x as u64;
    let y_bits = y as u64;
    splitmix64(seed ^ x_bits.rotate_left(32) ^ y_bits.rotate_left(1))
}

#[derive(Clone, Copy)]
pub(super) struct WorldPrototypeIds {
    pub(super) grass: TileId,
    pub(super) dirt: TileId,
    pub(super) water: TileId,
    pub(super) resources: [ItemId; 4],
    pub(super) crude_oil: ItemId,
}

impl WorldPrototypeIds {
    pub(super) fn from_catalog(prototypes: &PrototypeCatalog) -> Self {
        let ids = BasePrototypeIds::from_catalog(prototypes);
        Self {
            grass: ids.tiles.grass,
            dirt: ids.tiles.dirt,
            water: ids.tiles.water,
            resources: ids.items.resource_items(),
            crude_oil: ids.items.crude_oil,
        }
    }
}

pub(super) fn item_id(prototypes: &PrototypeCatalog, name: &str) -> ItemId {
    item_id_by_name(prototypes, name)
}

#[cfg(test)]
pub(super) fn recipe_id(prototypes: &PrototypeCatalog, name: &str) -> RecipeId {
    factory_data::recipe_id_by_name(prototypes, name)
}

#[cfg(test)]
pub(super) fn technology_id(prototypes: &PrototypeCatalog, name: &str) -> TechnologyId {
    factory_data::technology_id_by_name(prototypes, name)
}

/// Whether `item_id` marks a fluid resource cell (crude oil): some pumpjack
/// prototype extracts it. Fluid resources are excluded from solid mining by
/// drills and the player.
pub(super) fn is_fluid_resource_item(prototypes: &PrototypeCatalog, item_id: ItemId) -> bool {
    prototypes.entities.iter().any(|prototype| {
        prototype
            .pumpjack
            .as_ref()
            .is_some_and(|pumpjack| pumpjack.resource_item == item_id)
    })
}

pub(super) fn item_stack_size(prototypes: &PrototypeCatalog, item_id: ItemId) -> Option<u16> {
    prototypes
        .item(item_id)
        .map(|prototype| prototype.stack_size)
}

pub(super) fn fuel_value_joules(prototypes: &PrototypeCatalog, item_id: ItemId) -> Option<u64> {
    prototypes
        .item(item_id)
        .and_then(|prototype| prototype.fuel_value_joules)
}

pub(super) fn is_science_pack_item(catalog: &PrototypeCatalog, item_id: ItemId) -> bool {
    catalog
        .technologies
        .iter()
        .flat_map(|technology| &technology.science_packs)
        .any(|science_pack| science_pack.item == item_id)
}

pub(super) fn lab_can_accept_item(catalog: &PrototypeCatalog, item_id: ItemId) -> bool {
    is_science_pack_item(catalog, item_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_resource_candidates_can_affect_their_chunk() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let ids = WorldPrototypeIds::from_catalog(&catalog);

        for seed in [0, 123, 987_654_321] {
            for coord in [
                ChunkCoord { x: -2, y: -2 },
                ChunkCoord { x: 0, y: 0 },
                ChunkCoord { x: 2, y: 2 },
                ChunkCoord { x: 17, y: -23 },
            ] {
                let bounds = TileBounds::for_chunk(coord);
                let centers = generate_resource_patch_centers(seed, ids, bounds);

                assert!(
                    centers
                        .iter()
                        .all(|center| resource_patch_can_affect_bounds(*center, bounds)),
                    "a retained candidate cannot affect chunk {coord:?} for seed {seed}"
                );
            }
        }
    }
}
