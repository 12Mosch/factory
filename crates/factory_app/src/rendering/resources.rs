use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use factory_sim::{CHUNK_SIZE, ResourceCell, Simulation};
use std::collections::{BTreeMap, BTreeSet};

use crate::constants::RESOURCE_SIZE;
use crate::rendering::colors::{RenderPrototypeIds, resource_color};
use crate::rendering::transforms::tile_translation;
use crate::resources::SimResource;

#[derive(Component)]
pub(crate) struct ResourceSprite {
    x: i32,
    y: i32,
}

#[derive(Component)]
pub(crate) struct ResourceAmountLabel {
    x: i32,
    y: i32,
}

pub(crate) fn sync_resource_debug_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut sprites: Query<(Entity, &ResourceSprite, &mut Sprite)>,
    mut labels: Query<(Entity, &ResourceAmountLabel, &mut Text2d)>,
) {
    if !sim.is_changed() {
        return;
    }

    let ids = RenderPrototypeIds::from_catalog(sim.sim.catalog());
    let resources = collect_resource_tiles(&sim.sim);
    let mut seen_sprites = BTreeSet::new();
    let mut seen_labels = BTreeSet::new();

    for (entity, marker, mut sprite) in &mut sprites {
        let coord = (marker.x, marker.y);
        if let Some(resource) = resources.get(&coord) {
            seen_sprites.insert(coord);
            sprite.color = resource_color(*resource, ids);
        } else {
            commands.entity(entity).despawn();
        }
    }

    for (entity, marker, mut text) in &mut labels {
        let coord = (marker.x, marker.y);
        if let Some(resource) = resources.get(&coord) {
            seen_labels.insert(coord);
            text.0 = format_resource_amount(resource.amount);
        } else {
            commands.entity(entity).despawn();
        }
    }

    for ((x, y), resource) in resources {
        if !seen_sprites.contains(&(x, y)) {
            commands.spawn((
                Sprite::from_color(resource_color(resource, ids), Vec2::splat(RESOURCE_SIZE)),
                Transform::from_translation(tile_translation(x, y, 1.0)),
                ResourceSprite { x, y },
            ));
        }

        if !seen_labels.contains(&(x, y)) {
            commands.spawn((
                Text2d::new(format_resource_amount(resource.amount)),
                TextFont::from_font_size(4.0),
                TextColor(Color::WHITE),
                TextLayout::justify(Justify::Center),
                Transform::from_translation(tile_translation(x, y, 2.0)),
                Anchor::CENTER,
                Text2dShadow::default(),
                ResourceAmountLabel { x, y },
            ));
        }
    }
}

pub(crate) fn collect_resource_tiles(sim: &Simulation) -> BTreeMap<(i32, i32), ResourceCell> {
    let mut resources = BTreeMap::new();

    for chunk in sim.world().chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            if let Some(resource) = tile.resource {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                resources.insert(
                    (
                        chunk.coord.x * CHUNK_SIZE + local_x,
                        chunk.coord.y * CHUNK_SIZE + local_y,
                    ),
                    resource,
                );
            }
        }
    }

    resources
}

pub(crate) fn format_resource_amount(amount: u32) -> String {
    amount.to_string()
}
