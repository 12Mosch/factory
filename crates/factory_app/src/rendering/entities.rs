use bevy::prelude::*;
use factory_data::{EntityKind, EntityPrototypeId, PrototypeCatalog};
use factory_sim::{Direction, EntityFootprint, EntityId, Simulation};
use std::collections::HashSet;
use std::time::Instant;

use crate::constants::{
    BURNER_DRILL_SPRITE_PADDING, CHEST_SPRITE_SIZE, TILE_SIZE, TRANSPORT_BELT_SPRITE_SIZE,
};
use crate::rendering::colors::{
    assembler_color, burner_drill_color, chest_color, furnace_color, inserter_color, lab_color,
    splitter_color, transport_belt_color,
};
use crate::rendering::transforms::entity_translation;
use crate::resources::{RenderSyncStats, SimResource};

#[derive(Component)]
pub(crate) struct PlacedEntitySprite {
    entity_id: EntityId,
}

pub(crate) fn sync_placed_entity_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut sprites: Query<(Entity, &PlacedEntitySprite, &mut Transform, &mut Sprite)>,
) {
    let mut seen = HashSet::new();

    for (entity, marker, mut transform, mut sprite) in &mut sprites {
        if let Some((color, size)) = renderable_entity_style(&sim.sim, marker.entity_id) {
            let placed = sim
                .sim
                .entities()
                .placed_entity(marker.entity_id)
                .expect("validated renderable entity should still be placed");
            seen.insert(marker.entity_id);
            transform.translation = entity_translation(&placed.footprint, transform.translation.z);
            sprite.color = color;
            sprite.custom_size = Some(size);
        } else {
            commands.entity(entity).despawn();
        }
    }

    for placed in sim.sim.entities().placed_entities() {
        let Some((color, size)) = renderable_entity_style(&sim.sim, placed.id) else {
            continue;
        };
        if seen.contains(&placed.id) {
            continue;
        }

        commands.spawn((
            Sprite::from_color(color, size),
            Transform::from_translation(entity_translation(&placed.footprint, 3.0)),
            PlacedEntitySprite {
                entity_id: placed.id,
            },
        ));
    }
}

pub(crate) fn measured_sync_placed_entity_rendering(
    commands: Commands,
    sim: Res<SimResource>,
    sprites: Query<(Entity, &PlacedEntitySprite, &mut Transform, &mut Sprite)>,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_placed_entity_rendering(commands, sim, sprites);
    stats.record_placed_entities(started.elapsed());
}

pub(crate) fn renderable_entity_style(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<(Color, Vec2)> {
    let placed = sim.entities().placed_entity(entity_id)?;
    entity_prototype_render_style(sim.catalog(), placed.prototype_id, placed.direction)
}

pub(crate) fn entity_prototype_render_style(
    catalog: &PrototypeCatalog,
    prototype_id: EntityPrototypeId,
    direction: Direction,
) -> Option<(Color, Vec2)> {
    let prototype = catalog
        .entities
        .get(prototype_id.index())
        .filter(|prototype| prototype.id == prototype_id)?;
    let footprint = EntityFootprint::from_size(0, 0, prototype.size.x, prototype.size.y, direction);
    let machine_size = || {
        Vec2::new(
            footprint.width as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
            footprint.height as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
        )
    };

    match prototype.entity_kind {
        EntityKind::TransportBelt => Some((
            transport_belt_color(),
            Vec2::splat(TRANSPORT_BELT_SPRITE_SIZE),
        )),
        EntityKind::Splitter => Some((splitter_color(), machine_size())),
        EntityKind::Chest => Some((chest_color(), Vec2::splat(CHEST_SPRITE_SIZE))),
        EntityKind::MiningDrill => Some((burner_drill_color(), machine_size())),
        EntityKind::Furnace => Some((furnace_color(), machine_size())),
        EntityKind::AssemblingMachine => Some((assembler_color(), machine_size())),
        EntityKind::Lab => Some((lab_color(), machine_size())),
        EntityKind::Inserter => Some((inserter_color(), machine_size())),
        EntityKind::ResourcePatch => None,
    }
}
