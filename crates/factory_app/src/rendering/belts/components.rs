use bevy::prelude::*;
use factory_data::ItemId;
use factory_sim::{BeltItemId, EntityId};

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
    pub(super) key: BeltItemId,
    pub(super) item_id: ItemId,
    pub(super) active: bool,
}

#[derive(Component)]
pub(crate) struct BeltItemLabel {
    pub(super) key: BeltItemId,
    pub(super) item_id: ItemId,
    pub(super) active: bool,
}

#[derive(Clone, Copy)]
pub(super) struct VisibleBeltItemRenderState {
    pub(super) key: BeltItemId,
    pub(super) item_id: ItemId,
    pub(super) translation: Vec3,
    pub(super) color: Color,
}
