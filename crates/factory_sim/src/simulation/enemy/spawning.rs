use super::*;

const SPAWN_SEARCH_RINGS: i64 = 3;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SpawnError {
    SpawnerNotFound,
    NoFreeTile,
}

impl Simulation {
    /// Spawner behavior: drain pollution from the local chunk, convert it
    /// into attacking units, and keep a small idle guard detail alive.
    pub(in crate::simulation) fn advance_enemy_spawners(&mut self) {
        struct SpawnRequest {
            spawner_id: EntityId,
            unit: UnitPrototype,
            mission: EnemyMission,
            attack_budget_cost_micro: u64,
        }

        let mut alive_by_spawner = BTreeMap::<EntityId, u32>::new();
        let mut guards_by_spawner = BTreeMap::<EntityId, u32>::new();
        for enemy in self.enemies.enemies.values() {
            if let Some(spawner) = enemy.home_spawner {
                *alive_by_spawner.entry(spawner).or_default() += 1;
                if enemy.mode == EnemyMode::Guard {
                    *guards_by_spawner.entry(spawner).or_default() += 1;
                }
            }
        }
        let mut projected_alive_by_spawner = alive_by_spawner.clone();

        self.advance_evolution_time();
        let mut requests: Vec<SpawnRequest> = Vec::new();
        let mut absorbed_by_base = BTreeMap::<EnemyBaseId, u64>::new();
        let mut attack_budget_overflows = 0_u64;
        let tick = self.tick;
        let raid_size = u64::from(self.raid_target_size());
        let Simulation {
            world,
            entities,
            pollution,
            ..
        } = self;
        for (&spawner_id, state) in entities.enemy_spawners.iter_mut() {
            let Some(placed) = entities.placed_entities.get(&spawner_id) else {
                continue;
            };
            let Some(config) = world
                .prototypes
                .entity(placed.prototype_id)
                .and_then(|prototype| prototype.enemy_spawner.as_ref())
            else {
                continue;
            };
            let Some(coord) = ChunkCoord::from_tile(placed.x, placed.y) else {
                continue;
            };

            let Some(base_id) = self.enemies.spawner_bases.get(&spawner_id).copied() else {
                continue;
            };
            let cap = u64::from(config.unit_spawn_pollution_cost_milli) * 1000 * raid_size * 10;
            // Count what sibling spawners of this base already absorbed this
            // pass, so a multi-spawner base cannot overshoot its budget cap.
            let existing_budget = self
                .enemies
                .bases
                .get(&base_id)
                .map_or(0, |base| base.attack_budget_micro);
            let (budget, overflowed) = saturating_add_with_overflow(
                existing_budget,
                absorbed_by_base.get(&base_id).copied().unwrap_or(0),
            );
            attack_budget_overflows = attack_budget_overflows.saturating_add(u64::from(overflowed));
            let absorption = u64::from(config.pollution_absorption_per_tick_milli)
                * 1000
                * u64::from(self.config.runtime.pollution_sensitivity_percent)
                / 100;
            let absorbed = if budget < cap {
                pollution.remove_micro(coord, absorption.min(cap - budget))
            } else {
                0
            };
            let absorbed_for_base = absorbed_by_base.entry(base_id).or_default();
            let (sum, overflowed) = saturating_add_with_overflow(*absorbed_for_base, absorbed);
            *absorbed_for_base = sum;
            attack_budget_overflows = attack_budget_overflows.saturating_add(u64::from(overflowed));

            let projected_alive = projected_alive_by_spawner.entry(spawner_id).or_default();
            let guards = guards_by_spawner.get(&spawner_id).copied().unwrap_or(0);
            if tick >= state.next_free_spawn_tick {
                state.next_free_spawn_tick = tick + u64::from(config.free_spawn_interval_ticks);
                if guards < config.guard_units && *projected_alive < config.max_alive_units {
                    requests.push(SpawnRequest {
                        spawner_id,
                        unit: config.unit,
                        mission: EnemyMission::Guard,
                        attack_budget_cost_micro: 0,
                    });
                    *projected_alive += 1;
                }
            }
        }

        let pollution_changed = absorbed_by_base.values().any(|absorbed| *absorbed != 0);
        if pollution_changed {
            self.pollution_map_revision = self.pollution_map_revision.wrapping_add(1);
            self.enemy_map_revision = self.enemy_map_revision.wrapping_add(1);
        }
        for (base_id, absorbed) in absorbed_by_base {
            if absorbed == 0 {
                continue;
            }
            let became_active = self
                .enemies
                .bases
                .get(&base_id)
                .is_some_and(|base| !base.pollution_contact);
            if let Some(base) = self.enemies.bases.get_mut(&base_id) {
                let (budget, overflowed) =
                    saturating_add_with_overflow(base.attack_budget_micro, absorbed);
                base.attack_budget_micro = budget;
                base.pollution_contact = true;
                attack_budget_overflows =
                    attack_budget_overflows.saturating_add(u64::from(overflowed));
            }
            self.add_pollution_evolution(absorbed);
            if became_active {
                self.emit_base_event(base_id, ThreatEventKind::PollutionContact);
            }
        }
        self.capacity_overflows.attack_budget_additions = self
            .capacity_overflows
            .attack_budget_additions
            .saturating_add(attack_budget_overflows);

        if self.config.runtime.proactive_raids {
            let base_ids: Vec<_> = self.enemies.bases.keys().copied().collect();
            let target_size = usize::from(self.raid_target_size());
            for base_id in base_ids {
                let Some((&spawner_id, unit, cost, can_spawn)) = self
                    .enemies
                    .bases
                    .get(&base_id)
                    .and_then(|base| base.spawners.iter().next())
                    .and_then(|spawner_id| {
                        let placed = self.entities.placed_entities.get(spawner_id)?;
                        let cfg = self
                            .world
                            .prototypes
                            .entity(placed.prototype_id)?
                            .enemy_spawner
                            .as_ref()?;
                        Some((
                            spawner_id,
                            cfg.unit,
                            u64::from(cfg.unit_spawn_pollution_cost_milli) * 1000,
                            projected_alive_by_spawner
                                .get(spawner_id)
                                .copied()
                                .unwrap_or(0)
                                < cfg.max_alive_units,
                        ))
                    })
                else {
                    continue;
                };
                let base = self
                    .enemies
                    .bases
                    .get(&base_id)
                    .expect("listed base must exist");
                if cost > 0
                    && base.attack_budget_micro >= cost
                    && base.staged_units.len() < target_size
                    && can_spawn
                {
                    requests.push(SpawnRequest {
                        spawner_id,
                        unit,
                        mission: EnemyMission::Staging(base_id),
                        attack_budget_cost_micro: cost,
                    });
                    *projected_alive_by_spawner.entry(spawner_id).or_default() += 1;
                }
            }
        }

        for request in requests {
            if self
                .spawn_enemy_near_spawner(request.spawner_id, &request.unit, request.mission)
                .is_ok()
                && request.attack_budget_cost_micro > 0
            {
                let EnemyMission::Staging(base_id) = request.mission else {
                    unreachable!("only staged enemies consume attack budget");
                };
                let base = self
                    .enemies
                    .bases
                    .get_mut(&base_id)
                    .expect("a successful staged spawn must retain its base");
                base.attack_budget_micro -= request.attack_budget_cost_micro;
            }
        }
        self.cleanup_enemy_groups();
        self.launch_ready_raids();
        self.advance_expansions_and_growth();
    }

