use super::*;
use crate::enemies::{EnemyBase, Expansion, Raid};
use factory_data::{EnemyBaseGenerationConfig, EnemyGameplayConfig, UnitPrototype};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Ticks between target rescans for units without a target.
const ENEMY_TARGET_RESCAN_TICKS: u64 = 120;
/// Ticks between path recomputations while a target is set.
const ENEMY_REPATH_INTERVAL_TICKS: u64 = 90;
/// Ticks between wander moves for idle guards.
const ENEMY_WANDER_INTERVAL_TICKS: u64 = 300;
/// Upper bound on A* node expansions per path request.
const ENEMY_PATHFIND_MAX_EXPANSIONS: usize = 600;
/// Targets farther than this (in tiles, Chebyshev) are approached greedily
/// instead of path-searched.
const ENEMY_PATHFIND_MAX_RANGE_TILES: i64 = 40;
/// How far around its footprint a spawner looks for a free tile to place a
/// freshly spawned unit.
const SPAWN_SEARCH_RINGS: i64 = 3;
/// Salt mixed into the world seed for spawner placement rolls so they are
/// independent of terrain and resource noise.
const SPAWNER_PLACEMENT_SALT: u64 = 0x656e_656d_795f_6261;

impl Simulation {
    pub fn enemies(&self) -> &EnemySubsystem {
        &self.enemies
    }

    pub fn enemy_settings(&self) -> SimulationConfig {
        self.config
    }

    pub fn threat_events_after(&self, sequence: u64) -> Vec<ThreatEvent> {
        self.enemies
            .threat_events
            .iter()
            .copied()
            .filter(|event| event.sequence > sequence)
            .collect()
    }

    pub fn threat_snapshot(&self) -> ThreatSnapshot {
        let active = self
            .enemies
            .bases
            .values()
            .filter(|base| base.pollution_contact)
            .count();
        let staged = self
            .enemies
            .bases
            .values()
            .map(|base| base.staged_units.len())
            .sum();
        let inbound = self.enemies.raids.len();
        let spotted = self
            .enemies
            .expansions
            .values()
            .filter(|party| party.spotted)
            .count();
        let half_staged = self.enemies.bases.values().any(|base| {
            base.staged_units.len() >= usize::from(self.raid_target_size().div_ceil(2))
        });
        let damaged_recently = self.enemies.threat_events.iter().rev().any(|event| {
            event.kind == ThreatEventKind::StructureUnderAttack
                && self.tick.saturating_sub(event.tick) <= 600
        });
        let tier = if damaged_recently || inbound > 1 {
            ThreatTier::Critical
        } else if inbound > 0 || half_staged {
            ThreatTier::High
        } else if active > 0 {
            ThreatTier::Elevated
        } else {
            ThreatTier::Low
        };
        let maximum_launch_countdown_ticks = self
            .enemies
            .bases
            .values()
            .filter_map(|base| {
                base.staging_started_tick.map(|start| {
                    self.gameplay().map_or(0, |cfg| {
                        u64::from(cfg.raid_staging_timeout_ticks)
                            .saturating_sub(self.tick.saturating_sub(start))
                    })
                })
            })
            .max()
            .unwrap_or(0);
        ThreatSnapshot {
            tier,
            evolution_percent: (self.enemies.evolution_points / 100) as u8,
            total_pollution_micro: self.pollution.total_micro(),
            pollution_active_colonies: active,
            staged_units: staged,
            maximum_launch_countdown_ticks,
            inbound_raids: inbound,
            spotted_expansions: spotted,
        }
    }

    pub fn enemy_map_snapshot(&self) -> EnemyMapSnapshot {
        let mut snapshot = EnemyMapSnapshot::default();
        for base in self.enemies.bases.values() {
            if base.pollution_contact {
                snapshot.contacted_sectors.push(base.anchor);
            }
            if self.chart.revealed_chunks.contains(&base.anchor)
                && let Some(spawner) = base
                    .spawners
                    .iter()
                    .next()
                    .and_then(|id| self.entities.placed_entities.get(id))
            {
                snapshot.known_bases.push((base.id, spawner.x, spawner.y));
            }
        }
        for raid in self.enemies.raids.values() {
            let location = raid
                .members
                .iter()
                .next()
                .and_then(|id| self.enemies.enemies.get(id))
                .map(|unit| ThreatLocation::Exact {
                    x: unit.tile().0,
                    y: unit.tile().1,
                })
                .unwrap_or(ThreatLocation::Sector(
                    self.enemies.bases[&raid.base_id].anchor,
                ));
            snapshot.raids.push((raid.id, location));
            if let Some(target) = raid.target {
                snapshot.raid_targets.push((raid.id, target));
            }
        }
        for party in self
            .enemies
            .expansions
            .values()
            .filter(|party| party.spotted)
        {
            snapshot.expansions.push((
                party.id,
                ThreatLocation::Exact {
                    x: party.destination.0,
                    y: party.destination.1,
                },
            ));
        }
        snapshot
    }

    fn gameplay(&self) -> Option<&EnemyGameplayConfig> {
        self.world.prototypes.enemy_gameplay.as_ref()
    }

    pub(super) fn set_enemy_runtime_settings(&mut self, settings: EnemyRuntimeSettings) {
        let mut candidate = self.config;
        candidate.runtime = settings;
        candidate.preset = EnemyDifficultyPreset::Custom;
        if !candidate.is_valid() {
            return;
        }
        let old = self.config.runtime;
        self.config = candidate;
        let Some(gameplay) = self.gameplay().copied() else {
            return;
        };
        for base in self.enemies.bases.values_mut() {
            base.next_raid_tick = self.tick
                + scaled_interval(
                    gameplay.raid_cooldown_ticks,
                    settings.raid_frequency_percent,
                );
            base.next_expansion_tick = if settings.expansion {
                self.tick
                    + scaled_interval(
                        gameplay.expansion_interval_ticks,
                        settings.expansion_frequency_percent,
                    )
            } else {
                u64::MAX
            };
            if old.proactive_raids && !settings.proactive_raids {
                base.attack_budget_micro = 0;
                for id in std::mem::take(&mut base.staged_units) {
                    if let Some(unit) = self.enemies.enemies.get_mut(&id) {
                        unit.mode = EnemyMode::Guard;
                        unit.mission = EnemyMission::Guard;
                    }
                }
                base.staging_started_tick = None;
            }
        }
    }

