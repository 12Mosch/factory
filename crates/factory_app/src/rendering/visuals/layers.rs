use bevy::prelude::*;
use factory_sim::Direction;

use crate::constants::TILE_SIZE;

#[derive(Clone, Copy, Debug)]
pub(super) struct VisualLayer {
    pub(super) size: Vec2,
    pub(super) offset: Vec2,
    pub(super) z: f32,
    pub(super) color: Color,
    pub(super) primitive: VisualPrimitive,
}

/// A rasterizable silhouette for a visual layer.
///
/// Keeping primitives in world units lets recipes remain resolution independent while the
/// rasterizer can choose an appropriate texture resolution for each entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum VisualPrimitive {
    Rectangle,
    RoundedRectangle { radius: f32 },
    Ellipse,
}

pub(super) struct VisualLayerBuilder {
    layers: Vec<VisualLayer>,
    base_size: Vec2,
}

impl VisualLayerBuilder {
    pub(super) fn new(base_size: Vec2) -> Self {
        Self {
            layers: Vec::with_capacity(10),
            base_size,
        }
    }

    pub(super) fn rect(&mut self, size: Vec2, offset: Vec2, z: f32, color: Color) -> &mut Self {
        self.push(size, offset, z, color, VisualPrimitive::Rectangle)
    }

    pub(super) fn rounded_rect(
        &mut self,
        size: Vec2,
        offset: Vec2,
        z: f32,
        color: Color,
        radius: f32,
    ) -> &mut Self {
        self.push(
            size,
            offset,
            z,
            color,
            VisualPrimitive::RoundedRectangle { radius },
        )
    }

    pub(super) fn ellipse(&mut self, size: Vec2, offset: Vec2, z: f32, color: Color) -> &mut Self {
        self.push(size, offset, z, color, VisualPrimitive::Ellipse)
    }

    fn push(
        &mut self,
        size: Vec2,
        offset: Vec2,
        z: f32,
        color: Color,
        primitive: VisualPrimitive,
    ) -> &mut Self {
        self.layers.push(VisualLayer {
            size,
            offset,
            z,
            color,
            primitive,
        });
        self
    }

    pub(super) fn scaled(
        &mut self,
        size_scale: Vec2,
        offset_scale: Vec2,
        z: f32,
        color: Color,
    ) -> &mut Self {
        self.rect(
            self.base_size * size_scale,
            self.base_size * offset_scale,
            z,
            color,
        )
    }

    pub(super) fn scaled_rounded(
        &mut self,
        size_scale: Vec2,
        offset_scale: Vec2,
        z: f32,
        color: Color,
        radius_scale: f32,
    ) -> &mut Self {
        let size = self.base_size * size_scale;
        self.rounded_rect(
            size,
            self.base_size * offset_scale,
            z,
            color,
            size.min_element() * radius_scale,
        )
    }

    pub(super) fn scaled_ellipse(
        &mut self,
        size_scale: Vec2,
        offset_scale: Vec2,
        z: f32,
        color: Color,
    ) -> &mut Self {
        self.ellipse(
            self.base_size * size_scale,
            self.base_size * offset_scale,
            z,
            color,
        )
    }

    pub(super) fn tile(
        &mut self,
        size_tiles: Vec2,
        offset_tiles: Vec2,
        z: f32,
        color: Color,
    ) -> &mut Self {
        self.rect(size_tiles * TILE_SIZE, offset_tiles * TILE_SIZE, z, color)
    }

    pub(super) fn oriented(
        &mut self,
        sizes: (Vec2, Vec2),
        offsets: (Vec2, Vec2),
        direction: Direction,
        z: f32,
        color: Color,
    ) -> &mut Self {
        let (horizontal_size, vertical_size) = sizes;
        let (horizontal_offset, vertical_offset) = offsets;
        if is_horizontal(direction) {
            self.scaled(horizontal_size, horizontal_offset, z, color)
        } else {
            self.scaled(vertical_size, vertical_offset, z, color)
        }
    }

    pub(super) fn directional_offset(&self, direction: Direction, size_scale: Vec2) -> Vec2 {
        direction_offset(direction, self.base_size * size_scale)
    }

    pub(super) fn finish(self) -> Vec<VisualLayer> {
        self.layers
    }
}

pub(super) fn is_horizontal(direction: Direction) -> bool {
    matches!(direction, Direction::East | Direction::West)
}

pub(super) fn direction_vec(direction: Direction) -> Vec2 {
    match direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => Vec2::NEG_Y,
        Direction::West => Vec2::NEG_X,
    }
}

pub(super) fn direction_offset(direction: Direction, size: Vec2) -> Vec2 {
    let along = direction_vec(direction);
    Vec2::new(along.x * size.x, along.y * size.y)
}

pub(super) fn tinted(color: Color, amount: f32) -> Color {
    let color = color.to_srgba();
    Color::srgba(
        color.red + (1.0 - color.red) * amount,
        color.green + (1.0 - color.green) * amount,
        color.blue + (1.0 - color.blue) * amount,
        color.alpha,
    )
}

