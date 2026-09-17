use super::*;
use bevy::prelude::*;
use factory_data::{BasePrototypeIds, EntityPrototypeId};
use factory_sim::{CHUNK_SIZE, ChunkCoord, Direction, EntityId, Simulation};

use crate::constants::BELT_DIRECTION_HEAD_SIZE;
use crate::rendering::belts::items::{
    collect_visible_belt_items_into, transport_item_render_state_with_ids,
};
use crate::rendering::belts::labels::transport_item_label_render_state;
use crate::rendering::resources::{
    BELT_ITEM_POOL_MAX_UNUSED, BELT_ITEM_POOL_SPARE_UNUSED, BeltItemRenderPool, RenderDetail,
    VisibleEntityIds,
};
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
    app.insert_resource(SimResource::new(sim))
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
    app.insert_resource(SimResource::new(sim))
        .insert_resource(visible_entity_ids([belt_id]))
        .init_resource::<RenderDetail>()
        .init_resource::<BeltItemRenderPool>()
        .add_systems(Update, sync_belt_item_rendering);

    app.update();
    let (first_sprite, first_sprite_translation) =
        active_belt_item_sprite_state(&mut app).expect("sprite should spawn");
    let (first_label, first_label_translation) =
        active_belt_item_label_state(&mut app).expect("label should spawn");

    app.world_mut()
        .resource_mut::<SimResource>()
        .write_for_tests()
        .tick();
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
fn belt_item_rendering_recovers_when_cached_sprite_is_missing() {
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
    app.insert_resource(SimResource::new(sim))
        .insert_resource(visible_entity_ids([belt_id]))
        .init_resource::<RenderDetail>()
        .init_resource::<BeltItemRenderPool>()
        .add_systems(Update, sync_belt_item_rendering);

    app.update();
    let missing_sprite = active_belt_item_sprite(&mut app).expect("sprite should spawn");
    assert!(app.world_mut().despawn(missing_sprite));
    app.world_mut()
        .resource_mut::<SimResource>()
        .write_for_tests()
        .tick();

    app.update();

    let replacement = active_belt_item_sprite(&mut app).expect("sprite should respawn");
    assert_ne!(replacement, missing_sprite);
    assert!(
        !app.world()
            .resource::<BeltItemRenderPool>()
            .sprites
            .contains(&missing_sprite)
    );
}

#[test]
fn belt_item_rendering_interpolates_between_fixed_ticks() {
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
    app.insert_resource(SimResource::new(sim))
        .insert_resource(visible_entity_ids([belt_id]))
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .init_resource::<RenderDetail>()
        .init_resource::<BeltItemRenderPool>()
        .add_systems(Update, sync_belt_item_rendering);

    app.update();
    let (_, before) = active_belt_item_sprite_state(&mut app).expect("sprite should spawn");

    app.world_mut()
        .resource_mut::<SimResource>()
        .write_for_tests()
        .tick();
    let after =
        belt_item_render_state(&app.world().resource::<SimResource>().read(), belt_id, 0, 0)
            .expect("ticked item should have render state")
            .0;
    let timestep = app.world().resource::<Time<Fixed>>().timestep();
    app.world_mut()
        .resource_mut::<Time<Fixed>>()
        .accumulate_overstep(timestep / 2);
    app.update();

    let (_, interpolated) =
        active_belt_item_sprite_state(&mut app).expect("sprite should remain active");
    assert!((interpolated.x - before.x).abs() > f32::EPSILON);
    assert!((after.x - interpolated.x).abs() > f32::EPSILON);
    assert!((interpolated.x - before.x - (after.x - before.x) * 0.5).abs() < 0.001);
}

