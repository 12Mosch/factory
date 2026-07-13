use super::*;

impl Simulation {
    pub(in crate::simulation) fn set_enemy_runtime_settings(
        &mut self,
        settings: EnemyRuntimeSettings,
    ) {
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
            base.next_raid_tick = next_scaled_tick(
                self.tick,
                gameplay.raid_cooldown_ticks,
                settings.raid_frequency_percent,
            );
            base.next_expansion_tick = if settings.expansion {
                next_scaled_tick(
                    self.tick,
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
                        unit.target = None;
                        unit.path.clear();
                    }
                }
                base.staging_started_tick = None;
            }
        }
    }
    pub(in crate::simulation) fn raid_target_size(&self) -> u8 {
        4 + (self.enemies.evolution_points / 2500).min(4) as u8
    }

    pub(in crate::simulation) fn attack_budget_cap(&self, base_id: EnemyBaseId) -> Option<u64> {
        let raid_size = u64::from(self.raid_target_size());
        self.enemies
            .bases
            .get(&base_id)?
            .spawners
            .iter()
            .filter_map(|spawner_id| {
                let placed = self.entities.placed_entities.get(spawner_id)?;
                let config = self
                    .world
                    .prototypes
                    .entity(placed.prototype_id)?
                    .enemy_spawner
                    .as_ref()?;
                Some(u64::from(config.unit_spawn_pollution_cost_milli) * 1000 * raid_size * 10)
            })
            .max()
    }

    pub(super) fn launch_ready_raids(&mut self) {
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
                base.next_raid_tick = next_scaled_tick(
                    self.tick,
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

    pub(in crate::simulation) fn cleanup_enemy_groups(&mut self) {
        for base in self.enemies.bases.values_mut() {
            base.staged_units
                .retain(|id| self.enemies.enemies.contains_key(id));
            // A wiped-out staging wave must not leave its timer behind, or
            // the next wave would skip RaidPreparing and time out instantly.
            if base.staged_units.is_empty() {
                base.staging_started_tick = None;
            }
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
}
