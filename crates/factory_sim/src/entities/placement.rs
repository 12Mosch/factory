use crate::entities::EntityFootprint;
use crate::ids::EntityId;
use factory_data::{EntityPrototypeId, ItemId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    MissingPrototype(EntityPrototypeId),
    InvalidFootprint { width: i32, height: i32 },
    OutsideGeneratedChunks { x: i32, y: i32 },
    TileBlocked { x: i32, y: i32 },
    EntityOccupied { x: i32, y: i32, entity_id: EntityId },
    MissingEntity(EntityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerBuildError {
    Build(BuildError),
    MissingPrototype(EntityPrototypeId),
    EntityLocked {
        prototype_id: EntityPrototypeId,
    },
    MissingBuildItem {
        prototype_id: EntityPrototypeId,
    },
    ItemDoesNotBuildEntity {
        item_id: ItemId,
        prototype_id: EntityPrototypeId,
    },
    InsufficientInventory {
        item_id: ItemId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildPlacementPreview {
    pub footprint: Option<EntityFootprint>,
    pub issues: Vec<BuildPlacementIssue>,
}

impl BuildPlacementPreview {
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn first_issue(&self) -> Option<&BuildPlacementIssue> {
        self.issues.first()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildPlacementIssue {
    pub tile: Option<(i32, i32)>,
    pub kind: BuildPlacementIssueKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildPlacementIssueKind {
    MissingPrototype(EntityPrototypeId),
    InvalidFootprint {
        width: i32,
        height: i32,
    },
    OutsideGeneratedChunks,
    TerrainBlocked,
    EntityOccupied {
        entity_id: EntityId,
    },
    GhostOccupied,
    PlayerOccupied,
    MissingBuildItem {
        prototype_id: EntityPrototypeId,
    },
    ItemDoesNotBuildEntity {
        item_id: ItemId,
        prototype_id: EntityPrototypeId,
    },
    EntityLocked {
        prototype_id: EntityPrototypeId,
    },
    InsufficientInventory {
        item_id: ItemId,
    },
    MissingRequiredResource,
    MissingAdjacentWater,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityDestroyError {
    MissingEntity(EntityId),
    MissingBuildItem { prototype_id: EntityPrototypeId },
    InsufficientInventory { item_id: ItemId },
    UnknownItem(ItemId),
}
