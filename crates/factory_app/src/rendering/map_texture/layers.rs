use bevy::color::Srgba;
use bevy::prelude::{Color, ColorToPacked};
use factory_sim::Simulation;

use crate::map::resources::MapLayer;
use crate::rendering::colors::{RenderPrototypeIds, resource_color_variant, tile_color};
use crate::rendering::map_texture::UNREVEALED_PIXEL;

#[derive(Clone, Copy)]
pub(super) struct MapLayerPainter {
    layer: MapLayer,
    ids: RenderPrototypeIds,
    seed: u64,
}

impl MapLayerPainter {
    pub(super) fn new(layer: MapLayer, sim: &Simulation) -> Self {
        Self {
            layer,
            ids: RenderPrototypeIds::from_catalog(sim.catalog()),
            seed: sim.seed(),
        }
    }

    pub(super) fn pixel_for_tile(
        self,
        tile: &factory_sim::TileCell,
        x: i64,
        y: i64,
        revealed: bool,
    ) -> [u8; 4] {
        if revealed {
            self.revealed_tile_pixel(tile, x, y)
        } else {
            UNREVEALED_PIXEL
        }
    }

    fn revealed_tile_pixel(self, tile: &factory_sim::TileCell, x: i64, y: i64) -> [u8; 4] {
        match self.layer {
            MapLayer::Surface => {
                let terrain = darkened(tile_color(tile.tile_id, self.ids, self.seed, x, y), 0.58);
                tile.resource
                    .map(|resource| {
                        color_to_pixel(resource_color_variant(resource, self.ids, self.seed, x, y))
                    })
                    .unwrap_or(terrain)
            }
            MapLayer::Resources => tile
                .resource
                .map(|resource| {
                    color_to_pixel(resource_color_variant(resource, self.ids, self.seed, x, y))
                })
                .unwrap_or_else(|| {
                    darkened(tile_color(tile.tile_id, self.ids, self.seed, x, y), 0.24)
                }),
            MapLayer::Entities => {
                darkened(tile_color(tile.tile_id, self.ids, self.seed, x, y), 0.30)
            }
            MapLayer::Threat => darkened(tile_color(tile.tile_id, self.ids, self.seed, x, y), 0.16),
        }
    }
}

fn darkened(color: Color, factor: f32) -> [u8; 4] {
    let srgba = color.to_srgba();
    Srgba::new(
        srgba.red * factor,
        srgba.green * factor,
        srgba.blue * factor,
        srgba.alpha,
    )
    .to_u8_array()
}

fn color_to_pixel(color: Color) -> [u8; 4] {
    color.to_srgba().to_u8_array()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_layer_painter_keeps_unrevealed_tiles_hidden_for_all_layers() {
        let sim = Simulation::new_test_world(123);
        let tile = sim
            .world()
            .chunks
            .values()
            .next()
            .and_then(|chunk| chunk.tiles.first())
            .expect("test world should contain at least one tile");

        for layer in [
            MapLayer::Surface,
            MapLayer::Resources,
            MapLayer::Entities,
            MapLayer::Threat,
        ] {
            let painter = MapLayerPainter::new(layer, &sim);
            assert_eq!(painter.pixel_for_tile(tile, 0, 0, false), UNREVEALED_PIXEL);
        }
    }
}
