use bevy::prelude::{Color, Vec2, Vec3};
use factory_data::{BasePrototypeIds, ItemId};
#[cfg(test)]
use factory_sim::BELT_SUBTILES_PER_TILE;
use factory_sim::{EntityId, Simulation};
use std::collections::HashSet;

#[cfg(test)]
use crate::constants::TILE_SIZE;
use crate::rendering::transforms::tile_translation;

use super::super::components::VisibleBeltItemRenderState;
#[cfg(test)]
use super::super::render_state::direction_render_vector;
use super::super::render_state::{
    splitter_port_tiles_for_render, transport_item_render_state_from_parts,
};

#[cfg(test)]
pub(crate) fn collect_visible_belt_items_into(
    sim: &Simulation,
    ids: BasePrototypeIds,
    visible_ids: &HashSet<EntityId>,
    items: &mut Vec<VisibleBeltItemRenderState>,
) {
    items.clear();
    for &entity_id in visible_ids {
        collect_belt_items_append(sim, ids, entity_id, items);
    }
}

pub(super) fn collect_belt_items_into(
    sim: &Simulation,
    ids: BasePrototypeIds,
    entity_id: EntityId,
    items: &mut Vec<VisibleBeltItemRenderState>,
) {
    items.clear();
    collect_belt_items_append(sim, ids, entity_id, items);
}

fn collect_belt_items_append(
    sim: &Simulation,
    ids: BasePrototypeIds,
    entity_id: EntityId,
    items: &mut Vec<VisibleBeltItemRenderState>,
) {
    let Some(placed) = sim.entities().placed_entity(entity_id) else {
        return;
    };
    if let Ok(segment) = factory_sim::entity_access::belt_segment(sim, placed.id) {
        let center = tile_translation(placed.x, placed.y, 4.0);
        for (lane_index, lane) in segment.lanes.iter().enumerate() {
            for item in &lane.items {
                items.push(transport_item_render_state_from_parts(
                    item.id,
                    lane_index,
                    segment.dir,
                    center,
                    item.item_id,
                    item.position_subtile,
                    belt_item_color(item.item_id, ids),
                ));
            }
        }
        return;
    }

    let Ok(state) = factory_sim::entity_access::splitter_state(sim, placed.id) else {
        return;
    };
    let Some(port_tiles) = splitter_port_tiles_for_render(&placed.footprint) else {
        return;
    };
    for (input_port, input_lanes) in state.input_lanes.iter().enumerate() {
        let port_tile = port_tiles[input_port];
        let center = tile_translation(port_tile.0, port_tile.1, 4.0);
        for (lane_index, lane) in input_lanes.iter().enumerate() {
            for item in &lane.items {
                items.push(transport_item_render_state_from_parts(
                    item.id,
                    lane_index,
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
pub(crate) fn transport_item_render_state_with_ids(
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