    /// Rolls spawner placement for every generated chunk that has not been
    /// seeded yet. Runs after any chunk generation opportunity; placement is
    /// a pure function of the world seed and chunk coordinate.
    pub(super) fn seed_enemy_spawners_in_new_chunks(&mut self) {
        let Some(config) = self.world.prototypes.world_generation.enemy_bases else {
            return;
        };
        // Seeded chunks only ever come from generated chunks, so equal sizes
        // mean there is nothing new.
        if self.enemies.seeded_chunks.len() == self.world.chunks.len() {
            return;
        }

        let new_chunks: Vec<ChunkCoord> = self
            .world
            .chunks
            .keys()
            .filter(|coord| !self.enemies.seeded_chunks.contains(coord))
            .copied()
            .collect();
        for coord in new_chunks {
            self.enemies.seeded_chunks.insert(coord);
            self.try_place_spawner_in_chunk(coord, &config);
        }
    }

    fn try_place_spawner_in_chunk(
        &mut self,
        coord: ChunkCoord,
        config: &EnemyBaseGenerationConfig,
    ) {
        let Some(gameplay) = self.gameplay().copied() else {
            return;
        };
        let (min_x, min_y) = coord.min_tile();
        let center_x = min_x + i64::from(CHUNK_SIZE) / 2;
        let center_y = min_y + i64::from(CHUNK_SIZE) / 2;
        let min_distance = i64::from(self.config.world.starting_safe_radius_tiles);
        let center_distance_squared = i128::from(center_x) * i128::from(center_x)
            + i128::from(center_y) * i128::from(center_y);
        let min_distance_squared = i128::from(min_distance) * i128::from(min_distance);
        if center_distance_squared < min_distance_squared {
            return;
        }

        let roll = splitmix64(
            self.world.seed ^ SPAWNER_PLACEMENT_SALT ^ hash_world(self.world.seed, min_x, min_y),
        );
        let density_chance =
            u64::from(config.frequency_percent) * u64::from(self.config.world.base_density_percent);
        if roll % 10_000 >= density_chance {
            return;
        }

        let Some(prototype) = self.world.prototypes.entity(config.spawner_entity) else {
            return;
        };
        let prototype_size = prototype.size;
        // Keep the footprint fully inside the chunk so seeding one chunk
        // never depends on whether its neighbors exist yet.
        let margin = 2;
        let span_x = i64::from(CHUNK_SIZE) - 2 * margin - i64::from(prototype_size.x);
        let span_y = i64::from(CHUNK_SIZE) - 2 * margin - i64::from(prototype_size.y);
        if span_x <= 0 || span_y <= 0 {
            return;
        }
        let anchor_x = min_x + margin + ((roll >> 8) % span_x as u64) as i64;
        let anchor_y = min_y + margin + ((roll >> 24) % span_y as u64) as i64;
        let id = self.enemies.allocate_base_id();
        let count_range =
            gameplay.generated_colony_max_spawners - gameplay.generated_colony_min_spawners + 1;
        let count =
            gameplay.generated_colony_min_spawners + ((roll >> 40) % u64::from(count_range)) as u8;
        self.enemies.bases.insert(
            id,
            EnemyBase {
                id,
                anchor: coord,
                spawners: BTreeSet::new(),
                creation_tick: self.tick,
                attack_budget_micro: 0,
                staged_units: BTreeSet::new(),
                staging_started_tick: None,
                next_raid_tick: self.tick,
                next_expansion_tick: self.tick
                    + scaled_interval(
                        gameplay.expansion_interval_ticks,
                        self.config.runtime.expansion_frequency_percent,
                    ),
                next_growth_tick: self.tick + u64::from(gameplay.outpost_growth_interval_ticks),
                pollution_contact: false,
            },
        );
        for index in 0..count {
            let site_roll = splitmix64(roll ^ u64::from(index).wrapping_mul(0x9e37_79b9));
            let radius = i64::from(gameplay.colony_spawner_radius_tiles);
            let dx = (site_roll % (radius as u64 * 2 + 1)) as i64 - radius;
            let dy = ((site_roll >> 16) % (radius as u64 * 2 + 1)) as i64 - radius;
            let x = (anchor_x + dx).clamp(
                min_x + margin,
                min_x + i64::from(CHUNK_SIZE) - margin - i64::from(prototype_size.x),
            );
            let y = (anchor_y + dy).clamp(
                min_y + margin,
                min_y + i64::from(CHUNK_SIZE) - margin - i64::from(prototype_size.y),
            );
            self.enemies.placement_base = Some(id);
            let _ = placement::place(
                self,
                placement::EntityPlacementRequest {
                    prototype_id: config.spawner_entity,
                    x,
                    y,
                    direction: Direction::North,
                },
            );
            self.enemies.placement_base = None;
        }
        if self
            .enemies
            .bases
            .get(&id)
            .is_some_and(|base| base.spawners.is_empty())
        {
            self.enemies.bases.remove(&id);
        }
    }

