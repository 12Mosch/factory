use crate::entities::{
    BuildError, Direction, EntityDestroyError, EntityFootprint, PlayerBuildError,
};
use crate::ids::EntityId;
use factory_data::{EntityPrototypeId, RecipeId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Identifier for a ghost entity. Ghost ids live in their own namespace,
/// allocated from [`ConstructionState::next_ghost_id`]; they are unrelated to
/// [`EntityId`]s and are never reused within a simulation.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct GhostId(u64);

impl GhostId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// A planned entity that has not been built yet. Ghosts reserve their tiles
/// against other ghosts but never against real entities or the player; they
/// have no simulation behavior until they are built (manually today, by
/// construction robots later).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct GhostEntity {
    pub id: GhostId,
    pub prototype_id: EntityPrototypeId,
    pub x: crate::world::WorldTileCoord,
    pub y: crate::world::WorldTileCoord,
    pub direction: Direction,
    pub footprint: EntityFootprint,
    /// Recipe to preselect when the ghost is built, captured from blueprints
    /// of configured assembling machines.
    pub recipe: Option<RecipeId>,
}

/// A pending construction job. Jobs are queued in plan order; manual
/// construction may complete them in any order, while future construction
/// robots will consume the queue front-to-back.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum ConstructionJob {
    BuildGhost(GhostId),
    Deconstruct(EntityId),
}

/// One entity entry of a [`Blueprint`], positioned relative to the blueprint
/// origin (the minimum captured tile coordinate).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BlueprintEntity {
    pub prototype_id: EntityPrototypeId,
    pub dx: i32,
    pub dy: i32,
    pub direction: Direction,
    pub recipe: Option<RecipeId>,
}

/// A reusable construction plan captured from a world area. Pasting a
/// blueprint places one ghost per entry; entries that cannot be placed
/// (occupied or invalid terrain) are skipped.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Blueprint {
    pub name: String,
    pub entities: Vec<BlueprintEntity>,
}

impl Blueprint {
    /// Tile extents of the blueprint entities' origins, as `(width, height)`.
    /// Entity footprints may extend further right/up than the origin extents.
    pub fn origin_extents(&self) -> (i32, i32) {
        blueprint_entity_extents(&self.entities)
    }
}

fn blueprint_entity_extents(entities: &[BlueprintEntity]) -> (i32, i32) {
    let mut width = 0;
    let mut height = 0;
    for entity in entities {
        width = width.max(entity.dx + 1);
        height = height.max(entity.dy + 1);
    }
    (width, height)
}

/// Rotates blueprint entity offsets and directions 90 degrees clockwise
/// around their bounding box, `steps` times (`steps % 4`). Pure and
/// idempotent after four steps. This is paste-time-only: callers recompute
/// it from the canonical (unrotated) blueprint for preview and paste alike;
/// it is never written back into a saved [`Blueprint`].
pub fn rotate_blueprint_entities(entities: &[BlueprintEntity], steps: u8) -> Vec<BlueprintEntity> {
    let mut current = entities.to_vec();
    for _ in 0..(steps % 4) {
        current = rotate_blueprint_entities_once(&current);
    }
    current
}

fn rotate_blueprint_entities_once(entities: &[BlueprintEntity]) -> Vec<BlueprintEntity> {
    let (width, _height) = blueprint_entity_extents(entities);
    entities
        .iter()
        .map(|entity| BlueprintEntity {
            prototype_id: entity.prototype_id,
            dx: entity.dy,
            dy: width - 1 - entity.dx,
            direction: rotate_direction_clockwise(entity.direction),
            recipe: entity.recipe,
        })
        .collect()
}

