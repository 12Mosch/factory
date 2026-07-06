use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use factory_data::EntityKind;
use factory_sim::Direction;
use std::collections::HashMap;

use crate::constants::TILE_SIZE;

const VISUAL_TEXTURE_PIXELS: u32 = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EntityVisualStyle {
    pub(crate) base_color: Color,
    pub(crate) size: Vec2,
    pub(crate) kind: EntityKind,
    pub(crate) direction: Direction,
}

#[derive(Default, Resource)]
pub(crate) struct VisualAssetCache {
    handles: HashMap<VisualCacheKey, Handle<Image>>,
}

#[derive(SystemParam)]
pub(crate) struct VisualAssets<'w> {
    cache: Option<ResMut<'w, VisualAssetCache>>,
    images: Option<ResMut<'w, Assets<Image>>>,
}

impl VisualAssets<'_> {
    pub(crate) fn entity_sprite(&mut self, style: EntityVisualStyle) -> Sprite {
        self.sprite_for(
            VisualTemplate::Entity {
                kind: style.kind,
                direction: style.direction,
            },
            style.base_color,
            style.size,
        )
    }

    pub(crate) fn belt_item_sprite(&mut self, color: Color, size: Vec2) -> Sprite {
        self.sprite_for(VisualTemplate::BeltItem, color, size)
    }

    pub(crate) fn resource_sprite(&mut self, color: Color, size: Vec2) -> Sprite {
        self.sprite_for(VisualTemplate::Resource, color, size)
    }

    fn sprite_for(&mut self, template: VisualTemplate, color: Color, size: Vec2) -> Sprite {
        let Some(cache) = self.cache.as_deref_mut() else {
            return Sprite::from_color(color, size);
        };
        let Some(images) = self.images.as_deref_mut() else {
            return Sprite::from_color(color, size);
        };

        let key = VisualCacheKey::new(template, color, size);
        let handle = cache
            .handles
            .entry(key)
            .or_insert_with(|| {
                let visual = rasterize_visual(template, color, size);
                images.add(visual.image)
            })
            .clone();
        let visual_size = visual_sprite_size(template, color, size);
        let mut sprite = Sprite::from_image(handle);
        sprite.color = Color::WHITE;
        sprite.custom_size = Some(visual_size);
        sprite
    }
}

pub(crate) fn spawn_entity_visual<B: Bundle>(
    commands: &mut Commands,
    visual_assets: &mut VisualAssets,
    style: EntityVisualStyle,
    translation: Vec3,
    marker: B,
) -> Entity {
    commands
        .spawn((
            visual_assets.entity_sprite(style),
            Transform::from_translation(translation),
            marker,
        ))
        .id()
}

pub(crate) fn spawn_belt_item_visual<B: Bundle>(
    commands: &mut Commands,
    visual_assets: &mut VisualAssets,
    color: Color,
    size: Vec2,
    translation: Vec3,
    marker: B,
) -> Entity {
    commands
        .spawn((
            visual_assets.belt_item_sprite(color, size),
            Transform::from_translation(translation),
            marker,
        ))
        .id()
}