    /// Spawner behavior: drain pollution from the local chunk, convert it
    /// into attacking units, and keep a small idle guard detail alive.
    pub(super) fn advance_enemy_spawners(&mut self) {
        struct SpawnRequest {
            spawner_id: EntityId,
            unit: UnitPrototype,
            mission: EnemyMission,
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

        self.advance_evolution_time();
        let mut requests: Vec<SpawnRequest> = Vec::new();
        let mut absorbed_by_base = BTreeMap::<EnemyBaseId, u64>::new();
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
            let budget = self
                .enemies
                .bases
                .get(&base_id)
                .map_or(0, |base| base.attack_budget_micro);
            let absorption = u64::from(config.pollution_absorption_per_tick_milli)
                * 1000
                * u64::from(self.config.runtime.pollution_sensitivity_percent)
                / 100;
            let absorbed = if budget < cap {
                pollution.remove_micro(coord, absorption.min(cap - budget))
            } else {
                0
            };
            *absorbed_by_base.entry(base_id).or_default() += absorbed;
            state.absorbed_pollution_micro = 0;

            let alive = alive_by_spawner.get(&spawner_id).copied().unwrap_or(0);
            let guards = guards_by_spawner.get(&spawner_id).copied().unwrap_or(0);
            if tick >= state.next_free_spawn_tick {
                state.next_free_spawn_tick = tick + u64::from(config.free_spawn_interval_ticks);
                if guards < config.guard_units && alive < config.max_alive_units {
                    requests.push(SpawnRequest {
                        spawner_id,
                        unit: config.unit,
                        mission: EnemyMission::Guard,
                    });
                }
            }
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
                base.attack_budget_micro = base.attack_budget_micro.saturating_add(absorbed);
                base.pollution_contact = true;
            }
            self.add_pollution_evolution(absorbed);
            if became_active {
                self.emit_base_event(base_id, ThreatEventKind::PollutionContact);
            }
        }

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
                            alive_by_spawner.get(spawner_id).copied().unwrap_or(0)
                                < cfg.max_alive_units,
                        ))
                    })
                else {
                    continue;
                };
                let base = self
                    .enemies
                    .bases
                    .get_mut(&base_id)
                    .expect("listed base must exist");
                if cost > 0
                    && base.attack_budget_micro >= cost
                    && base.staged_units.len() < target_size
                    && can_spawn
                {
                    base.attack_budget_micro -= cost;
                    requests.push(SpawnRequest {
                        spawner_id,
                        unit,
                        mission: EnemyMission::Staging(base_id),
                    });
                }
            }
        }

        for request in requests {
            self.spawn_enemy_near_spawner(request.spawner_id, &request.unit, request.mission);
        }
        self.launch_ready_raids();
        self.advance_expansions_and_growth();
        self.cleanup_enemy_groups();
    }

    fn spawn_enemy_near_spawner(
        &mut self,
        spawner_id: EntityId,
        unit: &UnitPrototype,
        mission: EnemyMission,
    ) {
        let Some(placed) = self.entities.placed_entities.get(&spawner_id) else {
            return;
        };
        let footprint = placed.footprint;
        let Some((tile_x, tile_y)) = free_tile_around_footprint(
            &self.world,
            &self.entities.occupancy,
            &footprint,
            SPAWN_SEARCH_RINGS,
        ) else {
            return;
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
                health: scale_stat(unit.max_health, strength, evolution),
                max_health: scale_stat(unit.max_health, strength, evolution),
                damage: scale_stat(unit.damage, strength, evolution),
                attack_cooldown_ticks: unit.attack_cooldown_ticks,
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
    }

    /// Unit AI: validate or acquire a target, path toward it, and attack
    /// whatever stands adjacent. Damage is collected first and applied after
    /// the loop so unit order cannot observe half-applied destruction.
    pub(super) fn advance_enemies(&mut self) {
        for raid in self.enemies.raids.values_mut() {
            if raid
                .target
                .is_some_and(|target| !self.entities.placed_entities.contains_key(&target))
            {
                raid.target = None;
            }
            if raid.target.is_none() {
                raid.target = raid
                    .members
                    .iter()
                    .filter_map(|id| self.enemies.enemies.get(id))
                    .find_map(|unit| acquire_target(&self.world, &self.entities, unit));
            }
            for member in &raid.members {
                if let Some(unit) = self.enemies.enemies.get_mut(member) {
                    unit.target = raid.target;
                }
            }
        }
        for party in self.enemies.expansions.values() {
            for member in &party.members {
                if let Some(unit) = self.enemies.enemies.get_mut(member)
                    && unit.path.is_empty()
                {
                    unit.path.push_back(party.destination);
                }
            }
        }
        let newly_spotted: Vec<_> = self
            .enemies
            .expansions
            .iter()
            .filter_map(|(&id, party)| {
                (!party.spotted
                    && party.members.iter().any(|member| {
                        self.enemies.enemies.get(member).is_some_and(|unit| {
                            ChunkCoord::from_tile(unit.tile().0, unit.tile().1)
                                .is_some_and(|chunk| self.chart.revealed_chunks.contains(&chunk))
                        })
                    }))
                .then_some(id)
            })
            .collect();
        for id in newly_spotted {
            let destination = self.enemies.expansions.get_mut(&id).map(|party| {
                party.spotted = true;
                party.destination
            });
            if let Some((x, y)) = destination {
                self.emit_event(
                    ThreatEventKind::ExpansionSpotted,
                    ThreatLocation::Exact { x, y },
                );
            }
        }
        let mut attacks: Vec<(EntityId, u32)> = Vec::new();
        {
            let Simulation {
                world,
                entities,
                enemies,
                tick,
                ..
            } = self;
            let tick = *tick;
            let seed = world.seed;

            for enemy in enemies.enemies.values_mut() {
                step_enemy(world, entities, seed, tick, enemy, &mut attacks);
            }
        }

        for (entity_id, damage) in attacks {
            self.damage_entity(entity_id, damage);
        }
        self.resolve_arrived_expansions();
    }

    pub(super) fn on_enemy_spawner_placed(
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
                    next_raid_tick: self.tick,
                    next_expansion_tick: gameplay.map_or(u64::MAX, |cfg| {
                        self.tick
                            + scaled_interval(
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

    pub(super) fn on_enemy_spawner_removed(
        &mut self,
        entity_id: EntityId,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) {
        let Some(base_id) = self.enemies.spawner_bases.remove(&entity_id) else {
            return;
        };
        let destroyed = self
            .gameplay()
            .map_or(100, |cfg| cfg.evolution_spawner_destroyed_points);
        self.add_evolution_points(u32::from(destroyed));
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
                    }
                }
            }
            let bonus = self
                .gameplay()
                .map_or(200, |cfg| cfg.evolution_colony_destroyed_points);
            self.add_evolution_points(u32::from(bonus));
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

    pub(super) fn emit_structure_damage_warning(&mut self, x: WorldTileCoord, y: WorldTileCoord) {
        let Some(chunk) = ChunkCoord::from_tile(x, y) else {
            return;
        };
        if self
            .enemies
            .structure_warning_ticks
            .get(&chunk)
            .is_some_and(|tick| self.tick.saturating_sub(*tick) < 600)
        {
            return;
        }
        self.enemies
            .structure_warning_ticks
            .insert(chunk, self.tick);
        self.emit_event(
            ThreatEventKind::StructureUnderAttack,
            ThreatLocation::Exact { x, y },
        );
    }

    fn emit_base_event(&mut self, base_id: EnemyBaseId, kind: ThreatEventKind) {
        let Some(base) = self.enemies.bases.get(&base_id) else {
            return;
        };
        let location = if self.chart.revealed_chunks.contains(&base.anchor) {
            base.spawners
                .iter()
                .next()
                .and_then(|id| self.entities.placed_entities.get(id))
                .map_or(ThreatLocation::Sector(base.anchor), |entity| {
                    ThreatLocation::Exact {
                        x: entity.x,
                        y: entity.y,
                    }
                })
        } else {
            ThreatLocation::Sector(base.anchor)
        };
        self.emit_event(kind, location);
    }

    fn emit_event(&mut self, kind: ThreatEventKind, location: ThreatLocation) {
        self.enemies.threat_sequence = self.enemies.threat_sequence.saturating_add(1);
        self.enemies.threat_events.push_back(ThreatEvent {
            sequence: self.enemies.threat_sequence,
            tick: self.tick,
            kind,
            location,
        });
        while self.enemies.threat_events.len() > 256 {
            self.enemies.threat_events.pop_front();
        }
    }

    fn advance_evolution_time(&mut self) {
        let Some(cfg) = self.gameplay().copied() else {
            return;
        };
        if self
            .tick
            .is_multiple_of(u64::from(cfg.evolution_time_interval_ticks))
        {
            self.add_evolution_points(u32::from(cfg.evolution_time_points));
        }
    }

    fn add_pollution_evolution(&mut self, absorbed_micro: u64) {
        let Some(cfg) = self.gameplay().copied() else {
            return;
        };
        let per_point = u64::from(cfg.evolution_pollution_units_per_point) * 1_000_000;
        self.enemies.pollution_evolution_micro_remainder = self
            .enemies
            .pollution_evolution_micro_remainder
            .saturating_add(absorbed_micro);
        let points = self.enemies.pollution_evolution_micro_remainder / per_point;
        self.enemies.pollution_evolution_micro_remainder %= per_point;
        self.add_evolution_points(points.min(u64::from(u32::MAX)) as u32);
    }

    fn add_evolution_points(&mut self, raw: u32) {
        let scaled = raw
            .saturating_mul(u32::from(self.config.runtime.evolution_rate_percent))
            .saturating_add(self.enemies.evolution_remainder);
        self.enemies.evolution_remainder = scaled % 100;
        self.enemies.evolution_points =
            u16::try_from((u32::from(self.enemies.evolution_points) + scaled / 100).min(10_000))
                .unwrap_or(10_000);
    }

    fn raid_target_size(&self) -> u8 {
        4 + (self.enemies.evolution_points / 2500).min(4) as u8
    }

    fn launch_ready_raids(&mut self) {
        let Some(cfg) = self.gameplay().copied() else {
            return;
        };
        let target_size = usize::from(self.raid_target_size());
        let ids: Vec<_> = self
            .enemies
            .bases
            .iter()
            .filter_map(|(&id, base)| {
                let timed_out = base.staging_started_tick.is_some_and(|start| {
                    self.tick.saturating_sub(start) >= u64::from(cfg.raid_staging_timeout_ticks)
                });
                (self.tick >= base.next_raid_tick
                    && (base.staged_units.len() >= target_size
                        || timed_out && base.staged_units.len() >= 2))
                    .then_some(id)
            })
            .collect();
        for base_id in ids {
            let raid_id = self.enemies.allocate_raid_id();
            let members = {
                let base = self
                    .enemies
                    .bases
                    .get_mut(&base_id)
                    .expect("launch base must exist");
                base.staging_started_tick = None;
                base.next_raid_tick = self.tick
                    + scaled_interval(
                        cfg.raid_cooldown_ticks,
                        self.config.runtime.raid_frequency_percent,
                    );
                std::mem::take(&mut base.staged_units)
            };
            for id in &members {
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
                    members,
                    target: None,
                    launched_tick: self.tick,
                },
            );
            self.emit_base_event(base_id, ThreatEventKind::RaidLaunched);
        }
    }

    pub(super) fn cleanup_enemy_groups(&mut self) {
        for base in self.enemies.bases.values_mut() {
            base.staged_units
                .retain(|id| self.enemies.enemies.contains_key(id));
        }
        self.enemies.raids.retain(|_, raid| {
            raid.members
                .retain(|id| self.enemies.enemies.contains_key(id));
            !raid.members.is_empty()
        });
        self.enemies.expansions.retain(|_, party| {
            party
                .members
                .retain(|id| self.enemies.enemies.contains_key(id));
            !party.members.is_empty()
        });
    }

    fn advance_expansions_and_growth(&mut self) {
        let Some(cfg) = self.gameplay().copied() else {
            return;
        };
        let base_ids: Vec<_> = self.enemies.bases.keys().copied().collect();
        for base_id in base_ids {
            let (spawner_count, creation, next_expansion, next_growth) = {
                let base = &self.enemies.bases[&base_id];
                (
                    base.spawners.len(),
                    base.creation_tick,
                    base.next_expansion_tick,
                    base.next_growth_tick,
                )
            };
            if spawner_count < usize::from(cfg.max_spawners_per_colony) && self.tick >= next_growth
            {
                self.try_grow_base(base_id, cfg);
                if let Some(base) = self.enemies.bases.get_mut(&base_id) {
                    base.next_growth_tick =
                        self.tick + u64::from(cfg.outpost_growth_interval_ticks);
                }
            }
            if self.config.runtime.expansion
                && spawner_count >= 3
                && self.tick.saturating_sub(creation) >= u64::from(cfg.expansion_minimum_age_ticks)
                && self.tick >= next_expansion
            {
                if let Some(destination) = self.find_expansion_site(base_id, cfg) {
                    self.dispatch_expansion(base_id, destination);
                    if let Some(base) = self.enemies.bases.get_mut(&base_id) {
                        base.next_expansion_tick = self.tick
                            + scaled_interval(
                                cfg.expansion_interval_ticks,
                                self.config.runtime.expansion_frequency_percent,
                            );
                    }
                } else if let Some(base) = self.enemies.bases.get_mut(&base_id) {
                    base.next_expansion_tick = self.tick + u64::from(cfg.expansion_retry_ticks);
                }
            }
        }
    }

    fn try_grow_base(&mut self, base_id: EnemyBaseId, cfg: EnemyGameplayConfig) {
        let Some((&spawner, anchor)) = self
            .enemies
            .bases
            .get(&base_id)
            .and_then(|base| base.spawners.iter().next().map(|id| (id, base.anchor)))
        else {
            return;
        };
        let Some(placed) = self.entities.placed_entities.get(&spawner).cloned() else {
            return;
        };
        for attempt in 0..16_u64 {
            let roll = splitmix64(self.world.seed ^ base_id.raw() ^ self.tick ^ attempt);
            let radius = i64::from(cfg.colony_spawner_radius_tiles);
            let x = placed.x + (roll % (radius as u64 * 2 + 1)) as i64 - radius;
            let y = placed.y + ((roll >> 16) % (radius as u64 * 2 + 1)) as i64 - radius;
            if ChunkCoord::from_tile(x, y) != Some(anchor) {
                continue;
            }
            self.enemies.placement_base = Some(base_id);
            let result = placement::place(
                self,
                placement::EntityPlacementRequest {
                    prototype_id: placed.prototype_id,
                    x,
                    y,
                    direction: Direction::North,
                },
            );
            self.enemies.placement_base = None;
            if result.is_ok() {
                break;
            }
        }
    }

    fn find_expansion_site(
        &self,
        base_id: EnemyBaseId,
        cfg: EnemyGameplayConfig,
    ) -> Option<(WorldTileCoord, WorldTileCoord)> {
        let source = self.enemies.bases.get(&base_id)?.anchor;
        let mut candidates: Vec<_> = self
            .world
            .chunks
            .keys()
            .copied()
            .filter(|coord| {
                let distance = (coord.x - source.x).abs().max((coord.y - source.y).abs());
                distance >= i32::from(cfg.expansion_min_distance_chunks)
                    && distance <= i32::from(cfg.expansion_max_distance_chunks)
            })
            .collect();
        candidates.sort_by_key(|coord| {
            splitmix64(
                self.world.seed
                    ^ base_id.raw()
                    ^ hash_world(self.world.seed, i64::from(coord.x), i64::from(coord.y)),
            )
        });
        candidates
            .into_iter()
            .take(usize::from(cfg.expansion_candidate_limit))
            .find_map(|coord| {
                let (min_x, min_y) = coord.min_tile();
                let roll = splitmix64(
                    self.world.seed ^ base_id.raw() ^ hash_world(self.world.seed, min_x, min_y),
                );
                let x = min_x + 4 + (roll % (CHUNK_SIZE - 8) as u64) as i64;
                let y = min_y + 4 + ((roll >> 16) % (CHUNK_SIZE - 8) as u64) as i64;
                self.expansion_site_clear(base_id, x, y, cfg)
                    .then_some((x, y))
            })
    }

    fn expansion_site_clear(
        &self,
        source: EnemyBaseId,
        x: WorldTileCoord,
        y: WorldTileCoord,
        cfg: EnemyGameplayConfig,
    ) -> bool {
        let Some(tile) = self.world.tile_at(x, y) else {
            return false;
        };
        if tile.resource.is_some()
            || self.entities.occupancy.entity_at(x, y).is_some()
            || !tile.collision.walkable
            || !tile.collision.buildable
        {
            return false;
        }
        let Some(chunk) = ChunkCoord::from_tile(x, y) else {
            return false;
        };
        if self.enemies.bases.values().any(|base| {
            base.id != source
                && (base.anchor.x - chunk.x)
                    .abs()
                    .max((base.anchor.y - chunk.y).abs())
                    < i32::from(cfg.expansion_colony_spacing_chunks)
        }) {
            return false;
        }
        let spacing = i64::from(cfg.expansion_player_spacing_tiles);
        let player_tile = self.player.tile_position();
        if (player_tile.0 - x).abs().max((player_tile.1 - y).abs()) < spacing {
            return false;
        }
        !self
            .entities
            .placed_entities
            .values()
            .filter(|entity| {
                self.world
                    .prototypes
                    .entity(entity.prototype_id)
                    .is_some_and(|p| {
                        p.entity_kind != EntityKind::EnemySpawner
                            && p.entity_kind != EntityKind::ResourcePatch
                    })
            })
            .any(|entity| (entity.x - x).abs().max((entity.y - y).abs()) < spacing)
    }

    fn dispatch_expansion(
        &mut self,
        base_id: EnemyBaseId,
        destination: (WorldTileCoord, WorldTileCoord),
    ) {
        let Some((&spawner_id, unit, spawner_prototype)) = self
            .enemies
            .bases
            .get(&base_id)
            .and_then(|base| base.spawners.iter().next())
            .and_then(|id| {
                let placed = self.entities.placed_entities.get(id)?;
                Some((
                    id,
                    self.world
                        .prototypes
                        .entity(placed.prototype_id)?
                        .enemy_spawner
                        .as_ref()?
                        .unit,
                    placed.prototype_id,
                ))
            })
        else {
            return;
        };
        let expansion_id = self.enemies.allocate_expansion_id();
        let count = 3 + (self.enemies.evolution_points / 5000).min(2) as usize;
        let before: BTreeSet<_> = self.enemies.enemies.keys().copied().collect();
        for _ in 0..count {
            self.spawn_enemy_near_spawner(spawner_id, &unit, EnemyMission::Expansion(expansion_id));
        }
        let members = self
            .enemies
            .enemies
            .keys()
            .filter(|id| !before.contains(id))
            .copied()
            .collect::<BTreeSet<_>>();
        if members.is_empty() {
            return;
        }
        let destination_chunk = ChunkCoord::from_tile(destination.0, destination.1);
        let spotted = self
            .enemies
            .bases
            .get(&base_id)
            .is_some_and(|base| self.chart.revealed_chunks.contains(&base.anchor))
            || destination_chunk.is_some_and(|chunk| self.chart.revealed_chunks.contains(&chunk));
        self.enemies.expansions.insert(
            expansion_id,
            Expansion {
                id: expansion_id,
                base_id,
                members,
                destination,
                spotted,
                spawner_prototype,
            },
        );
        if spotted {
            self.emit_event(
                ThreatEventKind::ExpansionSpotted,
                ThreatLocation::Exact {
                    x: destination.0,
                    y: destination.1,
                },
            );
        }
    }

    fn resolve_arrived_expansions(&mut self) {
        let arrivals: Vec<_> = self
            .enemies
            .expansions
            .iter()
            .filter_map(|(&id, party)| {
                party
                    .members
                    .iter()
                    .filter_map(|member| self.enemies.enemies.get(member))
                    .any(|unit| unit.tile() == party.destination)
                    .then_some(id)
            })
            .collect();
        for expansion_id in arrivals {
            let Some(party) = self.enemies.expansions.remove(&expansion_id) else {
                continue;
            };
            let Some(cfg) = self.gameplay().copied() else {
                continue;
            };
            let Some(founder) = party
                .members
                .iter()
                .filter(|id| self.enemies.enemies.contains_key(id))
                .min()
                .copied()
            else {
                continue;
            };
            if !self.expansion_site_clear(
                party.base_id,
                party.destination.0,
                party.destination.1,
                cfg,
            ) {
                for id in party.members {
                    if let Some(unit) = self.enemies.enemies.get_mut(&id) {
                        unit.mission = EnemyMission::Guard;
                        unit.mode = EnemyMode::Guard;
                        unit.path.clear();
                    }
                }
                continue;
            }
            let prototype_id = party.spawner_prototype;
            let new_base = self.enemies.allocate_base_id();
            let anchor = ChunkCoord::from_tile(party.destination.0, party.destination.1)
                .expect("generated destination has a chunk");
            self.enemies.bases.insert(
                new_base,
                EnemyBase {
                    id: new_base,
                    anchor,
                    spawners: BTreeSet::new(),
                    creation_tick: self.tick,
                    attack_budget_micro: 0,
                    staged_units: BTreeSet::new(),
                    staging_started_tick: None,
                    next_raid_tick: self.tick,
                    next_expansion_tick: self.tick
                        + scaled_interval(
                            cfg.expansion_interval_ticks,
                            self.config.runtime.expansion_frequency_percent,
                        ),
                    next_growth_tick: self.tick + u64::from(cfg.outpost_growth_interval_ticks),
                    pollution_contact: false,
                },
            );
            self.enemies.placement_base = Some(new_base);
            let placed = placement::place(
                self,
                placement::EntityPlacementRequest {
                    prototype_id,
                    x: party.destination.0,
                    y: party.destination.1,
                    direction: Direction::North,
                },
            )
            .is_ok();
            self.enemies.placement_base = None;
            if placed {
                self.enemies.enemies.remove(&founder);
                for id in party.members {
                    if let Some(unit) = self.enemies.enemies.get_mut(&id) {
                        unit.mission = EnemyMission::Guard;
                        unit.mode = EnemyMode::Guard;
                        unit.home_spawner = self.enemies.bases[&new_base]
                            .spawners
                            .iter()
                            .next()
                            .copied();
                        unit.path.clear();
                    }
                }
            } else {
                self.enemies.bases.remove(&new_base);
            }
        }
    }
}

fn scaled_interval(base_ticks: u32, percent: u16) -> u64 {
    if percent == 0 {
        u64::MAX
    } else {
        (u64::from(base_ticks) * 100)
            .div_ceil(u64::from(percent))
            .max(1)
    }
}

fn scale_stat(base: u32, strength_percent: u32, evolution_percent: u32) -> u32 {
    u32::try_from(
        (u64::from(base) * u64::from(strength_percent) * u64::from(evolution_percent) / 10_000)
            .max(1),
    )
    .unwrap_or(u32::MAX)
}

fn step_enemy(
    world: &WorldSim,
    entities: &EntityStore,
    seed: u64,
    tick: u64,
    enemy: &mut Enemy,
    attacks: &mut Vec<(EntityId, u32)>,
) {
    // Drop targets that no longer exist.
    if let Some(target) = enemy.target
        && !entities.placed_entities.contains_key(&target)
    {
        enemy.target = None;
        enemy.path.clear();
    }

    if enemy.target.is_none() && tick >= enemy.next_decision_tick {
        enemy.target = acquire_target(world, entities, enemy);
        enemy.next_decision_tick = tick + ENEMY_TARGET_RESCAN_TICKS + enemy.id.raw() % 16;
        if enemy.target.is_some() {
            enemy.path.clear();
        }
    }

    let Some(target) = enemy.target else {
        wander(world, entities, seed, tick, enemy);
        return;
    };
    let Some(target_footprint) = entities
        .placed_entities
        .get(&target)
        .map(|placed| placed.footprint)
    else {
        return;
    };

    // Attack when standing next to (or on the edge of) the target.
    let tile = enemy.tile();
    if chebyshev_distance_to_footprint(tile, &target_footprint) <= 1 {
        enemy.path.clear();
        if tick >= enemy.next_attack_tick {
            attacks.push((target, enemy.damage));
            enemy.next_attack_tick = tick + u64::from(enemy.attack_cooldown_ticks);
        }
        return;
    }

    // Recompute the path when it ran out, was invalidated, or grew stale.
    let next_waypoint_blocked = enemy
        .path
        .front()
        .is_some_and(|&(x, y)| !tile_open_for_enemy(world, entities, x, y, Some(target)));
    if (enemy.path.is_empty() || next_waypoint_blocked) && tick >= enemy.next_decision_tick {
        enemy.next_decision_tick = tick + ENEMY_REPATH_INTERVAL_TICKS + enemy.id.raw() % 16;
        enemy.path.clear();
        if chebyshev_distance_to_footprint(tile, &target_footprint)
            <= ENEMY_PATHFIND_MAX_RANGE_TILES
            && let Some(path) = find_path(world, entities, tile, target, &target_footprint)
        {
            enemy.path = path;
        }
        if enemy.path.is_empty() {
            // No route: walk straight at the target and gnaw through the
            // first structure in the way.
            greedy_step(world, entities, enemy, target, &target_footprint);
            return;
        }
    }

    follow_path(enemy);
}

/// Chooses what a unit fights: guards react to player structures near them,
/// attackers march on the closest structure anywhere in the world.
fn acquire_target(world: &WorldSim, entities: &EntityStore, enemy: &Enemy) -> Option<EntityId> {
    let (tile_x, tile_y) = enemy.tile();
    match enemy.mode {
        EnemyMode::Guard => {
            let radius = i64::from(enemy.aggro_radius_tiles);
            let candidates = entities.occupancy.entity_ids_in_tile_rect(
                tile_x - radius,
                tile_x + radius,
                tile_y - radius,
                tile_y + radius,
            );
            nearest_attackable(world, entities, (tile_x, tile_y), candidates.into_iter())
        }
        EnemyMode::Attack => {
            nearest_attackable_in_expanding_ranges(world, entities, (tile_x, tile_y))
        }
    }
}

/// Nearest player structure among `candidates`; enemy-owned entities are
/// never targets. Ties resolve to the lowest entity id because candidates
/// iterate in ascending id order.
fn nearest_attackable(
    world: &WorldSim,
    entities: &EntityStore,
    from: (WorldTileCoord, WorldTileCoord),
    candidates: impl Iterator<Item = EntityId>,
) -> Option<EntityId> {
    nearest_attackable_with_distance(world, entities, from, candidates)
        .map(|(_, entity_id)| entity_id)
}

fn nearest_attackable_with_distance(
    world: &WorldSim,
    entities: &EntityStore,
    from: (WorldTileCoord, WorldTileCoord),
    candidates: impl Iterator<Item = EntityId>,
) -> Option<(i128, EntityId)> {
    let mut best: Option<(i128, EntityId)> = None;
    for entity_id in candidates {
        let Some(placed) = entities.placed_entities.get(&entity_id) else {
            continue;
        };
        if !is_attackable_kind(world, placed) {
            continue;
        }
        let (center_x, center_y) = footprint_center_tile(&placed.footprint);
        let dx = center_x - from.0;
        let dy = center_y - from.1;
        let distance = i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy);
        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, entity_id));
        }
    }
    best
}

