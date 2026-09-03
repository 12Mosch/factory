use std::collections::{HashMap, HashSet};

use glam::IVec2;

use crate::error::PrototypeLoadError;
use crate::ids::ItemId;
use crate::model::{
    BiomeConfig, ClimateNoiseConfig, ClimateRange, EnemyBaseGenerationConfig, EntityPrototype,
    ResourceDistanceScalingConfig, ResourceGenerationConfig, ResourcePatchGridConfig,
    StartingAreaConfig, TerrainNoiseConfig, TilePrototype, WORLD_GENERATION_FORMAT_VERSION,
    WorldGenerationConfig,
};
use crate::raw::{
    RawBiomeConfig, RawClimateNoise, RawClimateRange, RawEnemyBaseGeneration,
    RawResourceGeneration, RawTerrainNoise, RawWorldGenerationConfig,
};

pub(super) fn load_world_generation(
    raw: Option<RawWorldGenerationConfig>,
    item_ids_by_name: &HashMap<String, ItemId>,
    tiles: &[TilePrototype],
    entities: &[EntityPrototype],
) -> Result<WorldGenerationConfig, PrototypeLoadError> {
    let Some(raw) = raw else {
        return Ok(WorldGenerationConfig::default());
    };

    validate_world_generation(&raw)?;

    let climate_noise = resolve_climate_noise(&raw.climate_noise);
    let biomes = resolve_biomes(raw.biomes, tiles)?;
    let resources = resolve_resources(raw.resources, item_ids_by_name)?;
    let enemy_bases = raw
        .enemy_bases
        .map(|bases| resolve_enemy_bases(bases, entities))
        .transpose()?;

    Ok(WorldGenerationConfig {
        version: raw.version,
        starting_area: StartingAreaConfig {
            min_chunk: raw.starting_area.min_chunk,
            max_chunk: raw.starting_area.max_chunk,
        },
        climate_noise,
        biomes,
        patch_grid: ResourcePatchGridConfig {
            cell_size: raw.patch_grid.cell_size,
            jitter: raw.patch_grid.jitter,
            edge_noise: raw.patch_grid.edge_noise,
            patch_chance_percent: raw.patch_grid.patch_chance_percent,
        },
        distance_scaling: raw
            .distance_scaling
            .map(|scaling| ResourceDistanceScalingConfig {
                interval_tiles: scaling.interval_tiles,
                richness_bonus_percent: scaling.richness_bonus_percent,
                radius_bonus_tiles: scaling.radius_bonus_tiles,
                max_radius_bonus_tiles: scaling.max_radius_bonus_tiles,
            }),
        resources,
        enemy_bases,
    })
}

/// Resolve the enemy base spawner entity name and validate placement rules.
fn resolve_enemy_bases(
    bases: RawEnemyBaseGeneration,
    entities: &[EntityPrototype],
) -> Result<EnemyBaseGenerationConfig, PrototypeLoadError> {
    let spawner_entity = entities
        .iter()
        .find(|entity| entity.name == bases.spawner_entity)
        .ok_or(PrototypeLoadError::MissingWorldGenerationSpawnerEntity {
            entity: bases.spawner_entity.clone(),
        })?;
    if spawner_entity.enemy_spawner.is_none() {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "enemy base spawner entity must declare an enemy_spawner section",
        });
    }
    if bases.frequency_percent > 100 {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "enemy base frequency_percent must not exceed 100",
        });
    }

    Ok(EnemyBaseGenerationConfig {
        spawner_entity: spawner_entity.id,
        frequency_percent: bases.frequency_percent,
        min_distance_tiles: bases.min_distance_tiles,
    })
}

