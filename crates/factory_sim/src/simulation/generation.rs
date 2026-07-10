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
        // Slice the coherent noise field into bands: each layer covers the
        // slice of the value range proportional to its weight, in declaration
        // order from the lowest values upward. Contiguous low regions become
        // lakes instead of scattered single tiles.
        let field = terrain_field(
            seed ^ TERRAIN_FIELD_SALT,
            x,
            y,
            rules.noise_scale,
            rules.noise_octaves,
        );
        let terrain_roll = (field * rules.terrain_total_weight) >> NOISE_ONE_BITS;
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

const TERRAIN_FIELD_SALT: u64 = 0x5e2d_58d8_b3bc_e8ee;

/// Q16 fixed-point one for the noise field; noise values live in
/// `[0, NOISE_ONE)`.
const NOISE_ONE_BITS: u32 = 16;
const NOISE_ONE: u64 = 1 << NOISE_ONE_BITS;

/// Fractal value noise in `[0, NOISE_ONE)`: `octaves` layers of
/// lattice-interpolated [`hash_world`] values, each octave halving both the
/// wavelength (starting at `scale` tiles) and the amplitude. Integer-only so
/// results are identical across platforms for a given seed.
pub(super) fn terrain_field(
    seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    scale: u32,
    octaves: u32,
) -> u64 {
    let mut total = 0u64;
    let mut amplitude_total = 0u64;
    let mut amplitude = NOISE_ONE;
    let mut wavelength = i64::from(scale.max(1));

    for octave in 0..octaves {
        if amplitude == 0 {
            break;
        }
        let octave_seed = seed ^ splitmix64(u64::from(octave).wrapping_add(0x9d8f_3b1a));
        total += value_noise(octave_seed, x, y, wavelength) * amplitude;
        amplitude_total += amplitude;
        amplitude >>= 1;
        wavelength = (wavelength / 2).max(1);
    }

    if amplitude_total == 0 {
        return 0;
    }
    total / amplitude_total
}

/// Single-octave value noise in `[0, NOISE_ONE)`: hashes the four corners of
/// the `wavelength`-sized lattice cell containing `(x, y)` and blends them
/// with smoothstep-eased bilinear interpolation.
fn value_noise(seed: u64, x: WorldTileCoord, y: WorldTileCoord, wavelength: i64) -> u64 {
    let cell_x = x.div_euclid(wavelength);
    let cell_y = y.div_euclid(wavelength);
    let fraction_x = (x.rem_euclid(wavelength) as u64 * NOISE_ONE) / wavelength as u64;
    let fraction_y = (y.rem_euclid(wavelength) as u64 * NOISE_ONE) / wavelength as u64;
    let ease_x = smoothstep_q16(fraction_x);
    let ease_y = smoothstep_q16(fraction_y);

    let corner_00 = lattice_value(seed, cell_x, cell_y);
    let corner_10 = lattice_value(seed, cell_x.wrapping_add(1), cell_y);
    let corner_01 = lattice_value(seed, cell_x, cell_y.wrapping_add(1));
    let corner_11 = lattice_value(seed, cell_x.wrapping_add(1), cell_y.wrapping_add(1));

    let top = lerp_q16(corner_00, corner_10, ease_x);
    let bottom = lerp_q16(corner_01, corner_11, ease_x);
    lerp_q16(top, bottom, ease_y)
}

/// Deterministic lattice corner value in `[0, NOISE_ONE)`.
fn lattice_value(seed: u64, cell_x: i64, cell_y: i64) -> u64 {
    hash_world(seed, cell_x, cell_y) >> (64 - NOISE_ONE_BITS)
}

/// Smoothstep `3t^2 - 2t^3` for `t` in Q16, yielding Q16.
fn smoothstep_q16(t: u64) -> u64 {
    (t * t * (3 * NOISE_ONE - 2 * t)) >> (2 * NOISE_ONE_BITS)
}

/// Linear interpolation between Q16 values `a` and `b` by Q16 factor `t`.
fn lerp_q16(a: u64, b: u64, t: u64) -> u64 {
    (a * (NOISE_ONE - t) + b * t) >> NOISE_ONE_BITS
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
    noise_scale: u32,
    noise_octaves: u32,
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
            noise_scale: config.terrain_noise.scale,
            noise_octaves: config.terrain_noise.octaves,
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

    /// Terrain statistics over the configured starting area: how much water
    /// there is and how strongly it clumps (fraction of water tiles with at
    /// least two orthogonal water neighbours — near zero for independent
    /// per-tile rolls, high for coherent lakes).
    fn starting_area_water_stats(seed: u64, catalog: &PrototypeCatalog) -> (f64, f64) {
        let rules = WorldGenRules::from_catalog(catalog);
        let area = catalog.world_generation.starting_area;
        let min_tile = i64::from(area.min_chunk) * i64::from(CHUNK_SIZE);
        let max_tile = (i64::from(area.max_chunk) + 1) * i64::from(CHUNK_SIZE) - 1;

        let is_water = |x: i64, y: i64| {
            let (_, collision) = generate_terrain(seed, x, y, &rules);
            !collision.walkable && !collision.buildable
        };

        let mut total = 0u64;
        let mut water = 0u64;
        let mut clustered = 0u64;
        for y in min_tile..=max_tile {
            for x in min_tile..=max_tile {
                total += 1;
                if !is_water(x, y) {
                    continue;
                }
                water += 1;
                let neighbours = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                    .into_iter()
                    .filter(|&(nx, ny)| is_water(nx, ny))
                    .count();
                if neighbours >= 2 {
                    clustered += 1;
                }
            }
        }

        let water_fraction = water as f64 / total as f64;
        let clustered_fraction = if water == 0 {
            0.0
        } else {
            clustered as f64 / water as f64
        };
        (water_fraction, clustered_fraction)
    }

    #[test]
    fn terrain_water_forms_coherent_lakes() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

        for seed in [0, 42, 123, 8675309] {
            let (water_fraction, clustered_fraction) = starting_area_water_stats(seed, &catalog);

            assert!(
                (0.02..0.45).contains(&water_fraction),
                "seed {seed}: water fraction {water_fraction:.3} outside expected range"
            );
            assert!(
                clustered_fraction > 0.8,
                "seed {seed}: only {clustered_fraction:.3} of water tiles sit in \
                 coherent bodies; terrain looks like salt-and-pepper noise"
            );
        }
    }

    #[test]
    fn terrain_field_is_deterministic_and_seed_dependent() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);
        let coord = ChunkCoord { x: 0, y: 0 };

        assert_eq!(
            generate_chunk(123, coord, &rules),
            generate_chunk(123, coord, &rules)
        );
        assert_ne!(
            generate_chunk(123, coord, &rules),
            generate_chunk(124, coord, &rules)
        );
    }

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
