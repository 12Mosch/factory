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
        tile.0 >= self.min_x
            && tile.0 < self.min_x + self.width as i32
            && tile.1 >= self.min_y
            && tile.1 < self.min_y + self.height as i32
    }

    pub fn contains_chunk(self, coord: ChunkCoord) -> bool {
        self.contains_tile((coord.x * CHUNK_SIZE, coord.y * CHUNK_SIZE))
            && self.contains_tile((
                (coord.x + 1) * CHUNK_SIZE - 1,
                (coord.y + 1) * CHUNK_SIZE - 1,
            ))
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
