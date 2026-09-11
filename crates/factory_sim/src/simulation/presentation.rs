use super::*;

const ENTITY_VISUAL_CHANGE_HISTORY_LIMIT: usize = 4_096;

/// The local region whose placed-entity visuals may have changed at one
/// topology revision. The rectangle includes both footprints when an entity
/// moves or rotates; presentation expands it by one tile to refresh connected
/// belts, pipes, and heat pipes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntityVisualChange {
    pub revision: u64,
    pub entity_id: EntityId,
    pub min_x: WorldTileCoord,
    pub max_x: WorldTileCoord,
    pub min_y: WorldTileCoord,
    pub max_y: WorldTileCoord,
}

#[derive(Clone, Debug, Default)]
pub(super) struct EntityVisualChangeHistory {
    changes: VecDeque<EntityVisualChange>,
    /// Revisions at or above this value can be answered exactly. This advances
    /// only when retained history is evicted; revisions with no visual change
    /// do not need placeholder entries.
    floor_revision: u64,
}

impl_runtime_only_identity!(EntityVisualChangeHistory);

impl EntityVisualChangeHistory {
    pub(super) fn push(
        &mut self,
        revision: u64,
        entity_id: EntityId,
        footprint: EntityFootprint,
        previous_footprint: Option<EntityFootprint>,
    ) {
        let previous = previous_footprint.unwrap_or(footprint);
        let change = EntityVisualChange {
            revision,
            entity_id,
            min_x: footprint.x.min(previous.x),
            max_x: footprint
                .x
                .saturating_add(i64::from(footprint.width) - 1)
                .max(previous.x.saturating_add(i64::from(previous.width) - 1)),
            min_y: footprint.y.min(previous.y),
            max_y: footprint
                .y
                .saturating_add(i64::from(footprint.height) - 1)
                .max(previous.y.saturating_add(i64::from(previous.height) - 1)),
        };
        self.changes.push_back(change);
        while self.changes.len() > ENTITY_VISUAL_CHANGE_HISTORY_LIMIT {
            if let Some(evicted) = self.changes.pop_front() {
                self.floor_revision = evicted.revision;
            }
        }
    }

    pub(super) fn since(
        &self,
        revision: u64,
    ) -> Option<impl Iterator<Item = EntityVisualChange> + '_> {
        (revision >= self.floor_revision).then(|| {
            self.changes
                .iter()
                .copied()
                .filter(move |change| change.revision > revision)
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct EntityStyleChangeHistory {
    changes: VecDeque<(u64, EntityId)>,
    floor_revision: u64,
}

impl_runtime_only_identity!(EntityStyleChangeHistory);

impl EntityStyleChangeHistory {
    pub(super) fn push(&mut self, revision: u64, entity_id: EntityId) {
        self.changes.push_back((revision, entity_id));
        while self.changes.len() > ENTITY_VISUAL_CHANGE_HISTORY_LIMIT {
            if let Some((evicted_revision, _)) = self.changes.pop_front() {
                self.floor_revision = evicted_revision;
            }
        }
    }

    pub(super) fn since(&self, revision: u64) -> Option<impl Iterator<Item = EntityId> + '_> {
        (revision >= self.floor_revision).then(|| {
            self.changes
                .iter()
                .filter(move |(changed_revision, _)| *changed_revision > revision)
                .map(|(_, entity_id)| *entity_id)
        })
    }
}

/// Chunk membership for movers that do not live in the placed-entity
/// occupancy grid. Vec capacity is retained across ticks, and the index is
/// excluded from deterministic simulation identity.
#[derive(Clone, Debug, Default)]
pub(super) struct DynamicUnitChunkIndex {
    robot_ids: BTreeMap<ChunkCoord, Vec<RobotId>>,
    rolling_stock_ids: BTreeMap<ChunkCoord, Vec<RollingStockId>>,
}

impl_runtime_only_identity!(DynamicUnitChunkIndex);

impl DynamicUnitChunkIndex {
    fn clear(&mut self) {
        for ids in self.robot_ids.values_mut() {
            ids.clear();
        }
        for ids in self.rolling_stock_ids.values_mut() {
            ids.clear();
        }
    }

    fn retain_populated(&mut self) {
        self.robot_ids.retain(|_, ids| !ids.is_empty());
        self.rolling_stock_ids.retain(|_, ids| !ids.is_empty());
    }
}

impl Simulation {
    pub(super) fn refresh_dynamic_unit_chunk_index(&mut self) {
        let mut index = std::mem::take(&mut self.dynamic_unit_chunks);
        index.clear();

        for robot in self.robot_flights.iter() {
            if let Some(chunk) = ChunkCoord::from_tile(robot.tile().0, robot.tile().1) {
                index.robot_ids.entry(chunk).or_default().push(robot.id);
            }
        }
        for stock in self.rolling_stock.iter() {
            if let Some((x, y)) = self.rolling_stock_tile(stock.id)
                && let Some(chunk) = ChunkCoord::from_tile(x, y)
            {
                index
                    .rolling_stock_ids
                    .entry(chunk)
                    .or_default()
                    .push(stock.id);
            }
        }

        index.retain_populated();
        self.dynamic_unit_chunks = index;
    }

    pub fn robot_ids_in_chunk(&self, chunk: ChunkCoord) -> &[RobotId] {
        self.dynamic_unit_chunks
            .robot_ids
            .get(&chunk)
            .map_or(&[], Vec::as_slice)
    }

    pub fn enemy_ids_in_chunk(&self, chunk: ChunkCoord) -> &[EnemyId] {
        self.enemy_target_chunks.ids_in_chunk(chunk)
    }

    pub fn rolling_stock_ids_in_chunk(&self, chunk: ChunkCoord) -> &[RollingStockId] {
        self.dynamic_unit_chunks
            .rolling_stock_ids
            .get(&chunk)
            .map_or(&[], Vec::as_slice)
    }

    pub fn entity_visual_changes_since(
        &self,
        revision: u64,
    ) -> Option<impl Iterator<Item = EntityVisualChange> + '_> {
        self.entity_visual_changes.since(revision)
    }

    pub fn entity_style_revision(&self) -> u64 {
        self.entity_style_revision
    }

    pub fn entity_style_changes_since(
        &self,
        revision: u64,
    ) -> Option<impl Iterator<Item = EntityId> + '_> {
        self.entity_style_changes.since(revision)
    }
}
