use super::*;
use factory_data::{
    ClimateNoiseConfig, ClimateRange, CollisionLayer, CollisionMask, ResourceDistanceScalingConfig,
    ResourceExtraction, TerrainNoiseConfig, item_id_by_name,
};

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
    rules: &WorldGenRules,
    tile_pollution_absorption_per_minute_milli: &[u64],
) -> BTreeMap<ChunkCoord, Chunk> {
    let area = prototypes.world_generation.starting_area;
    let mut chunks = BTreeMap::new();

    for chunk_y in area.min_chunk..=area.max_chunk {
        for chunk_x in area.min_chunk..=area.max_chunk {
            let coord = ChunkCoord {
                x: chunk_x,
                y: chunk_y,
            };
            chunks.insert(
                coord,
                generate_chunk(
                    seed,
                    coord,
                    rules,
                    tile_pollution_absorption_per_minute_milli,
                ),
            );
        }
    }

    chunks
}

pub(super) fn generate_chunk(
    seed: u64,
    coord: ChunkCoord,
    rules: &WorldGenRules,
    tile_pollution_absorption_per_minute_milli: &[u64],
) -> Chunk {
    let mut tiles = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);
    let mut pollution_absorption_per_minute_milli = 0;
    let bounds = TileBounds::for_chunk(coord);
    let centers = generate_resource_patch_centers(seed, rules, bounds);

    for local_y in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            let (x, y) = coord.tile_at(local_x, local_y);
            let tile = generate_tile(seed, x, y, rules, &centers);
            pollution_absorption_per_minute_milli += tile_pollution_absorption_per_minute_milli
                .get(tile.tile_id.index())
                .copied()
                .unwrap_or(0);
            tiles.push(tile);
        }
    }

    Chunk {
        coord,
        tiles,
        pollution_absorption_per_minute_milli,
    }
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
    // Sample three independent climate channels. Elevation drives land vs.
    // water, so the spawn bias clamps it toward buildable land near the origin;
    // moisture and temperature vary freely to distinguish land biomes.
    let elevation_field = climate_field(seed ^ ELEVATION_SALT, x, y, rules.climate_noise.elevation);
    let elevation_field = match &rules.spawn_bias {
        Some(bias) => bias.apply(x, y, elevation_field),
        None => elevation_field,
    };
    let elevation = field_to_percent(elevation_field);
    let moisture = field_to_percent(climate_field(
        seed ^ MOISTURE_SALT,
        x,
        y,
        rules.climate_noise.moisture,
    ));
    let temperature = field_to_percent(climate_field(
        seed ^ TEMPERATURE_SALT,
        x,
        y,
        rules.climate_noise.temperature,
    ));

    // First biome (in declaration order) whose three ranges all contain the
    // sample wins; order encodes priority. No match falls back to the first
    // tile prototype, which is always buildable ground.
    for biome in &rules.biomes {
        if biome.elevation.contains(elevation)
            && biome.moisture.contains(moisture)
            && biome.temperature.contains(temperature)
        {
            return (biome.tile_id, biome.collision);
        }
    }

    (rules.fallback_tile, rules.fallback_collision)
}

/// Sample one climate channel's warped fractal field for a tile.
fn climate_field(
    channel_seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    noise: TerrainNoiseConfig,
) -> u64 {
    warped_terrain_field(channel_seed, x, y, noise.scale, noise.octaves)
}

/// Map a Q16 noise field value in `[0, NOISE_ONE)` to a percent in `0..=99`,
/// the unit biome climate ranges are expressed in.
fn field_to_percent(field: u64) -> u8 {
    ((field * 100) >> NOISE_ONE_BITS) as u8
}

const ELEVATION_SALT: u64 = 0x8f27_9a1e_4c6b_d305;
const MOISTURE_SALT: u64 = 0x2b17_63e0_9d4a_f1c8;
const TEMPERATURE_SALT: u64 = 0xc4e9_50a7_1f82_6db3;

const TERRAIN_FIELD_SALT: u64 = 0x5e2d_58d8_b3bc_e8ee;
const WARP_X_SALT: u64 = 0x3c6e_f372_fe94_f82a;
const WARP_Y_SALT: u64 = 0xd1b5_4a32_d192_ed03;

/// Octave count of the warp fields: low, so offsets bend coastlines in broad
/// curves instead of re-adding the per-tile jitter the coherent field removed.
const WARP_OCTAVES: u32 = 2;

