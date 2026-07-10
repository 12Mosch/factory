use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::layers::{VisualLayer, VisualPrimitive, color_to_unit_array, unit_to_u8};
use super::recipes::visual_layers;
use super::templates::VisualTemplate;
use crate::constants::TILE_SIZE;

const VISUAL_TEXTURE_PIXELS_PER_TILE: f32 = 64.0;
const MIN_VISUAL_TEXTURE_PIXELS: u32 = 16;
const MAX_VISUAL_TEXTURE_PIXELS: u32 = 256;

#[derive(Clone)]
pub(super) struct RasterizedVisual {
    pub(super) image: Image,
    pub(super) visual_size: Vec2,
}

pub(super) fn rasterize_visual(
    template: VisualTemplate,
    color: Color,
    size: Vec2,
) -> RasterizedVisual {
    let layers = visual_layers(template, color, size);
    let visual_size = visual_size_for_layers(&layers, size);
    let mut sorted_layers = layers;
    sorted_layers.sort_by(|a, b| a.z.total_cmp(&b.z));

    let (width, height) = visual_texture_size(visual_size);
    let mut data = vec![0; width as usize * height as usize * 4];
    for layer in sorted_layers {
        paint_layer(&mut data, width, height, visual_size, layer);
    }

    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    RasterizedVisual { image, visual_size }
}

fn visual_size_for_layers(layers: &[VisualLayer], fallback_size: Vec2) -> Vec2 {
    layers.iter().fold(fallback_size, |size, layer| {
        let half = layer.size * 0.5 + layer.offset.abs();
        size.max(half * 2.0)
    })
}

fn visual_texture_size(visual_size: Vec2) -> (u32, u32) {
    let pixels_for_axis = |axis_size: f32| {
        ((axis_size / TILE_SIZE) * VISUAL_TEXTURE_PIXELS_PER_TILE)
            .ceil()
            .clamp(
                MIN_VISUAL_TEXTURE_PIXELS as f32,
                MAX_VISUAL_TEXTURE_PIXELS as f32,
            ) as u32
    };

    (
        pixels_for_axis(visual_size.x),
        pixels_for_axis(visual_size.y),
    )
}

fn paint_layer(data: &mut [u8], width: u32, height: u32, visual_size: Vec2, layer: VisualLayer) {
    let color = color_to_unit_array(layer.color);
    for y in 0..height {
        let world_y = (0.5 - (y as f32 + 0.5) / height as f32) * visual_size.y;
        let local_y = world_y - layer.offset.y;
        if local_y.abs() > layer.size.y * 0.5 {
            continue;
        }

        for x in 0..width {
            let world_x = ((x as f32 + 0.5) / width as f32 - 0.5) * visual_size.x;
            let local_x = world_x - layer.offset.x;
            if !contains_point(layer.primitive, layer.size, Vec2::new(local_x, local_y)) {
                continue;
            }

            let index = ((y * width + x) * 4) as usize;
            blend_pixel(&mut data[index..index + 4], color);
        }
    }
}

fn contains_point(primitive: VisualPrimitive, size: Vec2, point: Vec2) -> bool {
    let half_size = size * 0.5;
    match primitive {
        VisualPrimitive::Rectangle => point.abs().cmple(half_size).all(),
        VisualPrimitive::Ellipse => {
            let normalized = point / half_size;
            normalized.length_squared() <= 1.0
        }
        VisualPrimitive::RoundedRectangle { radius } => {
            let radius = radius.clamp(0.0, half_size.min_element());
            let inner_half_size = (half_size - Vec2::splat(radius)).max(Vec2::ZERO);
            (point.abs() - inner_half_size)
                .max(Vec2::ZERO)
                .length_squared()
                <= radius * radius
        }
    }
}

fn blend_pixel(pixel: &mut [u8], source: [f32; 4]) {
    let destination = [
        f32::from(pixel[0]) / 255.0,
        f32::from(pixel[1]) / 255.0,
        f32::from(pixel[2]) / 255.0,
        f32::from(pixel[3]) / 255.0,
    ];
    let source_alpha = source[3];
    let destination_alpha = destination[3];
    let alpha = source_alpha + destination_alpha * (1.0 - source_alpha);

    let color = if alpha > 0.0 {
        [
            (source[0] * source_alpha + destination[0] * destination_alpha * (1.0 - source_alpha))
                / alpha,
            (source[1] * source_alpha + destination[1] * destination_alpha * (1.0 - source_alpha))
                / alpha,
            (source[2] * source_alpha + destination[2] * destination_alpha * (1.0 - source_alpha))
                / alpha,
        ]
    } else {
        [0.0, 0.0, 0.0]
    };

    pixel[0] = unit_to_u8(color[0]);
    pixel[1] = unit_to_u8(color[1]);
    pixel[2] = unit_to_u8(color[2]);
    pixel[3] = unit_to_u8(alpha);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_size_for_layers_accounts_for_offsets() {
        let layers = [
            VisualLayer {
                size: Vec2::new(4.0, 6.0),
                offset: Vec2::new(5.0, -2.0),
                z: 0.0,
                color: Color::WHITE,
                primitive: VisualPrimitive::Rectangle,
            },
            VisualLayer {
                size: Vec2::new(8.0, 2.0),
                offset: Vec2::new(-1.0, 8.0),
                z: 0.0,
                color: Color::WHITE,
                primitive: VisualPrimitive::Rectangle,
            },
        ];

        assert_eq!(
            visual_size_for_layers(&layers, Vec2::new(3.0, 3.0)),
            Vec2::new(14.0, 18.0)
        );
    }

    #[test]
    fn procedural_primitives_have_non_rectangular_silhouettes() {
        let size = Vec2::splat(2.0);
        let corner = Vec2::new(0.9, 0.9);

        assert!(contains_point(VisualPrimitive::Rectangle, size, corner));
        assert!(!contains_point(VisualPrimitive::Ellipse, size, corner));
        assert!(!contains_point(
            VisualPrimitive::RoundedRectangle { radius: 0.5 },
            size,
            corner
        ));
        assert!(contains_point(
            VisualPrimitive::RoundedRectangle { radius: 0.5 },
            size,
            Vec2::new(0.5, 0.5)
        ));
    }
}
