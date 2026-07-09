use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_data::{BasePrototypeIds, ItemId};
#[cfg(test)]
use factory_sim::BELT_SUBTILES_PER_TILE;
use factory_sim::{EntityId, Simulation};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::constants::BELT_ITEM_SPRITE_SIZE;
#[cfg(test)]
use crate::constants::TILE_SIZE;
use crate::rendering::resources::{
    BeltItemRenderPool, RenderDetail, RenderSyncStats, VisibleEntityIds,
};
use crate::rendering::transforms::tile_translation;
use crate::rendering::visuals::{VisualAssets, spawn_belt_item_visual};
use crate::resources::SimResource;

use super::components::{BeltItemKey, BeltItemLabel, BeltItemSprite, VisibleBeltItemRenderState};
use super::labels::{belt_item_label, label_translation, spawn_or_reuse_belt_item_label};
#[cfg(test)]
use super::render_state::direction_render_vector;
use super::render_state::{splitter_port_tiles_for_render, transport_item_render_state_from_parts};

#[derive(Default)]
struct BeltItemRenderScratch {
    visible_items: Vec<VisibleBeltItemRenderState>,
    visible_by_key: HashMap<BeltItemKey, usize>,
    seen_sprites: Vec<bool>,
    seen_labels: Vec<bool>,
}

#[derive(SystemParam)]
pub(crate) struct BeltItemRenderParams<'w, 's> {
    commands: Commands<'w, 's>,
    sim: Res<'w, SimResource>,
    visible_entity_ids: Res<'w, VisibleEntityIds>,
    detail: Res<'w, RenderDetail>,
    pool: ResMut<'w, BeltItemRenderPool>,
    visual_assets: VisualAssets<'w>,
    scratch: Local<'s, BeltItemRenderScratch>,
    sprites: Query<
        'w,
        's,
        (
            Entity,
            &'static mut BeltItemSprite,
            &'static mut Transform,
            &'static mut Sprite,
            &'static mut Visibility,
        ),
        Without<BeltItemLabel>,
    >,
    labels: Query<
        'w,
        's,
        (
            Entity,
            &'static mut BeltItemLabel,
            &'static mut Transform,
            &'static mut Text2d,
            &'static mut Visibility,
        ),
        Without<BeltItemSprite>,
    >,
}

pub(crate) fn sync_belt_item_rendering(params: BeltItemRenderParams) {
    let BeltItemRenderParams {
        mut commands,
        sim,
        visible_entity_ids,
        detail,
        mut pool,
        mut visual_assets,
        mut scratch,
        mut sprites,
        mut labels,
    } = params;

    if !detail.show_belt_items {
        if detail.is_changed() {
            pool_all_belt_items(&mut pool, &mut sprites, &mut labels);
        }
        return;
    }
    if !sim.is_changed() && !visible_entity_ids.is_changed() && !detail.is_changed() {
        return;
    }

    let ids = BasePrototypeIds::from_catalog(sim.sim.catalog());
    collect_visible_belt_items_into(
        &sim.sim,
        ids,
        &visible_entity_ids.ids,
        &mut scratch.visible_items,
    );
    sync_belt_item_entity_pool(
        &mut commands,
        &sim.sim,
        &mut pool,
        &mut visual_assets,
        detail.show_belt_item_labels,
        &mut scratch,
        &mut sprites,
        &mut labels,
    );
}

pub(crate) fn measured_sync_belt_item_rendering(
    params: BeltItemRenderParams,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_belt_item_rendering(params);
    stats.record_belt_items(started.elapsed());
}

