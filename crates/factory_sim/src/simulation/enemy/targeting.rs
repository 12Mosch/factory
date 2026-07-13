use super::*;

/// Derived index and shared target selections for attacking enemies.
///
/// None of this is authoritative simulation state. The index is rebuilt from
/// placed entities after topology changes, and the selections are discarded
/// with it so placements and removals become visible to every attacking
/// group without serializing redundant data.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct AttackTargetCache {
    pub(super) revision: Option<u64>,
    pub(super) index: AttackableStructureIndex,
    pub(super) base_targets: BTreeMap<EnemyBaseId, Option<EntityId>>,
    pub(super) raid_targets: BTreeMap<RaidId, Option<EntityId>>,
    #[cfg(test)]
    pub(super) shared_target_queries: usize,
}

impl AttackTargetCache {
    /// Refreshes derived state and reports whether a previously built index
    /// was invalidated. The first build (including after load) is not an
    /// invalidation of durable unit and raid targets.
    pub(super) fn refresh(
        &mut self,
        revision: u64,
        world: &WorldSim,
        entities: &EntityStore,
    ) -> bool {
        if self.revision == Some(revision) {
            return false;
        }

        let invalidated = self.revision.is_some();
        self.index.rebuild(world, entities);
        self.base_targets.clear();
        self.raid_targets.clear();
        self.revision = Some(revision);
        invalidated
    }

    pub(super) fn target_for_base(
        &mut self,
        base_id: EnemyBaseId,
        from: EntityFootprint,
    ) -> Option<EntityId> {
        if let Some(target) = self.base_targets.get(&base_id) {
            return *target;
        }
        #[cfg(test)]
        {
            self.shared_target_queries += 1;
        }
        let target = self.index.nearest(&from);
        self.base_targets.insert(base_id, target);
        target
    }

    pub(super) fn target_for_raid(
        &mut self,
        raid_id: RaidId,
        from: EntityFootprint,
        current: Option<EntityId>,
    ) -> Option<EntityId> {
        if let Some(target) = self.raid_targets.get(&raid_id) {
            return *target;
        }
        #[cfg(test)]
        {
            self.shared_target_queries += 1;
        }
        let target = current.or_else(|| self.index.nearest(&from));
        self.raid_targets.insert(raid_id, target);
        target
    }

    pub(super) fn retain_active_groups(&mut self, enemies: &EnemySubsystem) {
        self.base_targets
            .retain(|base_id, _| enemies.bases.contains_key(base_id));
        self.raid_targets
            .retain(|raid_id, _| enemies.raids.contains_key(raid_id));
    }
}