/// Domain-warped terrain field: offsets the sample position by two extra
/// low-octave noise fields before evaluating [`terrain_field`]. Smoothstepped
/// value noise on a square lattice produces visibly axis-aligned, blobby
/// features at chunk scale; warping the input coordinates through independent
/// coherent offsets turns round lakes and straight shores into irregular,
/// winding ones while reusing the same integer-only, per-tile machinery, so
/// generation stays deterministic and chunk-order independent.
pub(super) fn warped_terrain_field(
    seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    scale: u32,
    octaves: u32,
) -> u64 {
    let warp_x = warp_offset(seed ^ WARP_X_SALT, x, y, scale);
    let warp_y = warp_offset(seed ^ WARP_Y_SALT, x, y, scale);
    terrain_field(
        seed ^ TERRAIN_FIELD_SALT,
        x.wrapping_add(warp_x),
        y.wrapping_add(warp_y),
        scale,
        octaves,
    )
}

/// Coherent coordinate offset in `[-scale / 2, scale / 2]` for domain
/// warping. The warp field shares the base wavelength of the terrain field it
/// distorts, and its amplitude of half that wavelength displaces shores by up
/// to a quarter of a feature — enough to break up lattice-aligned blobs
/// without tearing the field's continuity.
fn warp_offset(seed: u64, x: WorldTileCoord, y: WorldTileCoord, scale: u32) -> i64 {
    let amplitude = i64::from(scale / 2);
    if amplitude == 0 {
        return 0;
    }
    let field = terrain_field(seed, x, y, scale, WARP_OCTAVES);
    ((field * (amplitude as u64 * 2 + 1)) >> NOISE_ONE_BITS) as i64 - amplitude
}

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

/// Minimum full-strength spawn bias radius in tiles, so the spawn tile sits
/// on open ground even when no starting patches are configured.
const SPAWN_LAND_MIN_RADIUS: i64 = 8;

/// Radial elevation bias that guarantees open ground around the spawn point.
///
/// Elevation is the land-vs-water climate channel, so biasing it toward land
/// near the origin forces the spawn tile and every starting resource patch onto
/// buildable ground. Within `inner_radius` tiles the elevation field is clamped
/// into `[min_field, max_field]` — the value range of the widest contiguous
/// elevation band that no non-buildable biome occupies, so any biome selected
/// there (or the buildable fallback tile) is walkable+buildable. Between
/// `inner_radius` and `outer_radius` the clamp relaxes linearly back to the
/// full range so the guaranteed land blends into the surrounding coastline.
/// Integer-only, so generation stays deterministic across platforms.
#[derive(Clone, Copy, Debug)]
pub(super) struct SpawnTerrainBias {
    inner_radius: i64,
    outer_radius: i64,
    min_field: u64,
    max_field: u64,
}

impl SpawnTerrainBias {
    fn derive(
        biomes: &[BiomeRule],
        resources: &[ResourceRule],
        edge_noise: i32,
        elevation_scale: u32,
    ) -> Option<Self> {
        // Elevation percents (0..=99) blocked by a non-buildable biome. Clamping
        // into a gap between these guarantees the spawn resolves to a buildable
        // biome or the buildable fallback tile.
        let mut blocked = [false; 100];
        for biome in biomes {
            if biome.collision.walkable && biome.collision.buildable {
                continue;
            }
            let lo = usize::from(biome.elevation.min).min(blocked.len());
            let hi = usize::from(biome.elevation.max).min(blocked.len());
            for cell in &mut blocked[lo..hi] {
                *cell = true;
            }
        }

        // Widest contiguous run of unblocked elevation percents [lo, hi).
        let mut best: Option<(u64, u64)> = None;
        let mut run_start: Option<usize> = None;
        for percent in 0..=blocked.len() {
            let open = percent < blocked.len() && !blocked[percent];
            match (open, run_start) {
                (true, None) => run_start = Some(percent),
                (false, Some(start)) => {
                    let (start, end) = (start as u64, percent as u64);
                    if best.is_none_or(|(bstart, bend)| bend - bstart < end - start) {
                        best = Some((start, end));
                    }
                    run_start = None;
                }
                _ => {}
            }
        }
        let (lo, hi) = best?;

        // Tightest Q16 elevation field range whose percent stays inside [lo, hi):
        // percent(field) = (field * 100) >> NOISE_ONE_BITS.
        let min_field = (lo << NOISE_ONE_BITS).div_ceil(100);
        let max_field = (((hi << NOISE_ONE_BITS) - 1) / 100).min(NOISE_ONE - 1);
        if min_field > max_field {
            return None;
        }

        // The full-strength zone must reach past every starting patch's noisy
        // edge; the fade band beyond it spans one base noise wavelength.
        let mut inner_radius = SPAWN_LAND_MIN_RADIUS;
        for resource in resources {
            let Some((x, y)) = resource.starting_patch else {
                continue;
            };
            let distance_sq =
                (i128::from(x) * i128::from(x) + i128::from(y) * i128::from(y)) as u128;
            let mut distance = distance_sq.isqrt() as i64;
            if (distance as u128) * (distance as u128) < distance_sq {
                distance += 1;
            }
            inner_radius = inner_radius.max(distance + resource.radius + i64::from(edge_noise));
        }
        let outer_radius = inner_radius + i64::from(elevation_scale.max(1));

        Some(Self {
            inner_radius,
            outer_radius,
            min_field,
            max_field,
        })
    }

