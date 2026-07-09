use super::*;
use bevy::prelude::*;
use factory_data::{BasePrototypeIds, EntityPrototypeId};
use factory_sim::{CHUNK_SIZE, Direction, EntityId, Simulation};

use crate::constants::BELT_DIRECTION_HEAD_SIZE;
use crate::rendering::belts::items::{
    collect_visible_belt_items_into, transport_item_render_state_with_ids,
};
use crate::rendering::belts::labels::transport_item_label_render_state;
use crate::rendering::resources::{BeltItemRenderPool, RenderDetail, VisibleEntityIds};
use crate::resources::SimResource;
use crate::utils::find_entity_prototype_id;

#[test]
pub(crate) fn belt_item_render_state_changes_only_when_sim_position_changes() {
    let mut sim = Simulation::new_test_world(123);
    let belt = find_entity_prototype_id(sim.catalog(), "transport_belt");
    let iron_ore = BasePrototypeIds::from_catalog(sim.catalog()).items.iron_ore;
    let (x, y) = first_placeable_tile(&sim, belt, Direction::East);
    let belt_id = factory_sim::placement::place(
        &mut sim,
        factory_sim::placement::EntityPlacementRequest {
            prototype_id: belt,
            x,
            y,
            direction: Direction::East,
        },
    )
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
    let belt_id = factory_sim::placement::place(
        &mut sim,
        factory_sim::placement::EntityPlacementRequest {
            prototype_id: belt,
            x,
            y,
            direction: Direction::North,
        },
    )
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
    let belt_id = factory_sim::placement::place(
        &mut sim,
        factory_sim::placement::EntityPlacementRequest {
            prototype_id: belt,
            x,
            y,
            direction: Direction::East,
        },
    )
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
    let belt_id = factory_sim::placement::place(
        &mut sim,
        factory_sim::placement::EntityPlacementRequest {
            prototype_id: belt,
            x,
            y,
            direction: Direction::East,
        },
    )
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

#[test]
fn belt_item_rendering_reuses_active_entities_when_sim_ticks() {
    let mut sim = Simulation::new_test_world(123);
    let belt = find_entity_prototype_id(sim.catalog(), "transport_belt");
    let iron_ore = BasePrototypeIds::from_catalog(sim.catalog()).items.iron_ore;
    let (x, y) = first_placeable_tile(&sim, belt, Direction::East);
    let belt_id = factory_sim::placement::place(
        &mut sim,
        factory_sim::placement::EntityPlacementRequest {
            prototype_id: belt,
            x,
            y,
            direction: Direction::East,
        },
    )
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
    let (first_sprite, first_sprite_translation) =
        active_belt_item_sprite_state(&mut app).expect("sprite should spawn");
    let (first_label, first_label_translation) =
        active_belt_item_label_state(&mut app).expect("label should spawn");

    app.world_mut().resource_mut::<SimResource>().sim.tick();
    app.update();

    let (second_sprite, second_sprite_translation) =
        active_belt_item_sprite_state(&mut app).expect("sprite should remain active");
    let (second_label, second_label_translation) =
        active_belt_item_label_state(&mut app).expect("label should remain active");

    assert_eq!(second_sprite, first_sprite);
    assert_eq!(second_label, first_label);
    assert!(second_sprite_translation.x > first_sprite_translation.x);
    assert_eq!(second_sprite_translation.y, first_sprite_translation.y);
    assert!(second_label_translation.x > first_label_translation.x);
    assert_eq!(second_label_translation.y, first_label_translation.y);
}

#[test]
fn collect_visible_belt_items_into_clears_stale_items_when_visibility_empty() {
    let mut sim = Simulation::new_test_world(123);
    let belt = find_entity_prototype_id(sim.catalog(), "transport_belt");
    let ids = BasePrototypeIds::from_catalog(sim.catalog());
    let (x, y) = first_placeable_tile(&sim, belt, Direction::East);
    let belt_id = factory_sim::placement::place(
        &mut sim,
        factory_sim::placement::EntityPlacementRequest {
            prototype_id: belt,
            x,
            y,
            direction: Direction::East,
        },
    )
    .expect("belt should be placeable");
    sim.insert_item_onto_belt(belt_id, 0, ids.items.iron_ore)
        .expect("empty belt should accept item");

    let mut items = Vec::new();
    collect_visible_belt_items_into(&sim, ids, &visible_entity_ids([belt_id]).ids, &mut items);
    assert_eq!(items.len(), 1);

    collect_visible_belt_items_into(&sim, ids, &visible_entity_ids([]).ids, &mut items);
    assert!(items.is_empty());
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

fn active_belt_item_sprite_state(app: &mut App) -> Option<(Entity, Vec3)> {
    app.world_mut()
        .query::<(Entity, &BeltItemSprite, &Transform, &Visibility)>()
        .iter(app.world())
        .find_map(|(entity, marker, transform, visibility)| {
            (marker.active && *visibility == Visibility::Visible)
                .then_some((entity, transform.translation))
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

fn active_belt_item_label_state(app: &mut App) -> Option<(Entity, Vec3)> {
    app.world_mut()
        .query::<(Entity, &BeltItemLabel, &Transform, &Visibility)>()
        .iter(app.world())
        .find_map(|(entity, marker, transform, visibility)| {
            (marker.active && *visibility == Visibility::Visible)
                .then_some((entity, transform.translation))
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

            if factory_sim::placement::validate(
                sim,
                factory_sim::placement::EntityPlacementRequest {
                    prototype_id,
                    x,
                    y,
                    direction,
                },
            )
            .is_ok()
            {
                return (x, y);
            }
        }
    }

    panic!("expected at least one placeable tile");
}