/// Finds the nearest player structure through the occupancy grid, doubling
/// the searched square until it proves no unseen footprint can be nearer.
fn nearest_attackable_in_expanding_ranges(
    world: &WorldSim,
    entities: &EntityStore,
    from: (WorldTileCoord, WorldTileCoord),
) -> Option<EntityId> {
    let mut radius = i64::from(CHUNK_SIZE);
    let mut candidates = BTreeSet::new();

    loop {
        candidates.extend(entities.occupancy.entity_ids_in_tile_rect(
            from.0.saturating_sub(radius),
            from.0.saturating_add(radius),
            from.1.saturating_sub(radius),
            from.1.saturating_add(radius),
        ));
        let best =
            nearest_attackable_with_distance(world, entities, from, candidates.iter().copied());
        if let Some((distance, entity_id)) = best {
            let min_unseen_center_distance = radius.saturating_add(1);
            let unseen_distance_squared =
                i128::from(min_unseen_center_distance) * i128::from(min_unseen_center_distance);
            if distance < unseen_distance_squared {
                return Some(entity_id);
            }
        }

        if radius == i64::MAX {
            return best.map(|(_, entity_id)| entity_id);
        }
        radius = radius.saturating_mul(2);
    }
}

fn is_attackable_kind(world: &WorldSim, placed: &PlacedEntity) -> bool {
    world
        .prototypes
        .entity(placed.prototype_id)
        .is_some_and(|prototype| {
            !matches!(
                prototype.entity_kind,
                EntityKind::EnemySpawner | EntityKind::ResourcePatch
            )
        })
}

