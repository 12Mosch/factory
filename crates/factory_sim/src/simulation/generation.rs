use super::*;
use factory_data::{CollisionLayer, CollisionMask, ResourceExtraction, item_id_by_name};

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
    let rules = WorldGenRules::from_catalog(prototypes);
    let area = prototypes.world_generation.starting_area;
    let mut chunks = BTreeMap::new();

    for chunk_y in area.min_chunk..=area.max_chunk {
        for chunk_x in area.min_chunk..=area.max_chunk {
            let coord = ChunkCoord {
                x: chunk_x,
                y: chunk_y,
            };
            chunks.insert(coord, generate_chunk(seed, coord, &rules));
        }
    }

    chunks
}

pub(super) fn generate_chunk(seed: u64, coord: ChunkCoord, rules: &WorldGenRules) -> Chunk {
    let mut tiles = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);
    let bounds = TileBounds::for_chunk(coord);
    let centers = generate_resource_patch_centers(seed, rules, bounds);

    for local_y in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            let (x, y) = coord.tile_at(local_x, local_y);
            tiles.push(generate_tile(seed, x, y, rules, &centers));
        }
    }

    Chunk { coord, tiles }
}

pub(super) fn generate_tile(
    seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    rules: &WorldGenRules,
    centers: &[ResourcePatchCenter],
) -> TileCell {
    let (tile_id, mut collision) = generate_terrain(seed, x, y, rules);
    // Resource patches only overlay ground-like terrain.
    let resource = if collision.walkable && collision.buildable {
        resource_at_patch_tile(seed, x, y, centers, rules.edge_noise)
    } else {
        None
    };

    if let Some(resource) = resource {
        // Fluid resources are extracted by pumpjacks, never mined.
        collision = TileCollision {
            walkable: true,
            buildable: false,
            minable: rules.resource_is_minable(resource.resource_item),
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
    rules: &WorldGenRules,
) -> (TileId, TileCollision) {
    if rules.terrain_total_weight > 0 {
        let terrain_roll = hash_world(seed, x, y) % rules.terrain_total_weight;
        let mut cumulative_weight = 0;
        for layer in &rules.terrain {
            cumulative_weight += layer.weight;
            if terrain_roll < cumulative_weight {
                return (layer.tile_id, layer.collision);
            }
        }
    }

    (rules.fallback_tile, rules.fallback_collision)
}

pub(super) fn ground_collision() -> TileCollision {
    TileCollision {
        walkable: true,
        buildable: true,
        minable: false,
    }
}

/// Terrain collision behaviour derived from a tile prototype's collision
/// mask: water-layer tiles block movement and building, everything else is
/// open ground.
pub(super) fn collision_from_mask(mask: &CollisionMask) -> TileCollision {
    if mask.layers.contains(&CollisionLayer::Water) {
        TileCollision {
            walkable: false,
            buildable: false,
            minable: false,
        }
    } else {
        ground_collision()
    }
}

pub(super) fn tile_collision(prototypes: &PrototypeCatalog, tile_id: TileId) -> TileCollision {
    prototypes
        .tile(tile_id)
        .map(|tile| collision_from_mask(&tile.collision_mask))
        .unwrap_or_else(ground_collision)
}

pub(super) fn generate_resource_patch_centers(
    seed: u64,
    rules: &WorldGenRules,
    bounds: TileBounds,
) -> Vec<ResourcePatchCenter> {
    let mut centers = Vec::new();
    if rules.resources.is_empty() {
        return centers;
    }

    for resource in &rules.resources {
        let Some((x, y)) = resource.starting_patch else {
            continue;
        };
        let center = ResourcePatchCenter {
            resource_item: resource.resource_item,
            x,
            y,
            radius: resource.radius,
            richness: resource.richness,
        };
        if resource_patch_can_affect_bounds(center, bounds, rules.edge_noise) {
            centers.push(center);
        }
    }

    let max_reach = rules
        .resources
        .iter()
        .map(|resource| resource.radius)
        .max()
        .unwrap_or(0)
        + rules.edge_noise
        + rules.grid_jitter;
    let max_reach = i64::from(max_reach);
    let grid_size = i64::from(rules.grid_cell_size);
    let min_grid_x = (bounds.min_x - max_reach).div_euclid(grid_size);
    let max_grid_x = (bounds.max_x + max_reach).div_euclid(grid_size);
    let min_grid_y = (bounds.min_y - max_reach).div_euclid(grid_size);
    let max_grid_y = (bounds.max_y + max_reach).div_euclid(grid_size);

    for grid_y in min_grid_y..=max_grid_y {
        for grid_x in min_grid_x..=max_grid_x {
            for resource in &rules.resources {
                let hash = hash_resource_center(seed, grid_x, grid_y, resource.resource_item);
                if hash % 100 >= u64::from(resource.frequency_percent) {
                    continue;
                }

                let jitter_x = ((hash >> 8) % (rules.grid_jitter * 2 + 1) as u64) as i64
                    - i64::from(rules.grid_jitter);
                let jitter_y = ((hash >> 16) % (rules.grid_jitter * 2 + 1) as u64) as i64
                    - i64::from(rules.grid_jitter);

                let center = ResourcePatchCenter {
                    resource_item: resource.resource_item,
                    x: grid_x * grid_size + grid_size / 2 + jitter_x,
                    y: grid_y * grid_size + grid_size / 2 + jitter_y,
                    radius: resource.radius,
                    richness: resource.richness,
                };
                if resource_patch_can_affect_bounds(center, bounds, rules.edge_noise) {
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

fn resource_patch_can_affect_bounds(
    center: ResourcePatchCenter,
    bounds: TileBounds,
    edge_noise: i32,
) -> bool {
    let closest_x = center.x.clamp(bounds.min_x, bounds.max_x);
    let closest_y = center.y.clamp(bounds.min_y, bounds.max_y);
    let dx = i128::from(center.x) - i128::from(closest_x);
    let dy = i128::from(center.y) - i128::from(closest_y);
    let reach = i128::from(center.radius + edge_noise);

    dx * dx + dy * dy <= reach * reach
}

pub(super) fn resource_at_patch_tile(
    seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    centers: &[ResourcePatchCenter],
    edge_noise: i32,
) -> Option<ResourceCell> {
    let mut best: Option<ResourceCandidate> = None;

    for center in centers {
        let dx = i128::from(x) - i128::from(center.x);
        let dy = i128::from(y) - i128::from(center.y);
        let distance_sq = dx * dx + dy * dy;
        let radius =
            center.radius + resource_edge_noise(seed, x, y, center.resource_item, edge_noise);
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

pub(super) fn resource_edge_noise(
    seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    resource_item: ItemId,
    edge_noise: i32,
) -> i32 {
    let hash = hash_world(
        seed ^ 0x7b5d_1f25_8c92_f6a3 ^ u64::from(resource_item.raw()),
        x,
        y,
    );
    (hash % (edge_noise * 2 + 1) as u64) as i32 - edge_noise
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

/// World generation rules resolved from a catalog's
/// [`factory_data::WorldGenerationConfig`]: terrain layers with their derived
/// collision, resource patch definitions with minability, and placement grid
/// parameters. Resolution is infallible; the loader already validated the
/// config against the catalog.
#[derive(Clone, Debug)]
pub(super) struct WorldGenRules {
    terrain: Vec<TerrainLayerRule>,
    terrain_total_weight: u64,
    /// Tile used when no terrain layers are configured (empty config).
    fallback_tile: TileId,
    fallback_collision: TileCollision,
    resources: Vec<ResourceRule>,
    grid_cell_size: i32,
    grid_jitter: i32,
    edge_noise: i32,
}

#[derive(Clone, Copy, Debug)]
struct TerrainLayerRule {
    tile_id: TileId,
    weight: u64,
    collision: TileCollision,
}

#[derive(Clone, Copy, Debug)]
struct ResourceRule {
    resource_item: ItemId,
    minable: bool,
    frequency_percent: u8,
    radius: i32,
    richness: u32,
    starting_patch: Option<(WorldTileCoord, WorldTileCoord)>,
}

impl WorldGenRules {
    pub(super) fn from_catalog(prototypes: &PrototypeCatalog) -> Self {
        let config = &prototypes.world_generation;
        let terrain: Vec<TerrainLayerRule> = config
            .terrain
            .iter()
            .map(|layer| TerrainLayerRule {
                tile_id: layer.tile,
                weight: u64::from(layer.weight),
                collision: tile_collision(prototypes, layer.tile),
            })
            .collect();
        let terrain_total_weight = terrain.iter().map(|layer| layer.weight).sum();
        let (fallback_tile, fallback_collision) = prototypes
            .tiles
            .first()
            .map(|tile| (tile.id, collision_from_mask(&tile.collision_mask)))
            .unwrap_or_else(|| (TileId::new(0), ground_collision()));
        let resources = config
            .resources
            .iter()
            .map(|resource| ResourceRule {
                resource_item: resource.resource_item,
                minable: resource.extraction == ResourceExtraction::Solid,
                frequency_percent: resource.frequency_percent,
                radius: resource.radius,
                richness: resource.richness,
                starting_patch: resource
                    .starting_patch
                    .map(|offset| (i64::from(offset.x), i64::from(offset.y))),
            })
            .collect();

        Self {
            terrain,
            terrain_total_weight,
            fallback_tile,
            fallback_collision,
            resources,
            grid_cell_size: config.patch_grid.cell_size,
            grid_jitter: config.patch_grid.jitter,
            edge_noise: config.patch_grid.edge_noise,
        }
    }

    fn resource_is_minable(&self, resource_item: ItemId) -> bool {
        self.resources
            .iter()
            .find(|resource| resource.resource_item == resource_item)
            .is_some_and(|resource| resource.minable)
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

/// Whether `item_id` marks a fluid resource cell: the world generation config
/// declares its extraction type as [`ResourceExtraction::Fluid`]. Fluid
/// resources are extracted by pumpjacks and excluded from solid mining by
/// drills and the player.
pub(super) fn is_fluid_resource_item(prototypes: &PrototypeCatalog, item_id: ItemId) -> bool {
    prototypes
        .world_generation
        .resources
        .iter()
        .any(|resource| {
            resource.resource_item == item_id && resource.extraction == ResourceExtraction::Fluid
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
        let rules = WorldGenRules::from_catalog(&catalog);

        for seed in [0, 123, 987_654_321] {
            for coord in [
                ChunkCoord { x: -2, y: -2 },
                ChunkCoord { x: 0, y: 0 },
                ChunkCoord { x: 2, y: 2 },
                ChunkCoord { x: 17, y: -23 },
            ] {
                let bounds = TileBounds::for_chunk(coord);
                let centers = generate_resource_patch_centers(seed, &rules, bounds);

                assert!(
                    centers
                        .iter()
                        .all(|center| resource_patch_can_affect_bounds(
                            *center,
                            bounds,
                            rules.edge_noise
                        )),
                    "a retained candidate cannot affect chunk {coord:?} for seed {seed}"
                );
            }
        }
    }
}
