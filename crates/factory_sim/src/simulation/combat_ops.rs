use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct AccumulatedDamage {
    amount: u64,
    retaliation_target: Option<EntityId>,
}

impl Simulation {
    /// Advances every player defensive turret from one shared target snapshot.
    pub(super) fn advance_defensive_turrets(&mut self, commands: &mut CombatCommandBuffer) {
        if self.onboarding_progress.loaded_gun_turrets == 0
            && self.entities.gun_turrets.iter().any(|(entity_id, state)| {
                self.entities.placed_entities.contains_key(entity_id)
                    && (state.loaded_shots > 0
                        || state
                            .ammo
                            .slots()
                            .iter()
                            .filter_map(|slot| slot.stack())
                            .any(|stack| {
                                self.world
                                    .prototypes
                                    .item(stack.item_id())
                                    .is_some_and(|item| item.ammo.is_some())
                            }))
            })
        {
            self.onboarding_progress
                .record_counter(|progress| &mut progress.loaded_gun_turrets, 1);
        }
        // Units move during their own simulation step, not during turret
        // fire, so one index is valid for this whole pass.
        self.enemy_target_chunks.rebuild(&self.enemies);
        {
            let tick = self.tick;
            let Simulation {
                world,
                entities,
                enemies,
                enemy_target_chunks,
                ..
            } = self;
            let placed_entities = &entities.placed_entities;
            let enemy_spawners = &entities.enemy_spawners;
            let occupancy = &entities.occupancy;

            for (&turret_id, state) in entities.gun_turrets.iter_mut() {
                if tick < state.next_ready_tick {
                    continue;
                }
                let Some(placed) = placed_entities.get(&turret_id) else {
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

                let target = defensive_target_from_parts(
                    enemies,
                    enemy_target_chunks,
                    enemy_spawners,
                    placed_entities,
                    occupancy,
                    &placed.footprint,
                    turret.range_tiles,
                );

                if let Some(target) = target {
                    let attack = AttackDefinition::hitscan(
                        state.loaded_damage,
                        turret.cooldown_ticks,
                        turret.range_tiles,
                    );
                    commands.attack(
                        CombatSource {
                            owner: CombatantId::Entity(turret_id),
                            faction: Faction::Player,
                        },
                        target,
                        attack,
                    );
                    state.loaded_shots -= 1;
                    state.next_ready_tick = tick + u64::from(attack.cooldown_ticks);
                }
            }
        }

        let mut demand_changed = Vec::new();
        {
            let Simulation {
                world,
                entities,
                enemies,
                enemy_target_chunks,
                power,
                ..
            } = self;
            let placed_entities = &entities.placed_entities;
            let enemy_spawners = &entities.enemy_spawners;
            let occupancy = &entities.occupancy;
            let electric_consumers = &mut entities.electric_consumers;

            for (&turret_id, state) in entities.laser_turrets.iter_mut() {
                let Some(placed) = placed_entities.get(&turret_id) else {
                    continue;
                };
                let Some(turret) = world
                    .prototypes
                    .entity(placed.prototype_id)
                    .and_then(|prototype| prototype.laser_turret)
                else {
                    continue;
                };
                let target = defensive_target_from_parts(
                    enemies,
                    enemy_target_chunks,
                    enemy_spawners,
                    placed_entities,
                    occupancy,
                    &placed.footprint,
                    turret.range_tiles,
                );

                let Some(target) = target else {
                    if state.engaged {
                        state.engaged = false;
                        state.cooldown_remaining_ticks = 0;
                        demand_changed.push(turret_id);
                    }
                    continue;
                };
                if !state.engaged {
                    state.engaged = true;
                    state.cooldown_remaining_ticks = 0;
                    demand_changed.push(turret_id);
                    // Power for active usage was not included in this tick's
                    // accounting. Firing starts after the next accounting pass.
                    continue;
                }
                if !electric_work_allowed_for(power, electric_consumers, turret_id) {
                    continue;
                }
                if state.cooldown_remaining_ticks > 0 {
                    state.cooldown_remaining_ticks -= 1;
                    if state.cooldown_remaining_ticks > 0 {
                        continue;
                    }
                }
                commands.attack(
                    CombatSource {
                        owner: CombatantId::Entity(turret_id),
                        faction: Faction::Player,
                    },
                    target,
                    AttackDefinition::hitscan(
                        Damage::new(turret.damage, DamageType::Laser),
                        turret.cooldown_ticks,
                        turret.range_tiles,
                    ),
                );
                state.cooldown_remaining_ticks = turret.cooldown_ticks;
            }
        }
        for entity_id in demand_changed {
            self.invalidate_consumer_power_demand(entity_id);
        }
    }

    #[cfg(test)]
    pub(super) fn advance_gun_turrets(&mut self, commands: &mut CombatCommandBuffer) {
        self.advance_defensive_turrets(commands);
    }

    /// Applies every attack committed from the tick's pre-resolution combat
    /// snapshot. A combatant destroyed here still contributes its own queued
    /// attack, so neither side receives an initiative advantage.
    pub fn resolve_combat_commands(&mut self, commands: CombatCommandBuffer) {
        let mut accumulated = BTreeMap::<CombatantId, AccumulatedDamage>::new();
        for command in commands.iter().copied() {
            let Some(target_health) = self.combatant_health(command.target) else {
                continue;
            };
            if !command.source.faction.is_hostile_to(target_health.faction) {
                continue;
            }
            let amount = command.damage.after_resistance(&target_health.resistances);
            if amount == 0 {
                continue;
            }
            let target_damage = accumulated.entry(command.target).or_default();
            target_damage.amount = target_damage.amount.saturating_add(u64::from(amount));
            if let CombatantId::Entity(entity_id) = command.source.owner {
                target_damage.retaliation_target = Some(
                    target_damage
                        .retaliation_target
                        .map_or(entity_id, |current| current.min(entity_id)),
                );
            }
        }

        for (target, damage) in accumulated {
            let amount = u32::try_from(damage.amount).unwrap_or(u32::MAX);
            match target {
                CombatantId::Player => {
                    let amount = self.absorb_player_damage_with_shields(amount);
                    self.player.health.current = self.player.health.current.saturating_sub(amount);
                }
                CombatantId::Entity(entity_id) => {
                    self.apply_entity_damage(entity_id, amount);
                }
                CombatantId::Enemy(enemy_id) => {
                    let Some(enemy) = self.enemies.enemies.get_mut(&enemy_id) else {
                        continue;
                    };
                    enemy.health.current = enemy.health.current.saturating_sub(amount);
                    if enemy.health.current == 0 {
                        self.enemies.enemies.remove(&enemy_id);
                    } else if let Some(retaliation_target) = damage.retaliation_target {
                        // Being fired on pulls a surviving unit onto the
                        // lowest-ID structure that participated in the volley.
                        enemy.target = Some(retaliation_target);
                        enemy.path.clear();
                    }
                }
            }
        }
    }

    fn combatant_health(&self, combatant: CombatantId) -> Option<&HealthState> {
        match combatant {
            CombatantId::Player => Some(&self.player.health),
            CombatantId::Entity(entity_id) => self.entities.entity_health.get(&entity_id),
            CombatantId::Enemy(enemy_id) => self
                .enemies
                .enemies
                .get(&enemy_id)
                .map(|enemy| &enemy.health),
        }
    }

    pub fn combatant_health_state(&self, combatant: CombatantId) -> Option<HealthState> {
        self.combatant_health(combatant).copied()
    }

    pub fn faction_of(&self, combatant: CombatantId) -> Option<Faction> {
        self.combatant_health(combatant)
            .map(|health| health.faction)
    }

    /// Applies damage to a placed entity's health; at zero the entity is
    /// violently destroyed (no item recovery). Entities without health state
    /// are indestructible. Zero damage is a no-op. Returns true when the entity
    /// was destroyed.
    pub fn damage_entity(&mut self, entity_id: EntityId, amount: u32) -> bool {
        self.damage_entity_with(entity_id, Damage::physical(amount))
    }

    /// Applies typed damage without a faction check. Environmental damage and
    /// scripted effects use this path; attacks should use a command buffer.
    pub fn damage_entity_with(&mut self, entity_id: EntityId, damage: Damage) -> bool {
        let Some(health) = self.entities.entity_health.get(&entity_id) else {
            return false;
        };
        let amount = damage.after_resistance(&health.resistances);
        self.apply_entity_damage(entity_id, amount)
    }

    fn apply_entity_damage(&mut self, entity_id: EntityId, amount: u32) -> bool {
        // Entities without health state shrug the hit off entirely, so they
        // must not raise an under-attack alarm either.
        if amount == 0 || !self.entities.entity_health.contains_key(&entity_id) {
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
        let Some(health) = self.entities.entity_health.get_mut(&entity_id) else {
            return false;
        };
        health.current = health.current.saturating_sub(amount);
        let destroyed = health.current == 0;

        if let Some((x, y)) = warning_location {
            self.emit_structure_damage_warning(x, y);
        }

        if destroyed {
            entity_mutation::remove(self, entity_id);
        }
        destroyed
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
            .entities
            .entity_health
            .get(&entity_id)
            .map(|health| health.maximum)
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
                .slots()
                .iter()
                .filter_map(|slot| slot.stack())
                .map(|stack| stack.item_id())
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
        let health = self.entities.entity_health.get(&entity_id)?;
        Some((health.current, health.maximum))
    }

    pub fn player_health(&self) -> (u32, u32) {
        (self.player.health.current, self.player.health.maximum)
    }
}

fn defensive_target_from_parts(
    enemies: &EnemySubsystem,
    enemy_chunks: &EnemyChunkIndex,
    enemy_spawners: &BTreeMap<EntityId, EnemySpawnerState>,
    placed_entities: &BTreeMap<EntityId, PlacedEntity>,
    occupancy: &OccupancyGrid,
    footprint: &EntityFootprint,
    range_tiles: u32,
) -> Option<CombatantId> {
    let range = i64::from(range_tiles);
    nearest_enemy_in_range(enemies, enemy_chunks, footprint, range)
        .map(CombatantId::Enemy)
        .or_else(|| {
            nearest_spawner_in_range(enemy_spawners, placed_entities, occupancy, footprint, range)
                .map(CombatantId::Entity)
        })
}

/// Breaks one magazine out of the turret's ammo inventory into loose shots.
fn load_magazine(world: &WorldSim, state: &mut GunTurretState) {
    let Some((item_id, ammo)) = state
        .ammo
        .slots()
        .iter()
        .filter_map(|slot| slot.stack())
        .find_map(|stack| {
            world
                .prototypes
                .item(stack.item_id())
                .and_then(|item| item.ammo)
                .map(|ammo| (stack.item_id(), ammo))
        })
    else {
        return;
    };
    if state.ammo.remove(item_id, 1).is_err() {
        return;
    }
    state.loaded_shots = ammo.shots_per_item;
    state.loaded_damage = Damage::new(ammo.damage_per_shot, ammo.damage_type);
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
        if best.is_none_or(|current| (distance, enemy.id) < current) {
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

/// Runtime-only spatial index reused by each turret pass. It deliberately
/// compares and hashes as empty derived state so capacity does not affect
/// deterministic simulation identity.
#[derive(Clone, Debug, Default)]
pub(super) struct EnemyChunkIndex {
    chunks: BTreeMap<ChunkCoord, Vec<EnemyId>>,
}

impl EnemyChunkIndex {
    fn rebuild(&mut self, enemies: &EnemySubsystem) {
        for enemy_ids in self.chunks.values_mut() {
            enemy_ids.clear();
        }
        for enemy in enemies.enemies.values() {
            if let Some(coord) = ChunkCoord::from_tile(enemy.tile().0, enemy.tile().1) {
                self.chunks.entry(coord).or_default().push(enemy.id);
            }
        }
        self.chunks.retain(|_, enemy_ids| !enemy_ids.is_empty());
    }

    fn ids_in_expanded_footprint(
        &self,
        footprint: &EntityFootprint,
        range: i64,
    ) -> impl Iterator<Item = &EnemyId> {
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
        ChunkCoord::from_tile(min_x, min_y)
            .zip(ChunkCoord::from_tile(max_x, max_y))
            .into_iter()
            .flat_map(move |(min_chunk, max_chunk)| {
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
                    .flat_map(|(_, enemy_ids)| enemy_ids)
            })
    }
}

impl_runtime_only_identity!(EnemyChunkIndex);