/// Idle guards drift around their home spawner so nests look alive.
fn wander(world: &WorldSim, entities: &EntityStore, seed: u64, tick: u64, enemy: &mut Enemy) {
    if !enemy.path.is_empty() {
        follow_path(enemy);
        return;
    }
    if tick < enemy.next_decision_tick {
        return;
    }
    enemy.next_decision_tick = tick + ENEMY_WANDER_INTERVAL_TICKS + enemy.id.raw() % 64;

    let anchor = enemy
        .home_spawner
        .and_then(|spawner| entities.placed_entities.get(&spawner))
        .map(|placed| footprint_center_tile(&placed.footprint))
        .unwrap_or_else(|| enemy.tile());
    let roll = splitmix64(seed ^ enemy.id.raw().wrapping_mul(0x9e37_79b9) ^ tick);
    let dx = ((roll & 0x7) as i64) - 3;
    let dy = (((roll >> 3) & 0x7) as i64) - 3;
    let goal = (anchor.0 + dx, anchor.1 + dy);
    if tile_open_for_enemy(world, entities, goal.0, goal.1, None) {
        enemy.path.push_back(goal);
    }
}

/// Advances the unit along its waypoints by one tick's movement budget.
/// Waypoints are 4-connected tile centers, so per-leg movement is
/// axis-aligned and stays exact in fixed-point integers.
fn follow_path(enemy: &mut Enemy) {
    let mut budget = i64::from(enemy.speed_fixed_per_tick);
    while budget > 0 {
        let Some(&(waypoint_x, waypoint_y)) = enemy.path.front() else {
            return;
        };
        let goal_x = tile_center_fixed(waypoint_x);
        let goal_y = tile_center_fixed(waypoint_y);

        let dx = goal_x - enemy.x;
        let step_x = dx.signum() * dx.abs().min(budget);
        enemy.x += step_x;
        budget -= step_x.abs();

        let dy = goal_y - enemy.y;
        let step_y = dy.signum() * dy.abs().min(budget);
        enemy.y += step_y;
        budget -= step_y.abs();

        if enemy.x == goal_x && enemy.y == goal_y {
            enemy.path.pop_front();
        } else {
            return;
        }
    }
}

