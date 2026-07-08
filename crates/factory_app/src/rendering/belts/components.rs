use bevy::prelude::*;
use factory_data::ItemId;
use factory_sim::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BeltDirectionPart {
    Shaft,
    Head,
}

#[derive(Component)]
pub(crate) struct BeltDirectionSprite {
    pub(super) entity_id: EntityId,
    pub(super) part: BeltDirectionPart,
}

#[derive(Component)]
pub(crate) struct BeltItemSprite {
    pub(super) key: BeltItemKey,
    pub(super) item_id: ItemId,
    pub(super) active: bool,
}

#[derive(Component)]
pub(crate) struct BeltItemLabel {
    pub(super) key: BeltItemKey,
    pub(super) item_id: ItemId,
    pub(super) active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct BeltItemKey {
    pub(super) entity_id: EntityId,
    pub(super) input_port: Option<usize>,
    pub(super) lane_index: usize,
    pub(super) item_index: usize,
}

#[derive(Clone, Copy)]
pub(super) struct VisibleBeltItemRenderState {
    pub(super) key: BeltItemKey,
    pub(super) item_id: ItemId,
    pub(super) translation: Vec3,
    pub(super) color: Color,
}
