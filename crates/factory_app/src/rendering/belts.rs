use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use factory_data::{BasePrototypeIds, ItemId};
use factory_sim::{BELT_SUBTILES_PER_TILE, Direction, EntityId, Simulation};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::constants::{
    BELT_DIRECTION_HEAD_SIZE, BELT_DIRECTION_SHAFT_LENGTH, BELT_DIRECTION_SHAFT_WIDTH,
    BELT_ITEM_LABEL_FONT_SIZE, BELT_ITEM_SPRITE_SIZE, TILE_SIZE,
};
use crate::rendering::transforms::{entity_translation, tile_translation};
use crate::rendering::visuals::{VisualAssets, spawn_belt_item_visual};
use crate::resources::{
    BeltItemRenderPool, RenderDetail, RenderSyncStats, SimResource, VisibleEntityIds,
};
use crate::utils::compact_item_name;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BeltDirectionPart {
    Shaft,
    Head,
}

#[derive(Component)]
pub(crate) struct BeltDirectionSprite {
    entity_id: EntityId,
    part: BeltDirectionPart,
}

#[derive(Component)]
pub(crate) struct BeltItemSprite {
    key: BeltItemKey,
    item_id: ItemId,
    active: bool,
}

#[derive(Component)]
pub(crate) struct BeltItemLabel {
    key: BeltItemKey,
    item_id: ItemId,
    active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BeltItemKey {
    entity_id: EntityId,
    input_port: Option<usize>,
    lane_index: usize,
    item_index: usize,
}

#[derive(Clone, Copy)]
struct VisibleBeltItemRenderState {
    key: BeltItemKey,
    item_id: ItemId,
    translation: Vec3,
    color: Color,
}

#[derive(SystemParam)]
pub(crate) struct BeltItemRenderParams<'w, 's> {
    commands: Commands<'w, 's>,
    sim: Res<'w, SimResource>,
    visible_entity_ids: Res<'w, VisibleEntityIds>,
    detail: Res<'w, RenderDetail>,
    pool: ResMut<'w, BeltItemRenderPool>,
    visual_assets: VisualAssets<'w>,
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

pub(crate) fn sync_belt_direction_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    detail: Res<RenderDetail>,
    mut sprites: Query<(Entity, &BeltDirectionSprite, &mut Transform, &mut Sprite)>,
) {
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
                belt_direction_render_state(&sim.sim, marker.entity_id, marker.part)
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
        let Some(placed) = sim.sim.entities().placed_entity(entity_id) else {
            continue;
        };
        if sim.sim.belt_segment(placed.id).is_err() && sim.sim.splitter_state(placed.id).is_err() {
            continue;
        }

        for part in [BeltDirectionPart::Shaft, BeltDirectionPart::Head] {
            let key = (placed.id, part);
            if seen.contains(&key) {
                continue;
            }

            let Some((translation, size, color)) =
                belt_direction_render_state(&sim.sim, placed.id, part)
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

pub(crate) fn sync_belt_item_rendering(params: BeltItemRenderParams) {
    let BeltItemRenderParams {
        mut commands,
        sim,
        visible_entity_ids,
        detail,
        mut pool,
        mut visual_assets,
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
    let visible_items = collect_visible_belt_items(&sim.sim, ids, &visible_entity_ids.ids);
    sync_belt_item_entity_pool(
        &mut commands,
        &sim.sim,
        &mut pool,
        &mut visual_assets,
        detail.show_belt_item_labels,
        &visible_items,
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

fn pool_all_belt_items(
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
    visible_items: &[VisibleBeltItemRenderState],
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
    let visible_by_key = visible_items
        .iter()
        .map(|item| (item.key, *item))
        .collect::<HashMap<_, _>>();
    let mut seen_sprites = HashSet::with_capacity(visible_items.len());
    let mut seen_labels = HashSet::with_capacity(visible_items.len());

    for (entity, mut marker, mut transform, mut sprite, mut visibility) in &mut *sprites {
        if marker.active
            && let Some(item) = visible_by_key.get(&marker.key)
        {
            seen_sprites.insert(marker.key);
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

    for item in visible_items {
        if seen_sprites.contains(&item.key) {
            continue;
        }
        spawn_or_reuse_belt_item_sprite(commands, pool, visual_assets, *item);
    }

    if show_labels {
        for (entity, mut marker, mut transform, mut text, mut visibility) in &mut *labels {
            if marker.active
                && let Some(item) = visible_by_key.get(&marker.key)
            {
                seen_labels.insert(marker.key);
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

        for item in visible_items {
            if seen_labels.contains(&item.key) {
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

fn spawn_or_reuse_belt_item_sprite(
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

fn spawn_or_reuse_belt_item_label(
    commands: &mut Commands,
    sim: &Simulation,
    pool: &mut BeltItemRenderPool,
    item: VisibleBeltItemRenderState,
) {
    let marker = BeltItemLabel {
        key: item.key,
        item_id: item.item_id,
        active: true,
    };
    let translation = label_translation(item.translation);
    let label = belt_item_label(sim, item.item_id);

    if let Some(entity) = pool.labels.pop() {
        commands.entity(entity).insert((
            Text2d::new(label),
            TextFont::from_font_size(BELT_ITEM_LABEL_FONT_SIZE),
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
            Transform::from_translation(translation),
            Anchor::CENTER,
            Text2dShadow::default(),
            Visibility::Visible,
            marker,
        ));
        return;
    }

    commands.spawn((
        Text2d::new(label),
        TextFont::from_font_size(BELT_ITEM_LABEL_FONT_SIZE),
        TextColor(Color::WHITE),
        TextLayout::justify(Justify::Center),
        Transform::from_translation(translation),
        Anchor::CENTER,
        Text2dShadow::default(),
        Visibility::Visible,
        marker,
    ));
}

fn collect_visible_belt_items(
    sim: &Simulation,
    ids: BasePrototypeIds,
    visible_ids: &HashSet<EntityId>,
) -> Vec<VisibleBeltItemRenderState> {
    let mut items = Vec::new();
    for &entity_id in visible_ids {
        let Some(placed) = sim.entities().placed_entity(entity_id) else {
            continue;
        };
        if let Ok(segment) = sim.belt_segment(placed.id) {
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
                        ids,
                        key,
                        segment.dir,
                        center,
                        item.item_id,
                        item.position_subtile,
                    ));
                }
            }
            continue;
        }

        let Ok(state) = sim.splitter_state(placed.id) else {
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
                        ids,
                        key,
                        state.dir,
                        center,
                        item.item_id,
                        item.position_subtile,
                    ));
                }
            }
        }
    }
    items
}

fn transport_item_render_state_from_parts(
    ids: BasePrototypeIds,
    key: BeltItemKey,
    dir: Direction,
    center: Vec3,
    item_id: ItemId,
    position_subtile: u16,
) -> VisibleBeltItemRenderState {
    let along = direction_render_vector(dir);
    let perpendicular = Vec2::new(-along.y, along.x);
    let progress = f32::from(position_subtile) / f32::from(BELT_SUBTILES_PER_TILE) - 0.5;
    let lane_offset = if key.lane_index == 0 { -0.18 } else { 0.18 };
    let offset = (along * progress + perpendicular * lane_offset) * TILE_SIZE;
    let translation = Vec3::new(center.x + offset.x, center.y + offset.y, 4.0);

    VisibleBeltItemRenderState {
        key,
        item_id,
        translation,
        color: belt_item_color(item_id, ids),
    }
}

fn label_translation(mut translation: Vec3) -> Vec3 {
    translation.z += 0.2;
    translation
}

fn belt_item_label(sim: &Simulation, item_id: ItemId) -> String {
    let name = sim
        .catalog()
        .items
        .get(item_id.index())
        .map(|item| item.name.as_str())
        .unwrap_or("?");
    compact_item_name(name)
}

fn push_unique(pool: &mut Vec<Entity>, entity: Entity) {
    if !pool.contains(&entity) {
        pool.push(entity);
    }
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
fn transport_item_render_state_with_ids(
    sim: &Simulation,
    ids: BasePrototypeIds,
    entity_id: EntityId,
    input_port: Option<usize>,
    lane_index: usize,
    item_index: usize,
) -> Option<(Vec3, Color)> {
    let placed = sim.entities().placed_entity(entity_id)?;
    let (dir, item, center) = if let Some(input_port) = input_port {
        let state = sim.splitter_state(entity_id).ok()?;
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
        let segment = sim.belt_segment(entity_id).ok()?;
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

#[cfg(test)]
pub(crate) fn belt_item_label_render_state(
    sim: &Simulation,
    entity_id: EntityId,
    lane_index: usize,
    item_index: usize,
) -> Option<(Vec3, String)> {
    transport_item_label_render_state(sim, entity_id, None, lane_index, item_index)
}

#[cfg(test)]
fn transport_item_label_render_state(
    sim: &Simulation,
    entity_id: EntityId,
    input_port: Option<usize>,
    lane_index: usize,
    item_index: usize,
) -> Option<(Vec3, String)> {
    let (mut translation, _) = transport_item_render_state_with_ids(
        sim,
        BasePrototypeIds::from_catalog(sim.catalog()),
        entity_id,
        input_port,
        lane_index,
        item_index,
    )?;
    let item_id = if let Some(input_port) = input_port {
        sim.splitter_state(entity_id)
            .ok()?
            .input_lanes
            .get(input_port)?
            .get(lane_index)?
            .items
            .get(item_index)?
            .item_id
    } else {
        sim.belt_segment(entity_id)
            .ok()?
            .lanes
            .get(lane_index)?
            .items
            .get(item_index)?
            .item_id
    };
    let name = sim
        .catalog()
        .items
        .get(item_id.index())
        .map(|item| item.name.as_str())
        .unwrap_or("?");

    translation.z += 0.2;
    Some((translation, compact_item_name(name)))
}

fn transport_flow_direction(sim: &Simulation, entity_id: EntityId) -> Option<Direction> {
    sim.belt_segment(entity_id)
        .ok()
        .map(|segment| segment.dir)
        .or_else(|| sim.splitter_state(entity_id).ok().map(|state| state.dir))
}

fn splitter_port_tiles_for_render(
    footprint: &factory_sim::EntityFootprint,
) -> Option<[(i32, i32); 2]> {
    let mut tiles = footprint.tiles();
    if tiles.len() != 2 {
        return None;
    }

    tiles.sort_unstable();
    Some([tiles[0], tiles[1]])
}

pub(crate) fn direction_render_vector(direction: Direction) -> Vec2 {
    match direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => Vec2::NEG_Y,
        Direction::West => Vec2::NEG_X,
    }
}

pub(crate) fn belt_direction_color() -> Color {
    Color::srgba(0.12, 0.08, 0.025, 0.86)
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

#[cfg(test)]
mod tests {
    use super::*;
    use factory_data::EntityPrototypeId;
    use factory_sim::CHUNK_SIZE;

    use crate::utils::find_entity_prototype_id;

    #[test]
    pub(crate) fn belt_item_render_state_changes_only_when_sim_position_changes() {
        let mut sim = Simulation::new_test_world(123);
        let belt = find_entity_prototype_id(sim.catalog(), "transport_belt");
        let iron_ore = BasePrototypeIds::from_catalog(sim.catalog()).items.iron_ore;
        let (x, y) = first_placeable_tile(&sim, belt, Direction::East);
        let belt_id = sim
            .place_entity(belt, x, y, Direction::East)
            .expect("belt should be placeable");

        sim.insert_item_onto_belt(belt_id, 0, iron_ore)
            .expect("empty belt should accept item");

        let (before, _) = belt_item_render_state(&sim, belt_id, 0, 0)
            .expect("inserted belt item should have render state");
        let (same_tick, _) = belt_item_render_state(&sim, belt_id, 0, 0)
            .expect("inserted belt item should keep render state");
        assert_eq!(same_tick, before);

        sim.tick();

        let (after_tick, _) = belt_item_render_state(&sim, belt_id, 0, 0)
            .expect("ticked belt item should have render state");
        assert!(after_tick.x > before.x);
        assert_eq!(after_tick.y, before.y);

        let (without_tick, _) = belt_item_render_state(&sim, belt_id, 0, 0)
            .expect("unticked belt item should keep render state");
        assert_eq!(without_tick, after_tick);
    }

    #[test]
    pub(crate) fn belt_direction_render_state_marks_downstream_direction() {
        let mut sim = Simulation::new_test_world(123);
        let belt = find_entity_prototype_id(sim.catalog(), "transport_belt");
        let (x, y) = first_placeable_tile(&sim, belt, Direction::North);
        let belt_id = sim
            .place_entity(belt, x, y, Direction::North)
            .expect("belt should be placeable");

        let (shaft_translation, shaft_size, _) =
            belt_direction_render_state(&sim, belt_id, BeltDirectionPart::Shaft)
                .expect("belt shaft should have render state");
        let (head_translation, head_size, _) =
            belt_direction_render_state(&sim, belt_id, BeltDirectionPart::Head)
                .expect("belt head should have render state");

        assert!(head_translation.y > shaft_translation.y);
        assert!(shaft_size.y > shaft_size.x);
        assert_eq!(head_size, Vec2::splat(BELT_DIRECTION_HEAD_SIZE));
    }

    #[test]
    fn belt_item_label_uses_item_prototype_initials() {
        let mut sim = Simulation::new_test_world(123);
        let belt = find_entity_prototype_id(sim.catalog(), "transport_belt");
        let copper_ore = BasePrototypeIds::from_catalog(sim.catalog())
            .items
            .copper_ore;
        let (x, y) = first_placeable_tile(&sim, belt, Direction::East);
        let belt_id = sim
            .place_entity(belt, x, y, Direction::East)
            .expect("belt should be placeable");

        sim.insert_item_onto_belt(belt_id, 0, copper_ore)
            .expect("empty belt should accept item");

        let (_, label) = belt_item_label_render_state(&sim, belt_id, 0, 0)
            .expect("inserted belt item should have label render state");
        assert_eq!(label, "CO");
    }

    #[test]
    fn belt_item_rendering_reuses_pooled_sprite_and_label_entities() {
        let mut sim = Simulation::new_test_world(123);
        let belt = find_entity_prototype_id(sim.catalog(), "transport_belt");
        let iron_ore = BasePrototypeIds::from_catalog(sim.catalog()).items.iron_ore;
        let (x, y) = first_placeable_tile(&sim, belt, Direction::East);
        let belt_id = sim
            .place_entity(belt, x, y, Direction::East)
            .expect("belt should be placeable");
        sim.insert_item_onto_belt(belt_id, 0, iron_ore)
            .expect("empty belt should accept item");

        let mut app = App::new();
        app.insert_resource(SimResource { sim })
            .insert_resource(visible_entity_ids([belt_id]))
            .init_resource::<RenderDetail>()
            .init_resource::<BeltItemRenderPool>()
            .add_systems(Update, sync_belt_item_rendering);

        app.update();
        let first_sprite = active_belt_item_sprite(&mut app).expect("sprite should spawn");
        let first_label = active_belt_item_label(&mut app).expect("label should spawn");

        *app.world_mut().resource_mut::<VisibleEntityIds>() = visible_entity_ids([]);
        app.update();
        assert_eq!(active_belt_item_sprite(&mut app), None);
        assert_eq!(active_belt_item_label(&mut app), None);
        assert!(
            app.world()
                .resource::<BeltItemRenderPool>()
                .sprites
                .contains(&first_sprite)
        );
        assert!(
            app.world()
                .resource::<BeltItemRenderPool>()
                .labels
                .contains(&first_label)
        );

        *app.world_mut().resource_mut::<VisibleEntityIds>() = visible_entity_ids([belt_id]);
        app.update();

        assert_eq!(active_belt_item_sprite(&mut app), Some(first_sprite));
        assert_eq!(active_belt_item_label(&mut app), Some(first_label));
    }

    fn visible_entity_ids<const N: usize>(ids: [EntityId; N]) -> VisibleEntityIds {
        VisibleEntityIds {
            ids: ids.into_iter().collect(),
            visible_revision: 1,
            entity_topology_revision: 1,
        }
    }

    fn active_belt_item_sprite(app: &mut App) -> Option<Entity> {
        app.world_mut()
            .query::<(Entity, &BeltItemSprite, &Visibility)>()
            .iter(app.world())
            .find_map(|(entity, marker, visibility)| {
                (marker.active && *visibility == Visibility::Visible).then_some(entity)
            })
    }

    fn active_belt_item_label(app: &mut App) -> Option<Entity> {
        app.world_mut()
            .query::<(Entity, &BeltItemLabel, &Visibility)>()
            .iter(app.world())
            .find_map(|(entity, marker, visibility)| {
                (marker.active && *visibility == Visibility::Visible).then_some(entity)
            })
    }

    fn first_placeable_tile(
        sim: &Simulation,
        prototype_id: EntityPrototypeId,
        direction: Direction,
    ) -> (i32, i32) {
        for chunk in sim.world().chunks.values() {
            for (index, _) in chunk.tiles.iter().enumerate() {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                let x = chunk.coord.x * CHUNK_SIZE + local_x;
                let y = chunk.coord.y * CHUNK_SIZE + local_y;

                if sim.can_place_entity(prototype_id, x, y, direction).is_ok() {
                    return (x, y);
                }
            }
        }

        panic!("expected at least one placeable tile");
    }
}