/// Fallback movement when no path exists: step toward the target, and when a
/// structure blocks the step, attack it instead (walls become chew targets).
fn greedy_step(
    world: &WorldSim,
    entities: &EntityStore,
    enemy: &mut Enemy,
    target: EntityId,
    target_footprint: &EntityFootprint,
) {
    let (tile_x, tile_y) = enemy.tile();
    let (goal_x, goal_y) = footprint_center_tile(target_footprint);
    let dx = goal_x - tile_x;
    let dy = goal_y - tile_y;

    let mut steps = [(0, 0); 2];
    if dx.abs() >= dy.abs() {
        steps[0] = (dx.signum(), 0);
        steps[1] = (0, dy.signum());
    } else {
        steps[0] = (0, dy.signum());
        steps[1] = (dx.signum(), 0);
    }

    for (step_x, step_y) in steps {
        if step_x == 0 && step_y == 0 {
            continue;
        }
        let next = (tile_x + step_x, tile_y + step_y);
        if tile_open_for_enemy(world, entities, next.0, next.1, Some(target)) {
            enemy.path.push_back(next);
            follow_path(enemy);
            return;
        }
        // Blocked by a structure: switch targets and chew through it.
        if let Some(blocker) = entities.occupancy.entity_at(next.0, next.1)
            && blocker != target
            && entities
                .placed_entities
                .get(&blocker)
                .is_some_and(|placed| is_attackable_kind(world, placed))
        {
            enemy.target = Some(blocker);
            enemy.path.clear();
            return;
        }
    }
}