    fn apply(&self, x: WorldTileCoord, y: WorldTileCoord, field: u64) -> u64 {
        let dx = i128::from(x);
        let dy = i128::from(y);
        let distance_sq = (dx * dx + dy * dy) as u128;
        let outer = self.outer_radius as u128;
        if distance_sq >= outer * outer {
            return field;
        }

        // Q16 clamp strength: full inside the inner radius, fading linearly
        // to zero at the outer radius.
        let distance = distance_sq.isqrt() as i64;
        let strength = if distance <= self.inner_radius {
            NOISE_ONE
        } else {
            ((self.outer_radius - distance) as u64 * NOISE_ONE)
                / (self.outer_radius - self.inner_radius) as u64
        };
        let lower = (self.min_field * strength) >> NOISE_ONE_BITS;
        let upper =
            (NOISE_ONE - 1) - (((NOISE_ONE - 1 - self.max_field) * strength) >> NOISE_ONE_BITS);
        field.clamp(lower, upper)
    }
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
        + i64::from(rules.edge_noise)
        + i64::from(rules.grid_jitter)
        + rules
            .distance_scaling
            .map_or(0, |scaling| i64::from(scaling.max_radius_bonus_tiles));
    let grid_size = i64::from(rules.grid_cell_size);
    let min_grid_x = (bounds.min_x - max_reach).div_euclid(grid_size);
    let max_grid_x = (bounds.max_x + max_reach).div_euclid(grid_size);
    let min_grid_y = (bounds.min_y - max_reach).div_euclid(grid_size);
    let max_grid_y = (bounds.max_y + max_reach).div_euclid(grid_size);

    for grid_y in min_grid_y..=max_grid_y {
        for grid_x in min_grid_x..=max_grid_x {
            let Some(center) = resource_patch_center_for_grid_cell(seed, rules, grid_x, grid_y)
            else {
                continue;
            };
            if resource_patch_can_affect_bounds(center, bounds, rules.edge_noise) {
                centers.push(center);
            }
        }
    }

    centers
}

/// Selects exactly one resource rule for a grid cell, using each configured
/// `frequency_percent` as its relative weight. This keeps random patches from
/// different resources sharing a cell and competing tile-by-tile at their
/// overlapping borders.
fn resource_patch_center_for_grid_cell(
    seed: u64,
    rules: &WorldGenRules,
    grid_x: WorldTileCoord,
    grid_y: WorldTileCoord,
) -> Option<ResourcePatchCenter> {
    let hash = hash_resource_center(seed, grid_x, grid_y);
    let resource = select_resource_for_grid_cell(&rules.resources, hash)?;
    let jitter_diameter = i64::from(rules.grid_jitter) * 2 + 1;
    let jitter_x =
        ((hash & 0xFFFF_FFFF) % jitter_diameter as u64) as i64 - i64::from(rules.grid_jitter);
    let jitter_y = ((hash >> 32) % jitter_diameter as u64) as i64 - i64::from(rules.grid_jitter);
    let grid_size = i64::from(rules.grid_cell_size);
    let x = grid_x * grid_size + grid_size / 2 + jitter_x;
    let y = grid_y * grid_size + grid_size / 2 + jitter_y;
    let (radius, richness) = match &rules.distance_scaling {
        Some(scaling) => scale_patch_with_distance(scaling, resource, x, y),
        None => (resource.radius, resource.richness),
    };

    Some(ResourcePatchCenter {
        resource_item: resource.resource_item,
        x,
        y,
        radius,
        richness,
    })
}

fn select_resource_for_grid_cell(resources: &[ResourceRule], hash: u64) -> Option<&ResourceRule> {
    let total_weight: u64 = resources
        .iter()
        .map(|resource| u64::from(resource.frequency_percent))
        .sum();
    let mut roll = hash % total_weight.max(1);

    for resource in resources {
        let weight = u64::from(resource.frequency_percent);
        if roll < weight {
            return Some(resource);
        }
        roll -= weight;
    }

    None
}

