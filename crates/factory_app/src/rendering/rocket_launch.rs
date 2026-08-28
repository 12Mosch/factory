//! Presentation for a rocket leaving a silo.
//!
//! Launch completion lives in the simulation. This module only mirrors the
//! fixed-tick [`factory_sim::RocketLaunchPhase::Rising`] progress into a
//! transient sprite, pooling those sprites so repeated launches and world
//! reloads do not leak entities.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_sim::{EntityFootprint, EntityId};
use std::collections::HashSet;

use crate::constants::TILE_SIZE;
use crate::rendering::resources::VisibleEntityIds;
use crate::rendering::transforms::entity_translation;
use crate::rendering::visuals::VisualAssets;
use crate::resources::SimResource;
use crate::save_load::PresentationReloadToken;

const ROCKET_SPRITE_Z: f32 = 5.0;
const RISE_HEIGHT: f32 = TILE_SIZE * 18.0;
const ROCKET_SIZE: Vec2 = Vec2::new(TILE_SIZE * 0.5, TILE_SIZE * 1.7);

#[derive(Component)]
pub(crate) struct RocketLaunchSprite {
    pub(crate) silo_id: EntityId,
}

#[derive(Resource, Default)]
pub(crate) struct RocketLaunchRenderPool {
    unused: Vec<Entity>,
    last_reload_token: u64,
}

#[derive(SystemParam)]
pub(crate) struct RocketLaunchRenderParams<'w> {
    sim: Res<'w, SimResource>,
    visible_entity_ids: Res<'w, VisibleEntityIds>,
    reload: Option<Res<'w, PresentationReloadToken>>,
    pool: ResMut<'w, RocketLaunchRenderPool>,
    fixed_time: Option<Res<'w, Time<Fixed>>>,
}

pub(crate) fn rocket_rise_translation(footprint: &EntityFootprint, progress: f32) -> Vec3 {
    let mut translation = entity_translation(footprint, ROCKET_SPRITE_Z);
    translation.y += progress.clamp(0.0, 1.0) * RISE_HEIGHT;
    translation
}

pub(crate) fn sync_rocket_launch_rendering(
    mut commands: Commands,
    params: RocketLaunchRenderParams,
    mut visual_assets: VisualAssets,
    mut sprites: Query<(
        Entity,
        &mut RocketLaunchSprite,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    let RocketLaunchRenderParams {
        sim,
        visible_entity_ids,
        reload,
        mut pool,
        fixed_time,
    } = params;
    let reload_token = reload.as_deref().map_or(0, |token| token.value);
    let overstep = fixed_time
        .as_deref()
        .map_or(0.0, Time::<Fixed>::overstep_fraction)
        .clamp(0.0, 1.0);
    let sim = sim.read();
    let needed = visible_rising_rockets(&sim, &visible_entity_ids.ids, overstep);

    if reload_token != pool.last_reload_token {
        for entity in pool.unused.drain(..) {
            commands.entity(entity).despawn();
        }
        for (entity, _, _, _) in &sprites {
            commands.entity(entity).despawn();
        }
        pool.last_reload_token = reload_token;
        for (entity_id, translation) in needed {
            commands.spawn((
                visual_assets.launch_rocket_sprite(ROCKET_SIZE),
                Transform::from_translation(translation),
                Visibility::Visible,
                RocketLaunchSprite { silo_id: entity_id },
            ));
        }
        return;
    }

    let mut assigned = HashSet::new();
    for (entity, sprite, mut transform, mut visibility) in &mut sprites {
        if *visibility == Visibility::Hidden {
            continue;
        }
        if let Some((_, translation)) = needed
            .iter()
            .find(|(entity_id, _)| *entity_id == sprite.silo_id)
        {
            transform.translation = *translation;
            assigned.insert(sprite.silo_id);
        } else {
            *visibility = Visibility::Hidden;
            pool.unused.push(entity);
        }
    }

    for (entity_id, translation) in needed {
        if assigned.contains(&entity_id) {
            continue;
        }
        if let Some(entity) = pool.unused.pop()
            && let Ok((_, mut sprite, mut transform, mut visibility)) = sprites.get_mut(entity)
        {
            sprite.silo_id = entity_id;
            *visibility = Visibility::Visible;
            transform.translation = translation;
            continue;
        }
        commands.spawn((
            visual_assets.launch_rocket_sprite(ROCKET_SIZE),
            Transform::from_translation(translation),
            Visibility::Visible,
            RocketLaunchSprite { silo_id: entity_id },
        ));
    }
}

fn visible_rising_rockets(
    sim: &factory_sim::Simulation,
    visible_ids: &HashSet<EntityId>,
    overstep: f32,
) -> Vec<(EntityId, Vec3)> {
    let mut rockets = visible_ids
        .iter()
        .copied()
        .filter_map(|entity_id| {
            let progress = rising_progress(sim, entity_id, overstep)?;
            let placed = sim.entities().placed_entity(entity_id)?;
            Some((
                entity_id,
                rocket_rise_translation(&placed.footprint, progress),
            ))
        })
        .collect::<Vec<_>>();
    rockets.sort_unstable_by_key(|(entity_id, _)| entity_id.raw());
    rockets
}

fn rising_progress(
    sim: &factory_sim::Simulation,
    entity_id: EntityId,
    overstep: f32,
) -> Option<f32> {
    factory_sim::entity_access::rocket_silo_state(sim, entity_id)
        .ok()?
        .launch_phase
        .rise_progress(overstep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::{EntityFootprint, LAUNCH_RISE_TICKS, RocketLaunchPhase};

    #[test]
    fn rocket_height_follows_phase_progress_after_skipped_ticks() {
        let footprint = EntityFootprint {
            x: 0,
            y: 0,
            width: 9,
            height: 9,
        };
        let early = RocketLaunchPhase::Rising {
            ticks_remaining: LAUNCH_RISE_TICKS - 10,
        }
        .rise_progress(0.0)
        .expect("rising");
        let later = RocketLaunchPhase::Rising {
            ticks_remaining: LAUNCH_RISE_TICKS - 70,
        }
        .rise_progress(0.0)
        .expect("rising");

        let early_y = rocket_rise_translation(&footprint, early).y;
        let later_y = rocket_rise_translation(&footprint, later).y;
        assert!(later_y > early_y);
    }
}
