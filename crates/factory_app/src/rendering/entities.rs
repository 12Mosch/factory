use bevy::prelude::*;
use factory_data::{CraftingCategory, EntityKind, EntityPrototypeId, PrototypeCatalog};
use factory_sim::{Direction, EntityFootprint, EntityId, PlacedEntity, Simulation};
use std::collections::HashSet;
use std::time::Instant;

use crate::constants::{
    CHEST_SPRITE_SIZE, MINING_DRILL_SPRITE_PADDING, TILE_SIZE, TRANSPORT_BELT_SPRITE_SIZE,
};
use crate::map::resources::VisibleChunks;
use crate::rendering::colors::{
    assembler_color, boiler_color, chemical_plant_color, chest_color, electric_pole_color,
    enemy_spawner_color, furnace_color, gun_turret_color, inserter_color, lab_color,
    mining_drill_color, offshore_pump_color, oil_refinery_color, pipe_color, pumpjack_color,
    splitter_color, steam_engine_color, storage_tank_color, transport_belt_color, wall_color,
};
use crate::rendering::resources::{RenderSyncStats, VisibleEntityIds};
use crate::rendering::transforms::entity_translation;
use crate::rendering::visuals::{
    ConnectionMask, EntityVisualStyle, VisualAssets, spawn_entity_visual,
};
use crate::resources::SimResource;

#[derive(Component)]
pub(crate) struct PlacedEntitySprite {
    entity_id: EntityId,
}

pub(crate) fn update_visible_entity_ids(
    sim: Res<SimResource>,
    visible: Res<VisibleChunks>,
    mut visible_entity_ids: ResMut<VisibleEntityIds>,
) {
    let entity_topology_revision = sim.read().entity_topology_revision();
    if visible_entity_ids.visible_revision == visible.revision
        && visible_entity_ids.entity_topology_revision == entity_topology_revision
    {
        return;
    }

    visible_entity_ids.ids = visible_entity_ids_for_chunks(&sim.read(), &visible);
    visible_entity_ids.visible_revision = visible.revision;
    visible_entity_ids.entity_topology_revision = entity_topology_revision;
}

pub(crate) fn sync_placed_entity_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    mut visual_assets: VisualAssets,
    mut sprites: Query<(Entity, &PlacedEntitySprite, &mut Transform, &mut Sprite)>,
) {
    let sim = sim.read();
    if !visible_entity_ids.is_changed() {
        return;
    }

    let visible_ids = &visible_entity_ids.ids;
    let mut seen = HashSet::new();

    for (entity, marker, mut transform, mut sprite) in &mut sprites {
        if visible_ids.contains(&marker.entity_id)
            && let Some(style) = renderable_entity_visual_style(&sim, marker.entity_id)
        {
            let placed = sim
                .entities()
                .placed_entity(marker.entity_id)
                .expect("validated renderable entity should still be placed");
            seen.insert(marker.entity_id);
            transform.translation = entity_translation(&placed.footprint, transform.translation.z);
            *sprite = visual_assets.entity_sprite(style);
        } else {
            commands.entity(entity).despawn();
        }
    }

    for &entity_id in visible_ids {
        let Some(placed) = sim.entities().placed_entity(entity_id) else {
            continue;
        };
        let Some(style) = renderable_entity_visual_style(&sim, placed.id) else {
            continue;
        };
        if seen.contains(&placed.id) {
            continue;
        }

        spawn_entity_visual(
            &mut commands,
            &mut visual_assets,
            style,
            entity_translation(&placed.footprint, 3.0),
            PlacedEntitySprite {
                entity_id: placed.id,
            },
        );
    }
}

pub(crate) fn measured_sync_placed_entity_rendering(
    commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    visual_assets: VisualAssets,
    sprites: Query<(Entity, &PlacedEntitySprite, &mut Transform, &mut Sprite)>,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_placed_entity_rendering(commands, sim, visible_entity_ids, visual_assets, sprites);
    stats.record_placed_entities(started.elapsed());
}

fn visible_entity_ids_for_chunks(sim: &Simulation, visible: &VisibleChunks) -> HashSet<EntityId> {
    let Some(bounds) = visible.tile_bounds else {
        return HashSet::new();
    };
    let max_x = bounds.min_x + i64::from(bounds.width) - 1;
    let max_y = bounds.min_y + i64::from(bounds.height) - 1;
    sim.entities()
        .occupancy()
        .entity_ids_in_tile_rect(bounds.min_x, max_x, bounds.min_y, max_y)
        .into_iter()
        .collect()
}

