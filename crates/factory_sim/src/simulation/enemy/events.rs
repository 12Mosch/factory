use super::*;

#[derive(Clone, Copy)]
struct EnemyMapBounds {
    min_x: WorldTileCoord,
    max_x: WorldTileCoord,
    min_y: WorldTileCoord,
    max_y: WorldTileCoord,
}

impl EnemyMapBounds {
    fn new(
        min_x: WorldTileCoord,
        max_x: WorldTileCoord,
        min_y: WorldTileCoord,
        max_y: WorldTileCoord,
    ) -> Option<Self> {
        (min_x <= max_x && min_y <= max_y).then_some(Self {
            min_x,
            max_x,
            min_y,
            max_y,
        })
    }

    fn contains_point(self, x: WorldTileCoord, y: WorldTileCoord) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    fn intersects_sector(self, coord: ChunkCoord) -> bool {
        let (x, y) = coord.min_tile();
        let chunk_max_x = x + i64::from(CHUNK_SIZE) - 1;
        let chunk_max_y = y + i64::from(CHUNK_SIZE) - 1;
        x <= self.max_x && chunk_max_x >= self.min_x && y <= self.max_y && chunk_max_y >= self.min_y
    }

    fn intersects_location(self, location: ThreatLocation) -> bool {
        match location {
            ThreatLocation::Exact { x, y } => self.contains_point(x, y),
            ThreatLocation::Sector(coord) => self.intersects_sector(coord),
        }
    }
}

impl Simulation {
    pub fn enemies(&self) -> &EnemySubsystem {
        &self.enemies
    }

    pub fn enemy_settings(&self) -> SimulationConfig {
        self.config
    }

    /// Threat events with a sequence beyond `sequence`, oldest first. The log
    /// is ordered, so the scan starts at the cursor instead of walking the
    /// whole log.
    pub fn threat_events_after(&self, sequence: u64) -> Vec<ThreatEvent> {
        let start = self
            .enemies
            .threat_events
            .partition_point(|event| event.sequence <= sequence);
        self.enemies.threat_events.range(start..).copied().collect()
    }

    /// Sequence of the most recently emitted threat event; the starting
    /// cursor for observers that only care about events from now on.
    pub fn latest_threat_sequence(&self) -> u64 {
        self.enemies.threat_sequence
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
        self.build_enemy_map_snapshot(None)
    }

    pub fn enemy_map_snapshot_in_tile_rect(
        &self,
        min_x: WorldTileCoord,
        max_x: WorldTileCoord,
        min_y: WorldTileCoord,
        max_y: WorldTileCoord,
    ) -> EnemyMapSnapshot {
        let Some(bounds) = EnemyMapBounds::new(min_x, max_x, min_y, max_y) else {
            return EnemyMapSnapshot::default();
        };
        self.build_enemy_map_snapshot(Some(bounds))
    }