pub(crate) fn spawn_resource_visual<B: Bundle>(
    commands: &mut Commands,
    visual_assets: &mut VisualAssets,
    color: Color,
    size: Vec2,
    translation: Vec3,
    marker: B,
) -> Entity {
    commands
        .spawn((
            visual_assets.resource_sprite(color, size),
            Transform::from_translation(translation),
            marker,
        ))
        .id()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum VisualTemplate {
    Entity {
        kind: EntityKind,
        direction: Direction,
    },
    BeltItem,
    Resource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct VisualCacheKey {
    template: VisualTemplate,
    color: [u8; 4],
    size: [i32; 2],
}

impl VisualCacheKey {
    fn new(template: VisualTemplate, color: Color, size: Vec2) -> Self {
        Self {
            template,
            color: color_key(color),
            size: size_key(size),
        }
    }
}

#[derive(Clone)]
struct RasterizedVisual {
    image: Image,
}

#[derive(Clone, Copy, Debug)]
struct VisualLayer {
    size: Vec2,
    offset: Vec2,
    z: f32,
    color: Color,
}

fn rasterize_visual(template: VisualTemplate, color: Color, size: Vec2) -> RasterizedVisual {
    let layers = visual_layers(template, color, size);
    let visual_size = visual_size_for_layers(&layers, size);
    let mut sorted_layers = layers;
    sorted_layers.sort_by(|a, b| a.z.total_cmp(&b.z));

    let width = VISUAL_TEXTURE_PIXELS;
    let height = VISUAL_TEXTURE_PIXELS;
    let mut data = vec![0; width as usize * height as usize * 4];
    for layer in sorted_layers {
        paint_layer(&mut data, width, height, visual_size, layer);
    }

    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.sampler = ImageSampler::nearest();
    RasterizedVisual { image }
}

fn visual_sprite_size(template: VisualTemplate, color: Color, size: Vec2) -> Vec2 {
    visual_size_for_layers(&visual_layers(template, color, size), size)
}

fn visual_layers(template: VisualTemplate, color: Color, size: Vec2) -> Vec<VisualLayer> {
    let mut layers = Vec::with_capacity(10);
    match template {
        VisualTemplate::Entity { kind, direction } => {
            let style = EntityVisualStyle {
                base_color: color,
                size,
                kind,
                direction,
            };
            spawn_entity_layers(&mut layers, style);
            push_layer(&mut layers, size, Vec2::ZERO, 0.0, color);
        }
        VisualTemplate::BeltItem => {
            push_layer(
                &mut layers,
                Vec2::new(size.x * 1.12, size.y * 1.12),
                Vec2::new(size.x * 0.10, -size.y * 0.10),
                -0.10,
                Color::srgba(0.02, 0.018, 0.014, 0.42),
            );
            push_layer(&mut layers, size, Vec2::ZERO, 0.0, color);
            push_layer(
                &mut layers,
                Vec2::new(size.x * 0.72, size.y * 0.22),
                Vec2::new(0.0, size.y * 0.18),
                0.10,
                Color::srgba(1.0, 0.96, 0.78, 0.30),
            );
            push_layer(
                &mut layers,
                Vec2::new(size.x * 0.22, size.y * 0.70),
                Vec2::ZERO,
                0.12,
                Color::srgba(0.02, 0.02, 0.02, 0.24),
            );
        }
        VisualTemplate::Resource => {
            push_layer(
                &mut layers,
                Vec2::new(size.x * 1.18, size.y * 0.58),
                Vec2::new(size.x * 0.08, -size.y * 0.18),
                -0.08,
                Color::srgba(0.02, 0.018, 0.014, 0.38),
            );
            push_layer(&mut layers, size, Vec2::ZERO, 0.0, color);
            push_layer(
                &mut layers,
                Vec2::new(size.x * 0.52, size.y * 0.22),
                Vec2::new(-size.x * 0.12, size.y * 0.16),
                0.08,
                Color::srgba(1.0, 0.94, 0.74, 0.25),
            );
        }
    }
    layers
}

fn spawn_entity_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    spawn_shadow(layers, style.size);
    spawn_entity_outline(layers, style.size);

    match style.kind {
        EntityKind::TransportBelt => spawn_transport_belt_layers(layers, style),
        EntityKind::Splitter => spawn_splitter_layers(layers, style),
        EntityKind::Chest => spawn_chest_layers(layers, style),
        EntityKind::MiningDrill => spawn_drill_layers(layers, style),
        EntityKind::Furnace => spawn_furnace_layers(layers, style),
        EntityKind::AssemblingMachine => spawn_assembler_layers(layers, style),
        EntityKind::Lab => spawn_lab_layers(layers, style),
        EntityKind::Inserter => spawn_inserter_layers(layers, style),
        EntityKind::ElectricPole => spawn_electric_pole_layers(layers, style),
        EntityKind::SteamEngine => spawn_steam_engine_layers(layers, style),
        EntityKind::Boiler => spawn_boiler_layers(layers, style),
        EntityKind::OffshorePump => spawn_offshore_pump_layers(layers, style),
        EntityKind::Pipe => spawn_pipe_layers(layers, style),
        EntityKind::StorageTank => spawn_storage_tank_layers(layers, style),
        EntityKind::ResourcePatch => {}
    }
}

fn spawn_transport_belt_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    let horizontal = is_horizontal(style.direction);
    let rail = if horizontal {
        Vec2::new(style.size.x * 0.88, style.size.y * 0.13)
    } else {
        Vec2::new(style.size.x * 0.13, style.size.y * 0.88)
    };
    let lane_offset = if horizontal {
        Vec2::new(0.0, style.size.y * 0.23)
    } else {
        Vec2::new(style.size.x * 0.23, 0.0)
    };
    let seam = if horizontal {
        Vec2::new(style.size.x * 0.84, style.size.y * 0.05)
    } else {
        Vec2::new(style.size.x * 0.05, style.size.y * 0.84)
    };

    push_layer(
        layers,
        rail,
        lane_offset,
        0.08,
        Color::srgba(0.09, 0.075, 0.055, 0.64),
    );
    push_layer(
        layers,
        rail,
        -lane_offset,
        0.08,
        Color::srgba(0.09, 0.075, 0.055, 0.64),
    );
    push_layer(
        layers,
        seam,
        Vec2::ZERO,
        0.09,
        Color::srgba(1.0, 0.88, 0.48, 0.36),
    );
}