/// Radius and richness of a grid patch centered at `(x, y)` after distance
/// scaling: per `interval_tiles` of distance from the world origin the patch
/// gains `richness_bonus_percent` of its base richness and
/// `radius_bonus_tiles` of radius, the latter capped at
/// `max_radius_bonus_tiles` (which also bounds the center scan reach).
/// Integer-only so generation stays deterministic across platforms.
fn scale_patch_with_distance(
    scaling: &ResourceDistanceScalingConfig,
    resource: &ResourceRule,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> (i64, u32) {
    let distance_sq = (i128::from(x) * i128::from(x) + i128::from(y) * i128::from(y)) as u128;
    let distance = distance_sq.isqrt();
    let interval = u128::from(scaling.interval_tiles.max(1));

    let radius_bonus = (distance * u128::from(scaling.radius_bonus_tiles) / interval)
        .min(u128::from(scaling.max_radius_bonus_tiles)) as i64;
    let richness_bonus =
        u128::from(resource.richness) * u128::from(scaling.richness_bonus_percent) * distance
            / (interval * 100);
    let richness = u128::from(resource.richness) + richness_bonus;

    (
        resource.radius + radius_bonus,
        u32::try_from(richness).unwrap_or(u32::MAX),
    )
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
    let reach = i128::from(center.radius + i64::from(edge_noise));

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
        let radius = center.radius
            + i64::from(resource_edge_noise(
                seed,
                x,
                y,
                center.resource_item,
                edge_noise,
            ));
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
        // Distance-scaled richness can approach u32::MAX, so the amount math
        // runs in u64 and saturates on the way back.
        let falloff = u64::from((radius_sq - distance_sq).max(1));
        let base = u64::from(candidate.center.richness / 3);
        let scaled = u64::from(candidate.center.richness) * falloff / u64::from(radius_sq);

        ResourceCell {
            resource_item: candidate.center.resource_item,
            // Richness should read as a smooth gradient toward the center.
            // The coherent edge field still makes patch outlines organic, but
            // independent per-tile variation would obscure this radial falloff.
            amount: u32::try_from(base + scaled).unwrap_or(u32::MAX),
        }
    })
}

const RESOURCE_EDGE_SALT: u64 = 0x7b5d_1f25_8c92_f6a3;

/// Wavelength and octave count of the patch edge field: small enough that a
/// single patch grows several lobes, coarse enough that neighbouring tiles
/// agree instead of flickering per tile.
const RESOURCE_EDGE_SCALE: u32 = 8;
const RESOURCE_EDGE_OCTAVES: u32 = 2;

/// Coherent radius offset in `[-edge_noise, edge_noise]` for a resource patch
/// boundary. Samples a small-scale [`terrain_field`] salted per resource, so
/// patch outlines bulge in organic lobes rather than single-tile fuzz —
/// the same fix the terrain bands use against salt-and-pepper noise.
pub(super) fn resource_edge_noise(
    seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    resource_item: ItemId,
    edge_noise: i32,
) -> i32 {
    if edge_noise <= 0 {
        return 0;
    }
    let field = terrain_field(
        seed ^ RESOURCE_EDGE_SALT ^ u64::from(resource_item.raw()),
        x,
        y,
        RESOURCE_EDGE_SCALE,
        RESOURCE_EDGE_OCTAVES,
    );
    ((field * (edge_noise as u64 * 2 + 1)) >> NOISE_ONE_BITS) as i32 - edge_noise
}

pub(super) fn hash_resource_center(
    seed: u64,
    grid_x: WorldTileCoord,
    grid_y: WorldTileCoord,
) -> u64 {
    hash_world(seed ^ 0xa24b_aed4_963e_e407, grid_x, grid_y)
}