pub(super) fn color_to_unit_array(color: Color) -> [f32; 4] {
    let color = color.to_srgba();
    [color.red, color.green, color.blue, color.alpha]
}

pub(super) fn unit_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_horizontal_identifies_east_and_west() {
        assert!(!is_horizontal(Direction::North));
        assert!(is_horizontal(Direction::East));
        assert!(!is_horizontal(Direction::South));
        assert!(is_horizontal(Direction::West));
    }

    #[test]
    fn direction_vec_maps_cardinal_axes() {
        assert_eq!(direction_vec(Direction::North), Vec2::Y);
        assert_eq!(direction_vec(Direction::East), Vec2::X);
        assert_eq!(direction_vec(Direction::South), Vec2::NEG_Y);
        assert_eq!(direction_vec(Direction::West), Vec2::NEG_X);
    }

    #[test]
    fn direction_offset_applies_axis_specific_size() {
        let size = Vec2::new(3.0, 5.0);

        assert_eq!(
            direction_offset(Direction::North, size),
            Vec2::new(0.0, 5.0)
        );
        assert_eq!(direction_offset(Direction::East, size), Vec2::new(3.0, 0.0));
        assert_eq!(
            direction_offset(Direction::South, size),
            Vec2::new(0.0, -5.0)
        );
        assert_eq!(
            direction_offset(Direction::West, size),
            Vec2::new(-3.0, 0.0)
        );
    }

    #[test]
    fn tinted_moves_color_toward_white_and_preserves_alpha() {
        let original = Color::srgba(0.20, 0.40, 0.60, 0.70);
        let tinted = tinted(original, 0.25).to_srgba();

        assert_close(tinted.red, 0.40);
        assert_close(tinted.green, 0.55);
        assert_close(tinted.blue, 0.70);
        assert_close(tinted.alpha, 0.70);
    }

    #[test]
    fn tinted_handles_identity_and_full_white_edges() {
        let original = Color::srgba(0.20, 0.40, 0.60, 0.70);
        let unchanged = tinted(original, 0.0).to_srgba();
        let white = tinted(original, 1.0).to_srgba();

        assert_close(unchanged.red, 0.20);
        assert_close(unchanged.green, 0.40);
        assert_close(unchanged.blue, 0.60);
        assert_close(unchanged.alpha, 0.70);
        assert_close(white.red, 1.0);
        assert_close(white.green, 1.0);
        assert_close(white.blue, 1.0);
        assert_close(white.alpha, 0.70);
    }

    #[test]
    fn builder_scaled_matches_manual_multiplication() {
        let color = Color::WHITE;
        let mut builder = VisualLayerBuilder::new(Vec2::new(10.0, 20.0));

        builder.scaled(Vec2::new(0.25, 0.50), Vec2::new(-0.10, 0.20), 0.5, color);

        let layers = builder.finish();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].size, Vec2::new(2.5, 10.0));
        assert_eq!(layers[0].offset, Vec2::new(-1.0, 4.0));
        assert_eq!(layers[0].z, 0.5);
        assert_eq!(layers[0].primitive, VisualPrimitive::Rectangle);
    }

    #[test]
    fn rounded_scaled_uses_the_smaller_axis_for_its_radius() {
        let mut builder = VisualLayerBuilder::new(Vec2::new(10.0, 20.0));
        builder.scaled_rounded(Vec2::new(0.80, 0.50), Vec2::ZERO, 0.0, Color::WHITE, 0.25);

        let layer = builder.finish()[0];
        assert_eq!(layer.size, Vec2::new(8.0, 10.0));
        assert_eq!(
            layer.primitive,
            VisualPrimitive::RoundedRectangle { radius: 2.0 }
        );
    }

    #[test]
    fn builder_oriented_selects_axis_specs() {
        let color = Color::WHITE;
        let horizontal_size = Vec2::new(0.80, 0.20);
        let vertical_size = Vec2::new(0.20, 0.80);
        let horizontal_offset = Vec2::new(0.10, 0.0);
        let vertical_offset = Vec2::new(0.0, 0.10);
        let base_size = Vec2::new(100.0, 200.0);

        for direction in [Direction::East, Direction::West] {
            let mut builder = VisualLayerBuilder::new(base_size);
            builder.oriented(
                (horizontal_size, vertical_size),
                (horizontal_offset, vertical_offset),
                direction,
                0.0,
                color,
            );
            let layer = builder.finish()[0];
            assert_eq!(layer.size, base_size * horizontal_size);
            assert_eq!(layer.offset, base_size * horizontal_offset);
        }

        for direction in [Direction::North, Direction::South] {
            let mut builder = VisualLayerBuilder::new(base_size);
            builder.oriented(
                (horizontal_size, vertical_size),
                (horizontal_offset, vertical_offset),
                direction,
                0.0,
                color,
            );
            let layer = builder.finish()[0];
            assert_eq!(layer.size, base_size * vertical_size);
            assert_eq!(layer.offset, base_size * vertical_offset);
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-5,
            "expected {actual} to equal {expected}"
        );
    }
}
