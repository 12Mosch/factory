use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
#[cfg(test)]
use factory_data::BasePrototypeIds;
use factory_data::ItemId;
#[cfg(test)]
use factory_sim::EntityId;
use factory_sim::Simulation;

use crate::constants::BELT_ITEM_LABEL_FONT_SIZE;
use crate::rendering::resources::BeltItemRenderPool;
use crate::utils::compact_item_name;

use super::components::{BeltItemLabel, VisibleBeltItemRenderState};
#[cfg(test)]
use super::items::transport_item_render_state_with_ids;

pub(super) fn spawn_or_reuse_belt_item_label(
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

pub(super) fn label_translation(mut translation: Vec3) -> Vec3 {
    translation.z += 0.2;
    translation
}

pub(super) fn belt_item_label(sim: &Simulation, item_id: ItemId) -> String {
    let name = sim
        .catalog()
        .item(item_id)
        .map(|item| item.name.as_str())
        .unwrap_or("?");
    compact_item_name(name)
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
pub(super) fn transport_item_label_render_state(
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
        factory_sim::entity_access::splitter_state(sim, entity_id)
            .ok()?
            .input_lanes
            .get(input_port)?
            .get(lane_index)?
            .items
            .get(item_index)?
            .item_id
    } else {
        factory_sim::entity_access::belt_segment(sim, entity_id)
            .ok()?
            .lanes
            .get(lane_index)?
            .items
            .get(item_index)?
            .item_id
    };
    let name = sim
        .catalog()
        .item(item_id)
        .map(|item| item.name.as_str())
        .unwrap_or("?");

    translation.z += 0.2;
    Some((translation, compact_item_name(name)))
}