#[derive(Clone, Copy)]
pub(super) struct ResourcePatchCenter {
    resource_item: ItemId,
    x: WorldTileCoord,
    y: WorldTileCoord,
    radius: i64,
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
pub(crate) struct WorldGenRules {
    biomes: Vec<BiomeRule>,
    climate_noise: ClimateNoiseConfig,
    /// Tile used when no biome matches (or the biome table is empty).
    fallback_tile: TileId,
    fallback_collision: TileCollision,
    resources: Vec<ResourceRule>,
    grid_cell_size: i32,
    grid_jitter: i32,
    edge_noise: i32,
    /// Distance-based richness/radius growth for grid patches; starting
    /// patches stay at their configured base values.
    distance_scaling: Option<ResourceDistanceScalingConfig>,
    /// Derived from the biome table and starting patches; `None` when every
    /// biome is buildable so no elevation bias is needed.
    spawn_bias: Option<SpawnTerrainBias>,
}

#[derive(Clone, Copy, Debug)]
struct BiomeRule {
    tile_id: TileId,
    collision: TileCollision,
    elevation: ClimateRange,
    moisture: ClimateRange,
    temperature: ClimateRange,
}

#[derive(Clone, Copy, Debug)]
struct ResourceRule {
    resource_item: ItemId,
    minable: bool,
    frequency_percent: u8,
    radius: i64,
    richness: u32,
    starting_patch: Option<(WorldTileCoord, WorldTileCoord)>,
}

impl WorldGenRules {
    pub(super) fn from_catalog(prototypes: &PrototypeCatalog) -> Self {
        let config = &prototypes.world_generation;
        let biomes: Vec<BiomeRule> = config
            .biomes
            .iter()
            .map(|biome| BiomeRule {
                tile_id: biome.tile,
                collision: tile_collision(prototypes, biome.tile),
                elevation: biome.elevation,
                moisture: biome.moisture,
                temperature: biome.temperature,
            })
            .collect();
        let (fallback_tile, fallback_collision) = prototypes
            .tiles
            .first()
            .map(|tile| (tile.id, collision_from_mask(&tile.collision_mask)))
            .unwrap_or_else(|| (TileId::new(0), ground_collision()));
        let resources: Vec<ResourceRule> = config
            .resources
            .iter()
            .map(|resource| ResourceRule {
                resource_item: resource.resource_item,
                minable: resource.extraction == ResourceExtraction::Solid,
                frequency_percent: resource.frequency_percent,
                radius: i64::from(resource.radius),
                richness: resource.richness,
                starting_patch: resource
                    .starting_patch
                    .map(|offset| (i64::from(offset.x), i64::from(offset.y))),
            })
            .collect();
        let spawn_bias = SpawnTerrainBias::derive(
            &biomes,
            &resources,
            config.patch_grid.edge_noise,
            config.climate_noise.elevation.scale,
        );

        Self {
            biomes,
            climate_noise: config.climate_noise,
            fallback_tile,
            fallback_collision,
            resources,
            grid_cell_size: config.patch_grid.cell_size,
            grid_jitter: config.patch_grid.jitter,
            edge_noise: config.patch_grid.edge_noise,
            distance_scaling: config.distance_scaling,
            spawn_bias,
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

pub(super) fn item_is_ammo(catalog: &PrototypeCatalog, item_id: ItemId) -> bool {
    catalog
        .item(item_id)
        .is_some_and(|prototype| prototype.ammo.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Terrain statistics over a broad region with the spawn-bias disk around
    /// the origin excluded, so it measures the unbiased biome distribution:
    /// how much water there is and how strongly it clumps (fraction of water
    /// tiles with at least two orthogonal water neighbours — near zero for
    /// independent per-tile rolls, high for coherent lakes). Sampling a large
    /// area rather than one window keeps the fraction stable across seeds
    /// instead of landing on a single continent or basin.
    fn natural_water_stats(seed: u64, catalog: &PrototypeCatalog) -> (f64, f64) {
        let rules = WorldGenRules::from_catalog(catalog);
        // Comfortably beyond the spawn elevation bias's outer radius.
        const SPAWN_EXCLUSION: i64 = 160;
        let half_extent = 256;

        let is_water = |x: i64, y: i64| {
            let (_, collision) = generate_terrain(seed, x, y, &rules);
            !collision.walkable && !collision.buildable
        };

        let mut total = 0u64;
        let mut water = 0u64;
        let mut clustered = 0u64;
        for y in -half_extent..=half_extent {
            for x in -half_extent..=half_extent {
                if x * x + y * y <= SPAWN_EXCLUSION * SPAWN_EXCLUSION {
                    continue;
                }
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
            let (water_fraction, clustered_fraction) = natural_water_stats(seed, &catalog);

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
    fn climate_channels_are_independent() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let noise = catalog.world_generation.climate_noise;

        for seed in [0u64, 42, 123] {
            let mut elevation_eq_moisture = 0u64;
            let mut elevation_eq_temperature = 0u64;
            let mut total = 0u64;
            for y in -48..48i64 {
                for x in -48..48i64 {
                    let elevation = field_to_percent(climate_field(
                        seed ^ ELEVATION_SALT,
                        x,
                        y,
                        noise.elevation,
                    ));
                    let moisture =
                        field_to_percent(climate_field(seed ^ MOISTURE_SALT, x, y, noise.moisture));
                    let temperature = field_to_percent(climate_field(
                        seed ^ TEMPERATURE_SALT,
                        x,
                        y,
                        noise.temperature,
                    ));
                    total += 1;
                    if elevation == moisture {
                        elevation_eq_moisture += 1;
                    }
                    if elevation == temperature {
                        elevation_eq_temperature += 1;
                    }
                }
            }

            // Independent channels drawn from distinct salts rarely land in the
            // same percent bucket; identical channels would match every tile.
            let moisture_agreement = elevation_eq_moisture as f64 / total as f64;
            let temperature_agreement = elevation_eq_temperature as f64 / total as f64;
            assert!(
                moisture_agreement < 0.2,
                "seed {seed}: elevation and moisture agree on {moisture_agreement:.3} of tiles; \
                 the channels are not independent"
            );
            assert!(
                temperature_agreement < 0.2,
                "seed {seed}: elevation and temperature agree on {temperature_agreement:.3} of \
                 tiles; the channels are not independent"
            );
        }
    }

    #[test]
    fn biome_table_produces_varied_terrain() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);

        for seed in [0u64, 42, 123, 8675309] {
            let mut tiles = std::collections::BTreeSet::new();
            for y in -96..96i64 {
                for x in -96..96i64 {
                    let (tile_id, _) = generate_terrain(seed, x, y, &rules);
                    tiles.insert(tile_id);
                }
            }
            assert!(
                tiles.len() >= 4,
                "seed {seed}: only {} distinct biomes appear over the sample area; the climate \
                 table is not producing variety",
                tiles.len()
            );
        }
    }

    #[test]
    fn biome_selection_is_deterministic_per_seed() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);

        for &(x, y) in &[(0i64, 0i64), (17, -33), (-64, 50), (120, -8)] {
            let first = generate_terrain(777, x, y, &rules);
            let second = generate_terrain(777, x, y, &rules);
            assert_eq!(
                first.0, second.0,
                "generate_terrain must be deterministic at ({x}, {y})"
            );
        }
    }

    #[test]
    fn spawn_area_terrain_is_open_ground_for_any_seed() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);
        let bias = rules
            .spawn_bias
            .expect("base catalog should derive a spawn bias");
        let radius = bias.inner_radius;

        for seed in [0, 42, 123, 8675309, 0xdead_beef] {
            for y in -radius..=radius {
                for x in -radius..=radius {
                    if x * x + y * y > radius * radius {
                        continue;
                    }
                    let (_, collision) = generate_terrain(seed, x, y, &rules);
                    assert!(
                        collision.walkable && collision.buildable,
                        "seed {seed}: tile ({x}, {y}) inside the spawn bias radius \
                         {radius} is not open ground"
                    );
                }
            }
        }
    }

