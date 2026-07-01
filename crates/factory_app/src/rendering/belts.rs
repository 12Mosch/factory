use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use factory_data::{BasePrototypeIds, ItemId};
use factory_sim::{BELT_SUBTILES_PER_TILE, Direction, EntityId, Simulation};
use std::collections::HashSet;
use std::time::Instant;

use crate::constants::{
    BELT_DIRECTION_HEAD_SIZE, BELT_DIRECTION_SHAFT_LENGTH, BELT_DIRECTION_SHAFT_WIDTH,
    BELT_ITEM_LABEL_FONT_SIZE, BELT_ITEM_SPRITE_SIZE, TILE_SIZE,
};
use crate::rendering::transforms::{entity_translation, tile_translation};
use crate::resources::{RenderSyncStats, SimResource, VisibleEntityIds};
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
    entity_id: EntityId,
    input_port: Option<usize>,
    lane_index: usize,
    item_index: usize,
}

#[derive(Component)]
pub(crate) struct BeltItemLabel {
    entity_id: EntityId,
    input_port: Option<usize>,
    lane_index: usize,
    item_index: usize,
}

pub(crate) fn sync_belt_direction_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    mut sprites: Query<(Entity, &BeltDirectionSprite, &mut Transform, &mut Sprite)>,
) {
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
    sprites: Query<(Entity, &BeltDirectionSprite, &mut Transform, &mut Sprite)>,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_belt_direction_rendering(commands, sim, visible_entity_ids, sprites);
    stats.record_belt_directions(started.elapsed());
}

pub(crate) fn sync_belt_item_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    mut sprites: Query<
        (Entity, &BeltItemSprite, &mut Transform, &mut Sprite),
        Without<BeltItemLabel>,
    >,
    mut labels: Query<
        (Entity, &BeltItemLabel, &mut Transform, &mut Text2d),
        Without<BeltItemSprite>,
    >,
) {
    let ids = BasePrototypeIds::from_catalog(sim.sim.catalog());
    let visible_ids = &visible_entity_ids.ids;
    let mut seen_sprites = HashSet::new();
    let mut seen_labels = HashSet::new();

    for (entity, marker, mut transform, mut sprite) in &mut sprites {
        let key = (
            marker.entity_id,
            marker.input_port,
            marker.lane_index,
            marker.item_index,
        );
        if visible_ids.contains(&marker.entity_id)
            && let Some((translation, color)) = transport_item_render_state_with_ids(
                &sim.sim,
                ids,
                marker.entity_id,
                marker.input_port,
                marker.lane_index,
                marker.item_index,
            )
        {
            seen_sprites.insert(key);
            transform.translation = translation;
            sprite.color = color;
            sprite.custom_size = Some(Vec2::splat(BELT_ITEM_SPRITE_SIZE));
        } else {
            commands.entity(entity).despawn();
        }
    }

    for (entity, marker, mut transform, mut text) in &mut labels {
        let key = (
            marker.entity_id,
            marker.input_port,
            marker.lane_index,
            marker.item_index,
        );
        if visible_ids.contains(&marker.entity_id)
            && let Some((translation, label)) = transport_item_label_render_state(
                &sim.sim,
                marker.entity_id,
                marker.input_port,
                marker.lane_index,
                marker.item_index,
            )
        {
            seen_labels.insert(key);
            transform.translation = translation;
            text.0 = label;
        } else {
            commands.entity(entity).despawn();
        }
    }

    for &entity_id in visible_ids {
        let Some(placed) = sim.sim.entities().placed_entity(entity_id) else {
            continue;
        };
        let Ok(segment) = sim.sim.belt_segment(placed.id) else {
            sync_splitter_item_rendering_for_entity(
                &mut commands,
                &sim.sim,
                ids,
                placed.id,
                &seen_sprites,
                &seen_labels,
            );
            continue;
        };

        for (lane_index, lane) in segment.lanes.iter().enumerate() {
            for item_index in 0..lane.items.len() {
                let key = (placed.id, None, lane_index, item_index);
                if !seen_sprites.contains(&key) {
                    let Some((translation, color)) = transport_item_render_state_with_ids(
                        &sim.sim, ids, placed.id, None, lane_index, item_index,
                    ) else {
                        continue;
                    };
                    commands.spawn((
                        Sprite::from_color(color, Vec2::splat(BELT_ITEM_SPRITE_SIZE)),
                        Transform::from_translation(translation),
                        BeltItemSprite {
                            entity_id: placed.id,
                            input_port: None,
                            lane_index,
                            item_index,
                        },
                    ));
                }

                if !seen_labels.contains(&key) {
                    let Some((translation, label)) = transport_item_label_render_state(
                        &sim.sim, placed.id, None, lane_index, item_index,
                    ) else {
                        continue;
                    };
                    commands.spawn((
                        Text2d::new(label),
                        TextFont::from_font_size(BELT_ITEM_LABEL_FONT_SIZE),
                        TextColor(Color::WHITE),
                        TextLayout::justify(Justify::Center),
                        Transform::from_translation(translation),
                        Anchor::CENTER,
                        Text2dShadow::default(),
                        BeltItemLabel {
                            entity_id: placed.id,
                            input_port: None,
                            lane_index,
                            item_index,
                        },
                    ));
                }
            }
        }
    }
}