    pub(super) fn spawn_enemy_near_spawner(
        &mut self,
        spawner_id: EntityId,
        unit: &UnitPrototype,
        mission: EnemyMission,
    ) -> Result<EnemyId, SpawnError> {
        let Some(placed) = self.entities.placed_entities.get(&spawner_id) else {
            return Err(SpawnError::SpawnerNotFound);
        };
        let footprint = placed.footprint;
        let Some((tile_x, tile_y)) = free_tile_around_footprint(
            &self.world,
            &self.entities.occupancy,
            &footprint,
            SPAWN_SEARCH_RINGS,
        ) else {
            return Err(SpawnError::NoFreeTile);
        };

        let id = self.enemies.allocate_id();
        let stagger = id.raw() % 16;
        let strength = u32::from(self.config.runtime.strength_percent);
        let evolution = 100 + u32::from(self.enemies.evolution_points) / 100;
        let mode = if mission == EnemyMission::Guard {
            EnemyMode::Guard
        } else {
            EnemyMode::Attack
        };
        self.enemies.enemies.insert(
            id,
            Enemy {
                id,
                x: tile_center_fixed(tile_x),
                y: tile_center_fixed(tile_y),
                health: HealthState::new(
                    scale_stat(unit.max_health, strength, evolution),
                    Faction::Enemy,
                ),
                attack: AttackDefinition::melee(
                    Damage::physical(scale_stat(unit.damage, strength, evolution)),
                    unit.attack_cooldown_ticks,
                    1,
                ),
                speed_fixed_per_tick: unit.speed_fixed_per_tick,
                aggro_radius_tiles: unit.aggro_radius_tiles,
                mode,
                mission,
                home_spawner: Some(spawner_id),
                target: None,
                path: VecDeque::new(),
                next_attack_tick: 0,
                next_decision_tick: self.tick + stagger,
            },
        );
        if let EnemyMission::Staging(base_id) = mission
            && let Some(base) = self.enemies.bases.get_mut(&base_id)
        {
            base.staged_units.insert(id);
            if base.staging_started_tick.is_none() {
                base.staging_started_tick = Some(self.tick);
                self.emit_base_event(base_id, ThreatEventKind::RaidPreparing);
            }
        }
        Ok(id)
    }

