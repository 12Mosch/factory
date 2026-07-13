use super::*;
use crate::enemies::{EnemyBase, Expansion, Raid};
use factory_data::{EnemyBaseGenerationConfig, EnemyGameplayConfig, UnitPrototype};

mod events;
mod evolution;
mod expansion;
mod generation;
mod navigation;
mod raids;
mod spawning;
mod targeting;

pub(super) use navigation::EnemyNavigation;
pub(super) use targeting::AttackTargetCache;

/// Next tick a frequency-scaled schedule fires: `base_ticks` stretched by the
/// inverse of `percent`. Owns the "never" semantics — a zero percent (and any
/// overflow) saturates to `u64::MAX` instead of wrapping past `tick`.
fn next_scaled_tick(tick: u64, base_ticks: u32, percent: u16) -> u64 {
    if percent == 0 {
        return u64::MAX;
    }
    let interval = (u64::from(base_ticks) * 100)
        .div_ceil(u64::from(percent))
        .max(1);
    tick.saturating_add(interval)
}

#[cfg(test)]
mod enemy_feature_tests {
    use super::targeting::{AttackableStructureIndex, IndexedAttackCell, IndexedAttackTarget};
    use super::*;

    fn indexed_target(entity_id: u64, x: i64, y: i64) -> IndexedAttackTarget {
        IndexedAttackTarget {
            entity_id: EntityId::new(entity_id),
            footprint: EntityFootprint::single_tile(x, y),
        }
    }

    fn indexed_cell(targets: impl IntoIterator<Item = IndexedAttackTarget>) -> IndexedAttackCell {
        let mut targets = targets.into_iter();
        let first = targets.next().expect("test cell must contain a target");
        let mut cell = IndexedAttackCell::new(first);
        for target in targets {
            cell.push(target);
        }
        cell
    }

    #[test]
    fn spatial_index_finds_exact_nearest_target_and_breaks_ties_by_id() {
        let mut index = AttackableStructureIndex::default();
        index
            .cells
            .insert((0, 0), indexed_cell([indexed_target(9, 31, 31)]));
        index
            .cells
            .insert((1, 0), indexed_cell([indexed_target(8, 32, 0)]));
        index
            .cells
            .insert((-2, 0), indexed_cell([indexed_target(3, -40, 0)]));

        assert_eq!(
            index.nearest(&EntityFootprint::single_tile(0, 0)),
            Some(EntityId::new(8))
        );

        index
            .cells
            .get_mut(&(1, 0))
            .unwrap()
            .push(indexed_target(7, 32, 0));
        assert_eq!(
            index.nearest(&EntityFootprint::single_tile(0, 0)),
            Some(EntityId::new(7))
        );
    }

    #[test]
    fn spatial_index_ranks_targets_by_nearest_footprint_edge() {
        let mut index = AttackableStructureIndex::default();
        index
            .cells
            .insert((0, 0), indexed_cell([indexed_target(2, 20, 0)]));
        index.cells.insert(
            (1, 0),
            indexed_cell([IndexedAttackTarget {
                entity_id: EntityId::new(1),
                footprint: EntityFootprint {
                    x: 10,
                    y: 0,
                    width: 50,
                    height: 1,
                },
            }]),
        );

        assert_eq!(
            index.nearest(&EntityFootprint::single_tile(0, 0)),
            Some(EntityId::new(1)),
            "the nearer edge wins even when the other target's center is closer"
        );
    }

    #[test]
    fn shared_target_query_budget_scales_with_groups_not_units() {
        const BASES: u64 = 10;
        const ATTACKERS: u64 = 500;

        let mut cache = AttackTargetCache::default();
        cache
            .index
            .cells
            .insert((20, 20), indexed_cell([indexed_target(42, 640, 640)]));

        for unit in 0..ATTACKERS {
            let base_id = EnemyBaseId::new(unit % BASES + 1);
            assert_eq!(
                cache.target_for_base(base_id, EntityFootprint::single_tile(unit as i64, 0)),
                Some(EntityId::new(42))
            );
        }

        assert_eq!(
            cache.shared_target_queries, BASES as usize,
            "500 staging attackers should perform one global query per base"
        );
    }

    #[test]
    fn topology_revision_rebuilds_index_and_invalidates_group_targets() {
        let sim = Simulation::new_test_world(123);
        let mut cache = AttackTargetCache::default();
        assert!(!cache.refresh(
            sim.entity_topology_revision,
            &sim.world,
            &sim.entities,
            &sim.enemies,
        ));
        cache
            .base_targets
            .insert(EnemyBaseId::new(1), Some(EntityId::new(11)));
        cache
            .raid_targets
            .insert(RaidId::new(1), Some(EntityId::new(12)));

        assert!(cache.refresh(
            sim.entity_topology_revision.wrapping_add(1),
            &sim.world,
            &sim.entities,
            &sim.enemies,
        ));
        assert!(cache.base_targets.is_empty());
        assert!(cache.raid_targets.is_empty());
    }

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
    fn zero_frequency_percent_schedules_never_without_overflow() {
        assert_eq!(next_scaled_tick(u64::MAX - 5, 3_600, 0), u64::MAX);
        assert_eq!(next_scaled_tick(u64::MAX - 5, 3_600, 100), u64::MAX);
        assert_eq!(next_scaled_tick(100, 3_600, 100), 3_700);
        assert_eq!(next_scaled_tick(100, 3_600, 200), 1_900);
    }

    #[test]
    fn peaceful_runtime_settings_never_schedule_raids_on_existing_bases() {
        let mut sim = Simulation::new_test_world(123);
        let base_id = EnemyBaseId::new(1);
        sim.enemies.bases.insert(
            base_id,
            EnemyBase {
                id: base_id,
                anchor: ChunkCoord { x: 4, y: 4 },
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
        let runtime = EnemyDifficultyPreset::Peaceful.config().runtime;
        sim.apply_command(&SimCommand::SetEnemyRuntimeSettings(runtime))
            .unwrap();
        let base = &sim.enemies.bases[&base_id];
        assert_eq!(base.next_raid_tick, u64::MAX);
        assert_eq!(base.next_expansion_tick, u64::MAX);
    }

    #[test]
    fn new_bases_never_schedule_raids_at_zero_raid_frequency() {
        let mut sim = Simulation::new_test_world(123);
        let mut runtime = sim.enemy_settings().runtime;
        runtime.raid_frequency_percent = 0;
        sim.apply_command(&SimCommand::SetEnemyRuntimeSettings(runtime))
            .unwrap();

        let spawner = EntityId::new(9_999);
        sim.on_enemy_spawner_placed(spawner, 40, 40);

        let base_id = sim.enemies.spawner_bases[&spawner];
        assert_eq!(sim.enemies.bases[&base_id].next_raid_tick, u64::MAX);
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
