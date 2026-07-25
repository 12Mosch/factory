use bevy::prelude::*;
use factory_sim::Simulation;

use crate::map::resources::{
    MapDisplaySettings, MapLayerTextureCache, MapTextureCache, MapTextureLayer,
};
use crate::resources::SimResource;

use super::rasterizer::MapRasterizer;
use super::upload::{MapTextureUploadQueue, upload_layer_texture};

mod changes;
mod incremental;
mod pixels;
mod repaint;
#[cfg(test)]
mod tests;

use incremental::update_map_pixels_incremental;
use repaint::refresh_painted_chunks;

pub(crate) fn update_map_texture(
    sim: Res<SimResource>,
    settings: Res<MapDisplaySettings>,
    mut cache: ResMut<MapTextureCache>,
    mut uploads: ResMut<MapTextureUploadQueue>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let Some(mut images) = images else {
        return;
    };

    let sim = sim.read();
    // The surface layer also backs the minimap, so it stays fresh even while
    // the fullscreen map is closed. Other layers only update while displayed.
    let surface_cache = cache.layer_mut(MapTextureLayer::Surface);
    update_layer_map_texture(
        &sim,
        &settings,
        MapTextureLayer::Surface,
        surface_cache,
        &mut images,
        &mut uploads,
    );

    if settings
        .overlays
        .is_enabled(crate::map::resources::MapOverlay::Resources)
    {
        let layer_cache = cache.layer_mut(MapTextureLayer::Resources);
        update_layer_map_texture(
            &sim,
            &settings,
            MapTextureLayer::Resources,
            layer_cache,
            &mut images,
            &mut uploads,
        );
    }
}

fn update_layer_map_texture(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    layer: MapTextureLayer,
    cache: &mut MapLayerTextureCache,
    images: &mut Assets<Image>,
    uploads: &mut MapTextureUploadQueue,
) {
    let revealed_revision = sim.revealed_revision();
    let debug_flags = (settings.debug_reveal_all, settings.show_chunk_grid);
    let tick_count = sim.tick_count();
    let chunk_changed = cache.last_chunk_revision != sim.world().chunk_revision();
    let resource_changed = cache.last_resource_revision != sim.world().resource_revision();
    let terrain_changed = cache.last_terrain_revision != sim.world().terrain_revision();
    let revealed_changed = cache.last_revealed_revision != revealed_revision;
    let debug_changed = cache.last_debug_flags != debug_flags;
    let map_changed = if settings.debug_reveal_all {
        chunk_changed
    } else {
        revealed_changed
    };
    // Only the surface layer paints terrain; the resource layer draws resource
    // cells, which a terrain rewrite leaves alone.
    let needs_update = cache.handle.is_none()
        || map_changed
        || debug_changed
        || (layer == MapTextureLayer::Surface && terrain_changed)
        || (layer == MapTextureLayer::Resources && resource_changed);

    if !needs_update {
        return;
    }

    let rasterizer = MapRasterizer::new(sim, settings, layer);
    let full_rebuild = cache.bounds.is_none() || cache.pixels.is_none() || debug_changed;
    if full_rebuild {
        let map = rasterizer.generate();
        cache.bounds = Some(map.bounds);
        cache.pixels = Some(map.data);
        cache.dirty_regions.mark_full();
        refresh_painted_chunks(&rasterizer, cache);
    } else {
        update_map_pixels_incremental(&rasterizer, cache);
    }

    upload_layer_texture(cache, images, uploads);

    cache.last_chunk_revision = sim.world().chunk_revision();
    cache.last_resource_revision = sim.world().resource_revision();
    cache.last_terrain_revision = sim.world().terrain_revision();
    cache.last_revealed_revision = revealed_revision;
    cache.last_debug_flags = debug_flags;
    cache.last_texture_update_tick = tick_count;
}