/// Validate the top-level world generation fields that do not require
/// resolving names against loaded prototypes.
fn validate_world_generation(raw: &RawWorldGenerationConfig) -> Result<(), PrototypeLoadError> {
    const MAX_STARTING_AREA_AXIS_CHUNKS: u64 = 64;
    const MAX_STARTING_AREA_CHUNKS: u64 = 4_096;
    const MAX_PATCH_GRID_CELL_SIZE: i32 = 1_048_576;
    const MAX_PATCH_GRID_JITTER: i32 = 1_048_576;
    const MAX_PATCH_EDGE_NOISE: i32 = 4_096;
    const MAX_RADIUS_BONUS_TILES: u8 = 128;
    const MAX_RICHNESS_BONUS_PERCENT: u32 = 10_000;
    const MAX_PATCH_REACH_CELL_MULTIPLE: i64 = 32;

    if raw.version != WORLD_GENERATION_FORMAT_VERSION {
        return Err(PrototypeLoadError::UnsupportedWorldGenerationVersion {
            found: raw.version,
            supported: WORLD_GENERATION_FORMAT_VERSION,
        });
    }
    if raw.starting_area.min_chunk > raw.starting_area.max_chunk {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "starting area min_chunk must not exceed max_chunk",
        });
    }
    let starting_axis_chunks = i64::from(raw.starting_area.max_chunk)
        .checked_sub(i64::from(raw.starting_area.min_chunk))
        .and_then(|span| span.checked_add(1))
        .and_then(|span| u64::try_from(span).ok())
        .ok_or(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "starting area dimensions overflow",
        })?;
    if starting_axis_chunks > MAX_STARTING_AREA_AXIS_CHUNKS {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "starting area axis must not exceed 64 chunks",
        });
    }
    let starting_chunk_count = starting_axis_chunks
        .checked_mul(starting_axis_chunks)
        .ok_or(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "starting area chunk count overflow",
        })?;
    if starting_chunk_count > MAX_STARTING_AREA_CHUNKS {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "starting area must not exceed 4096 total chunks",
        });
    }
    if raw.patch_grid.cell_size < 1 {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "patch grid cell_size must be at least 1",
        });
    }
    if raw.patch_grid.jitter < 0 || raw.patch_grid.edge_noise < 0 {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "patch grid jitter and edge_noise must not be negative",
        });
    }
    if raw.patch_grid.cell_size > MAX_PATCH_GRID_CELL_SIZE {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "patch grid cell_size must not exceed 1048576",
        });
    }
    if raw.patch_grid.jitter > MAX_PATCH_GRID_JITTER {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "patch grid jitter must not exceed 1048576",
        });
    }
    if raw.patch_grid.edge_noise > MAX_PATCH_EDGE_NOISE {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "patch grid edge_noise must not exceed 4096",
        });
    }
    if raw.patch_grid.patch_chance_percent > 100 {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "patch grid patch_chance_percent must not exceed 100",
        });
    }
    if raw.patch_grid.patch_chance_percent > 0
        && !raw.resources.is_empty()
        && raw
            .resources
            .iter()
            .all(|resource| resource.selection_weight == 0)
    {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "resources must include a positive selection_weight when patch_chance_percent \
                     is positive",
        });
    }
    raw.patch_grid
        .jitter
        .checked_mul(2)
        .and_then(|diameter| diameter.checked_add(1))
        .ok_or(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "patch grid jitter range overflow",
        })?;
    if raw.biomes.is_empty() {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "biomes must declare at least one entry",
        });
    }
    for biome in &raw.biomes {
        for range in [&biome.elevation, &biome.moisture, &biome.temperature] {
            if range.max > 100 {
                return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                    detail: "biome climate range max must not exceed 100",
                });
            }
            if range.min >= range.max {
                return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                    detail: "biome climate range min must be less than max",
                });
            }
        }
    }
    for noise in [
        &raw.climate_noise.elevation,
        &raw.climate_noise.moisture,
        &raw.climate_noise.temperature,
    ] {
        if noise.scale < 1 {
            return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                detail: "climate noise scale must be at least 1",
            });
        }
        if noise.octaves < 1 || noise.octaves > 8 {
            return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                detail: "climate noise octaves must be between 1 and 8",
            });
        }
    }
    if let Some(scaling) = &raw.distance_scaling {
        if scaling.interval_tiles < 1 {
            return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                detail: "distance scaling interval_tiles must be at least 1",
            });
        }
        if scaling.radius_bonus_tiles > scaling.max_radius_bonus_tiles {
            return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                detail: "distance scaling radius_bonus_tiles must not exceed \
                         max_radius_bonus_tiles",
            });
        }
        if scaling.max_radius_bonus_tiles > MAX_RADIUS_BONUS_TILES {
            return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                detail: "distance scaling max_radius_bonus_tiles must not exceed 128",
            });
        }
        if scaling.richness_bonus_percent > MAX_RICHNESS_BONUS_PERCENT {
            return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                detail: "distance scaling richness_bonus_percent must not exceed 10000",
            });
        }
    }
    let max_radius = raw
        .resources
        .iter()
        .map(|resource| resource.radius)
        .max()
        .unwrap_or(0);
    if max_radius > crate::MAX_RESOURCE_RADIUS_TILES {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "resource radius must not exceed 16384",
        });
    }
    let patch_scan_reach = i64::from(max_radius)
        .checked_add(i64::from(raw.patch_grid.edge_noise))
        .and_then(|reach| reach.checked_add(i64::from(raw.patch_grid.jitter)))
        .and_then(|reach| {
            reach.checked_add(i64::from(
                raw.distance_scaling
                    .as_ref()
                    .map_or(0, |scaling| scaling.max_radius_bonus_tiles),
            ))
        })
        .ok_or(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "resource patch scan reach overflow",
        })?;
    let max_patch_scan_reach = i64::from(raw.patch_grid.cell_size)
        .checked_mul(MAX_PATCH_REACH_CELL_MULTIPLE)
        .ok_or(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "patch grid cell_size scan bound overflow",
        })?;
    if patch_scan_reach > max_patch_scan_reach {
        return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
            detail: "resource patch scan reach must not exceed 32 grid cells",
        });
    }
    for resource in &raw.resources {
        i64::from(resource.radius)
            .checked_add(i64::from(raw.patch_grid.edge_noise))
            .ok_or(PrototypeLoadError::InvalidWorldGenerationConfig {
                detail: "resource radius plus edge_noise overflow",
            })?;
    }
    Ok(())
}

