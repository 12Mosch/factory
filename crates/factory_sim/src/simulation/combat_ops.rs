use super::*;
use std::collections::BTreeMap;

/// Attacks committed during a tick, before any combat damage is applied.
///
/// Damage is aggregated by target so resolution is independent of attacker
/// iteration order. For a surviving enemy hit by several turrets, the lowest
/// turret ID becomes its deterministic retaliation target.
#[derive(Default)]
pub(super) struct CombatIntents {
    structure_damage: BTreeMap<EntityId, u64>,
    enemy_damage: BTreeMap<EnemyId, EnemyDamageIntent>,
}

struct EnemyDamageIntent {
    amount: u64,
    retaliation_target: EntityId,
}

impl CombatIntents {
    pub(super) fn record_enemy_attack(&mut self, target: EntityId, amount: u32) {
        let damage = self.structure_damage.entry(target).or_default();
        *damage = damage.saturating_add(u64::from(amount));
    }

    fn record_turret_attack(&mut self, turret_id: EntityId, target: TurretTarget, amount: u32) {
        match target {
            TurretTarget::Enemy(enemy_id) => {
                self.enemy_damage
                    .entry(enemy_id)
                    .and_modify(|intent| {
                        intent.amount = intent.amount.saturating_add(u64::from(amount));
                        intent.retaliation_target = intent.retaliation_target.min(turret_id);
                    })
                    .or_insert(EnemyDamageIntent {
                        amount: u64::from(amount),
                        retaliation_target: turret_id,
                    });
            }
            TurretTarget::Spawner(spawner_id) => {
                let damage = self.structure_damage.entry(spawner_id).or_default();
                *damage = damage.saturating_add(u64::from(amount));
            }
        }
    }
}

#[derive(Clone, Copy)]
enum TurretTarget {
    Enemy(EnemyId),
    Spawner(EntityId),
}

impl Simulation {
    /// Gun turrets acquire the nearest enemy unit in range (falling back to
    /// enemy spawners), consume magazine shots, and commit their attacks for
    /// the tick's shared combat resolution phase.
    pub(super) fn advance_gun_turrets(&mut self, intents: &mut CombatIntents) {
        if self.onboarding_progress.loaded_gun_turrets == 0
            && self.entities.gun_turrets.iter().any(|(entity_id, state)| {
                self.entities.placed_entities.contains_key(entity_id)
                    && (state.loaded_shots > 0
                        || state.ammo.slots.iter().flatten().any(|stack| {
                            self.world
                                .prototypes
                                .item(stack.item_id)
                                .is_some_and(|item| item.ammo.is_some())
                        }))
            })
        {
            self.onboarding_progress
                .record_counter(|progress| &mut progress.loaded_gun_turrets, 1);
        }
        // Units move during their own simulation step, not during turret
        // fire, so one index is valid for this whole pass.
        let enemy_chunks = EnemyChunkIndex::from_enemies(&self.enemies);
        {
            let tick = self.tick;
            let Simulation {
                world,
                entities,
                enemies,
                ..
            } = self;

            for (&turret_id, state) in entities.gun_turrets.iter_mut() {
                if tick < state.next_ready_tick {
                    continue;
                }
                let Some(placed) = entities.placed_entities.get(&turret_id) else {
                    continue;
                };
                let Some(turret) = world
                    .prototypes
                    .entity(placed.prototype_id)
                    .and_then(|prototype| prototype.gun_turret)
                else {
                    continue;
                };

                if state.loaded_shots == 0 {
                    load_magazine(world, state);
                }
                if state.loaded_shots == 0 {
                    continue;
                }

                let footprint = placed.footprint;
                let range = i64::from(turret.range_tiles);
                let target = if let Some(enemy_id) =
                    nearest_enemy_in_range(enemies, &enemy_chunks, &footprint, range)
                {
                    Some(TurretTarget::Enemy(enemy_id))
                } else {
                    nearest_spawner_in_range(
                        &entities.enemy_spawners,
                        &entities.placed_entities,
                        &entities.occupancy,
                        &footprint,
                        range,
                    )
                    .map(TurretTarget::Spawner)
                };

                if let Some(target) = target {
                    intents.record_turret_attack(turret_id, target, state.loaded_damage);
                    state.loaded_shots -= 1;
                    state.next_ready_tick = tick + u64::from(turret.cooldown_ticks);
                }
            }
        }
    }