fn spawn_splitter_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    spawn_transport_belt_layers(layers, style);
    let horizontal = is_horizontal(style.direction);
    let divider = if horizontal {
        Vec2::new(style.size.x * 0.12, style.size.y * 0.82)
    } else {
        Vec2::new(style.size.x * 0.82, style.size.y * 0.12)
    };
    let port = Vec2::splat(TILE_SIZE * 0.22);
    let offset = if horizontal {
        Vec2::new(style.size.x * 0.30, 0.0)
    } else {
        Vec2::new(0.0, style.size.y * 0.30)
    };

    push_layer(
        layers,
        divider,
        Vec2::ZERO,
        0.14,
        Color::srgba(0.10, 0.08, 0.06, 0.70),
    );
    push_layer(
        layers,
        port,
        offset,
        0.16,
        Color::srgba(0.95, 0.90, 0.68, 0.48),
    );
    push_layer(
        layers,
        port,
        -offset,
        0.16,
        Color::srgba(0.95, 0.90, 0.68, 0.48),
    );
}

fn spawn_chest_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.82, style.size.y * 0.18),
        Vec2::new(0.0, style.size.y * 0.18),
        0.10,
        tinted(style.base_color, 0.24),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.18, style.size.y * 0.30),
        Vec2::ZERO,
        0.12,
        Color::srgba(0.95, 0.74, 0.38, 0.72),
    );
}

fn spawn_drill_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.74, style.size.y * 0.24),
        Vec2::new(0.0, style.size.y * 0.18),
        0.10,
        Color::srgba(0.13, 0.15, 0.14, 0.72),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.22, style.size.y * 0.70),
        direction_offset(style.direction, style.size * 0.14),
        0.12,
        Color::srgba(0.82, 0.68, 0.38, 0.84),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.40, style.size.y * 0.16),
        -direction_offset(style.direction, style.size * 0.20),
        0.12,
        Color::srgba(0.10, 0.10, 0.09, 0.66),
    );
}

fn spawn_furnace_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.46, style.size.y * 0.34),
        Vec2::new(0.0, -style.size.y * 0.10),
        0.10,
        Color::srgba(0.95, 0.36, 0.12, 0.72),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.18, style.size.y * 0.58),
        Vec2::new(style.size.x * 0.26, style.size.y * 0.05),
        0.11,
        Color::srgba(0.13, 0.12, 0.11, 0.72),
    );
}