    fn build_enemy_map_snapshot(&self, bounds: Option<EnemyMapBounds>) -> EnemyMapSnapshot {
        let mut snapshot = EnemyMapSnapshot::default();
        for base in self.enemies.bases.values() {
            if base.pollution_contact
                && bounds.is_none_or(|bounds| bounds.intersects_sector(base.anchor))
            {
                snapshot.contacted_sectors.push(base.anchor);
            }
            if self.chart.revealed_chunks.contains(&base.anchor)
                && let Some(spawner) = base
                    .spawners
                    .iter()
                    .next()
                    .and_then(|id| self.entities.placed_entities.get(id))
                && bounds.is_none_or(|bounds| bounds.contains_point(spawner.x, spawner.y))
            {
                snapshot.known_bases.push((base.id, spawner.x, spawner.y));
            }
        }
        for raid in self.enemies.raids.values() {
            // Only reveal a raid's exact position once it marches into
            // charted territory; until then the map shows its home sector.
            let member_location = raid
                .members
                .iter()
                .filter_map(|id| self.enemies.enemies.get(id))
                .map(|unit| unit.tile())
                .find(|&(x, y)| {
                    ChunkCoord::from_tile(x, y)
                        .is_some_and(|chunk| self.chart.revealed_chunks.contains(&chunk))
                })
                .map(|(x, y)| ThreatLocation::Exact { x, y });
            let Some(location) = member_location.or_else(|| {
                self.enemies
                    .bases
                    .get(&raid.base_id)
                    .map(|base| ThreatLocation::Sector(base.anchor))
            }) else {
                continue;
            };
            if bounds.is_some_and(|bounds| !bounds.intersects_location(location)) {
                continue;
            }
            snapshot.raids.push((raid.id, location));
            if let Some(target) = raid.target {
                snapshot.raid_targets.push((raid.id, target));
            }
        }
        for party in self.enemies.expansions.values().filter(|party| {
            party.spotted
                && bounds.is_none_or(|bounds| {
                    bounds.contains_point(party.destination.0, party.destination.1)
                })
        }) {
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

    /// The catalog's enemy gameplay tuning. Catalog loading rejects enemy
    /// content (spawner prototypes or base generation) without this section,
    /// so the `None` early-returns downstream only fire for catalogs that
    /// genuinely have no enemies.
    pub(in crate::simulation) fn gameplay(&self) -> Option<&EnemyGameplayConfig> {
        self.world.prototypes.enemy_gameplay()
    }
}

impl Simulation {
    pub(in crate::simulation) fn emit_structure_damage_warning(
        &mut self,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) {
        let Some(chunk) = ChunkCoord::from_tile(x, y) else {
            return;
        };
        if self
            .enemies
            .structure_warning_ticks
            .get(&chunk)
            .is_some_and(|tick| self.tick.saturating_sub(*tick) < STRUCTURE_WARNING_COOLDOWN_TICKS)
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

    pub(super) fn emit_base_event(&mut self, base_id: EnemyBaseId, kind: ThreatEventKind) {
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

    pub(super) fn emit_event(&mut self, kind: ThreatEventKind, location: ThreatLocation) {
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
}

#[cfg(test)]
mod map_snapshot_regression_tests {
    use super::*;
    use crate::enemies::{EnemyBase, Raid};

    const HIDDEN_TILE: (WorldTileCoord, WorldTileCoord) = (5, 5);
    const VISIBLE_TILE: (WorldTileCoord, WorldTileCoord) = (100, 100);
    const SECOND_VISIBLE_TILE: (WorldTileCoord, WorldTileCoord) = (103, 100);

    fn tile_center(tile: WorldTileCoord) -> i64 {
        tile * POSITION_SCALE + POSITION_SCALE / 2
    }

    fn raid_enemy(id: EnemyId, tile: (WorldTileCoord, WorldTileCoord), raid_id: RaidId) -> Enemy {
        Enemy {
            id,
            x: tile_center(tile.0),
            y: tile_center(tile.1),
            health: HealthState::new(100, Faction::Enemy),
            attack: AttackDefinition::melee(Damage::physical(5), 60, 1),
            speed_fixed_per_tick: 40,
            aggro_radius_tiles: 0,
            mode: EnemyMode::Attack,
            mission: EnemyMission::Raid(raid_id),
            home_spawner: None,
            target: None,
            path: VecDeque::new(),
            next_attack_tick: 0,
            next_decision_tick: 0,
        }
    }

    fn insert_base(sim: &mut Simulation, anchor: ChunkCoord) -> EnemyBaseId {
        let base_id = sim.enemies.allocate_base_id();
        sim.enemies.bases.insert(
            base_id,
            EnemyBase {
                id: base_id,
                anchor,
                spawners: BTreeSet::new(),
                creation_tick: 0,
                attack_budget_micro: 0,
                staged_units: BTreeSet::new(),
                staging_started_tick: None,
                next_raid_tick: 0,
                next_expansion_tick: 0,
                next_growth_tick: 0,
                pollution_contact: false,
            },
        );
        base_id
    }

    fn reveal_only(tile: (WorldTileCoord, WorldTileCoord), sim: &mut Simulation) {
        sim.chart.revealed_chunks.clear();
        sim.chart
            .revealed_chunks
            .insert(ChunkCoord::from_tile(tile.0, tile.1).expect("test tile maps to a chunk"));
    }

    fn insert_raid(
        sim: &mut Simulation,
        base_id: EnemyBaseId,
        tiles: &[(WorldTileCoord, WorldTileCoord)],
    ) -> RaidId {
        let raid_id = sim.enemies.allocate_raid_id();
        let mut members = BTreeSet::new();
        for tile in tiles {
            let enemy_id = sim.enemies.allocate_id();
            members.insert(enemy_id);
            sim.enemies
                .enemies
                .insert(enemy_id, raid_enemy(enemy_id, *tile, raid_id));
        }
        sim.enemies.raids.insert(
            raid_id,
            Raid {
                id: raid_id,
                base_id,
                members,
                target: None,
                launched_tick: 0,
            },
        );
        raid_id
    }

    fn raid_location(snapshot: &EnemyMapSnapshot, raid_id: RaidId) -> Option<ThreatLocation> {
        snapshot
            .raids
            .iter()
            .find(|(id, _)| *id == raid_id)
            .map(|(_, location)| *location)
    }

    #[test]
    fn split_visibility_reports_visible_member_exact_position() {
        let mut sim = Simulation::new_test_world(123);
        let base_id = insert_base(&mut sim, ChunkCoord { x: 9, y: 9 });
        reveal_only(VISIBLE_TILE, &mut sim);
        let raid_id = insert_raid(&mut sim, base_id, &[HIDDEN_TILE, VISIBLE_TILE]);

        let snapshot = sim.enemy_map_snapshot();
        assert_eq!(
            raid_location(&snapshot, raid_id),
            Some(ThreatLocation::Exact {
                x: VISIBLE_TILE.0,
                y: VISIBLE_TILE.1,
            })
        );
    }

    #[test]
    fn visible_member_survives_destroyed_source_base() {
        let mut sim = Simulation::new_test_world(123);
        // No base inserted: the source base was destroyed, so there is no
        // home-sector fallback for this raid.
        let missing_base = EnemyBaseId::new(999);
        reveal_only(VISIBLE_TILE, &mut sim);
        let raid_id = insert_raid(&mut sim, missing_base, &[HIDDEN_TILE, VISIBLE_TILE]);

        let snapshot = sim.enemy_map_snapshot();
        assert_eq!(
            raid_location(&snapshot, raid_id),
            Some(ThreatLocation::Exact {
                x: VISIBLE_TILE.0,
                y: VISIBLE_TILE.1,
            })
        );
    }

    #[test]
    fn no_visible_member_falls_back_to_home_sector() {
        let mut sim = Simulation::new_test_world(123);
        let anchor = ChunkCoord { x: 9, y: 9 };
        let base_id = insert_base(&mut sim, anchor);
        reveal_only(VISIBLE_TILE, &mut sim);
        let raid_id = insert_raid(&mut sim, base_id, &[HIDDEN_TILE, (6, 6)]);

        let snapshot = sim.enemy_map_snapshot();
        assert_eq!(
            raid_location(&snapshot, raid_id),
            Some(ThreatLocation::Sector(anchor))
        );
    }

    #[test]
    fn lowest_id_visible_member_wins_deterministically() {
        let mut sim = Simulation::new_test_world(123);
        let base_id = insert_base(&mut sim, ChunkCoord { x: 9, y: 9 });
        reveal_only(VISIBLE_TILE, &mut sim);
        // Lowest-ID member hidden; the next two in EnemyId order are both
        // visible, so the first visible one must be reported.
        let raid_id = insert_raid(
            &mut sim,
            base_id,
            &[HIDDEN_TILE, VISIBLE_TILE, SECOND_VISIBLE_TILE],
        );

        let snapshot = sim.enemy_map_snapshot();
        assert_eq!(
            raid_location(&snapshot, raid_id),
            Some(ThreatLocation::Exact {
                x: VISIBLE_TILE.0,
                y: VISIBLE_TILE.1,
            })
        );
    }
}
