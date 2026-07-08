use factory_sim::{CHUNK_SIZE, ChunkCoord, Simulation};

use crate::map::resources::{MapChunkPaintState, MapDisplaySettings, MapLayer, MapTextureBounds};

use super::bounds::map_texture_bounds;
use super::grid::{draw_chunk_grid, draw_chunk_grid_for_chunk};
use super::layers::MapLayerPainter;
use super::pixels::{MapPixels, set_world_pixel};

pub(super) struct MapRasterizer<'a> {
    pub(super) sim: &'a Simulation,
    pub(super) settings: &'a MapDisplaySettings,
    painter: MapLayerPainter,
}

impl<'a> MapRasterizer<'a> {
    pub(super) fn new(
        sim: &'a Simulation,
        settings: &'a MapDisplaySettings,
        layer: MapLayer,
    ) -> Self {
        Self {
            sim,
            settings,
            painter: MapLayerPainter::new(layer, sim),
        }
    }
}

impl MapRasterizer<'_> {
    pub fn generate(&self) -> MapPixels {
        let bounds = map_texture_bounds(self.sim, self.settings).unwrap_or_default();
        let len = bounds.width as usize * bounds.height as usize * 4;
        let mut data = vec![0; len];

        for chunk in self.sim.world().chunks.values() {
            self.repaint_chunk(&mut data, bounds, chunk.coord);
        }

        if self.settings.show_chunk_grid {
            draw_chunk_grid(&mut data, bounds);
        }

        MapPixels { bounds, data }
    }

    pub(super) fn repaint_chunk(
        &self,
        data: &mut [u8],
        bounds: MapTextureBounds,
        coord: ChunkCoord,
    ) {
        let Some(chunk) = self.sim.world().chunks.get(&coord) else {
            return;
        };
        let revealed = self.chunk_paint_state(coord).revealed;

        for (index, tile) in chunk.tiles.iter().enumerate() {
            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let world_x = chunk.coord.x * CHUNK_SIZE + local_x;
            let world_y = chunk.coord.y * CHUNK_SIZE + local_y;
            let color = self.painter.pixel_for_tile(tile, revealed);
            set_world_pixel(data, bounds, world_x, world_y, color);
        }

        if self.settings.show_chunk_grid {
            draw_chunk_grid_for_chunk(data, bounds, coord);
        }
    }

    pub(super) fn repaint_tile(&self, data: &mut [u8], bounds: MapTextureBounds, x: i32, y: i32) {
        let coord = ChunkCoord {
            x: x.div_euclid(CHUNK_SIZE),
            y: y.div_euclid(CHUNK_SIZE),
        };
        if let Some(tile) = self.sim.world().tile_at(x, y) {
            let revealed = self.chunk_paint_state(coord).revealed;
            let color = self.painter.pixel_for_tile(tile, revealed);
            set_world_pixel(data, bounds, x, y, color);
        }

        if self.settings.show_chunk_grid
            && (x.rem_euclid(CHUNK_SIZE) == 0 || y.rem_euclid(CHUNK_SIZE) == 0)
        {
            set_world_pixel(data, bounds, x, y, super::pixels::GRID_PIXEL);
        }
    }

    pub(super) fn chunk_paint_state(&self, coord: ChunkCoord) -> MapChunkPaintState {
        MapChunkPaintState {
            revealed: self.settings.debug_reveal_all || self.sim.is_chunk_revealed(coord),
        }
    }
}

pub fn generate_map_pixels(sim: &Simulation, settings: &MapDisplaySettings) -> MapPixels {
    generate_map_pixels_for_layer(sim, settings, MapLayer::Surface)
}

pub fn generate_map_pixels_for_layer(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    layer: MapLayer,
) -> MapPixels {
    MapRasterizer::new(sim, settings, layer).generate()
}
