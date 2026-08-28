mod cache;
mod layers;
mod rasterizer;
mod recipes;
mod templates;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_data::EntityKind;
use factory_sim::{Direction, RailPieceGeometry};

pub(crate) use cache::VisualAssetCache;
use cache::VisualCacheKey;
use rasterizer::rasterize_visual;
use templates::VisualTemplate;
pub(crate) use templates::{ConnectionMask, RocketSiloVisualPhase};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EntityVisualStyle {
    pub(crate) base_color: Color,
    pub(crate) size: Vec2,
    pub(crate) kind: EntityKind,
    pub(crate) direction: Direction,
    pub(crate) connections: ConnectionMask,
    /// Travel geometry for track, taken from the simulation and already rotated
    /// into this placement's frame. The drawn curve is this curve; the renderer
    /// authors no offsets of its own.
    pub(crate) rail: Option<RailPieceGeometry>,
    pub(crate) rocket_silo_phase: RocketSiloVisualPhase,
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
                connections: style.connections,
                rail: style.rail,
                rocket_silo_phase: style.rocket_silo_phase,
            },
            style.base_color,
            style.size,
        )
    }

    pub(crate) fn launch_rocket_sprite(&mut self, size: Vec2) -> Sprite {
        self.sprite_for(
            VisualTemplate::LaunchRocket,
            Color::srgb(0.90, 0.88, 0.82),
            size,
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
        let cached = cache.get_or_create(key, images, || rasterize_visual(template, color, size));
        let mut sprite = Sprite::from_image(cached.handle);
        sprite.color = Color::WHITE;
        sprite.custom_size = Some(cached.visual_size);
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
    let sprite = visual_assets.entity_sprite(style);
    spawn_visual(commands, sprite, translation, marker)
}

pub(crate) fn spawn_belt_item_visual<B: Bundle>(
    commands: &mut Commands,
    visual_assets: &mut VisualAssets,
    color: Color,
    size: Vec2,
    translation: Vec3,
    marker: B,
) -> Entity {
    let sprite = visual_assets.belt_item_sprite(color, size);
    spawn_visual(commands, sprite, translation, marker)
}

pub(crate) fn spawn_resource_visual<B: Bundle>(
    commands: &mut Commands,
    visual_assets: &mut VisualAssets,
    color: Color,
    size: Vec2,
    translation: Vec3,
    marker: B,
) -> Entity {
    let sprite = visual_assets.resource_sprite(color, size);
    spawn_visual(commands, sprite, translation, marker)
}

fn spawn_visual<B: Bundle>(
    commands: &mut Commands,
    sprite: Sprite,
    translation: Vec3,
    marker: B,
) -> Entity {
    commands
        .spawn((sprite, Transform::from_translation(translation), marker))
        .id()
}