pub(super) fn pool_all_belt_items(
    pool: &mut BeltItemRenderPool,
    sprites: &mut Query<
        (
            Entity,
            &mut BeltItemSprite,
            &mut Transform,
            &mut Sprite,
            &mut Visibility,
        ),
        Without<BeltItemLabel>,
    >,
    labels: &mut Query<
        (
            Entity,
            &mut BeltItemLabel,
            &mut Transform,
            &mut Text2d,
            &mut Visibility,
        ),
        Without<BeltItemSprite>,
    >,
) {
    for (entity, mut marker, _, _, mut visibility) in sprites {
        if marker.active {
            marker.active = false;
            *visibility = Visibility::Hidden;
            push_unique(&mut pool.sprites, entity);
        }
    }
    for (entity, mut marker, _, _, mut visibility) in labels {
        if marker.active {
            marker.active = false;
            *visibility = Visibility::Hidden;
            push_unique(&mut pool.labels, entity);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_belt_item_entity_pool(
    commands: &mut Commands,
    sim: &Simulation,
    pool: &mut BeltItemRenderPool,
    visual_assets: &mut VisualAssets,
    show_labels: bool,
    scratch: &mut BeltItemRenderScratch,
    sprites: &mut Query<
        (
            Entity,
            &mut BeltItemSprite,
            &mut Transform,
            &mut Sprite,
            &mut Visibility,
        ),
        Without<BeltItemLabel>,
    >,
    labels: &mut Query<
        (
            Entity,
            &mut BeltItemLabel,
            &mut Transform,
            &mut Text2d,
            &mut Visibility,
        ),
        Without<BeltItemSprite>,
    >,
) {
    let visible_items = &scratch.visible_items;
    let item_count = visible_items.len();
    scratch.visible_by_key.clear();
    scratch.visible_by_key.reserve(item_count);
    for (index, item) in visible_items.iter().enumerate() {
        scratch.visible_by_key.insert(item.key, index);
    }

    scratch.seen_sprites.clear();
    scratch.seen_sprites.resize(item_count, false);

    for (entity, mut marker, mut transform, mut sprite, mut visibility) in &mut *sprites {
        if marker.active
            && let Some(&index) = scratch.visible_by_key.get(&marker.key)
        {
            let item = visible_items[index];
            scratch.seen_sprites[index] = true;
            transform.translation = item.translation;
            if marker.item_id != item.item_id {
                marker.item_id = item.item_id;
                *sprite =
                    visual_assets.belt_item_sprite(item.color, Vec2::splat(BELT_ITEM_SPRITE_SIZE));
            }
            *visibility = Visibility::Visible;
        } else if marker.active {
            marker.active = false;
            *visibility = Visibility::Hidden;
            push_unique(&mut pool.sprites, entity);
        }
    }

    for (index, item) in visible_items.iter().enumerate() {
        if scratch.seen_sprites[index] {
            continue;
        }
        spawn_or_reuse_belt_item_sprite(commands, pool, visual_assets, *item);
    }

    if show_labels {
        scratch.seen_labels.clear();
        scratch.seen_labels.resize(item_count, false);

        for (entity, mut marker, mut transform, mut text, mut visibility) in &mut *labels {
            if marker.active
                && let Some(&index) = scratch.visible_by_key.get(&marker.key)
            {
                let item = visible_items[index];
                scratch.seen_labels[index] = true;
                transform.translation = label_translation(item.translation);
                if marker.item_id != item.item_id {
                    marker.item_id = item.item_id;
                    text.0 = belt_item_label(sim, item.item_id);
                }
                *visibility = Visibility::Visible;
            } else if marker.active {
                marker.active = false;
                *visibility = Visibility::Hidden;
                push_unique(&mut pool.labels, entity);
            }
        }

        for (index, item) in visible_items.iter().enumerate() {
            if scratch.seen_labels[index] {
                continue;
            }
            spawn_or_reuse_belt_item_label(commands, sim, pool, *item);
        }
    } else {
        for (entity, mut marker, _, _, mut visibility) in &mut *labels {
            if marker.active {
                marker.active = false;
                *visibility = Visibility::Hidden;
                push_unique(&mut pool.labels, entity);
            }
        }
    }
}

pub(super) fn spawn_or_reuse_belt_item_sprite(
    commands: &mut Commands,
    pool: &mut BeltItemRenderPool,
    visual_assets: &mut VisualAssets,
    item: VisibleBeltItemRenderState,
) {
    let marker = BeltItemSprite {
        key: item.key,
        item_id: item.item_id,
        active: true,
    };

    if let Some(entity) = pool.sprites.pop() {
        commands.entity(entity).insert((
            visual_assets.belt_item_sprite(item.color, Vec2::splat(BELT_ITEM_SPRITE_SIZE)),
            Transform::from_translation(item.translation),
            Visibility::Visible,
            marker,
        ));
        return;
    }

    spawn_belt_item_visual(
        commands,
        visual_assets,
        item.color,
        Vec2::splat(BELT_ITEM_SPRITE_SIZE),
        item.translation,
        (marker, Visibility::Visible),
    );
}

pub(super) fn collect_visible_belt_items_into(
    sim: &Simulation,
    ids: BasePrototypeIds,
    visible_ids: &HashSet<EntityId>,
    items: &mut Vec<VisibleBeltItemRenderState>,
) {
    items.clear();
    for &entity_id in visible_ids {
        let Some(placed) = sim.entities().placed_entity(entity_id) else {
            continue;
        };
        if let Ok(segment) = factory_sim::entity_access::belt_segment(sim, placed.id) {
            let center = tile_translation(placed.x, placed.y, 4.0);
            for (lane_index, lane) in segment.lanes.iter().enumerate() {
                for (item_index, item) in lane.items.iter().enumerate() {
                    let key = BeltItemKey {
                        entity_id: placed.id,
                        input_port: None,
                        lane_index,
                        item_index,
                    };
                    items.push(transport_item_render_state_from_parts(
                        key,
                        segment.dir,
                        center,
                        item.item_id,
                        item.position_subtile,
                        belt_item_color(item.item_id, ids),
                    ));
                }
            }
            continue;
        }

        let Ok(state) = factory_sim::entity_access::splitter_state(sim, placed.id) else {
            continue;
        };
        let Some(port_tiles) = splitter_port_tiles_for_render(&placed.footprint) else {
            continue;
        };
        for (input_port, input_lanes) in state.input_lanes.iter().enumerate() {
            let port_tile = port_tiles[input_port];
            let center = tile_translation(port_tile.0, port_tile.1, 4.0);
            for (lane_index, lane) in input_lanes.iter().enumerate() {
                for (item_index, item) in lane.items.iter().enumerate() {
                    let key = BeltItemKey {
                        entity_id: placed.id,
                        input_port: Some(input_port),
                        lane_index,
                        item_index,
                    };
                    items.push(transport_item_render_state_from_parts(
                        key,
                        state.dir,
                        center,
                        item.item_id,
                        item.position_subtile,
                        belt_item_color(item.item_id, ids),
                    ));
                }
            }
        }
    }
}

pub(super) fn push_unique(pool: &mut Vec<Entity>, entity: Entity) {
    if !pool.contains(&entity) {
        pool.push(entity);
    }
}

#[cfg(test)]
pub(crate) fn belt_item_render_state(
    sim: &Simulation,
    entity_id: EntityId,
    lane_index: usize,
    item_index: usize,
) -> Option<(Vec3, Color)> {
    transport_item_render_state_with_ids(
        sim,
        BasePrototypeIds::from_catalog(sim.catalog()),
        entity_id,
        None,
        lane_index,
        item_index,
    )
}

#[cfg(test)]
pub(super) fn transport_item_render_state_with_ids(
    sim: &Simulation,
    ids: BasePrototypeIds,
    entity_id: EntityId,
    input_port: Option<usize>,
    lane_index: usize,
    item_index: usize,
) -> Option<(Vec3, Color)> {
    let placed = sim.entities().placed_entity(entity_id)?;
    let (dir, item, center) = if let Some(input_port) = input_port {
        let state = factory_sim::entity_access::splitter_state(sim, entity_id).ok()?;
        let item = state
            .input_lanes
            .get(input_port)?
            .get(lane_index)?
            .items
            .get(item_index)?;
        let port_tile = splitter_port_tiles_for_render(&placed.footprint)?[input_port];
        (
            state.dir,
            item,
            tile_translation(port_tile.0, port_tile.1, 4.0),
        )
    } else {
        let segment = factory_sim::entity_access::belt_segment(sim, entity_id).ok()?;
        let item = segment.lanes.get(lane_index)?.items.get(item_index)?;
        (segment.dir, item, tile_translation(placed.x, placed.y, 4.0))
    };
    let along = direction_render_vector(dir);
    let perpendicular = Vec2::new(-along.y, along.x);
    let progress = f32::from(item.position_subtile) / f32::from(BELT_SUBTILES_PER_TILE) - 0.5;
    let lane_offset = if lane_index == 0 { -0.18 } else { 0.18 };
    let offset = (along * progress + perpendicular * lane_offset) * TILE_SIZE;
    let color = belt_item_color(item.item_id, ids);

    Some((
        Vec3::new(center.x + offset.x, center.y + offset.y, 4.0),
        color,
    ))
}

pub(crate) fn belt_item_color(item_id: ItemId, ids: BasePrototypeIds) -> Color {
    if item_id == ids.items.iron_ore {
        Color::srgb(0.72, 0.67, 0.58)
    } else if item_id == ids.items.copper_ore {
        Color::srgb(0.86, 0.42, 0.20)
    } else if item_id == ids.items.coal {
        Color::srgb(0.055, 0.055, 0.052)
    } else if item_id == ids.items.stone {
        Color::srgb(0.54, 0.51, 0.47)
    } else if item_id == ids.items.iron_plate || item_id == ids.items.steel_plate {
        Color::srgb(0.78, 0.82, 0.84)
    } else if item_id == ids.items.copper_plate || item_id == ids.items.copper_cable {
        Color::srgb(0.92, 0.54, 0.24)
    } else if item_id == ids.items.iron_gear_wheel {
        Color::srgb(0.66, 0.70, 0.74)
    } else if item_id == ids.items.electronic_circuit {
        Color::srgb(0.24, 0.70, 0.38)
    } else if item_id == ids.items.automation_science_pack {
        Color::srgb(0.88, 0.24, 0.20)
    } else if item_id == ids.items.logistic_science_pack {
        Color::srgb(0.32, 0.72, 0.36)
    } else {
        Color::srgb(0.58, 0.78, 0.94)
    }
}
