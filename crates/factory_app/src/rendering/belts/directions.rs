use bevy::prelude::*;
use factory_sim::{Direction, EntityId, Simulation};
use std::collections::HashMap;
use std::time::Instant;

use crate::constants::{
    BELT_DIRECTION_HEAD_SIZE, BELT_DIRECTION_SHAFT_LENGTH, BELT_DIRECTION_SHAFT_WIDTH, TILE_SIZE,
};
use crate::rendering::resources::{BeltDirectionsRenderSyncTime, RenderDetail, VisibleEntityIds};
use crate::rendering::transforms::entity_translation;
use crate::resources::SimResource;

use super::components::{BeltDirectionPart, BeltDirectionSprite};
use super::render_state;

#[derive(Default)]
pub(crate) struct BeltDirectionRenderState {
    initialized: bool,
    showing: bool,
    membership_revision: u64,
    entities: HashMap<(EntityId, BeltDirectionPart), Entity>,
    scratch_ids: Vec<EntityId>,
}

pub(crate) fn sync_belt_direction_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    detail: Res<RenderDetail>,
    mut state: Local<BeltDirectionRenderState>,
    mut sprites: Query<(&mut Transform, &mut Sprite), With<BeltDirectionSprite>>,
) {
    let state = &mut *state;
    let sim = sim.read();
    if !detail.show_belt_directions {
        if state.showing {
            for (_, render_entity) in state.entities.drain() {
                commands.entity(render_entity).despawn();
            }
        }
        state.initialized = true;
        state.showing = false;
        state.membership_revision = visible_entity_ids.membership_revision;
        return;
    }
    if state.initialized && !visible_entity_ids.is_changed() && !detail.is_changed() {
        return;
    }

    let full_refresh = !state.initialized
        || !state.showing
        || (visible_entity_ids.is_changed() && visible_entity_ids.reset);
    if full_refresh {
        for (_, render_entity) in state.entities.drain() {
            commands.entity(render_entity).despawn();
        }
    } else if state.membership_revision != visible_entity_ids.membership_revision {
        for &entity_id in &visible_entity_ids.removed {
            for part in [BeltDirectionPart::Shaft, BeltDirectionPart::Head] {
                if let Some(render_entity) = state.entities.remove(&(entity_id, part)) {
                    commands.entity(render_entity).despawn();
                }
            }
        }
    }

    state.scratch_ids.clear();
    if full_refresh {
        state
            .scratch_ids
            .extend(visible_entity_ids.ids.iter().copied());
    } else {
        state
            .scratch_ids
            .extend(visible_entity_ids.added.iter().copied());
        state
            .scratch_ids
            .extend(visible_entity_ids.style_dirty.iter().copied());
        state.scratch_ids.sort_unstable();
        state.scratch_ids.dedup();
    }

    while let Some(entity_id) = state.scratch_ids.pop() {
        for part in [BeltDirectionPart::Shaft, BeltDirectionPart::Head] {
            let key = (entity_id, part);
            let Some((translation, size, color)) =
                belt_direction_render_state(&sim, entity_id, part)
            else {
                if let Some(render_entity) = state.entities.remove(&key) {
                    commands.entity(render_entity).despawn();
                }
                continue;
            };

            if let Some(&render_entity) = state.entities.get(&key) {
                if let Ok((mut transform, mut sprite)) = sprites.get_mut(render_entity) {
                    if transform.translation != translation {
                        transform.translation = translation;
                    }
                    if sprite.color != color {
                        sprite.color = color;
                    }
                    if sprite.custom_size != Some(size) {
                        sprite.custom_size = Some(size);
                    }
                }
            } else {
                let render_entity = commands
                    .spawn((
                        Sprite::from_color(color, size),
                        Transform::from_translation(translation),
                        BeltDirectionSprite { entity_id, part },
                    ))
                    .id();
                state.entities.insert(key, render_entity);
            }
        }
    }

    state.initialized = true;
    state.showing = true;
    state.membership_revision = visible_entity_ids.membership_revision;
}

pub(crate) fn measured_sync_belt_direction_rendering(
    commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    detail: Res<RenderDetail>,
    state: Local<BeltDirectionRenderState>,
    sprites: Query<(&mut Transform, &mut Sprite), With<BeltDirectionSprite>>,
    mut timing: ResMut<BeltDirectionsRenderSyncTime>,
) {
    let started = Instant::now();
    sync_belt_direction_rendering(commands, sim, visible_entity_ids, detail, state, sprites);
    timing.0 = started.elapsed();
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