    /// Unit AI: validate or acquire a target, path toward it, and commit an
    /// attack against whatever stands adjacent. Damage remains pending until
    /// turret attacks have been collected from the same combat snapshot.
    pub(in crate::simulation) fn on_enemy_spawner_placed(
        &mut self,
        entity_id: EntityId,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) {
        let Some(anchor) = ChunkCoord::from_tile(x, y) else {
            return;
        };
        let base_id = if let Some(id) = self.enemies.placement_base {
            id
        } else {
            let id = self.enemies.allocate_base_id();
            let gameplay = self.gameplay().copied();
            self.enemies.bases.insert(
                id,
                EnemyBase {
                    id,
                    anchor,
                    spawners: BTreeSet::new(),
                    creation_tick: self.tick,
                    attack_budget_micro: 0,
                    staged_units: BTreeSet::new(),
                    staging_started_tick: None,
                    next_raid_tick: gameplay.map_or(u64::MAX, |cfg| {
                        next_scaled_tick(
                            self.tick,
                            cfg.raid_cooldown_ticks,
                            self.config.runtime.raid_frequency_percent,
                        )
                    }),
                    next_expansion_tick: gameplay.map_or(u64::MAX, |cfg| {
                        next_scaled_tick(
                            self.tick,
                            cfg.expansion_interval_ticks,
                            self.config.runtime.expansion_frequency_percent,
                        )
                    }),
                    next_growth_tick: gameplay.map_or(u64::MAX, |cfg| {
                        self.tick + u64::from(cfg.outpost_growth_interval_ticks)
                    }),
                    pollution_contact: false,
                },
            );
            id
        };
        if let Some(base) = self.enemies.bases.get_mut(&base_id) {
            base.spawners.insert(entity_id);
        }
        self.enemies.spawner_bases.insert(entity_id, base_id);
    }

    pub(in crate::simulation) fn on_enemy_spawner_removed(
        &mut self,
        entity_id: EntityId,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) {
        let Some(base_id) = self.enemies.spawner_bases.remove(&entity_id) else {
            return;
        };
        if let Some(destroyed) = self
            .gameplay()
            .map(|cfg| cfg.evolution_spawner_destroyed_points)
        {
            self.add_evolution_points(u32::from(destroyed));
        }
        let empty = self.enemies.bases.get_mut(&base_id).is_some_and(|base| {
            base.spawners.remove(&entity_id);
            base.spawners.is_empty()
        });
        if empty {
            let staged = self
                .enemies
                .bases
                .get_mut(&base_id)
                .map(|base| std::mem::take(&mut base.staged_units))
                .unwrap_or_default();
            if self.config.runtime.proactive_raids && !staged.is_empty() {
                let raid_id = self.enemies.allocate_raid_id();
                for id in &staged {
                    if let Some(unit) = self.enemies.enemies.get_mut(id) {
                        unit.mission = EnemyMission::Raid(raid_id);
                        unit.mode = EnemyMode::Attack;
                    }
                }
                self.enemies.raids.insert(
                    raid_id,
                    Raid {
                        id: raid_id,
                        base_id,
                        members: staged,
                        target: None,
                        launched_tick: self.tick,
                    },
                );
                self.emit_base_event(base_id, ThreatEventKind::RaidLaunched);
            } else {
                for id in staged {
                    if let Some(unit) = self.enemies.enemies.get_mut(&id) {
                        unit.mission = EnemyMission::Guard;
                        unit.mode = EnemyMode::Guard;
                        unit.target = None;
                        unit.path.clear();
                    }
                }
            }
            if let Some(bonus) = self
                .gameplay()
                .map(|cfg| cfg.evolution_colony_destroyed_points)
            {
                self.add_evolution_points(u32::from(bonus));
            }
            self.enemies.bases.remove(&base_id);
            self.emit_event(
                ThreatEventKind::BaseDestroyed,
                ThreatLocation::Exact { x, y },
            );
        }
        for unit in self
            .enemies
            .enemies
            .values_mut()
            .filter(|unit| unit.home_spawner == Some(entity_id))
        {
            unit.home_spawner = None;
            if matches!(unit.mission, EnemyMission::Guard) {
                unit.target = None;
            }
        }
    }
}

fn scale_stat(base: u32, strength_percent: u32, evolution_percent: u32) -> u32 {
    u32::try_from(
        (u64::from(base) * u64::from(strength_percent) * u64::from(evolution_percent) / 10_000)
            .max(1),
    )
    .unwrap_or(u32::MAX)
}
/// First free walkable tile in expanding rings around a footprint,
/// deterministic scan order.
fn free_tile_around_footprint(
    world: &WorldSim,
    occupancy: &OccupancyGrid,
    footprint: &EntityFootprint,
    max_rings: i64,
) -> Option<(WorldTileCoord, WorldTileCoord)> {
    for ring in 1..=max_rings {
        let min_x = footprint.x - ring;
        let max_x = footprint.x + i64::from(footprint.width) - 1 + ring;
        let min_y = footprint.y - ring;
        let max_y = footprint.y + i64::from(footprint.height) - 1 + ring;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let on_ring = x == min_x || x == max_x || y == min_y || y == max_y;
                if !on_ring {
                    continue;
                }
                let walkable = world
                    .tile_at(x, y)
                    .is_some_and(|tile| tile.collision.walkable);
                if walkable && occupancy.entity_at(x, y).is_none() {
                    return Some((x, y));
                }
            }
        }
    }
    None
}
