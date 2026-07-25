use factory_sim::{ChunkCoord, ResourceTileChange, TerrainTileChange};

use crate::map::resources::{MapLayerTextureCache, MapTextureLayer};

use super::super::rasterizer::MapRasterizer;

pub(super) struct MapTextureChanges {
    pub dirty_chunks: Option<Vec<ChunkCoord>>,
    pub dirty_resource_tiles: Option<Vec<ResourceTileChange>>,
    pub dirty_terrain_tiles: Option<Vec<TerrainTileChange>>,
}

impl MapTextureChanges {
    pub(super) fn collect(
        rasterizer: &MapRasterizer<'_>,
        cache: &MapLayerTextureCache,
        map_changed: bool,
    ) -> Self {
        Self {
            dirty_chunks: exact_dirty_chunks(rasterizer, cache, map_changed),
            dirty_resource_tiles: exact_dirty_resource_tiles(rasterizer, cache),
            dirty_terrain_tiles: exact_dirty_terrain_tiles(rasterizer, cache),
        }
    }
}

fn exact_dirty_chunks(
    rasterizer: &MapRasterizer<'_>,
    cache: &MapLayerTextureCache,
    map_changed: bool,
) -> Option<Vec<ChunkCoord>> {
    if !map_changed {
        return Some(Vec::new());
    }

    if rasterizer.settings.debug_reveal_all {
        rasterizer
            .sim
            .world()
            .chunk_generation_since(cache.last_chunk_revision)
            .map(|changes| changes.into_generated_chunks())
    } else {
        rasterizer
            .sim
            .revealed_chunks_since(cache.last_revealed_revision)
            .map(Iterator::collect)
    }
}

fn exact_dirty_resource_tiles(
    rasterizer: &MapRasterizer<'_>,
    cache: &MapLayerTextureCache,
) -> Option<Vec<ResourceTileChange>> {
    if rasterizer.layer != MapTextureLayer::Resources
        || cache.last_resource_revision == rasterizer.sim.world().resource_revision()
    {
        return Some(Vec::new());
    }

    rasterizer
        .sim
        .world()
        .resource_dirty_tiles_since(cache.last_resource_revision)
        .map(Iterator::collect)
}

fn exact_dirty_terrain_tiles(
    rasterizer: &MapRasterizer<'_>,
    cache: &MapLayerTextureCache,
) -> Option<Vec<TerrainTileChange>> {
    if rasterizer.layer != MapTextureLayer::Surface
        || cache.last_terrain_revision == rasterizer.sim.world().terrain_revision()
    {
        return Some(Vec::new());
    }

    rasterizer
        .sim
        .world()
        .terrain_dirty_tiles_since(cache.last_terrain_revision)
        .map(Iterator::collect)
}