/// A tile a unit may stand on: generated, walkable terrain, and free of
/// structures other than the unit's own target.
fn tile_open_for_enemy(
    world: &WorldSim,
    entities: &EntityStore,
    x: WorldTileCoord,
    y: WorldTileCoord,
    target: Option<EntityId>,
) -> bool {
    let Some(tile) = world.tile_at(x, y) else {
        return false;
    };
    if !tile.collision.walkable {
        return false;
    }
    match entities.occupancy.entity_at(x, y) {
        None => true,
        Some(occupant) => Some(occupant) == target,
    }
}

/// Bounded deterministic A* over 4-connected tiles toward any tile adjacent
/// to the target footprint. Returns waypoints excluding the start tile.
fn find_path(
    world: &WorldSim,
    entities: &EntityStore,
    start: (WorldTileCoord, WorldTileCoord),
    target: EntityId,
    target_footprint: &EntityFootprint,
) -> Option<VecDeque<(WorldTileCoord, WorldTileCoord)>> {
    type Tile = (WorldTileCoord, WorldTileCoord);

    let heuristic = |tile: Tile| -> i64 { manhattan_distance_to_footprint(tile, target_footprint) };

    let mut open: BinaryHeap<Reverse<(i64, i64, Tile)>> = BinaryHeap::new();
    let mut best_g: BTreeMap<Tile, i64> = BTreeMap::new();
    let mut came_from: BTreeMap<Tile, Tile> = BTreeMap::new();

    best_g.insert(start, 0);
    open.push(Reverse((heuristic(start), 0, start)));
    let mut expansions = 0;

    while let Some(Reverse((_, g, tile))) = open.pop() {
        if best_g.get(&tile).copied().is_some_and(|best| g > best) {
            continue;
        }
        if chebyshev_distance_to_footprint(tile, target_footprint) <= 1 {
            let mut path = VecDeque::new();
            let mut current = tile;
            while current != start {
                path.push_front(current);
                current = came_from[&current];
            }
            return Some(path);
        }
        expansions += 1;
        if expansions > ENEMY_PATHFIND_MAX_EXPANSIONS {
            return None;
        }

        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = (tile.0 + dx, tile.1 + dy);
            if (next.0 - start.0).abs() > ENEMY_PATHFIND_MAX_RANGE_TILES
                || (next.1 - start.1).abs() > ENEMY_PATHFIND_MAX_RANGE_TILES
            {
                continue;
            }
            if !tile_open_for_enemy(world, entities, next.0, next.1, Some(target)) {
                continue;
            }
            let next_g = g + 1;
            if best_g.get(&next).copied().is_none_or(|best| next_g < best) {
                best_g.insert(next, next_g);
                came_from.insert(next, tile);
                open.push(Reverse((next_g + heuristic(next), next_g, next)));
            }
        }
    }

    None
}