pub(crate) fn measured_sync_belt_item_rendering(
    commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    sprites: Query<(Entity, &BeltItemSprite, &mut Transform, &mut Sprite), Without<BeltItemLabel>>,
    labels: Query<(Entity, &BeltItemLabel, &mut Transform, &mut Text2d), Without<BeltItemSprite>>,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_belt_item_rendering(commands, sim, visible_entity_ids, sprites, labels);
    stats.record_belt_items(started.elapsed());
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

fn sync_splitter_item_rendering_for_entity(
    commands: &mut Commands,
    sim: &Simulation,
    ids: BasePrototypeIds,
    entity_id: EntityId,
    seen_sprites: &HashSet<(EntityId, Option<usize>, usize, usize)>,
    seen_labels: &HashSet<(EntityId, Option<usize>, usize, usize)>,
) {
    let Ok(state) = sim.splitter_state(entity_id) else {
        return;
    };

    for (input_port, input_lanes) in state.input_lanes.iter().enumerate() {
        for (lane_index, lane) in input_lanes.iter().enumerate() {
            for item_index in 0..lane.items.len() {
                let key = (entity_id, Some(input_port), lane_index, item_index);
                if !seen_sprites.contains(&key) {
                    let Some((translation, color)) = transport_item_render_state_with_ids(
                        sim,
                        ids,
                        entity_id,
                        Some(input_port),
                        lane_index,
                        item_index,
                    ) else {
                        continue;
                    };
                    commands.spawn((
                        Sprite::from_color(color, Vec2::splat(BELT_ITEM_SPRITE_SIZE)),
                        Transform::from_translation(translation),
                        BeltItemSprite {
                            entity_id,
                            input_port: Some(input_port),
                            lane_index,
                            item_index,
                        },
                    ));
                }

                if !seen_labels.contains(&key) {
                    let Some((translation, label)) = transport_item_label_render_state(
                        sim,
                        entity_id,
                        Some(input_port),
                        lane_index,
                        item_index,
                    ) else {
                        continue;
                    };
                    commands.spawn((
                        Text2d::new(label),
                        TextFont::from_font_size(BELT_ITEM_LABEL_FONT_SIZE),
                        TextColor(Color::WHITE),
                        TextLayout::justify(Justify::Center),
                        Transform::from_translation(translation),
                        Anchor::CENTER,
                        Text2dShadow::default(),
                        BeltItemLabel {
                            entity_id,
                            input_port: Some(input_port),
                            lane_index,
                            item_index,
                        },
                    ));
                }
            }
        }
    }
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
    Color::srgb(0.30, 0.22, 0.07)
}

pub(crate) fn belt_item_color(item_id: ItemId, ids: BasePrototypeIds) -> Color {
    if item_id == ids.items.iron_ore {
        Color::srgb(0.70, 0.66, 0.58)
    } else if item_id == ids.items.copper_ore {
        Color::srgb(0.86, 0.42, 0.20)
    } else if item_id == ids.items.coal {
        Color::srgb(0.05, 0.05, 0.05)
    } else if item_id == ids.items.stone {
        Color::srgb(0.54, 0.51, 0.47)
    } else {
        Color::srgb(0.64, 0.82, 0.95)
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