fn spawn_assembler_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.66, style.size.y * 0.12),
        Vec2::ZERO,
        0.10,
        Color::srgba(0.74, 0.88, 0.92, 0.50),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.12, style.size.y * 0.66),
        Vec2::ZERO,
        0.11,
        Color::srgba(0.74, 0.88, 0.92, 0.50),
    );
    push_layer(
        layers,
        Vec2::splat(TILE_SIZE * 0.26),
        Vec2::ZERO,
        0.12,
        Color::srgba(0.09, 0.12, 0.13, 0.68),
    );
}

fn spawn_lab_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.62, style.size.y * 0.38),
        Vec2::new(0.0, style.size.y * 0.04),
        0.10,
        Color::srgba(0.42, 0.86, 0.78, 0.52),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.82, style.size.y * 0.10),
        Vec2::new(0.0, -style.size.y * 0.28),
        0.11,
        Color::srgba(0.92, 0.80, 0.44, 0.42),
    );
}

fn spawn_inserter_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    let along = direction_vec(style.direction);
    let horizontal = is_horizontal(style.direction);
    let arm = if horizontal {
        Vec2::new(style.size.x * 0.64, style.size.y * 0.16)
    } else {
        Vec2::new(style.size.x * 0.16, style.size.y * 0.64)
    };
    let hand = Vec2::splat(TILE_SIZE * 0.22);

    push_layer(
        layers,
        Vec2::splat(TILE_SIZE * 0.44),
        Vec2::ZERO,
        0.08,
        Color::srgba(0.12, 0.10, 0.07, 0.68),
    );
    push_layer(
        layers,
        arm,
        along * TILE_SIZE * 0.10,
        0.12,
        Color::srgba(0.88, 0.72, 0.32, 0.86),
    );
    push_layer(
        layers,
        hand,
        along * TILE_SIZE * 0.34,
        0.14,
        Color::srgba(0.12, 0.10, 0.08, 0.76),
    );
}

fn spawn_electric_pole_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.20, style.size.y * 0.82),
        Vec2::ZERO,
        0.10,
        Color::srgba(0.18, 0.12, 0.08, 0.82),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.78, style.size.y * 0.14),
        Vec2::new(0.0, style.size.y * 0.20),
        0.12,
        Color::srgba(0.96, 0.82, 0.42, 0.72),
    );
}

fn spawn_steam_engine_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.78, style.size.y * 0.24),
        Vec2::ZERO,
        0.10,
        Color::srgba(0.70, 0.86, 0.90, 0.40),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.18, style.size.y * 0.70),
        Vec2::new(style.size.x * 0.28, 0.0),
        0.12,
        Color::srgba(0.12, 0.16, 0.17, 0.60),
    );
}

fn spawn_boiler_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.72, style.size.y * 0.26),
        Vec2::new(0.0, -style.size.y * 0.12),
        0.10,
        Color::srgba(0.96, 0.48, 0.16, 0.60),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.68, style.size.y * 0.16),
        Vec2::new(0.0, style.size.y * 0.22),
        0.11,
        Color::srgba(0.20, 0.22, 0.22, 0.70),
    );
}

fn spawn_offshore_pump_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.62, style.size.y * 0.32),
        Vec2::new(0.0, -style.size.y * 0.12),
        0.10,
        Color::srgba(0.48, 0.82, 0.94, 0.58),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.20, style.size.y * 0.72),
        Vec2::new(style.size.x * 0.24, 0.0),
        0.11,
        Color::srgba(0.08, 0.16, 0.20, 0.58),
    );
}

fn spawn_pipe_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.82, style.size.y * 0.18),
        Vec2::ZERO,
        0.10,
        Color::srgba(0.78, 0.90, 0.94, 0.40),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.18, style.size.y * 0.82),
        Vec2::ZERO,
        0.11,
        Color::srgba(0.20, 0.24, 0.25, 0.38),
    );
}