    #[test]
    fn starting_patches_generate_their_resource_for_any_seed() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);
        let starting_patches: Vec<_> = catalog
            .world_generation
            .resources
            .iter()
            .filter_map(|resource| resource.starting_patch)
            .collect();
        assert!(
            !starting_patches.is_empty(),
            "base catalog should configure starting patches"
        );

        for seed in [0, 42, 123, 8675309, 0xdead_beef] {
            for &offset in &starting_patches {
                let (x, y) = (i64::from(offset.x), i64::from(offset.y));
                let coord = ChunkCoord::from_tile(x, y)
                    .expect("starting patch centers are within chunk range");
                let bounds = TileBounds::for_chunk(coord);
                let centers = generate_resource_patch_centers(seed, &rules, bounds);
                let tile = generate_tile(seed, x, y, &rules, &centers);

                // An overlapping random patch may win the tile, but the spawn
                // bias guarantees some resource generates here instead of the
                // patch drowning in a lake.
                assert!(
                    tile.resource.is_some(),
                    "seed {seed}: no resource at starting patch center ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn resource_edge_noise_is_coherent_across_neighbouring_tiles() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);
        let resource = rules
            .resources
            .first()
            .expect("base catalog should configure resources");

        for seed in [0, 42, 123, 8675309] {
            let mut pairs = 0u64;
            let mut equal = 0u64;
            let mut seen_min = i32::MAX;
            let mut seen_max = i32::MIN;
            for y in -96..96i64 {
                for x in -96..96i64 {
                    let noise =
                        resource_edge_noise(seed, x, y, resource.resource_item, rules.edge_noise);
                    assert!(
                        (-rules.edge_noise..=rules.edge_noise).contains(&noise),
                        "seed {seed}: edge noise {noise} at ({x}, {y}) outside \
                         [-{0}, {0}]",
                        rules.edge_noise
                    );
                    seen_min = seen_min.min(noise);
                    seen_max = seen_max.max(noise);
                    let right = resource_edge_noise(
                        seed,
                        x + 1,
                        y,
                        resource.resource_item,
                        rules.edge_noise,
                    );
                    pairs += 1;
                    if noise == right {
                        equal += 1;
                    }
                }
            }

            // Independent per-tile hashing agrees with its neighbour about
            // 1/(2*edge_noise+1) of the time (~14% for edge_noise 3); a
            // coherent field agrees far more often.
            let equal_fraction = equal as f64 / pairs as f64;
            assert!(
                equal_fraction > 0.5,
                "seed {seed}: only {equal_fraction:.3} of neighbouring tiles share an \
                 edge offset; patch borders look like per-tile fuzz"
            );
            assert!(
                seen_max - seen_min >= rules.edge_noise,
                "seed {seed}: edge noise spread [{seen_min}, {seen_max}] is too flat \
                 to shape patch outlines"
            );
        }
    }

    #[test]
    fn resource_richness_falls_smoothly_from_patch_center() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let resource_item = catalog
            .world_generation
            .resources
            .first()
            .expect("base catalog should configure resources")
            .resource_item;
        let centers = [ResourcePatchCenter {
            resource_item,
            x: 0,
            y: 0,
            radius: 10,
            richness: 300,
        }];

        let amounts: Vec<_> = [0, 1, 5, 9]
            .into_iter()
            .map(|x| {
                resource_at_patch_tile(123, x, 0, &centers, 0)
                    .expect("tile should be inside the patch")
                    .amount
            })
            .collect();

        assert_eq!(amounts[0], 400);
        assert!(
            amounts.windows(2).all(|pair| pair[0] > pair[1]),
            "resource amounts should decrease monotonically from the center: {amounts:?}"
        );
    }

    #[test]
    fn warp_offsets_are_coherent_and_span_their_range() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);
        let amplitude = i64::from(rules.climate_noise.elevation.scale / 2);
        assert!(
            amplitude > 0,
            "base catalog noise scale should be large enough to warp"
        );

