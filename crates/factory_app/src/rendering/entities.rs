use bevy::prelude::*;
use factory_data::EntityKind;
use factory_sim::{EntityId, Simulation};
use std::collections::BTreeSet;

use crate::constants::{
    BURNER_DRILL_SPRITE_PADDING, CHEST_SPRITE_SIZE, TILE_SIZE, TRANSPORT_BELT_SPRITE_SIZE,
};
use crate::interaction::machine_kind::{OpenMachineKind, open_machine_kind};
use crate::rendering::colors::{
    assembler_color, burner_drill_color, chest_color, furnace_color, lab_color,
    transport_belt_color,
};
use crate::rendering::transforms::entity_translation;
use crate::resources::SimResource;

#[derive(Component)]
pub(crate) struct PlacedEntitySprite {
    entity_id: EntityId,
}

pub(crate) fn sync_placed_entity_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut sprites: Query<(Entity, &PlacedEntitySprite, &mut Transform, &mut Sprite)>,
) {
    let mut seen = BTreeSet::new();

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

pub(crate) fn renderable_entity_style(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<(Color, Vec2)> {
    let placed = sim.entities().placed_entity(entity_id)?;
    let prototype = sim.catalog().entities.get(placed.prototype_id.index())?;
    if prototype.entity_kind == EntityKind::TransportBelt {
        return Some((
            transport_belt_color(),
            Vec2::splat(TRANSPORT_BELT_SPRITE_SIZE),
        ));
    }

    match open_machine_kind(sim, entity_id) {
        Some(OpenMachineKind::Chest) => Some((chest_color(), Vec2::splat(CHEST_SPRITE_SIZE))),
        Some(OpenMachineKind::BurnerDrill) => Some((
            burner_drill_color(),
            Vec2::new(
                placed.footprint.width as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
                placed.footprint.height as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
            ),
        )),
        Some(OpenMachineKind::Furnace) => Some((
            furnace_color(),
            Vec2::new(
                placed.footprint.width as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
                placed.footprint.height as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
            ),
        )),
        Some(OpenMachineKind::Assembler) => Some((
            assembler_color(),
            Vec2::new(
                placed.footprint.width as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
                placed.footprint.height as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
            ),
        )),
        Some(OpenMachineKind::Lab) => Some((
            lab_color(),
            Vec2::new(
                placed.footprint.width as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
                placed.footprint.height as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
            ),
        )),
        None => None,
    }
}