fn rotate_direction_clockwise(direction: Direction) -> Direction {
    match direction {
        Direction::North => Direction::East,
        Direction::East => Direction::South,
        Direction::South => Direction::West,
        Direction::West => Direction::North,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(prototype_id: u16, dx: i32, dy: i32, direction: Direction) -> BlueprintEntity {
        BlueprintEntity {
            prototype_id: EntityPrototypeId::new(prototype_id),
            dx,
            dy,
            direction,
            recipe: None,
        }
    }

    #[test]
    fn four_rotations_return_to_the_original_blueprint() {
        let entities = vec![
            entity(0, 0, 0, Direction::North),
            entity(1, 2, 0, Direction::East),
            entity(2, 1, 3, Direction::South),
        ];

        let rotated = rotate_blueprint_entities(&entities, 4);

        assert_eq!(rotated, entities);
    }

    #[test]
    fn rotating_a_vertical_bar_matches_expected_offsets_and_directions() {
        let entities = vec![
            entity(0, 0, 0, Direction::North),
            entity(0, 0, 1, Direction::South),
        ];

        let rotated = rotate_blueprint_entities(&entities, 1);

        assert_eq!(
            rotated,
            vec![
                entity(0, 0, 0, Direction::East),
                entity(0, 1, 0, Direction::West),
            ]
        );
    }
}

/// Construction planning state: ghost entities, deconstruction marks, the
/// pending job queue, and the blueprint library. Part of the deterministic
/// simulation state (saved, hashed, validated).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ConstructionState {
    pub(crate) ghosts: BTreeMap<GhostId, GhostEntity>,
    pub(crate) ghost_occupancy:
        BTreeMap<(crate::world::WorldTileCoord, crate::world::WorldTileCoord), GhostId>,
    pub(crate) deconstruction_marks: BTreeSet<EntityId>,
    pub(crate) queue: VecDeque<ConstructionJob>,
    pub(crate) blueprints: Vec<Blueprint>,
    pub(crate) next_ghost_id: u64,
}

impl Default for ConstructionState {
    fn default() -> Self {
        Self {
            ghosts: BTreeMap::new(),
            ghost_occupancy: BTreeMap::new(),
            deconstruction_marks: BTreeSet::new(),
            queue: VecDeque::new(),
            blueprints: Vec::new(),
            next_ghost_id: 1,
        }
    }
}

impl ConstructionState {
    pub fn ghosts(&self) -> impl Iterator<Item = &GhostEntity> {
        self.ghosts.values()
    }

    pub fn ghost(&self, ghost_id: GhostId) -> Option<&GhostEntity> {
        self.ghosts.get(&ghost_id)
    }

    pub fn ghost_count(&self) -> usize {
        self.ghosts.len()
    }

    pub fn ghost_at(
        &self,
        x: crate::world::WorldTileCoord,
        y: crate::world::WorldTileCoord,
    ) -> Option<&GhostEntity> {
        let ghost_id = self.ghost_occupancy.get(&(x, y))?;
        self.ghosts.get(ghost_id)
    }

    pub fn ghost_ids_in_tile_rect(
        &self,
        min_x: crate::world::WorldTileCoord,
        max_x: crate::world::WorldTileCoord,
        min_y: crate::world::WorldTileCoord,
        max_y: crate::world::WorldTileCoord,
    ) -> BTreeSet<GhostId> {
        if min_x > max_x || min_y > max_y {
            return BTreeSet::new();
        }

        self.ghost_occupancy
            .range((min_x, i64::MIN)..=(max_x, i64::MAX))
            .filter_map(|(&(x, y), &ghost_id)| {
                (x >= min_x && x <= max_x && y >= min_y && y <= max_y).then_some(ghost_id)
            })
            .collect()
    }

    pub fn is_marked_for_deconstruction(&self, entity_id: EntityId) -> bool {
        self.deconstruction_marks.contains(&entity_id)
    }

    pub fn deconstruction_marks(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.deconstruction_marks.iter().copied()
    }

    pub fn deconstruction_mark_count(&self) -> usize {
        self.deconstruction_marks.len()
    }

    /// Pending construction jobs in plan order.
    pub fn queue(&self) -> impl Iterator<Item = ConstructionJob> + '_ {
        self.queue.iter().copied()
    }

    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    pub fn blueprints(&self) -> &[Blueprint] {
        &self.blueprints
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstructionError {
    Build(BuildError),
    PlayerBuild(PlayerBuildError),
    Destroy(EntityDestroyError),
    EntityLocked {
        prototype_id: EntityPrototypeId,
    },
    GhostOccupied {
        x: crate::world::WorldTileCoord,
        y: crate::world::WorldTileCoord,
        ghost_id: GhostId,
    },
    MissingGhost(GhostId),
    NotMarkedForDeconstruction(EntityId),
    EmptyBlueprintArea,
    BlueprintOffsetOutOfRange,
    MissingBlueprint {
        index: usize,
    },
}
