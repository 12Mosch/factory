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
    pub x: i32,
    pub y: i32,
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
        let mut width = 0;
        let mut height = 0;
        for entity in &self.entities {
            width = width.max(entity.dx + 1);
            height = height.max(entity.dy + 1);
        }
        (width, height)
    }
}

/// Construction planning state: ghost entities, deconstruction marks, the
/// pending job queue, and the blueprint library. Part of the deterministic
/// simulation state (saved, hashed, validated).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ConstructionState {
    pub(crate) ghosts: BTreeMap<GhostId, GhostEntity>,
    pub(crate) ghost_occupancy: BTreeMap<(i32, i32), GhostId>,
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

    pub fn ghost_at(&self, x: i32, y: i32) -> Option<&GhostEntity> {
        let ghost_id = self.ghost_occupancy.get(&(x, y))?;
        self.ghosts.get(ghost_id)
    }

    pub fn ghost_ids_in_tile_rect(
        &self,
        min_x: i32,
        max_x: i32,
        min_y: i32,
        max_y: i32,
    ) -> BTreeSet<GhostId> {
        if min_x > max_x || min_y > max_y {
            return BTreeSet::new();
        }

        self.ghost_occupancy
            .range((min_x, i32::MIN)..=(max_x, i32::MAX))
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
    EntityLocked { prototype_id: EntityPrototypeId },
    GhostOccupied { x: i32, y: i32, ghost_id: GhostId },
    MissingGhost(GhostId),
    NotMarkedForDeconstruction(EntityId),
    EmptyBlueprintArea,
    MissingBlueprint { index: usize },
}
