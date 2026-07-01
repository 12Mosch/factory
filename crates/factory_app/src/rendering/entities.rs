use bevy::prelude::*;
use factory_data::{EntityKind, EntityPrototypeId, PrototypeCatalog};
use factory_sim::{Direction, EntityFootprint, EntityId, Simulation};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::time::Instant;

use crate::constants::{
    BURNER_DRILL_SPRITE_PADDING, CHEST_SPRITE_SIZE, TILE_SIZE, TRANSPORT_BELT_SPRITE_SIZE,
};
use crate::rendering::colors::{
    assembler_color, boiler_color, burner_drill_color, chest_color, electric_pole_color,
    furnace_color, inserter_color, lab_color, offshore_pump_color, pipe_color, splitter_color,
    steam_engine_color, storage_tank_color, transport_belt_color,
};
use crate::rendering::transforms::entity_translation;
use crate::resources::{RenderSyncStats, SimResource, VisibleChunks, VisibleEntityIds};

#[derive(Component)]
pub(crate) struct PlacedEntitySprite {
    entity_id: EntityId,
}

pub(crate) fn update_visible_entity_ids(
    sim: Res<SimResource>,
    visible: Res<VisibleChunks>,
    mut visible_entity_ids: ResMut<VisibleEntityIds>,
) {
    let entity_signature = entity_signature(&sim.sim);
    if visible_entity_ids.visible_revision == visible.revision
        && visible_entity_ids.entity_signature == entity_signature
    {
        return;
    }

    visible_entity_ids.ids = visible_entity_ids_for_chunks(&sim.sim, &visible);
    visible_entity_ids.visible_revision = visible.revision;
    visible_entity_ids.entity_signature = entity_signature;
}

pub(crate) fn sync_placed_entity_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    mut sprites: Query<(Entity, &PlacedEntitySprite, &mut Transform, &mut Sprite)>,
) {
    if !visible_entity_ids.is_changed() {
        return;
    }

    let visible_ids = &visible_entity_ids.ids;
    let mut seen = HashSet::new();

    for (entity, marker, mut transform, mut sprite) in &mut sprites {
        if visible_ids.contains(&marker.entity_id)
            && let Some((color, size)) = renderable_entity_style(&sim.sim, marker.entity_id)
        {
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

    for &entity_id in visible_ids {
        let Some(placed) = sim.sim.entities().placed_entity(entity_id) else {
            continue;
        };
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
    visible_entity_ids: Res<VisibleEntityIds>,
    sprites: Query<(Entity, &PlacedEntitySprite, &mut Transform, &mut Sprite)>,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_placed_entity_rendering(commands, sim, visible_entity_ids, sprites);
    stats.record_placed_entities(started.elapsed());
}

fn visible_entity_ids_for_chunks(sim: &Simulation, visible: &VisibleChunks) -> HashSet<EntityId> {
    let Some(bounds) = visible.tile_bounds else {
        return HashSet::new();
    };
    let max_x = bounds.min_x + bounds.width as i32 - 1;
    let max_y = bounds.min_y + bounds.height as i32 - 1;
    sim.entities()
        .occupancy()
        .entity_ids_in_tile_rect(bounds.min_x, max_x, bounds.min_y, max_y)
        .into_iter()
        .collect()
}

pub(crate) fn entity_signature(sim: &Simulation) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for placed in sim.entities().placed_entities() {
        placed.id.raw().hash(&mut hasher);
        placed.prototype_id.hash(&mut hasher);
        placed.x.hash(&mut hasher);
        placed.y.hash(&mut hasher);
        placed.direction.hash(&mut hasher);
        placed.footprint.hash(&mut hasher);
    }
    hasher.finish()
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
            transport_belt_color(
                prototype
                    .transport_belt
                    .as_ref()
                    .map(|belt| belt.speed_subtiles_per_tick),
            ),
            Vec2::splat(TRANSPORT_BELT_SPRITE_SIZE),
        )),
        EntityKind::Splitter => Some((
            splitter_color(
                prototype
                    .splitter
                    .as_ref()
                    .map(|splitter| splitter.speed_subtiles_per_tick),
            ),
            machine_size(),
        )),
        EntityKind::Chest => Some((chest_color(), Vec2::splat(CHEST_SPRITE_SIZE))),
        EntityKind::MiningDrill => Some((burner_drill_color(), machine_size())),
        EntityKind::Furnace => Some((furnace_color(), machine_size())),
        EntityKind::AssemblingMachine => Some((assembler_color(), machine_size())),
        EntityKind::Lab => Some((lab_color(), machine_size())),
        EntityKind::Inserter => Some((inserter_color(prototype.inserter.as_ref()), machine_size())),
        EntityKind::ElectricPole => Some((electric_pole_color(), Vec2::splat(CHEST_SPRITE_SIZE))),
        EntityKind::SteamEngine => Some((steam_engine_color(), machine_size())),
        EntityKind::Boiler => Some((boiler_color(), machine_size())),
        EntityKind::OffshorePump => Some((offshore_pump_color(), machine_size())),
        EntityKind::Pipe => Some((pipe_color(), Vec2::splat(TRANSPORT_BELT_SPRITE_SIZE))),
        EntityKind::StorageTank => Some((storage_tank_color(), machine_size())),
        EntityKind::ResourcePatch => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluid_entities_have_render_styles() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

        for entity_name in ["pipe", "storage_tank"] {
            let prototype_id = factory_data::entity_prototype_id_by_name(&catalog, entity_name);
            assert!(
                entity_prototype_render_style(&catalog, prototype_id, Direction::North).is_some(),
                "{entity_name} should have a render style"
            );
        }
    }
}