        for seed in [0, 42, 123, 8675309] {
            let warp_seed = seed ^ WARP_X_SALT;
            let mut pairs = 0u64;
            let mut equal = 0u64;
            let mut seen_min = i64::MAX;
            let mut seen_max = i64::MIN;
            for y in -96..96i64 {
                for x in -96..96i64 {
                    let offset = warp_offset(warp_seed, x, y, rules.climate_noise.elevation.scale);
                    assert!(
                        (-amplitude..=amplitude).contains(&offset),
                        "seed {seed}: warp offset {offset} at ({x}, {y}) outside \
                         [-{amplitude}, {amplitude}]"
                    );
                    seen_min = seen_min.min(offset);
                    seen_max = seen_max.max(offset);
                    let right =
                        warp_offset(warp_seed, x + 1, y, rules.climate_noise.elevation.scale);
                    pairs += 1;
                    if offset == right {
                        equal += 1;
                    }
                }
            }

            // Independent per-tile hashing agrees with its neighbour about
            // 1/(2*amplitude+1) of the time; a coherent warp field agrees far
            // more often, which is what keeps warped shores connected.
            let equal_fraction = equal as f64 / pairs as f64;
            assert!(
                equal_fraction > 0.5,
                "seed {seed}: only {equal_fraction:.3} of neighbouring tiles share a \
                 warp offset; the warp field is not coherent"
            );
            assert!(
                seen_max - seen_min >= amplitude,
                "seed {seed}: warp offset spread [{seen_min}, {seen_max}] is too flat \
                 to reshape coastlines"
            );
        }
    }

    #[test]
    fn domain_warp_displaces_the_terrain_field() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);

        for seed in [0, 42, 123, 8675309] {
            let mut total = 0u64;
            let mut moved = 0u64;
            for y in -96..96i64 {
                for x in -96..96i64 {
                    let unwarped = terrain_field(
                        seed ^ TERRAIN_FIELD_SALT,
                        x,
                        y,
                        rules.climate_noise.elevation.scale,
                        rules.climate_noise.elevation.octaves,
                    );
                    let warped = warped_terrain_field(
                        seed,
                        x,
                        y,
                        rules.climate_noise.elevation.scale,
                        rules.climate_noise.elevation.octaves,
                    );
                    total += 1;
                    if warped != unwarped {
                        moved += 1;
                    }
                }
            }

            let moved_fraction = moved as f64 / total as f64;
            assert!(
                moved_fraction > 0.9,
                "seed {seed}: only {moved_fraction:.3} of tiles changed under domain \
                 warping; the warp is a near no-op"
            );
        }
    }

    #[test]
    fn terrain_field_is_deterministic_and_seed_dependent() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);
        let absorption_rates = WorldSim::tile_pollution_absorption_rates(&catalog);
        let coord = ChunkCoord { x: 0, y: 0 };

        assert_eq!(
            generate_chunk(123, coord, &rules, &absorption_rates),
            generate_chunk(123, coord, &rules, &absorption_rates)
        );
        assert_ne!(
            generate_chunk(123, coord, &rules, &absorption_rates),
            generate_chunk(124, coord, &rules, &absorption_rates)
        );
    }

    #[test]
    fn grid_cells_select_one_resource_weighted_by_frequency() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);
        let mut counts = vec![0u64; rules.resources.len()];
        let mut total = 0u64;

        for grid_y in -64..64 {
            for grid_x in -64..64 {
                let hash = hash_resource_center(123, grid_x, grid_y);
                let selected = select_resource_for_grid_cell(&rules.resources, hash)
                    .expect("base resource frequencies should select one resource per grid cell");
                let center = resource_patch_center_for_grid_cell(123, &rules, grid_x, grid_y)
                    .expect("selected grid resource should produce a patch center");
                assert_eq!(center.resource_item, selected.resource_item);

                let index = rules
                    .resources
                    .iter()
                    .position(|resource| resource.resource_item == selected.resource_item)
                    .expect("selected resource should be configured");
                counts[index] += 1;
                total += 1;
            }
        }

        let total_weight: u64 = rules
            .resources
            .iter()
            .map(|resource| u64::from(resource.frequency_percent))
            .sum();
        for (resource, count) in rules.resources.iter().zip(counts) {
            let expected = f64::from(resource.frequency_percent) / total_weight as f64;
            let actual = count as f64 / total as f64;
            assert!(
                (actual - expected).abs() < 0.025,
                "resource {:?} selected at {actual:.3}, expected about {expected:.3}",
                resource.resource_item
            );
        }
    }

    #[test]
    fn grid_patch_richness_and_radius_scale_with_distance() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);
        let scaling = catalog
            .world_generation
            .distance_scaling
            .expect("base catalog should configure distance scaling");

        // Far enough out that no starting patch reaches the chunk and every
        // relevant patch center sits past the radius bonus cap.
        let bounds = TileBounds::for_chunk(ChunkCoord { x: 20, y: 20 });
        let centers = generate_resource_patch_centers(123, &rules, bounds);
        assert!(
            !centers.is_empty(),
            "expected grid patches in the far chunk"
        );

        for center in &centers {
            let base = rules
                .resources
                .iter()
                .find(|resource| resource.resource_item == center.resource_item)
                .expect("center resource should be configured");
            assert!(
                center.richness > base.richness * 2,
                "richness {} at ({}, {}) should be well above base {}",
                center.richness,
                center.x,
                center.y,
                base.richness
            );
            assert_eq!(
                center.radius,
                base.radius + i64::from(scaling.max_radius_bonus_tiles),
                "radius bonus at ({}, {}) should be capped",
                center.x,
                center.y
            );
        }
    }

    #[test]
    fn starting_patches_keep_base_richness_and_radius() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);

        for resource in &rules.resources {
            let Some((x, y)) = resource.starting_patch else {
                continue;
            };
            let coord =
                ChunkCoord::from_tile(x, y).expect("starting patch centers are within chunk range");
            let bounds = TileBounds::for_chunk(coord);
            let centers = generate_resource_patch_centers(123, &rules, bounds);
            let center = centers
                .iter()
                .find(|center| {
                    center.resource_item == resource.resource_item && center.x == x && center.y == y
                })
                .expect("starting patch center should be generated");

            assert_eq!(center.radius, resource.radius);
            assert_eq!(center.richness, resource.richness);
        }
    }

    #[test]
    fn distance_scaled_patches_do_not_create_chunk_seams() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let rules = WorldGenRules::from_catalog(&catalog);

        for seed in [0, 123] {
            for coord in [ChunkCoord { x: 15, y: 0 }, ChunkCoord { x: -20, y: 20 }] {
                let bounds = TileBounds::for_chunk(coord);
                // Centers from a scan wide enough to include everything that
                // could possibly reach the chunk; any tile that differs means
                // the per-chunk scan missed a relevant center.
                let margin = i64::from(rules.grid_cell_size) * 3;
                let expanded = TileBounds {
                    min_x: bounds.min_x - margin,
                    max_x: bounds.max_x + margin,
                    min_y: bounds.min_y - margin,
                    max_y: bounds.max_y + margin,
                };
                let chunk_centers = generate_resource_patch_centers(seed, &rules, bounds);
                let expanded_centers = generate_resource_patch_centers(seed, &rules, expanded);

                for y in bounds.min_y..=bounds.max_y {
                    for x in bounds.min_x..=bounds.max_x {
                        assert_eq!(
                            resource_at_patch_tile(seed, x, y, &chunk_centers, rules.edge_noise),
                            resource_at_patch_tile(seed, x, y, &expanded_centers, rules.edge_noise),
                            "seed {seed}: tile ({x}, {y}) differs between per-chunk \
                             and expanded center scans"
                        );
                    }
                }
            }
        }
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
