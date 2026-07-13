use super::*;

impl Simulation {
    pub(super) fn advance_expansions_and_growth(&mut self) {
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
                        base.next_expansion_tick = next_scaled_tick(
                            self.tick,
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
            let _ = self.spawn_enemy_near_spawner(
                spawner_id,
                &unit,
                EnemyMission::Expansion(expansion_id),
            );
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

    pub(in crate::simulation) fn resolve_arrived_expansions(&mut self) {
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
                    next_raid_tick: next_scaled_tick(
                        self.tick,
                        cfg.raid_cooldown_ticks,
                        self.config.runtime.raid_frequency_percent,
                    ),
                    next_expansion_tick: next_scaled_tick(
                        self.tick,
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
