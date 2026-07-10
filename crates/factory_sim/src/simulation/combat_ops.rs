use super::enemy_ops::chebyshev_distance_to_footprint;
use super::*;
use std::collections::BTreeMap;

impl Simulation {
    /// Gun turrets acquire the nearest enemy unit in range (falling back to
    /// enemy spawners) and fire, consuming magazine shots. Spawner damage is
    /// applied after the turret loop so destruction never happens while
    /// entity state is borrowed.
    pub(super) fn advance_gun_turrets(&mut self) {
        let mut structure_damage: Vec<(EntityId, u32)> = Vec::new();
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
                let fired = if let Some(enemy_id) =
                    nearest_enemy_in_range(enemies, &enemy_chunks, &footprint, range)
                {
                    let enemy = enemies
                        .enemies
                        .get_mut(&enemy_id)
                        .expect("selected enemy id should exist");
                    let damage = state.loaded_damage;
                    if enemy.health <= damage {
                        enemies.enemies.remove(&enemy_id);
                    } else {
                        enemy.health -= damage;
                        // Retaliate: the shot pulls the unit onto the turret.
                        enemy.target = Some(turret_id);
                        enemy.path.clear();
                    }
                    true
                } else if let Some(spawner_id) = nearest_spawner_in_range(
                    &entities.enemy_spawners,
                    &entities.placed_entities,
                    &entities.occupancy,
                    &footprint,
                    range,
                ) {
                    structure_damage.push((spawner_id, state.loaded_damage));
                    true
                } else {
                    false
                };

                if fired {
                    state.loaded_shots -= 1;
                    state.next_ready_tick = tick + u64::from(turret.cooldown_ticks);
                }
            }
        }

        for (entity_id, damage) in structure_damage {
            self.damage_entity(entity_id, damage);
        }
    }

    /// Applies damage to a placed entity's health; at zero the entity is
    /// violently destroyed (no item recovery). Entities without health state
    /// are indestructible. Returns true when the entity was destroyed.
    pub(crate) fn damage_entity(&mut self, entity_id: EntityId, amount: u32) -> bool {
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
        let reach = REPAIR_REACH_TILES as i64;
        if chebyshev_distance_to_footprint((player_tile_x, player_tile_y), &footprint) > reach {
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
            // A preceding turret may have killed this indexed enemy.
            continue;
        };
        let distance = chebyshev_distance_to_footprint(enemy.tile(), footprint);
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
        let (center_x, center_y) = (
            placed.footprint.x + i64::from(placed.footprint.width) / 2,
            placed.footprint.y + i64::from(placed.footprint.height) / 2,
        );
        let distance = chebyshev_distance_to_footprint((center_x, center_y), footprint);
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
