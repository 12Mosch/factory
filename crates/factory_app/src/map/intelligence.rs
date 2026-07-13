use bevy::prelude::{Color, Vec2};
use factory_sim::{ChunkCoord, EntityId, GhostId};

/// Stable identity used to reconcile map presentation entities across frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MapPrimitiveKey {
    Entity(EntityId),
    Ghost(GhostId),
    Chunk(ChunkCoord),
    PoleConnection(EntityId, EntityId),
    TaggedEntity(u8, EntityId),
    Navigation(u64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MapPrimitiveShape {
    Point {
        position: Vec2,
        size: f32,
    },
    Rectangle {
        min: Vec2,
        max: Vec2,
        border_width: f32,
    },
    Line {
        start: Vec2,
        end: Vec2,
        width: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapPrimitive {
    pub key: MapPrimitiveKey,
    pub shape: MapPrimitiveShape,
    pub color: Color,
    pub fill: Color,
}