    /// Applies every attack committed from the tick's pre-resolution combat
    /// snapshot. A combatant destroyed here still contributes its own queued
    /// attack, so neither side receives an initiative advantage.
    pub(super) fn resolve_combat(&mut self, intents: CombatIntents) {
        for (entity_id, amount) in intents.structure_damage {
            let amount = u32::try_from(amount).unwrap_or(u32::MAX);
            self.damage_entity(entity_id, amount);
        }

        for (enemy_id, intent) in intents.enemy_damage {
            let Some(enemy) = self.enemies.enemies.get_mut(&enemy_id) else {
                continue;
            };
            if u64::from(enemy.health) <= intent.amount {
                self.enemies.enemies.remove(&enemy_id);
            } else {
                let amount = u32::try_from(intent.amount)
                    .expect("damage below surviving u32 health must fit in u32");
                enemy.health -= amount;
                // Being fired on pulls a surviving unit onto the lowest-ID
                // turret that participated in the simultaneous volley.
                enemy.target = Some(intent.retaliation_target);
                enemy.path.clear();
            }
        }
    }

    /// Applies damage to a placed entity's health; at zero the entity is
    /// violently destroyed (no item recovery). Entities without health state
    /// are indestructible. Returns true when the entity was destroyed.
    pub(crate) fn damage_entity(&mut self, entity_id: EntityId, amount: u32) -> bool {
        // Entities without health state shrug the hit off entirely, so they
        // must not raise an under-attack alarm either.
        if !self.entities.entity_health.contains_key(&entity_id) {
            return false;
        }
        let warning_location = self
            .entities
            .placed_entities
            .get(&entity_id)
            .and_then(|placed| {
                self.world
                    .prototypes
                    .entity(placed.prototype_id)
                    .filter(|prototype| {
                        !matches!(
                            prototype.entity_kind,
                            EntityKind::EnemySpawner | EntityKind::ResourcePatch
                        )
                    })
                    .map(|_| (placed.x, placed.y))
            });
        if let Some((x, y)) = warning_location {
            self.emit_structure_damage_warning(x, y);
        }
        let Some(health) = self.entities.entity_health.get_mut(&entity_id) else {
            return false;
        };
        if health.current > amount {
            health.current -= amount;
            return false;
        }

        entity_mutation::remove(self, entity_id);
        true
    }

    /// Player repair action: consumes repair pack durability to restore a
    /// nearby entity's health. The app repeats this command while the repair
    /// input is held; a fully repaired target is a no-op success.
    pub fn repair_entity(&mut self, entity_id: EntityId) -> Result<(), RepairError> {
        let placed = self
            .entities
            .placed_entities
            .get(&entity_id)
            .ok_or(RepairError::MissingEntity(entity_id))?;
        let max_health = self
            .world
            .prototypes
            .entity(placed.prototype_id)
            .and_then(|prototype| prototype.max_health)
            .ok_or(RepairError::NotRepairable(entity_id))?;
        let footprint = placed.footprint;

        let (player_tile_x, player_tile_y) = self.player.tile_position();
        let player_footprint = EntityFootprint::single_tile(player_tile_x, player_tile_y);
        let reach = REPAIR_REACH_TILES as i64;
        if player_footprint.chebyshev_distance_to(&footprint) > reach {
            return Err(RepairError::OutOfReach);
        }

        let current = self
            .entities
            .entity_health
            .get(&entity_id)
            .map(|health| health.current)
            .unwrap_or(max_health);
        if current >= max_health {
            return Ok(());
        }

        if self.player.repair_remaining_health == 0 {
            let repair_item = self
                .player_inventory
                .slots
                .iter()
                .flatten()
                .map(|stack| stack.item_id)
                .find(|item_id| {
                    self.world
                        .prototypes
                        .item(*item_id)
                        .is_some_and(|item| item.repair.is_some())
                })
                .ok_or(RepairError::NoRepairPacks)?;
            let restore_health = self
                .world
                .prototypes
                .item(repair_item)
                .and_then(|item| item.repair)
                .map(|repair| repair.restore_health)
                .expect("repair item was selected for its repair prototype");
            self.player_inventory
                .remove(repair_item, 1)
                .expect("repair pack was just found in the player inventory");
            self.player.repair_remaining_health = restore_health;
        }

        let heal = REPAIR_HEALTH_PER_ACTION
            .min(max_health - current)
            .min(self.player.repair_remaining_health);
        self.player.repair_remaining_health -= heal;
        if let Some(health) = self.entities.entity_health.get_mut(&entity_id) {
            health.current = current + heal;
        }

        Ok(())
    }

    /// Current and maximum health of an entity, when it is damageable.
    pub fn entity_health(&self, entity_id: EntityId) -> Option<(u32, u32)> {
        let placed = self.entities.placed_entities.get(&entity_id)?;
        let max_health = self
            .world
            .prototypes
            .entity(placed.prototype_id)?
            .max_health?;
        let current = self
            .entities
            .entity_health
            .get(&entity_id)
            .map(|health| health.current)
            .unwrap_or(max_health);
        Some((current, max_health))
    }
}