pub(crate) fn renderable_entity_visual_style(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<EntityVisualStyle> {
    let placed = sim.entities().placed_entity(entity_id)?;
    let mut style =
        entity_prototype_visual_style(sim.catalog(), placed.prototype_id, placed.direction)?;
    style.connections = entity_connection_mask(sim, placed, style.kind);
    Some(style)
}

/// Directions in which the placed entity visually joins a neighbor. Only pipes and belts
/// render connection overlays; every other kind keeps an empty mask so its cached visual
/// is shared across placements.
fn entity_connection_mask(
    sim: &Simulation,
    placed: &PlacedEntity,
    kind: EntityKind,
) -> ConnectionMask {
    match kind {
        EntityKind::Pipe => ConnectionMask::from_directions(
            factory_sim::entity_access::fluid_connection_directions(sim, placed.id),
        ),
        EntityKind::TransportBelt => belt_connection_mask(sim, placed),
        _ => ConnectionMask::EMPTY,
    }
}

fn belt_connection_mask(sim: &Simulation, placed: &PlacedEntity) -> ConnectionMask {
    let flow = belt_flow_direction(sim, placed);
    let mut connected = [false; 4];

    for direction in Direction::ALL {
        let (dx, dy) = direction_tile_delta(direction);
        let Some(neighbor_id) = sim
            .entities()
            .occupancy()
            .entity_at(placed.footprint.x + dx, placed.footprint.y + dy)
        else {
            continue;
        };
        let Some(neighbor) = sim.entities().placed_entity(neighbor_id) else {
            continue;
        };
        let Some(prototype) = sim.catalog().entity(neighbor.prototype_id) else {
            continue;
        };
        if !matches!(
            prototype.entity_kind,
            EntityKind::TransportBelt | EntityKind::Splitter
        ) {
            continue;
        }

        let neighbor_flow = belt_flow_direction(sim, neighbor);
        connected[direction.index()] = if direction == flow {
            // Downstream edge: joined unless the neighbor faces us head-on.
            neighbor_flow != direction.opposite()
        } else {
            // Upstream or side edge: joined when the neighbor flows into this tile.
            neighbor_flow == direction.opposite()
        };
    }

    ConnectionMask::from_directions(connected)
}

fn belt_flow_direction(sim: &Simulation, placed: &PlacedEntity) -> Direction {
    factory_sim::entity_access::belt_segment(sim, placed.id)
        .map(|segment| segment.dir)
        .unwrap_or(placed.direction)
}

fn direction_tile_delta(direction: Direction) -> (i64, i64) {
    match direction {
        Direction::North => (0, 1),
        Direction::East => (1, 0),
        Direction::South => (0, -1),
        Direction::West => (-1, 0),
    }
}

pub(crate) fn entity_prototype_render_style(
    catalog: &PrototypeCatalog,
    prototype_id: EntityPrototypeId,
    direction: Direction,
) -> Option<(Color, Vec2)> {
    let style = entity_prototype_visual_style(catalog, prototype_id, direction)?;
    Some((style.base_color, style.size))
}

pub(crate) fn entity_prototype_visual_style(
    catalog: &PrototypeCatalog,
    prototype_id: EntityPrototypeId,
    direction: Direction,
) -> Option<EntityVisualStyle> {
    let prototype = catalog.entity(prototype_id)?;
    let footprint = EntityFootprint::from_size(0, 0, prototype.size.x, prototype.size.y, direction);
    let machine_size = || {
        Vec2::new(
            footprint.width as f32 * TILE_SIZE - MINING_DRILL_SPRITE_PADDING,
            footprint.height as f32 * TILE_SIZE - MINING_DRILL_SPRITE_PADDING,
        )
    };

    match prototype.entity_kind {
        EntityKind::TransportBelt => Some(entity_visual_style(
            transport_belt_color(
                prototype
                    .transport_belt
                    .as_ref()
                    .map(|belt| belt.speed_subtiles_per_tick),
            ),
            Vec2::splat(TRANSPORT_BELT_SPRITE_SIZE),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Splitter => Some(entity_visual_style(
            splitter_color(
                prototype
                    .splitter
                    .as_ref()
                    .map(|splitter| splitter.speed_subtiles_per_tick),
            ),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Chest => Some(entity_visual_style(
            chest_color(),
            Vec2::splat(CHEST_SPRITE_SIZE),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::MiningDrill => Some(entity_visual_style(
            mining_drill_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Furnace => Some(entity_visual_style(
            furnace_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::AssemblingMachine => Some(entity_visual_style(
            match prototype
                .assembling_machine
                .as_ref()
                .map(|assembling_machine| assembling_machine.crafting_category)
            {
                Some(CraftingCategory::OilProcessing) => oil_refinery_color(),
                Some(CraftingCategory::Chemistry) => chemical_plant_color(),
                _ => assembler_color(),
            },
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Lab => Some(entity_visual_style(
            lab_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Inserter => Some(entity_visual_style(
            inserter_color(prototype.inserter.as_ref()),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::ElectricPole => Some(entity_visual_style(
            electric_pole_color(),
            Vec2::splat(CHEST_SPRITE_SIZE),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::SteamEngine => Some(entity_visual_style(
            steam_engine_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Boiler => Some(entity_visual_style(
            boiler_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::OffshorePump => Some(entity_visual_style(
            offshore_pump_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Pumpjack => Some(entity_visual_style(
            pumpjack_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Pipe => Some(entity_visual_style(
            pipe_color(),
            Vec2::splat(TRANSPORT_BELT_SPRITE_SIZE),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::StorageTank => Some(entity_visual_style(
            storage_tank_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Wall => Some(entity_visual_style(
            wall_color(),
            Vec2::splat(CHEST_SPRITE_SIZE),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::GunTurret => Some(entity_visual_style(
            gun_turret_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::EnemySpawner => Some(entity_visual_style(
            enemy_spawner_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::ResourcePatch => None,
    }
}

fn entity_visual_style(
    base_color: Color,
    size: Vec2,
    kind: EntityKind,
    direction: Direction,
) -> EntityVisualStyle {
    EntityVisualStyle {
        base_color,
        size,
        kind,
        direction,
        connections: ConnectionMask::EMPTY,
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
