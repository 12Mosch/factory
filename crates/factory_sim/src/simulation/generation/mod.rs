use super::*;
use factory_data::{
    ClimateNoiseConfig, ClimateRange, ResourceDistanceScalingConfig, ResourceExtraction,
};

mod catalog;
mod chunks;
mod hashing;
mod resources;
mod terrain;

pub(super) use catalog::*;
pub(super) use chunks::*;
pub(super) use hashing::*;
pub(super) use resources::*;
pub(super) use terrain::*;

/// Runtime-only world generation state resolved from a prototype catalog.
///
/// This owns terrain bands with derived collision, resource patch rules and
/// minability, patch-grid reach, spawn bias, and pollution absorption. The
/// loader already validated the source configuration, so resolution is
/// infallible and streaming chunks never rebuilds catalog-derived rules.
#[derive(Clone, Debug)]
pub(crate) struct WorldGenerator {
    biomes: Vec<BiomeRule>,
    climate_noise: ClimateNoiseConfig,
    /// Tile used when no biome matches (or the biome table is empty).
    fallback_tile: TileId,
    fallback_collision: TileCollision,
    resources: Vec<ResourceRule>,
    resource_minability: Vec<bool>,
    grid_cell_size: i32,
    grid_jitter: i32,
    edge_noise: i32,
    patch_chance_percent: u8,
    /// Maximum distance from a chunk's bounds at which a patch grid cell can
    /// still affect that chunk, including jitter and distance scaling.
    patch_grid_reach: i64,
    /// Distance-based richness/radius growth for grid patches; starting
    /// patches stay at their configured base values.
    distance_scaling: Option<ResourceDistanceScalingConfig>,
    /// Derived from the biome table and starting patches; `None` when every
    /// biome is buildable so no elevation bias is needed.
    spawn_bias: Option<SpawnTerrainBias>,
    tile_pollution_absorption_per_minute_milli: Vec<u64>,
    /// Per-`TileId` walking speed percentages, indexed like the absorption
    /// table so the player movement step is a table lookup.
    tile_walking_speed_percent: Vec<u16>,
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
    selection_weight: u32,
    radius: i64,
    richness: u32,
    starting_patch: Option<(WorldTileCoord, WorldTileCoord)>,
}

impl WorldGenerator {
    pub(super) fn from_catalog(prototypes: &PrototypeCatalog) -> Self {
        let config = &prototypes.world_generation();
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
            .tiles()
            .first()
            .map(|tile| (tile.id, collision_from_mask(&tile.collision_mask)))
            .unwrap_or_else(|| (TileId::new(0), ground_collision()));
        let resources: Vec<ResourceRule> = config
            .resources
            .iter()
            .map(|resource| ResourceRule {
                resource_item: resource.resource_item,
                minable: resource.extraction == ResourceExtraction::Solid,
                selection_weight: resource.selection_weight,
                radius: i64::from(resource.radius),
                richness: resource.richness,
                starting_patch: resource
                    .starting_patch
                    .map(|offset| (i64::from(offset.x), i64::from(offset.y))),
            })
            .collect();
        let mut resource_minability = vec![false; prototypes.items().len()];
        // Validated catalogs have unique resource items. Iterate backwards to
        // preserve the old first-match behavior for catalogs mutated directly
        // by callers after validation.
        for resource in resources.iter().rev() {
            if let Some(minable) = resource_minability.get_mut(resource.resource_item.index()) {
                *minable = resource.minable;
            }
        }
        let spawn_bias = SpawnTerrainBias::derive(
            &biomes,
            &resources,
            config.patch_grid.edge_noise,
            config.climate_noise.elevation.scale,
        );
        let patch_grid_reach = resources
            .iter()
            .map(|resource| resource.radius)
            .max()
            .unwrap_or(0)
            + i64::from(config.patch_grid.edge_noise)
            + i64::from(config.patch_grid.jitter)
            + config
                .distance_scaling
                .map_or(0, |scaling| i64::from(scaling.max_radius_bonus_tiles));
        let tile_pollution_absorption_per_minute_milli = prototypes
            .tiles()
            .iter()
            .map(|tile| u64::from(tile.pollution_absorption_per_minute_milli))
            .collect();
        let tile_walking_speed_percent = prototypes
            .tiles()
            .iter()
            .map(|tile| tile.walking_speed_percent)
            .collect();

        Self {
            biomes,
            climate_noise: config.climate_noise,
            fallback_tile,
            fallback_collision,
            resources,
            resource_minability,
            grid_cell_size: config.patch_grid.cell_size,
            grid_jitter: config.patch_grid.jitter,
            edge_noise: config.patch_grid.edge_noise,
            patch_chance_percent: config.patch_grid.patch_chance_percent,
            patch_grid_reach,
            distance_scaling: config.distance_scaling,
            spawn_bias,
            tile_pollution_absorption_per_minute_milli,
            tile_walking_speed_percent,
        }
    }

    /// Walking speed percentage for a tile; unknown ids fall back to the
    /// unmodified base speed.
    pub(super) fn walking_speed_percent(&self, tile_id: TileId) -> u16 {
        self.tile_walking_speed_percent
            .get(tile_id.index())
            .copied()
            .unwrap_or(100)
    }

    pub(super) fn pollution_absorption_per_minute_milli(&self, tile_id: TileId) -> u64 {
        self.tile_pollution_absorption_per_minute_milli
            .get(tile_id.index())
            .copied()
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub(super) fn clear_pollution_absorption_cache(&mut self) {
        self.tile_pollution_absorption_per_minute_milli.clear();
    }

    fn resource_is_minable(&self, resource_item: ItemId) -> bool {
        self.resource_minability
            .get(resource_item.index())
            .copied()
            .unwrap_or(false)
    }

    pub(super) fn has_pollution_absorption(&self) -> bool {
        self.tile_pollution_absorption_per_minute_milli
            .iter()
            .any(|rate| *rate != 0)
    }
}

// `WorldGenerator` is fully derived from serialized prototype data. Excluding
// it from equality and hashing keeps world identity tied to durable state.
impl PartialEq for WorldGenerator {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for WorldGenerator {}

impl Hash for WorldGenerator {
    fn hash<H: Hasher>(&self, _state: &mut H) {}
}

#[cfg(test)]
mod tests;
