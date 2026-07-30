use crate::entities::EntityFootprint;
use crate::ids::EntityId;
use factory_data::{EntityPrototypeId, ItemId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    MissingPrototype(EntityPrototypeId),
    InvalidFootprint {
        width: i32,
        height: i32,
    },
    OutsideGeneratedChunks {
        x: crate::world::WorldTileCoord,
        y: crate::world::WorldTileCoord,
    },
    TileBlocked {
        x: crate::world::WorldTileCoord,
        y: crate::world::WorldTileCoord,
    },
    EntityOccupied {
        x: crate::world::WorldTileCoord,
        y: crate::world::WorldTileCoord,
        entity_id: EntityId,
    },
    MissingEntity(EntityId),
    /// The prototype is rolling stock, which runs on rails rather than sitting
    /// on tiles. It has no footprint to reserve, so the tile placement path
    /// refuses it outright instead of quietly creating a locomotive that
    /// occupies a square of grass.
    RunsOnRails {
        prototype_id: EntityPrototypeId,
    },
    /// The prototype is a rail signal, and the tile it was aimed at has no rail
    /// joint beside it running the way the signal faces.
    ///
    /// A signal is a point on the railway together with a direction of travel
    /// through it, and both halves have to exist for it to govern anything. One
    /// that governed nothing would still cut the block partition, so a railway
    /// could be split in two by a signal that looked placed and did nothing —
    /// which is why this is refused rather than allowed and ignored.
    NeedsAlignedRail {
        prototype_id: EntityPrototypeId,
    },
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
    pub tile: Option<(crate::world::WorldTileCoord, crate::world::WorldTileCoord)>,
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
    /// The held item builds rolling stock, and the cursor is not over a clear
    /// run of rail long enough to hold it.
    NeedsClearRail {
        prototype_id: EntityPrototypeId,
    },
    /// The held item builds a rail signal, and there is no rail joint beside the
    /// cursor running the way the signal faces.
    NeedsAlignedRail {
        prototype_id: EntityPrototypeId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityDestroyError {
    MissingEntity(EntityId),
    MissingBuildItem { prototype_id: EntityPrototypeId },
    InsufficientInventory { item_id: ItemId },
    UnknownItem(ItemId),
}
