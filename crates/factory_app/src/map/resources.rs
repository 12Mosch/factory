use bevy::prelude::{Color, Handle, Image, Resource, Vec2};
use factory_sim::{CHUNK_SIZE, ChunkCoord};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MapLayer {
    #[default]
    Surface,
    Resources,
    Entities,
}

#[derive(Resource)]
pub struct MapViewState {
    pub open: bool,
    pub center_tile: Vec2,
    pub zoom: f32,
    pub follow_player: bool,
    pub selected_layer: MapLayer,
}

impl Default for MapViewState {
    fn default() -> Self {
        Self {
            open: false,
            center_tile: Vec2::ZERO,
            zoom: 1.0,
            follow_player: true,
            selected_layer: MapLayer::Surface,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapPointMarker {
    pub position: Vec2,
    pub color: Color,
}

#[derive(Resource, Default)]
pub struct MapOverlayMarkers {
    pub pings: Vec<MapPointMarker>,
    pub waypoints: Vec<MapPointMarker>,
}

#[derive(Clone, Copy, Debug, Resource, Default)]
pub struct MapDisplaySettings {
    pub debug_reveal_all: bool,
    pub show_chunk_grid: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapTextureBounds {
    pub min_x: i32,
    pub min_y: i32,
    pub width: u32,
    pub height: u32,
}

impl MapTextureBounds {
    pub fn contains_tile(self, tile: (i32, i32)) -> bool {
        self.contains_tile_wide((tile.0 as i64, tile.1 as i64))
    }

    pub fn contains_chunk(self, coord: ChunkCoord) -> bool {
        let chunk_size = i64::from(CHUNK_SIZE);
        let min_x = i64::from(coord.x) * chunk_size;
        let min_y = i64::from(coord.y) * chunk_size;
        let max_x = (i64::from(coord.x) + 1) * chunk_size - 1;
        let max_y = (i64::from(coord.y) + 1) * chunk_size - 1;

        self.contains_tile_wide((min_x, min_y)) && self.contains_tile_wide((max_x, max_y))
    }

    fn contains_tile_wide(self, tile: (i64, i64)) -> bool {
        let min_x = i64::from(self.min_x);
        let min_y = i64::from(self.min_y);
        let max_x = min_x + i64::from(self.width);
        let max_y = min_y + i64::from(self.height);

        tile.0 >= min_x && tile.0 < max_x && tile.1 >= min_y && tile.1 < max_y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_texture_bounds_contains_tile_handles_extreme_edges() {
        let bounds = MapTextureBounds {
            min_x: i32::MAX,
            min_y: i32::MIN,
            width: 1,
            height: 1,
        };

        assert!(bounds.contains_tile((i32::MAX, i32::MIN)));
        assert!(!bounds.contains_tile((i32::MAX - 1, i32::MIN)));
        assert!(!bounds.contains_tile((i32::MAX, i32::MIN + 1)));
    }

    #[test]
    fn map_texture_bounds_contains_chunk_handles_extreme_coords() {
        let bounds = MapTextureBounds {
            min_x: i32::MIN,
            min_y: i32::MIN,
            width: u32::MAX,
            height: u32::MAX,
        };

        assert!(!bounds.contains_chunk(ChunkCoord {
            x: i32::MAX,
            y: i32::MAX,
        }));
        assert!(!bounds.contains_chunk(ChunkCoord {
            x: i32::MIN,
            y: i32::MIN,
        }));
    }
}

#[derive(Resource, Default)]
pub struct MapTextureCache {
    pub layers: BTreeMap<MapLayer, MapLayerTextureCache>,
}

impl MapTextureCache {
    pub fn layer(&self, layer: MapLayer) -> Option<&MapLayerTextureCache> {
        self.layers.get(&layer)
    }

    pub fn surface(&self) -> Option<&MapLayerTextureCache> {
        self.layer(MapLayer::Surface)
    }

    pub fn layer_mut(&mut self, layer: MapLayer) -> &mut MapLayerTextureCache {
        self.layers.entry(layer).or_default()
    }
}

#[derive(Default)]
pub struct MapLayerTextureCache {
    pub handle: Option<Handle<Image>>,
    pub bounds: Option<MapTextureBounds>,
    pub pixels: Option<Vec<u8>>,
    pub painted_chunks: BTreeMap<ChunkCoord, MapChunkPaintState>,
    pub last_chunk_revision: u64,
    pub last_resource_revision: u64,
    pub last_revealed_revision: u64,
    pub last_debug_flags: (bool, bool),
    pub last_texture_update_tick: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapChunkPaintState {
    pub revealed: bool,
}

#[derive(Resource, Default)]
pub struct VisibleChunks {
    pub chunks: BTreeSet<ChunkCoord>,
    pub tile_bounds: Option<MapTextureBounds>,
    pub revision: u64,
}