/// Breaks one magazine out of the turret's ammo inventory into loose shots.
fn load_magazine(world: &WorldSim, state: &mut GunTurretState) {
    let Some((item_id, ammo)) = state.ammo.slots.iter().flatten().find_map(|stack| {
        world
            .prototypes
            .item(stack.item_id)
            .and_then(|item| item.ammo)
            .map(|ammo| (stack.item_id, ammo))
    }) else {
        return;
    };
    if state.ammo.remove(item_id, 1).is_err() {
        return;
    }
    state.loaded_shots = ammo.shots_per_item;
    state.loaded_damage = ammo.damage_per_shot;
}

/// Nearest enemy unit whose tile lies within `range` of the turret
/// footprint; ties resolve to the lowest enemy id.
fn nearest_enemy_in_range(
    enemies: &EnemySubsystem,
    enemy_chunks: &EnemyChunkIndex,
    footprint: &EntityFootprint,
    range: i64,
) -> Option<EnemyId> {
    let mut best: Option<(i64, EnemyId)> = None;
    for enemy_id in enemy_chunks.ids_in_expanded_footprint(footprint, range) {
        let Some(enemy) = enemies.enemies.get(enemy_id) else {
            continue;
        };
        let enemy_footprint = EntityFootprint::single_tile(enemy.tile().0, enemy.tile().1);
        let distance = footprint.chebyshev_distance_to(&enemy_footprint);
        if distance > range {
            continue;
        }
        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, enemy.id));
        }
    }
    best.map(|(_, id)| id)
}

fn nearest_spawner_in_range(
    enemy_spawners: &BTreeMap<EntityId, EnemySpawnerState>,
    placed_entities: &BTreeMap<EntityId, PlacedEntity>,
    occupancy: &OccupancyGrid,
    footprint: &EntityFootprint,
    range: i64,
) -> Option<EntityId> {
    let mut best: Option<(i64, EntityId)> = None;
    let min_x = footprint.x.saturating_sub(range);
    let max_x = footprint
        .x
        .saturating_add(i64::from(footprint.width) - 1)
        .saturating_add(range);
    let min_y = footprint.y.saturating_sub(range);
    let max_y = footprint
        .y
        .saturating_add(i64::from(footprint.height) - 1)
        .saturating_add(range);
    for spawner_id in occupancy.entity_ids_in_tile_rect(min_x, max_x, min_y, max_y) {
        if !enemy_spawners.contains_key(&spawner_id) {
            continue;
        }
        let Some(placed) = placed_entities.get(&spawner_id) else {
            continue;
        };
        let distance = footprint.chebyshev_distance_to(&placed.footprint);
        if distance > range {
            continue;
        }
        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, spawner_id));
        }
    }
    best.map(|(_, id)| id)
}

/// Short-lived spatial index for the turret pass. Keeping it local avoids
/// serializing derived state while turning one scan per turret into one scan
/// of the nearby generated chunks.
struct EnemyChunkIndex {
    chunks: BTreeMap<ChunkCoord, Vec<EnemyId>>,
}

impl EnemyChunkIndex {
    fn from_enemies(enemies: &EnemySubsystem) -> Self {
        let mut chunks = BTreeMap::<ChunkCoord, Vec<EnemyId>>::new();
        for enemy in enemies.enemies.values() {
            if let Some(coord) = ChunkCoord::from_tile(enemy.tile().0, enemy.tile().1) {
                chunks.entry(coord).or_default().push(enemy.id);
            }
        }
        Self { chunks }
    }

    fn ids_in_expanded_footprint(
        &self,
        footprint: &EntityFootprint,
        range: i64,
    ) -> Box<dyn Iterator<Item = &EnemyId> + '_> {
        let min_x = footprint.x.saturating_sub(range);
        let max_x = footprint
            .x
            .saturating_add(i64::from(footprint.width) - 1)
            .saturating_add(range);
        let min_y = footprint.y.saturating_sub(range);
        let max_y = footprint
            .y
            .saturating_add(i64::from(footprint.height) - 1)
            .saturating_add(range);
        let Some(min_chunk) = ChunkCoord::from_tile(min_x, min_y) else {
            return Box::new(std::iter::empty());
        };
        let Some(max_chunk) = ChunkCoord::from_tile(max_x, max_y) else {
            return Box::new(std::iter::empty());
        };

        Box::new(
            self.chunks
                .range(
                    ChunkCoord {
                        x: min_chunk.x,
                        y: i32::MIN,
                    }..=ChunkCoord {
                        x: max_chunk.x,
                        y: i32::MAX,
                    },
                )
                .filter(move |(coord, _)| coord.y >= min_chunk.y && coord.y <= max_chunk.y)
                .flat_map(|(_, enemy_ids)| enemy_ids),
        )
    }
}
