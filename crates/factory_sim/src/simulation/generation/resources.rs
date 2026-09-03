use super::*;
use factory_data::ResourceDistanceScalingConfig;

pub(in crate::simulation) fn generate_resource_patch_centers(
    seed: u64,
    rules: &WorldGenerator,
    bounds: GenerationBounds,
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

    let grid_size = i64::from(rules.grid_cell_size);
    let min_grid_x = (bounds.min_x - rules.patch_grid_reach).div_euclid(grid_size);
    let max_grid_x = (bounds.max_x + rules.patch_grid_reach).div_euclid(grid_size);
    let min_grid_y = (bounds.min_y - rules.patch_grid_reach).div_euclid(grid_size);
    let max_grid_y = (bounds.max_y + rules.patch_grid_reach).div_euclid(grid_size);

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

/// First rolls the configured total patch chance, then selects exactly one
/// resource rule using each configured `selection_weight` as its relative
/// weight. Density and resource composition are therefore independent, while
/// random patches still cannot compete tile-by-tile within one grid cell.
pub(super) fn resource_patch_center_for_grid_cell(
    seed: u64,
    rules: &WorldGenerator,
    grid_x: WorldTileCoord,
    grid_y: WorldTileCoord,
) -> Option<ResourcePatchCenter> {
    let presence_hash = hash_resource_patch_presence(seed, grid_x, grid_y);
    if presence_hash % 100 >= u64::from(rules.patch_chance_percent) {
        return None;
    }
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

pub(super) fn select_resource_for_grid_cell(
    resources: &[ResourceRule],
    hash: u64,
) -> Option<&ResourceRule> {
    let total_weight: u64 = resources
        .iter()
        .map(|resource| u64::from(resource.selection_weight))
        .sum();
    let mut roll = hash % total_weight.max(1);

    for resource in resources {
        let weight = u64::from(resource.selection_weight);
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

pub(super) fn resource_patch_can_affect_bounds(
    center: ResourcePatchCenter,
    bounds: GenerationBounds,
    edge_noise: i32,
) -> bool {
    let closest_x = center.x.clamp(bounds.min_x, bounds.max_x);
    let closest_y = center.y.clamp(bounds.min_y, bounds.max_y);
    let dx = i128::from(center.x) - i128::from(closest_x);
    let dy = i128::from(center.y) - i128::from(closest_y);
    let reach = i128::from(center.radius + i64::from(edge_noise));

    dx * dx + dy * dy <= reach * reach
}

pub(in crate::simulation) fn resource_at_patch_tile(
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
        if radius <= 0 {
            continue;
        }
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

    best.and_then(|candidate| {
        let radius_sq = u32::try_from(candidate.radius_sq.max(1)).ok()?;
        let distance_sq = u32::try_from(candidate.distance_sq.max(0)).ok()?;
        // Distance-scaled richness can approach u32::MAX, so the amount math
        // runs in u64 and saturates on the way back.
        let falloff = u64::from((radius_sq - distance_sq).max(1));
        let base = u64::from(candidate.center.richness / 3);
        let scaled = u64::from(candidate.center.richness) * falloff / u64::from(radius_sq);

        Some(ResourceCell {
            resource_item: candidate.center.resource_item,
            // Richness should read as a smooth gradient toward the center.
            // The coherent edge field still makes patch outlines organic, but
            // independent per-tile variation would obscure this radial falloff.
            amount: u32::try_from(base + scaled).unwrap_or(u32::MAX),
        })
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
pub(in crate::simulation) fn resource_edge_noise(
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

pub(in crate::simulation) fn hash_resource_center(
    seed: u64,
    grid_x: WorldTileCoord,
    grid_y: WorldTileCoord,
) -> u64 {
    hash_world(seed ^ 0xa24b_aed4_963e_e407, grid_x, grid_y)
}

fn hash_resource_patch_presence(seed: u64, grid_x: WorldTileCoord, grid_y: WorldTileCoord) -> u64 {
    hash_world(seed ^ 0x6c8e_9cf5_7093_2bd1, grid_x, grid_y)
}

#[derive(Clone, Copy)]
pub(in crate::simulation) struct ResourcePatchCenter {
    pub(super) resource_item: ItemId,
    pub(super) x: WorldTileCoord,
    pub(super) y: WorldTileCoord,
    pub(super) radius: i64,
    pub(super) richness: u32,
}

#[derive(Clone, Copy)]
struct ResourceCandidate {
    center: ResourcePatchCenter,
    distance_sq: i128,
    radius_sq: i128,
    score: i128,
}