fn spawn_storage_tank_layers(layers: &mut Vec<VisualLayer>, style: EntityVisualStyle) {
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.74, style.size.y * 0.18),
        Vec2::new(0.0, style.size.y * 0.20),
        0.10,
        Color::srgba(0.78, 0.90, 0.92, 0.46),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.74, style.size.y * 0.18),
        Vec2::new(0.0, -style.size.y * 0.20),
        0.10,
        Color::srgba(0.18, 0.23, 0.24, 0.42),
    );
    push_layer(
        layers,
        Vec2::new(style.size.x * 0.18, style.size.y * 0.76),
        Vec2::new(style.size.x * 0.25, 0.0),
        0.11,
        Color::srgba(0.12, 0.16, 0.17, 0.42),
    );
}

fn spawn_shadow(layers: &mut Vec<VisualLayer>, size: Vec2) {
    push_layer(
        layers,
        Vec2::new(size.x * 1.04, size.y * 1.04),
        Vec2::new(TILE_SIZE * 0.06, -TILE_SIZE * 0.06),
        -0.16,
        Color::srgba(0.015, 0.012, 0.010, 0.46),
    );
}

fn spawn_entity_outline(layers: &mut Vec<VisualLayer>, size: Vec2) {
    push_layer(
        layers,
        Vec2::new(size.x * 1.02, size.y * 1.02),
        Vec2::ZERO,
        -0.08,
        Color::srgba(0.035, 0.030, 0.026, 0.56),
    );
    push_layer(
        layers,
        Vec2::new(size.x * 0.78, size.y * 0.10),
        Vec2::new(0.0, size.y * 0.36),
        0.08,
        Color::srgba(1.0, 0.95, 0.72, 0.18),
    );
}

fn push_layer(layers: &mut Vec<VisualLayer>, size: Vec2, offset: Vec2, z: f32, color: Color) {
    layers.push(VisualLayer {
        size,
        offset,
        z,
        color,
    });
}

fn visual_size_for_layers(layers: &[VisualLayer], fallback_size: Vec2) -> Vec2 {
    layers.iter().fold(fallback_size, |size, layer| {
        let half = layer.size * 0.5 + layer.offset.abs();
        size.max(half * 2.0)
    })
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
            if local_x.abs() > layer.size.x * 0.5 {
                continue;
            }

            let index = ((y * width + x) * 4) as usize;
            blend_pixel(&mut data[index..index + 4], color);
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

fn color_to_unit_array(color: Color) -> [f32; 4] {
    let color = color.to_srgba();
    [color.red, color.green, color.blue, color.alpha]
}

fn color_key(color: Color) -> [u8; 4] {
    let color = color_to_unit_array(color);
    [
        unit_to_u8(color[0]),
        unit_to_u8(color[1]),
        unit_to_u8(color[2]),
        unit_to_u8(color[3]),
    ]
}

fn size_key(size: Vec2) -> [i32; 2] {
    [
        (size.x * 100.0).round() as i32,
        (size.y * 100.0).round() as i32,
    ]
}

fn unit_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn is_horizontal(direction: Direction) -> bool {
    matches!(direction, Direction::East | Direction::West)
}

fn direction_vec(direction: Direction) -> Vec2 {
    match direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => Vec2::NEG_Y,
        Direction::West => Vec2::NEG_X,
    }
}

fn direction_offset(direction: Direction, size: Vec2) -> Vec2 {
    let along = direction_vec(direction);
    Vec2::new(along.x * size.x, along.y * size.y)
}

fn tinted(color: Color, amount: f32) -> Color {
    let color = color.to_srgba();
    Color::srgba(
        color.red + (1.0 - color.red) * amount,
        color.green + (1.0 - color.green) * amount,
        color.blue + (1.0 - color.blue) * amount,
        color.alpha,
    )
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

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= f32::EPSILON,
            "expected {actual} to equal {expected}"
        );
    }
}
