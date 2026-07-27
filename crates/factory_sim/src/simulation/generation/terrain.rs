use super::*;
use factory_data::{CollisionLayer, CollisionMask, TerrainNoiseConfig};

pub(in crate::simulation) fn generate_terrain(
    seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    rules: &WorldGenerator,
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
pub(super) fn climate_field(
    channel_seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    noise: TerrainNoiseConfig,
) -> u64 {
    warped_terrain_field(channel_seed, x, y, noise.scale, noise.octaves)
}

/// Map a Q16 noise field value in `[0, NOISE_ONE)` to a percent in `0..=99`,
/// the unit biome climate ranges are expressed in.
pub(super) fn field_to_percent(field: u64) -> u8 {
    ((field * 100) >> NOISE_ONE_BITS) as u8
}

pub(super) const ELEVATION_SALT: u64 = 0x8f27_9a1e_4c6b_d305;
pub(super) const MOISTURE_SALT: u64 = 0x2b17_63e0_9d4a_f1c8;
pub(super) const TEMPERATURE_SALT: u64 = 0xc4e9_50a7_1f82_6db3;

pub(super) const TERRAIN_FIELD_SALT: u64 = 0x5e2d_58d8_b3bc_e8ee;
pub(super) const WARP_X_SALT: u64 = 0x3c6e_f372_fe94_f82a;
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
pub(in crate::simulation) fn warped_terrain_field(
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
pub(super) fn warp_offset(seed: u64, x: WorldTileCoord, y: WorldTileCoord, scale: u32) -> i64 {
    let amplitude = i64::from(scale / 2);
    if amplitude == 0 {
        return 0;
    }
    let field = terrain_field(seed, x, y, scale, WARP_OCTAVES);
    ((field * (amplitude as u64 * 2 + 1)) >> NOISE_ONE_BITS) as i64 - amplitude
}

/// Q16 fixed-point one for the noise field; noise values live in
/// `[0, NOISE_ONE)`.
pub(super) const NOISE_ONE_BITS: u32 = 16;
const NOISE_ONE: u64 = 1 << NOISE_ONE_BITS;

/// Fractal value noise in `[0, NOISE_ONE)`: `octaves` layers of
/// lattice-interpolated [`hash_world`] values, each octave halving both the
/// wavelength (starting at `scale` tiles) and the amplitude. Integer-only so
/// results are identical across platforms for a given seed.
pub(in crate::simulation) fn terrain_field(
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
pub(in crate::simulation) struct SpawnTerrainBias {
    pub(super) inner_radius: i64,
    outer_radius: i64,
    min_field: u64,
    max_field: u64,
}

impl SpawnTerrainBias {
    pub(super) fn derive(
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

    pub(in crate::simulation) fn apply(
        &self,
        x: WorldTileCoord,
        y: WorldTileCoord,
        field: u64,
    ) -> u64 {
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

pub(in crate::simulation) fn ground_collision() -> TileCollision {
    TileCollision {
        walkable: true,
        buildable: true,
        minable: false,
    }
}

/// Terrain collision behaviour derived from a tile prototype's collision
/// mask: water-layer tiles block movement and building, everything else is
/// open ground.
pub(in crate::simulation) fn collision_from_mask(mask: &CollisionMask) -> TileCollision {
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

pub(in crate::simulation) fn tile_collision(
    prototypes: &PrototypeCatalog,
    tile_id: TileId,
) -> TileCollision {
    prototypes
        .tile(tile_id)
        .map(|tile| collision_from_mask(&tile.collision_mask))
        .unwrap_or_else(ground_collision)
}
