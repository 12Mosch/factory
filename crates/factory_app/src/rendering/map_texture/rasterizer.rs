use factory_sim::{CHUNK_SIZE, ChunkCoord, Simulation};

use crate::map::resources::{
    MapChunkPaintState, MapDisplaySettings, MapTextureBounds, MapTextureLayer,
};

use super::bounds::map_texture_bounds;
use super::grid::{draw_chunk_grid, draw_chunk_grid_for_chunk};
use super::layers::MapLayerPainter;
use super::pixels::{MapPixels, set_world_pixel};
use crate::rendering::map_texture::UNREVEALED_PIXEL;

pub(super) struct MapRasterizer<'a> {
    pub(super) sim: &'a Simulation,
    pub(super) settings: &'a MapDisplaySettings,
    painter: MapLayerPainter,
    pub(super) layer: MapTextureLayer,
}

impl<'a> MapRasterizer<'a> {
    pub(super) fn new(
        sim: &'a Simulation,
        settings: &'a MapDisplaySettings,
        layer: MapTextureLayer,
    ) -> Self {
        Self {
            sim,
            settings,
            painter: MapLayerPainter::new(layer, sim),
            layer,
        }
    }
}

impl MapRasterizer<'_> {
    pub fn generate(&self) -> MapPixels {
        let bounds = map_texture_bounds(self.sim, self.settings).unwrap_or_default();
        let len = bounds.width as usize * bounds.height as usize * 4;
        let background = if self.layer == MapTextureLayer::Surface {
            UNREVEALED_PIXEL
        } else {
            [0; 4]
        };
        let mut data = background.repeat(len / 4);

        for coord in self.eligible_chunk_coords(bounds) {
            self.repaint_chunk(&mut data, bounds, coord);
        }

        if self.settings.show_chunk_grid && self.layer == MapTextureLayer::Surface {
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
            let world_x = chunk.coord.tile_at(local_x, 0).0;
            let world_y = chunk.coord.tile_at(0, local_y).1;
            let color = self
                .painter
                .pixel_for_tile(tile, world_x, world_y, revealed);
            set_world_pixel(data, bounds, world_x, world_y, color);
        }

        if self.settings.show_chunk_grid && self.layer == MapTextureLayer::Surface {
            draw_chunk_grid_for_chunk(data, bounds, coord);
        }
    }

    pub(super) fn repaint_tile(&self, data: &mut [u8], bounds: MapTextureBounds, x: i64, y: i64) {
        let Some(coord) = ChunkCoord::from_tile(x, y) else {
            return;
        };
        if let Some(tile) = self.sim.world().tile_at(x, y) {
            let revealed = self.chunk_paint_state(coord).revealed;
            let color = self.painter.pixel_for_tile(tile, x, y, revealed);
            set_world_pixel(data, bounds, x, y, color);
        }

        if self.settings.show_chunk_grid
            && self.layer == MapTextureLayer::Surface
            && (x.rem_euclid(i64::from(CHUNK_SIZE)) == 0
                || y.rem_euclid(i64::from(CHUNK_SIZE)) == 0)
        {
            set_world_pixel(data, bounds, x, y, super::pixels::GRID_PIXEL);
        }
    }

    pub(super) fn chunk_paint_state(&self, coord: ChunkCoord) -> MapChunkPaintState {
        MapChunkPaintState {
            revealed: self.settings.debug_reveal_all || self.sim.is_chunk_revealed(coord),
        }
    }

    /// Generated and chart-eligible chunks intersecting the requested bounds.
    /// Normal map work follows the sparse revealed set; debug reveal follows
    /// generated chunks so the explicit override still exposes uncharted land.
    pub(super) fn eligible_chunk_coords(
        &self,
        bounds: MapTextureBounds,
    ) -> impl Iterator<Item = ChunkCoord> + '_ {
        let candidates: Box<dyn Iterator<Item = ChunkCoord> + '_> =
            if self.settings.debug_reveal_all {
                Box::new(self.sim.world().chunks.keys().copied())
            } else {
                Box::new(self.sim.revealed_chunks().iter().copied())
            };
        let coords = candidates
            .filter(|coord| chunk_intersects_bounds(*coord, bounds))
            .filter(|coord| {
                (self.settings.debug_reveal_all || self.sim.world().chunks.contains_key(coord))
                    && self.chunk_paint_state(*coord).revealed
            })
            .collect::<Vec<_>>();
        coords.into_iter()
    }
}

fn chunk_intersects_bounds(coord: ChunkCoord, bounds: MapTextureBounds) -> bool {
    if bounds.width == 0 || bounds.height == 0 {
        return false;
    }
    let (chunk_min_x, chunk_min_y) = coord.min_tile();
    let chunk_max_x = chunk_min_x + i64::from(CHUNK_SIZE) - 1;
    let chunk_max_y = chunk_min_y + i64::from(CHUNK_SIZE) - 1;
    let bounds_max_x = bounds.min_x + i64::from(bounds.width) - 1;
    let bounds_max_y = bounds.min_y + i64::from(bounds.height) - 1;
    chunk_min_x <= bounds_max_x
        && chunk_max_x >= bounds.min_x
        && chunk_min_y <= bounds_max_y
        && chunk_max_y >= bounds.min_y
}

pub fn generate_map_pixels(sim: &Simulation, settings: &MapDisplaySettings) -> MapPixels {
    generate_map_pixels_for_layer(sim, settings, MapTextureLayer::Surface)
}

pub fn generate_map_pixels_for_layer(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    layer: MapTextureLayer,
) -> MapPixels {
    MapRasterizer::new(sim, settings, layer).generate()
}