/// Attackable entity ids grouped by the chunk-sized cell containing their
/// footprint center. A nearest query scans cell bounds first and only visits
/// entity ids in cells that can beat the current result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(super) struct AttackableStructureIndex {
    pub(super) cells: BTreeMap<(i64, i64), IndexedAttackCell>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct IndexedAttackCell {
    min_x: i128,
    max_x: i128,
    min_y: i128,
    max_y: i128,
    targets: Vec<IndexedAttackTarget>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct IndexedAttackTarget {
    pub(super) entity_id: EntityId,
    pub(super) footprint: EntityFootprint,
}

impl IndexedAttackCell {
    pub(super) fn new(target: IndexedAttackTarget) -> Self {
        let footprint = target.footprint;
        Self {
            min_x: i128::from(footprint.x),
            max_x: footprint_axis_end(footprint.x, footprint.width),
            min_y: i128::from(footprint.y),
            max_y: footprint_axis_end(footprint.y, footprint.height),
            targets: vec![target],
        }
    }

    pub(super) fn push(&mut self, target: IndexedAttackTarget) {
        let footprint = target.footprint;
        self.min_x = self.min_x.min(i128::from(footprint.x));
        self.max_x = self
            .max_x
            .max(footprint_axis_end(footprint.x, footprint.width));
        self.min_y = self.min_y.min(i128::from(footprint.y));
        self.max_y = self
            .max_y
            .max(footprint_axis_end(footprint.y, footprint.height));
        self.targets.push(target);
    }

    fn distance_squared_to(&self, from: &EntityFootprint) -> u128 {
        from.distance_squared_to_bounds(self.min_x, self.max_x, self.min_y, self.max_y)
    }
}

fn footprint_axis_end(start: WorldTileCoord, length: i32) -> i128 {
    i128::from(start) + i128::from(length) - 1
}

impl AttackableStructureIndex {
    fn rebuild(&mut self, _world: &WorldSim, entities: &EntityStore) {
        self.cells.clear();
        for (&entity_id, placed) in &entities.placed_entities {
            if !is_attackable_kind(entities, placed) {
                continue;
            }
            let (center_x, center_y) = footprint_center_tile(&placed.footprint);
            let cell = (
                center_x.div_euclid(i64::from(CHUNK_SIZE)),
                center_y.div_euclid(i64::from(CHUNK_SIZE)),
            );
            let target = IndexedAttackTarget {
                entity_id,
                footprint: placed.footprint,
            };
            self.cells
                .entry(cell)
                .and_modify(|cell| cell.push(target))
                .or_insert_with(|| IndexedAttackCell::new(target));
        }
    }

    pub(super) fn nearest(&self, from: &EntityFootprint) -> Option<EntityId> {
        let nearest_cell_distance = self
            .cells
            .values()
            .map(|cell| cell.distance_squared_to(from))
            .min()?;

        let mut best = None;
        for cell in self.cells.values() {
            if cell.distance_squared_to(from) == nearest_cell_distance {
                update_nearest_from_candidates(&mut best, from, &cell.targets);
            }
        }

        for cell in self.cells.values() {
            let cell_distance = cell.distance_squared_to(from);
            if cell_distance > nearest_cell_distance
                && best.is_none_or(|(distance, _)| cell_distance <= distance)
            {
                update_nearest_from_candidates(&mut best, from, &cell.targets);
            }
        }
        best.map(|(_, entity_id)| entity_id)
    }
}

fn update_nearest_from_candidates(
    best: &mut Option<(u128, EntityId)>,
    from: &EntityFootprint,
    candidates: &[IndexedAttackTarget],
) {
    for candidate in candidates {
        let distance = from.distance_squared_to(&candidate.footprint);
        let result = (distance, candidate.entity_id);
        if best.is_none_or(|current| result < current) {
            *best = Some(result);
        }
    }
}

/// Ticks between target rescans for units without a target.
/// Chooses what a unit fights: guards react to player structures near them,
/// attackers march on the closest structure anywhere in the world.
pub(super) fn acquire_target(
    entities: &EntityStore,
    attackable: &AttackableStructureIndex,
    enemy: &Enemy,
) -> Option<EntityId> {
    let (tile_x, tile_y) = enemy.tile();
    let footprint = enemy_footprint(enemy);
    match enemy.mode {
        EnemyMode::Guard => {
            let radius = i64::from(enemy.aggro_radius_tiles);
            let candidates = entities.occupancy.entity_ids_in_tile_rect(
                tile_x - radius,
                tile_x + radius,
                tile_y - radius,
                tile_y + radius,
            );
            nearest_attackable(entities, &footprint, candidates.into_iter())
        }
        EnemyMode::Attack => attackable.nearest(&footprint),
    }
}

/// Nearest player structure among `candidates`; enemy-owned entities are
/// never targets. Ties resolve to the lowest entity id because candidates
/// iterate in ascending id order.
fn nearest_attackable(
    entities: &EntityStore,
    from: &EntityFootprint,
    candidates: impl Iterator<Item = EntityId>,
) -> Option<EntityId> {
    nearest_attackable_with_distance(entities, from, candidates).map(|(_, entity_id)| entity_id)
}

fn nearest_attackable_with_distance(
    entities: &EntityStore,
    from: &EntityFootprint,
    candidates: impl Iterator<Item = EntityId>,
) -> Option<(u128, EntityId)> {
    let mut best: Option<(u128, EntityId)> = None;
    for entity_id in candidates {
        let Some(placed) = entities.placed_entities.get(&entity_id) else {
            continue;
        };
        if !is_attackable_kind(entities, placed) {
            continue;
        }
        let distance = from.distance_squared_to(&placed.footprint);
        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, entity_id));
        }
    }
    best
}

pub(super) fn is_attackable_kind(entities: &EntityStore, placed: &PlacedEntity) -> bool {
    entities
        .entity_health
        .get(&placed.id)
        .is_some_and(|health| Faction::Enemy.is_hostile_to(health.faction))
}

fn footprint_center_tile(footprint: &EntityFootprint) -> (WorldTileCoord, WorldTileCoord) {
    (
        footprint.x + i64::from(footprint.width) / 2,
        footprint.y + i64::from(footprint.height) / 2,
    )
}

fn enemy_footprint(enemy: &Enemy) -> EntityFootprint {
    let (x, y) = enemy.tile();
    EntityFootprint::single_tile(x, y)
}