#[test]
fn simulation_replacement_resets_interpolation_for_reused_belt_item_ids() {
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

    let original_id = factory_sim::entity_access::belt_segment(&sim, belt_id)
        .expect("placed belt should have state")
        .lanes[0]
        .items[0]
        .id;
    let mut replacement = sim.clone();
    replacement.tick();
    let replacement_id = factory_sim::entity_access::belt_segment(&replacement, belt_id)
        .expect("replacement belt should have state")
        .lanes[0]
        .items[0]
        .id;
    let expected = belt_item_render_state(&replacement, belt_id, 0, 0)
        .expect("item should render")
        .0;
    assert_eq!(replacement_id, original_id);

    let mut app = App::new();
    app.insert_resource(SimResource::new(sim))
        .insert_resource(visible_entity_ids([belt_id]))
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .init_resource::<RenderDetail>()
        .init_resource::<BeltItemRenderPool>()
        .add_systems(Update, sync_belt_item_rendering);

    app.update();
    let (_, before) = active_belt_item_sprite_state(&mut app).expect("sprite should spawn");
    assert_ne!(expected, before);

    app.world_mut()
        .resource_mut::<SimResource>()
        .replace(replacement)
        .expect("simulation replacement should succeed");
    app.update();

    let (_, after) =
        active_belt_item_sprite_state(&mut app).expect("replacement sprite should spawn");
    assert_eq!(after, expected);
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

#[test]
fn belt_item_pool_trims_spike_but_reuses_reserve_and_regrows() {
    const SPIKE_BELTS: usize = 600;
    const STEADY_BELTS: usize = 2;
    const ITEMS_PER_BELT: usize = 2;
    const SPIKE_ITEMS: usize = SPIKE_BELTS * ITEMS_PER_BELT;
    const STEADY_ITEMS: usize = STEADY_BELTS * ITEMS_PER_BELT;
    // SPIKE_ITEMS (1,200) exceeds the 512-entity spare reserve so trimming is
    // observable, while fitting the 4,096 hard cap to exercise gradual release.

    let mut sim = Simulation::new_test_world(123);
    for y in -4..=4 {
        for x in -4..=4 {
            sim.ensure_chunk_generated(ChunkCoord { x, y });
        }
    }
    let belt_ids = place_belts_with_items(&mut sim, SPIKE_BELTS, ITEMS_PER_BELT);
    let spike_ids: Vec<EntityId> = belt_ids.clone();
    let steady_ids: Vec<EntityId> = belt_ids[..STEADY_BELTS].to_vec();

    let visible_for = |ids: &[EntityId], revision: u64| VisibleEntityIds {
        ids: ids.iter().copied().collect(),
        membership_revision: revision,
        visible_revision: 1,
        entity_topology_revision: 1,
        ..Default::default()
    };

    let mut app = App::new();
    let spike_revision = 2;
    app.insert_resource(SimResource::new(sim))
        .insert_resource(visible_for(&spike_ids, spike_revision))
        .init_resource::<RenderDetail>()
        .init_resource::<BeltItemRenderPool>()
        .add_systems(Update, sync_belt_item_rendering);

    // Growth: a large visible population spawns pooled-reusable entities.
    app.update();
    assert_eq!(active_belt_item_sprite_count(&mut app), SPIKE_ITEMS);
    assert_eq!(active_belt_item_label_count(&mut app), SPIKE_ITEMS);
    assert_eq!(pooled_belt_item_counts(&app), (0, 0));
    let spike_total = total_belt_item_counts(&mut app);
    assert_eq!(spike_total, (SPIKE_ITEMS, SPIKE_ITEMS));

    // Collapse to a small steady state. Trimming runs in the same sync, so the
    // pool holds the spike minus steady demand minus at most one trim budget,
    // and crucially retains a large reusable reserve instead of clearing.
    *app.world_mut().resource_mut::<VisibleEntityIds>() = visible_for(&steady_ids, 3);
    app.update();
    assert_eq!(active_belt_item_sprite_count(&mut app), STEADY_ITEMS);
    assert_eq!(active_belt_item_label_count(&mut app), STEADY_ITEMS);
    let (pooled_sprites, pooled_labels) = pooled_belt_item_counts(&app);
    assert!(
        pooled_sprites <= SPIKE_ITEMS - STEADY_ITEMS,
        "collapse should pool the hidden spike items"
    );
    assert_eq!(pooled_sprites, pooled_labels);
    assert!(
        pooled_sprites > BELT_ITEM_POOL_SPARE_UNUSED,
        "one frame must not evict the whole reserve (gradual trimming)"
    );

    // Reuse: immediate regrowth still finds pooled entities instead of
    // spawning everything anew.
    *app.world_mut().resource_mut::<VisibleEntityIds>() =
        visible_for(&spike_ids, spike_revision + 1_000);
    app.update();
    assert_eq!(active_belt_item_sprite_count(&mut app), SPIKE_ITEMS);
    assert_eq!(active_belt_item_label_count(&mut app), SPIKE_ITEMS);
    // The pool covered most of the regrowth: total entities stay at the spike
    // peak instead of peak plus a second full allocation.
    assert_eq!(total_belt_item_counts(&mut app), spike_total);

    // Return to steady state and let trimming converge. Idle frames with no
    // visibility or item change must still release excess capacity.
    let steady_revision = spike_revision + 2_000;
    *app.world_mut().resource_mut::<VisibleEntityIds>() = visible_for(&steady_ids, steady_revision);
    app.update();
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(active_belt_item_sprite_count(&mut app), STEADY_ITEMS);
    assert_eq!(active_belt_item_label_count(&mut app), STEADY_ITEMS);
    let (trimmed_sprites, trimmed_labels) = pooled_belt_item_counts(&app);
    assert!(
        trimmed_sprites <= BELT_ITEM_POOL_SPARE_UNUSED,
        "spike pool {trimmed_sprites} should converge to the spare reserve"
    );
    assert!(
        trimmed_labels <= BELT_ITEM_POOL_SPARE_UNUSED,
        "spike label pool {trimmed_labels} should converge to the spare reserve"
    );
    assert!(
        trimmed_sprites < SPIKE_ITEMS - STEADY_ITEMS,
        "trimming should release spike capacity instead of retaining the high-water mark"
    );
    // Trimming only touches unused entities: the steady-state items stay live.
    assert_eq!(
        total_belt_item_counts(&mut app).0,
        STEADY_ITEMS + trimmed_sprites
    );
    assert_eq!(
        total_belt_item_counts(&mut app).1,
        STEADY_ITEMS + trimmed_labels
    );

    // Subsequent growth reuses the bounded reserve without leaking.
    *app.world_mut().resource_mut::<VisibleEntityIds>() =
        visible_for(&spike_ids, steady_revision + 1);
    app.update();
    assert_eq!(active_belt_item_sprite_count(&mut app), SPIKE_ITEMS);
    assert_eq!(active_belt_item_label_count(&mut app), SPIKE_ITEMS);
    let regrown_total = total_belt_item_counts(&mut app);
    assert!(
        regrown_total.0 <= spike_total.0 + (SPIKE_ITEMS - STEADY_ITEMS - trimmed_sprites),
        "regrowth should reuse the retained reserve"
    );
    assert!(
        regrown_total.0 < spike_total.0 + (SPIKE_ITEMS - STEADY_ITEMS),
        "regrowth after trimming should spawn fewer than a cold start"
    );
}

fn visible_entity_ids<const N: usize>(ids: [EntityId; N]) -> VisibleEntityIds {
    VisibleEntityIds {
        ids: ids.into_iter().collect(),
        membership_revision: N as u64 + 1,
        visible_revision: 1,
        entity_topology_revision: 1,
        ..Default::default()
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

fn place_belts_with_items(
    sim: &mut Simulation,
    count: usize,
    items_per_belt: usize,
) -> Vec<EntityId> {
    assert!(
        items_per_belt <= 2,
        "test helper inserts at most one item per lane"
    );
    let prototype_id = find_entity_prototype_id(sim.catalog(), "transport_belt");
    let iron_ore = BasePrototypeIds::from_catalog(sim.catalog()).items.iron_ore;
    let mut placed = Vec::with_capacity(count);
    let mut chunks: Vec<ChunkCoord> = sim.world().chunks.keys().copied().collect();
    chunks.sort_unstable();
    for coord in chunks {
        let (min_x, min_y) = coord.min_tile();
        for local_y in 0..CHUNK_SIZE {
            for local_x in 0..CHUNK_SIZE {
                if placed.len() == count {
                    return placed;
                }
                let (x, y) = coord.tile_at(local_x, local_y);
                let _ = (min_x, min_y);
                if factory_sim::placement::validate(
                    sim,
                    factory_sim::placement::EntityPlacementRequest {
                        prototype_id,
                        x,
                        y,
                        direction: Direction::East,
                    },
                )
                .is_err()
                {
                    continue;
                }
                let belt_id = factory_sim::placement::place(
                    sim,
                    factory_sim::placement::EntityPlacementRequest {
                        prototype_id,
                        x,
                        y,
                        direction: Direction::East,
                    },
                )
                .expect("validated belt should place");
                for lane in 0..items_per_belt {
                    sim.insert_item_onto_belt(belt_id, lane, iron_ore)
                        .expect("empty belt lane should accept item");
                }
                placed.push(belt_id);
            }
        }
    }
    panic!("could only place {} of {count} belts", placed.len());
}

fn active_belt_item_sprite_count(app: &mut App) -> usize {
    app.world_mut()
        .query::<(&BeltItemSprite, &Visibility)>()
        .iter(app.world())
        .filter(|(marker, visibility)| marker.active && **visibility == Visibility::Visible)
        .count()
}

fn active_belt_item_label_count(app: &mut App) -> usize {
    app.world_mut()
        .query::<(&BeltItemLabel, &Visibility)>()
        .iter(app.world())
        .filter(|(marker, visibility)| marker.active && **visibility == Visibility::Visible)
        .count()
}

fn pooled_belt_item_counts(app: &App) -> (usize, usize) {
    let pool = app.world().resource::<BeltItemRenderPool>();
    (pool.sprites.len(), pool.labels.len())
}

fn total_belt_item_counts(app: &mut App) -> (usize, usize) {
    let sprites = app
        .world_mut()
        .query::<&BeltItemSprite>()
        .iter(app.world())
        .count();
    let labels = app
        .world_mut()
        .query::<&BeltItemLabel>()
        .iter(app.world())
        .count();
    (sprites, labels)
}

fn first_placeable_tile(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    direction: Direction,
) -> (i64, i64) {
    for chunk in sim.world().chunks.values() {
        for (index, _) in chunk.tiles.iter().enumerate() {
            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let (x, y) = chunk.coord.tile_at(local_x, local_y);

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