/// Convert the three raw climate-noise channels into their validated form.
/// Numeric bounds were already checked by [`validate_world_generation`].
fn resolve_climate_noise(noise: &RawClimateNoise) -> ClimateNoiseConfig {
    let channel = |channel: &RawTerrainNoise| TerrainNoiseConfig {
        scale: channel.scale,
        octaves: channel.octaves,
    };
    ClimateNoiseConfig {
        elevation: channel(&noise.elevation),
        moisture: channel(&noise.moisture),
        temperature: channel(&noise.temperature),
    }
}

/// Resolve biome tile names against loaded tiles. Climate-range bounds were
/// already checked by [`validate_world_generation`].
fn resolve_biomes(
    biomes: Vec<RawBiomeConfig>,
    tiles: &[TilePrototype],
) -> Result<Vec<BiomeConfig>, PrototypeLoadError> {
    let range = |range: &RawClimateRange| ClimateRange {
        min: range.min,
        max: range.max,
    };
    biomes
        .into_iter()
        .map(|biome| {
            let tile = tiles
                .iter()
                .find(|tile| tile.name == biome.tile)
                .map(|tile| tile.id)
                .ok_or(PrototypeLoadError::MissingWorldGenerationTile { tile: biome.tile })?;
            Ok(BiomeConfig {
                tile,
                elevation: range(&biome.elevation),
                moisture: range(&biome.moisture),
                temperature: range(&biome.temperature),
            })
        })
        .collect::<Result<Vec<_>, PrototypeLoadError>>()
}

/// Resolve resource item names against loaded items and validate each entry.
fn resolve_resources(
    resources: Vec<RawResourceGeneration>,
    item_ids_by_name: &HashMap<String, ItemId>,
) -> Result<Vec<ResourceGenerationConfig>, PrototypeLoadError> {
    let mut seen_resource_items = HashSet::new();
    resources
        .into_iter()
        .map(|resource| {
            let resource_item = *item_ids_by_name.get(&resource.item).ok_or_else(|| {
                PrototypeLoadError::MissingWorldGenerationResourceItem {
                    item: resource.item.clone(),
                }
            })?;
            if !seen_resource_items.insert(resource_item) {
                return Err(PrototypeLoadError::DuplicateWorldGenerationResource {
                    item: resource.item,
                });
            }
            if resource.radius < 1 {
                return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                    detail: "resource radius must be at least 1",
                });
            }
            if resource.richness == 0 {
                return Err(PrototypeLoadError::InvalidWorldGenerationConfig {
                    detail: "resource richness must be at least 1",
                });
            }
            Ok(ResourceGenerationConfig {
                resource_item,
                extraction: resource.extraction,
                selection_weight: resource.selection_weight,
                radius: resource.radius,
                richness: resource.richness,
                starting_patch: resource
                    .starting_patch
                    .map(|offset| IVec2::new(offset.x, offset.y)),
            })
        })
        .collect::<Result<Vec<_>, PrototypeLoadError>>()
}
