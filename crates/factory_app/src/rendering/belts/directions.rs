use bevy::prelude::*;
use factory_sim::{Direction, EntityId, Simulation};
use std::collections::HashSet;
use std::time::Instant;

use crate::constants::{
    BELT_DIRECTION_HEAD_SIZE, BELT_DIRECTION_SHAFT_LENGTH, BELT_DIRECTION_SHAFT_WIDTH, TILE_SIZE,
};
use crate::rendering::resources::{RenderDetail, RenderSyncStats, VisibleEntityIds};
use crate::rendering::transforms::entity_translation;
use crate::resources::SimResource;

use super::components::{BeltDirectionPart, BeltDirectionSprite};
use super::render_state;

pub(crate) fn sync_belt_direction_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    detail: Res<RenderDetail>,
    mut sprites: Query<(Entity, &BeltDirectionSprite, &mut Transform, &mut Sprite)>,
) {
    let sim = sim.read();
    if !detail.show_belt_directions {
        for (entity, _, _, _) in &mut sprites {
            commands.entity(entity).despawn();
        }
        return;
    }
    if !visible_entity_ids.is_changed() && !detail.is_changed() {
        return;
    }

    let visible_ids = &visible_entity_ids.ids;
    let mut seen = HashSet::new();

    for (entity, marker, mut transform, mut sprite) in &mut sprites {
        let key = (marker.entity_id, marker.part);
        if visible_ids.contains(&marker.entity_id)
            && let Some((translation, size, color)) =
                belt_direction_render_state(&sim, marker.entity_id, marker.part)
        {
            seen.insert(key);
            transform.translation = translation;
            sprite.color = color;
            sprite.custom_size = Some(size);
        } else {
            commands.entity(entity).despawn();
        }
    }

    for &entity_id in visible_ids {
        let Some(placed) = sim.entities().placed_entity(entity_id) else {
            continue;
        };
        if factory_sim::entity_access::belt_segment(&sim, placed.id).is_err()
            && factory_sim::entity_access::splitter_state(&sim, placed.id).is_err()
        {
            continue;
        }

        for part in [BeltDirectionPart::Shaft, BeltDirectionPart::Head] {
            let key = (placed.id, part);
            if seen.contains(&key) {
                continue;
            }

            let Some((translation, size, color)) =
                belt_direction_render_state(&sim, placed.id, part)
            else {
                continue;
            };

            commands.spawn((
                Sprite::from_color(color, size),
                Transform::from_translation(translation),
                BeltDirectionSprite {
                    entity_id: placed.id,
                    part,
                },
            ));
        }
    }
}

pub(crate) fn measured_sync_belt_direction_rendering(
    commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    detail: Res<RenderDetail>,
    sprites: Query<(Entity, &BeltDirectionSprite, &mut Transform, &mut Sprite)>,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_belt_direction_rendering(commands, sim, visible_entity_ids, detail, sprites);
    stats.record_belt_directions(started.elapsed());
}

pub(crate) fn belt_direction_render_state(
    sim: &Simulation,
    entity_id: EntityId,
    part: BeltDirectionPart,
) -> Option<(Vec3, Vec2, Color)> {
    let placed = sim.entities().placed_entity(entity_id)?;
    let direction = transport_flow_direction(sim, entity_id)?;
    let center = entity_translation(&placed.footprint, 3.2);
    let along = direction_render_vector(direction);
    let translation = match part {
        BeltDirectionPart::Shaft => {
            let offset = along * TILE_SIZE * -0.06;
            Vec3::new(center.x + offset.x, center.y + offset.y, center.z)
        }
        BeltDirectionPart::Head => {
            let offset = along * TILE_SIZE * 0.24;
            Vec3::new(center.x + offset.x, center.y + offset.y, center.z + 0.1)
        }
    };
    let size = match part {
        BeltDirectionPart::Shaft if along.x.abs() > 0.0 => {
            Vec2::new(BELT_DIRECTION_SHAFT_LENGTH, BELT_DIRECTION_SHAFT_WIDTH)
        }
        BeltDirectionPart::Shaft => {
            Vec2::new(BELT_DIRECTION_SHAFT_WIDTH, BELT_DIRECTION_SHAFT_LENGTH)
        }
        BeltDirectionPart::Head => Vec2::splat(BELT_DIRECTION_HEAD_SIZE),
    };

    Some((translation, size, belt_direction_color()))
}

fn transport_flow_direction(sim: &Simulation, entity_id: EntityId) -> Option<Direction> {
    factory_sim::entity_access::belt_segment(sim, entity_id)
        .ok()
        .map(|segment| segment.dir)
        .or_else(|| {
            factory_sim::entity_access::splitter_state(sim, entity_id)
                .ok()
                .map(|state| state.dir)
        })
}

pub(crate) fn belt_direction_color() -> Color {
    Color::srgba(0.12, 0.08, 0.025, 0.86)
}

pub(crate) fn direction_render_vector(direction: Direction) -> Vec2 {
    render_state::direction_render_vector(direction)
}