fn footprint_center_tile(footprint: &EntityFootprint) -> (WorldTileCoord, WorldTileCoord) {
    (
        footprint.x + i64::from(footprint.width) / 2,
        footprint.y + i64::from(footprint.height) / 2,
    )
}

fn axis_distance_to_span(value: i64, span_start: i64, span_len: i32) -> i64 {
    if value < span_start {
        span_start - value
    } else if value >= span_start + i64::from(span_len) {
        value - (span_start + i64::from(span_len) - 1)
    } else {
        0
    }
}

pub(super) fn chebyshev_distance_to_footprint(
    tile: (WorldTileCoord, WorldTileCoord),
    footprint: &EntityFootprint,
) -> i64 {
    let dx = axis_distance_to_span(tile.0, footprint.x, footprint.width);
    let dy = axis_distance_to_span(tile.1, footprint.y, footprint.height);
    dx.max(dy)
}

fn manhattan_distance_to_footprint(
    tile: (WorldTileCoord, WorldTileCoord),
    footprint: &EntityFootprint,
) -> i64 {
    let dx = axis_distance_to_span(tile.0, footprint.x, footprint.width);
    let dy = axis_distance_to_span(tile.1, footprint.y, footprint.height);
    dx + dy
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

#[cfg(test)]
mod enemy_feature_tests {
    use super::*;

    #[test]
    fn difficulty_presets_match_balance_defaults() {
        let peaceful = EnemyDifficultyPreset::Peaceful.config();
        let standard = EnemyDifficultyPreset::Standard.config();
        let aggressive = EnemyDifficultyPreset::Aggressive.config();
        assert_eq!(
            (
                peaceful.world.base_density_percent,
                peaceful.world.starting_safe_radius_tiles
            ),
            (75, 180)
        );
        assert_eq!(
            (
                standard.runtime.strength_percent,
                standard.runtime.raid_frequency_percent
            ),
            (100, 100)
        );
        assert_eq!(
            (
                aggressive.runtime.strength_percent,
                aggressive.runtime.expansion_frequency_percent
            ),
            (150, 175)
        );
        assert!(!peaceful.runtime.proactive_raids && !peaceful.runtime.expansion);
    }

    #[test]
    fn density_zero_prevents_generated_colonies() {
        let standard = SimulationConfig::default();
        let config = SimulationConfig {
            preset: EnemyDifficultyPreset::Custom,
            world: EnemyWorldSettings {
                base_density_percent: 0,
                ..standard.world
            },
            ..standard
        };
        let mut sim =
            Simulation::new_with_config(123, PrototypeCatalog::load_base().unwrap(), config);
        for y in -20..=20 {
            for x in -20..=20 {
                sim.ensure_chunk_generated(ChunkCoord { x, y });
            }
        }
        assert!(sim.enemies.bases.is_empty());
    }

    #[test]
    fn runtime_command_preserves_immutable_world_settings() {
        let mut sim = Simulation::new_test_world(123);
        let world = sim.enemy_settings().world;
        let runtime = EnemyDifficultyPreset::Peaceful.config().runtime;
        sim.apply_command(&SimCommand::SetEnemyRuntimeSettings(runtime))
            .unwrap();
        assert_eq!(sim.enemy_settings().world, world);
        assert_eq!(sim.enemy_settings().runtime, runtime);
        assert_eq!(sim.enemy_settings().preset, EnemyDifficultyPreset::Custom);
    }

    #[test]
    fn threat_log_is_ordered_and_bounded() {
        let mut sim = Simulation::new_test_world(123);
        for index in 0..300 {
            sim.tick = index;
            sim.emit_event(
                ThreatEventKind::StructureUnderAttack,
                ThreatLocation::Sector(ChunkCoord { x: 0, y: 0 }),
            );
        }
        assert_eq!(sim.enemies.threat_events.len(), 256);
        assert_eq!(sim.enemies.threat_events.front().unwrap().sequence, 45);
        assert_eq!(sim.threat_events_after(298).len(), 2);
    }
}
