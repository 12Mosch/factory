use super::super::*;
use super::support::*;

fn enemy_damage_commands(
    target: EntityId,
    amounts: impl IntoIterator<Item = u32>,
) -> CombatCommandBuffer {
    let mut commands = CombatCommandBuffer::default();
    for amount in amounts {
        commands.push(CombatCommand {
            source: CombatSource {
                owner: CombatantId::Enemy(EnemyId::new(u64::MAX)),
                faction: Faction::Enemy,
            },
            target: CombatantId::Entity(target),
            damage: Damage::physical(amount),
        });
    }
    commands
}

fn chunk_of_entity(sim: &Simulation, entity_id: EntityId) -> ChunkCoord {
    let placed = sim
        .entities
        .placed_entity(entity_id)
        .expect("test entity should be placed");
    ChunkCoord::from_tile(placed.x, placed.y).expect("test entity should be in the chunk plane")
}

fn place_biter_spawner(sim: &mut Simulation) -> EntityId {
    let spawner = entity_id_by_name(&sim.world.prototypes, "biter_spawner");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 8, 8);
    // Center of the clear rect keeps room for spawned units on every side.
    place_at(sim, spawner, x + 3, y + 3, Direction::North)
}

fn load_turret_ammo(sim: &mut Simulation, turret_id: EntityId, count: u16) {
    let magazine = item_id_by_name(&sim.world.prototypes, "firearm_magazine");
    let catalog = sim.world.prototypes.clone();
    crate::entity_access::inventory_mut(sim, turret_id)
        .expect("turret should expose its ammo inventory")
        .insert(&catalog, magazine, count)
        .expect("turret ammo inventory should accept magazines");
}

fn spawn_test_enemy_at(sim: &mut Simulation, x: WorldTileCoord, y: WorldTileCoord) -> EnemyId {
    let unit = sim
        .world
        .prototypes
        .entity(entity_id_by_name(&sim.world.prototypes, "biter_spawner"))
        .and_then(|prototype| prototype.enemy_spawner.as_ref())
        .expect("biter spawner prototype should define a unit")
        .unit;
    let id = sim.enemies.allocate_id();
    sim.enemies.enemies.insert(
        id,
        Enemy {
            id,
            x: x * POSITION_SCALE + POSITION_SCALE / 2,
            y: y * POSITION_SCALE + POSITION_SCALE / 2,
            health: HealthState::new(unit.max_health, Faction::Enemy),
            attack: AttackDefinition::melee(
                Damage::physical(unit.damage),
                unit.attack_cooldown_ticks,
                1,
            ),
            speed_fixed_per_tick: unit.speed_fixed_per_tick,
            aggro_radius_tiles: unit.aggro_radius_tiles,
            mode: EnemyMode::Attack,
            mission: EnemyMission::Guard,
            home_spawner: None,
            target: None,
            path: VecDeque::new(),
            next_attack_tick: 0,
            next_decision_tick: 0,
        },
    );
    id
}

#[test]
fn working_furnace_emits_pollution_into_its_chunk() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id_by_name(&sim.world.prototypes, "iron_ore");
    let coal = item_id_by_name(&sim.world.prototypes, "coal");
    let furnace_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, furnace_id, iron_ore, coal);
    let coord = chunk_of_entity(&sim, furnace_id);

    for _ in 0..30 {
        sim.tick();
    }

    assert!(
        sim.pollution().amount_micro(coord) > 0,
        "working furnace should pollute its chunk"
    );
}

#[test]
fn idle_furnace_emits_no_pollution() {
    let mut sim = Simulation::new_test_world(123);
    let furnace_id = place_stone_furnace(&mut sim);
    let coord = chunk_of_entity(&sim, furnace_id);

    for _ in 0..30 {
        sim.tick();
    }

    assert_eq!(sim.pollution().amount_micro(coord), 0);
}

#[test]
fn pollution_emitter_index_tracks_placement_removal_and_work() {
    let mut sim = Simulation::new_test_world(123);
    let furnace_id = place_stone_furnace(&mut sim);
    let emitter = sim
        .pollution_emitters
        .emitters
        .get(&furnace_id)
        .expect("polluting prototype should be indexed on placement");
    assert_eq!(emitter.chunk, chunk_of_entity(&sim, furnace_id));
    assert!(!emitter.active, "new idle emitter should not be active");

    let iron_ore = item_id_by_name(&sim.world.prototypes, "iron_ore");
    let coal = item_id_by_name(&sim.world.prototypes, "coal");
    add_furnace_input_and_fuel(&mut sim, furnace_id, iron_ore, coal);
    sim.tick();
    assert!(sim.pollution_emitters.emitters[&furnace_id].active);

    crate::entity_mutation::remove(&mut sim, furnace_id)
        .expect("placed emitter should be removable");
    assert!(!sim.pollution_emitters.emitters.contains_key(&furnace_id));
    assert!(!sim.pollution_emitters.active_emitters.contains(&furnace_id));
    assert!(
        !sim.pollution
            .machine_emission_remainders
            .contains_key(&furnace_id),
        "removal should discard the emitter's fractional carry"
    );
}

#[test]
fn machine_emission_conserves_a_low_rate_over_one_minute_and_save_load() {
    let mut prototypes = PrototypeCatalog::load_base().expect("base prototypes should load");
    let assembler = entity_id_by_name(&prototypes, "assembling_machine");
    for prototype in &mut prototypes.entities {
        prototype.pollution_per_minute_milli = None;
    }
    prototypes.entities[assembler.index()].pollution_per_minute_milli = Some(1);
    let mut sim = Simulation::new(123, prototypes);
    let assembler_id = place_assembling_machine(&mut sim);
    assert_eq!(
        sim.pollution_emitters.emitters.len(),
        1,
        "non-polluting power infrastructure should stay out of the index"
    );
    add_assembler_gear_job(&mut sim, assembler_id);
    sim.tick();
    assert_eq!(
        sim.machine_status_for_entity(assembler_id),
        Some(MachineStatus::Working)
    );

    sim.pollution = PollutionState::default();
    sim.emit_pollution_from_machines();
    assert_eq!(sim.pollution().total_micro(), 0);

    let bytes = save_to_bytes(&sim).expect("fractional emission should save");
    let mut loaded = load_from_bytes(&bytes).expect("fractional emission should load");
    assert_eq!(sim.state_hash(), loaded.state_hash());

    for _ in 1..crate::pollution::POLLUTION_TICKS_PER_MINUTE {
        loaded.emit_pollution_from_machines();
    }

    assert_eq!(
        loaded.pollution().total_micro(),
        1_000,
        "one milli-unit per minute should emit exactly 1,000 micro-units"
    );
}

#[test]
fn pollution_spreads_to_neighbor_chunks_at_interval() {
    let mut sim = Simulation::new_test_world(123);
    let center = ChunkCoord { x: 0, y: 0 };
    let seeded = 10_000_000;
    sim.add_pollution_micro(center, seeded);

    for _ in 0..POLLUTION_SPREAD_INTERVAL_TICKS {
        sim.tick();
    }

    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neighbor = ChunkCoord {
            x: center.x + dx,
            y: center.y + dy,
        };
        assert!(
            sim.pollution().amount_micro(neighbor) > 0,
            "pollution should spread to neighbor {neighbor:?}"
        );
    }
    assert!(
        sim.pollution().amount_micro(center) < seeded,
        "spreading chunk should lose the shared pollution"
    );
}

fn spread_pollution_reference(chunks: &mut BTreeMap<ChunkCoord, u64>, world: &WorldSim) {
    let snapshot = chunks
        .iter()
        .filter(|(_, amount)| **amount >= POLLUTION_MIN_TO_SPREAD_MICRO)
        .map(|(coord, amount)| (*coord, *amount))
        .collect::<Vec<_>>();

    for (coord, amount) in snapshot {
        let share = amount / 1000 * POLLUTION_SPREAD_PER_NEIGHBOR_PERMILLE;
        if share == 0 {
            continue;
        }

        let mut moved = 0;
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (Some(x), Some(y)) = (coord.x.checked_add(dx), coord.y.checked_add(dy)) else {
                continue;
            };
            let destination = ChunkCoord { x, y };
            if !world.chunks.contains_key(&destination) {
                continue;
            }
            let amount = chunks.entry(destination).or_default();
            *amount = amount.saturating_add(share);
            moved += share;
        }

        let remove_source = if let Some(amount) = chunks.get_mut(&coord) {
            *amount = amount.saturating_sub(moved);
            *amount == 0
        } else {
            false
        };
        if remove_source {
            chunks.remove(&coord);
        }
    }
}

#[test]
fn buffered_pollution_diffusion_matches_ordered_updates_exactly() {
    let seeded = BTreeMap::from([
        (ChunkCoord { x: -1, y: 0 }, u64::MAX - 5),
        (ChunkCoord { x: 0, y: 0 }, u64::MAX - 10),
        (ChunkCoord { x: 1, y: 0 }, 10_000_000),
        (ChunkCoord { x: 0, y: 1 }, POLLUTION_MIN_TO_SPREAD_MICRO),
        (
            ChunkCoord {
                x: i32::MIN,
                y: i32::MIN,
            },
            1_000_000,
        ),
        (
            ChunkCoord {
                x: i32::MAX,
                y: i32::MAX,
            },
            2_000_000,
        ),
        (ChunkCoord { x: 4, y: 4 }, 99_999),
    ]);
    let mut expected = seeded.clone();
    let mut sim = Simulation::new_test_world(123);
    sim.pollution.chunks = seeded;

    for _ in 0..2 {
        spread_pollution_reference(&mut expected, &sim.world);
        sim.spread_pollution_to_neighbors();
        assert_eq!(sim.pollution.chunks, expected);
        assert!(sim.pollution_diffusion.deltas.is_empty());
        assert!(sim.pollution_diffusion.ordered_deltas.is_empty());
    }
}

#[test]
fn pollution_does_not_spread_beyond_generated_chunks() {
    let mut sim = Simulation::new_test_world(123);
    let area = sim.world.prototypes.world_generation.starting_area;
    let source = ChunkCoord {
        x: area.max_chunk,
        y: area.max_chunk,
    };
    let outside = [
        ChunkCoord {
            x: source.x + 1,
            y: source.y,
        },
        ChunkCoord {
            x: source.x,
            y: source.y + 1,
        },
    ];
    let seeded = 10_000_000;
    sim.add_pollution_micro(source, seeded);

    sim.spread_pollution_to_neighbors();

    assert_eq!(sim.pollution().total_micro(), seeded);
    for &coord in &outside {
        assert_eq!(sim.pollution().amount_micro(coord), 0);
    }
    assert!(sim.ensure_chunk_generated(outside[0]));
    assert_eq!(sim.pollution().amount_micro(outside[0]), 0);
    assert!(
        sim.pollution()
            .polluted_chunks()
            .all(|(coord, _)| sim.world.chunks.contains_key(&coord)),
        "diffusion should only create pollution entries for generated chunks"
    );
}

#[test]
fn terrain_absorbs_pollution_over_time() {
    let mut sim = Simulation::new_test_world(123);
    // Below the spread threshold, so only absorption changes the amount.
    let seeded = 50_000;
    let center = ChunkCoord { x: 0, y: 0 };
    sim.add_pollution_micro(center, seeded);
    let total_before = sim.pollution().total_micro();

    for _ in 0..POLLUTION_SPREAD_INTERVAL_TICKS {
        sim.tick();
    }

    assert!(
        sim.pollution().total_micro() < total_before,
        "terrain should absorb pollution"
    );
}

#[test]
fn terrain_absorption_conserves_its_rate_over_eight_minutes() {
    let mut prototypes = PrototypeCatalog::load_base().expect("base prototypes should load");
    for tile in &mut prototypes.tiles {
        tile.pollution_absorption_per_minute_milli = 1;
    }
    let mut sim = Simulation::new(123, prototypes);
    let coord = ChunkCoord { x: 0, y: 0 };
    let tile_count = sim.world.chunks[&coord].tiles.len() as u64;
    let minutes = 8;
    let expected_absorption = tile_count * 1_000 * minutes;
    let seeded = expected_absorption + 1;
    sim.add_pollution_micro(coord, seeded);

    let elapsed_ticks = crate::pollution::POLLUTION_TICKS_PER_MINUTE * minutes;
    assert!(elapsed_ticks.is_multiple_of(POLLUTION_SPREAD_INTERVAL_TICKS));
    for _ in 0..elapsed_ticks / POLLUTION_SPREAD_INTERVAL_TICKS {
        sim.absorb_pollution_by_terrain();
    }

    assert_eq!(
        seeded - sim.pollution().amount_micro(coord),
        expected_absorption,
        "terrain should preserve its configured per-minute absorption"
    );
}

#[test]
fn enemy_spawners_seed_in_distant_chunks_but_not_near_spawn() {
    let mut sim = Simulation::new_test_world(123);
    assert!(
        sim.entities.enemy_spawners.is_empty(),
        "starting area should stay clear of spawners"
    );

    for x in 4..12 {
        for y in 4..12 {
            sim.ensure_chunk_generated(ChunkCoord { x, y });
        }
    }

    assert!(
        !sim.entities.enemy_spawners.is_empty(),
        "distant chunks should contain enemy spawners"
    );
    for spawner_id in sim.entities.enemy_spawners.keys() {
        let placed = sim
            .entities
            .placed_entities
            .get(spawner_id)
            .expect("spawner should be placed");
        let distance_squared =
            EntityFootprint::single_tile(0, 0).distance_squared_to(&placed.footprint);
        let safe_radius = u128::from(sim.enemy_settings().world.starting_safe_radius_tiles);
        assert!(
            distance_squared >= safe_radius * safe_radius,
            "every spawner footprint should stay outside the starting safe radius"
        );
    }
    sim.validate().expect("seeded world should stay valid");
}

#[test]
fn spawner_spawns_guard_without_pollution() {
    let mut sim = Simulation::new_test_world(123);
    place_biter_spawner(&mut sim);

    sim.tick();

    assert_eq!(sim.enemies().len(), 1);
    let guard = sim.enemies().iter().next().expect("guard should exist");
    assert_eq!(guard.mode, EnemyMode::Guard);
}

#[test]
fn spawner_converts_absorbed_pollution_into_attackers() {
    let mut sim = Simulation::new_test_world(123);
    let spawner_id = place_biter_spawner(&mut sim);
    let coord = chunk_of_entity(&sim, spawner_id);
    // Seed well above the 4000 milli unit cost: chunk spread and terrain
    // absorption also drain the chunk while the spawner soaks it up.
    sim.add_pollution_micro(coord, 12_000_000);

    // The spawner drains 20 milli per tick, so absorbing the unit cost
    // takes at least 200 ticks; run with headroom.
    let mut attacker_spawned = false;
    for _ in 0..400 {
        sim.tick();
        if sim
            .enemies()
            .iter()
            .any(|enemy| enemy.mode == EnemyMode::Attack)
        {
            attacker_spawned = true;
            break;
        }
    }

    assert!(
        attacker_spawned,
        "absorbed pollution should produce an attacker"
    );
    assert!(
        sim.pollution().amount_micro(coord) < 12_000_000,
        "spawner should have drained chunk pollution"
    );
}

#[test]
fn blocked_spawner_preserves_attack_budget_when_enemy_spawn_fails() {
    let mut sim = Simulation::new_test_world(123);
    let spawner_id = place_biter_spawner(&mut sim);
    let placed = sim
        .entities
        .placed_entity(spawner_id)
        .expect("spawner should be placed");
    let footprint = placed.footprint;
    let spawner_config = sim
        .world
        .prototypes
        .entity(placed.prototype_id)
        .and_then(|prototype| prototype.enemy_spawner.as_ref())
        .expect("spawner should define enemy spawning");
    let attack_cost = u64::from(spawner_config.unit_spawn_pollution_cost_milli) * 1000;
    let base_id = sim.enemies.spawner_bases[&spawner_id];
    sim.enemies
        .bases
        .get_mut(&base_id)
        .unwrap()
        .attack_budget_micro = attack_cost;

    // Occupy every tile the spawner's deterministic three-ring search can
    // inspect. Reusing the spawner ID is sufficient for this placement-only
    // regression and avoids adding unrelated simulation entities.
    for y in footprint.y - 3..footprint.y + i64::from(footprint.height) + 3 {
        for x in footprint.x - 3..footprint.x + i64::from(footprint.width) + 3 {
            sim.entities
                .occupancy
                .occupied_tiles
                .insert((x, y), spawner_id);
        }
    }

    sim.advance_enemy_spawners();

    assert!(
        sim.enemies().is_empty(),
        "blocked spawner must not create a unit"
    );
    assert_eq!(
        sim.enemies.bases[&base_id].attack_budget_micro, attack_cost,
        "failed placement must not consume the colony's attack budget"
    );
}

#[test]
fn dead_staged_members_are_pruned_before_raid_launch() {
    let mut sim = Simulation::new_test_world(123);
    let spawner_id = place_biter_spawner(&mut sim);
    let base_id = sim.enemies.spawner_bases[&spawner_id];
    let raid_target_size = sim.raid_target_size();
    let base = sim.enemies.bases.get_mut(&base_id).unwrap();
    base.staged_units = (1..=u64::from(raid_target_size))
        .map(|offset| EnemyId::new(u64::MAX - offset))
        .collect();
    base.staging_started_tick = Some(0);
    base.next_raid_tick = 0;

    sim.advance_enemy_spawners();

    assert!(sim.enemies.raids.is_empty());
    assert!(sim.enemies.bases[&base_id].staged_units.is_empty());
    assert_eq!(sim.enemies.bases[&base_id].next_raid_tick, 0);
    assert!(
        !sim.enemies
            .threat_events
            .iter()
            .any(|event| event.kind == ThreatEventKind::RaidLaunched)
    );
}

#[test]
fn queued_guard_and_staging_spawns_respect_spawner_alive_cap() {
    let mut sim = Simulation::new_test_world(123);
    let spawner_id = place_biter_spawner(&mut sim);
    let placed = sim.entities.placed_entities[&spawner_id].clone();
    let config = sim.world.prototypes.entities[placed.prototype_id.index()]
        .enemy_spawner
        .as_ref()
        .unwrap();
    let max_alive = config.max_alive_units;
    let attack_cost = u64::from(config.unit_spawn_pollution_cost_milli) * 1_000;
    let base_id = sim.enemies.spawner_bases[&spawner_id];
    sim.enemies
        .bases
        .get_mut(&base_id)
        .unwrap()
        .attack_budget_micro = attack_cost;

    for offset in 0..max_alive - 1 {
        let id = spawn_test_enemy_at(&mut sim, placed.x + i64::from(offset), placed.y + 8);
        sim.enemies.enemies.get_mut(&id).unwrap().home_spawner = Some(spawner_id);
    }

    sim.advance_enemy_spawners();

    let alive = sim
        .enemies
        .enemies
        .values()
        .filter(|unit| unit.home_spawner == Some(spawner_id))
        .count();
    assert_eq!(alive, max_alive as usize);
    assert_eq!(
        sim.enemies.bases[&base_id].attack_budget_micro, attack_cost,
        "the staging request should be suppressed after the projected guard reaches the cap"
    );
}

#[test]
fn excessive_attack_budget_is_reported_by_diagnostics_and_validation() {
    let mut sim = Simulation::new_test_world(123);
    let spawner_id = place_biter_spawner(&mut sim);
    let base_id = sim.enemies.spawner_bases[&spawner_id];
    let cap = sim
        .attack_budget_cap(base_id)
        .expect("placed spawner should define an attack-budget cap");
    sim.enemies
        .bases
        .get_mut(&base_id)
        .unwrap()
        .attack_budget_micro = cap + 1;

    assert_eq!(
        sim.capacity_diagnostics()
            .attack_budgets_over_practical_limit,
        1
    );
    assert_eq!(
        sim.validate(),
        Err(SimValidationError::AttackBudgetCapacityExceeded { base_id })
    );
}

#[test]
fn biter_destroys_nearby_building() {
    let mut sim = Simulation::new_test_world(123);
    let spawner_id = place_biter_spawner(&mut sim);
    let spawner = sim
        .entities
        .placed_entity(spawner_id)
        .expect("spawner should be placed");
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let chest_x = spawner.x + 4;
    let chest_y = spawner.y;
    let chest_id = place_at(&mut sim, chest, chest_x, chest_y, Direction::North);

    let mut destroyed = false;
    for _ in 0..1500 {
        sim.tick();
        if sim.entities.placed_entity(chest_id).is_none() {
            destroyed = true;
            break;
        }
    }

    assert!(destroyed, "guard biter should destroy the nearby chest");
    assert!(
        sim.entities.occupancy.entity_at(chest_x, chest_y).is_none(),
        "destroyed chest should release its tile"
    );
    sim.validate()
        .expect("simulation should stay valid after destruction");
}

#[test]
fn gun_turret_kills_enemy_and_consumes_ammo() {
    let mut sim = Simulation::new_test_world(123);
    let turret = entity_id_by_name(&sim.world.prototypes, "gun_turret");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 6, 6);
    let turret_id = place_at(&mut sim, turret, x, y, Direction::North);
    load_turret_ammo(&mut sim, turret_id, 2);
    let enemy_id = spawn_test_enemy_at(&mut sim, x + 4, y + 4);

    let mut killed = false;
    for _ in 0..200 {
        sim.tick();
        if sim.enemies().get(enemy_id).is_none() {
            killed = true;
            break;
        }
    }

    assert!(killed, "turret should kill the enemy in range");
    let state = sim
        .entities
        .gun_turrets
        .get(&turret_id)
        .expect("turret state should exist");
    let remaining_magazines: u32 = state
        .ammo
        .slots()
        .iter()
        .flatten()
        .map(|stack| u32::from(stack.count()))
        .sum();
    assert!(
        remaining_magazines < 2 || state.loaded_shots < 10,
        "firing should consume ammo"
    );
}

#[test]
fn enemy_and_turret_attacks_resolve_simultaneously() {
    let mut sim = Simulation::new_test_world(123);
    let turret = entity_id_by_name(&sim.world.prototypes, "gun_turret");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 4, 3);
    let turret_id = place_at(&mut sim, turret, x, y, Direction::North);
    load_turret_ammo(&mut sim, turret_id, 1);
    sim.entities
        .entity_health
        .get_mut(&turret_id)
        .expect("turret should have health")
        .current = 15;

    let enemy_id = spawn_test_enemy_at(&mut sim, x + 2, y);
    sim.enemies
        .enemies
        .get_mut(&enemy_id)
        .expect("enemy should exist")
        .health
        .current = 5;

    sim.tick();

    assert!(
        sim.entities.placed_entity(turret_id).is_none(),
        "the enemy's committed attack should destroy the turret"
    );
    assert!(
        sim.enemies().get(enemy_id).is_none(),
        "the destroyed turret's committed shot should still kill the enemy"
    );
    sim.validate()
        .expect("simultaneous combat resolution should preserve validity");
}

#[test]
fn turret_targeting_uses_one_combat_snapshot_regardless_of_placement_order() {
    fn run_scenario(place_exclusive_turret_first: bool) -> (u32, u32, u32) {
        let mut sim = Simulation::new_test_world(123);
        let turret = entity_id_by_name(&sim.world.prototypes, "gun_turret");
        let (x, y) = first_buildable_rect_without_resource(&sim.world, 8, 2);
        let exclusive_position = (x, y);
        let flexible_position = (x + 6, y);

        let (exclusive_turret, flexible_turret) = if place_exclusive_turret_first {
            (
                place_at(
                    &mut sim,
                    turret,
                    exclusive_position.0,
                    exclusive_position.1,
                    Direction::North,
                ),
                place_at(
                    &mut sim,
                    turret,
                    flexible_position.0,
                    flexible_position.1,
                    Direction::North,
                ),
            )
        } else {
            let flexible_turret = place_at(
                &mut sim,
                turret,
                flexible_position.0,
                flexible_position.1,
                Direction::North,
            );
            let exclusive_turret = place_at(
                &mut sim,
                turret,
                exclusive_position.0,
                exclusive_position.1,
                Direction::North,
            );
            (exclusive_turret, flexible_turret)
        };
        load_turret_ammo(&mut sim, exclusive_turret, 1);
        load_turret_ammo(&mut sim, flexible_turret, 1);

        // Both turrets prefer the primary target, but only the flexible turret
        // can reach the secondary target. Immediate damage would let the
        // flexible turret retarget only when it happened to have the later ID.
        let primary_enemy = spawn_test_enemy_at(&mut sim, x + 3, y);
        sim.enemies
            .enemies
            .get_mut(&primary_enemy)
            .expect("primary enemy should exist")
            .health
            .current = 5;
        let secondary_enemy = spawn_test_enemy_at(&mut sim, x + 15, y);

        let mut commands = CombatCommandBuffer::default();
        sim.advance_gun_turrets(&mut commands);
        sim.resolve_combat_commands(commands);

        assert!(
            sim.enemies().get(primary_enemy).is_none(),
            "the simultaneous volley should kill the primary target"
        );
        let secondary_health = sim
            .enemies()
            .get(secondary_enemy)
            .expect("secondary enemy should not be targeted")
            .health
            .current;
        let exclusive_shots = sim.entities.gun_turrets[&exclusive_turret].loaded_shots;
        let flexible_shots = sim.entities.gun_turrets[&flexible_turret].loaded_shots;
        (secondary_health, exclusive_shots, flexible_shots)
    }

    let exclusive_first = run_scenario(true);
    let flexible_first = run_scenario(false);

    assert_eq!(
        exclusive_first, flexible_first,
        "changing placement order must not change targeting or ammo consumption"
    );
    assert_eq!(exclusive_first, (30, 9, 9));
}

#[test]
fn unloaded_turret_does_not_fire() {
    let mut sim = Simulation::new_test_world(123);
    let turret = entity_id_by_name(&sim.world.prototypes, "gun_turret");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 6, 6);
    place_at(&mut sim, turret, x, y, Direction::North);
    let enemy_id = spawn_test_enemy_at(&mut sim, x + 4, y + 4);

    for _ in 0..120 {
        sim.tick();
    }

    let enemy = sim.enemies().get(enemy_id).expect("enemy should survive");
    assert_eq!(enemy.health.current, enemy.health.maximum);
}

#[test]
fn gun_turret_destroys_spawner_in_range() {
    let mut sim = Simulation::new_test_world(123);
    let spawner_id = place_biter_spawner(&mut sim);
    let spawner = sim
        .entities
        .placed_entity(spawner_id)
        .expect("spawner should be placed");
    let turret = entity_id_by_name(&sim.world.prototypes, "gun_turret");
    let (turret_x, turret_y) = (spawner.x + 6, spawner.y);
    let turret_id = place_at(&mut sim, turret, turret_x, turret_y, Direction::North);
    load_turret_ammo(&mut sim, turret_id, 40);

    let mut destroyed = false;
    for _ in 0..3000 {
        sim.tick();
        if sim.entities.placed_entity(spawner_id).is_none() {
            destroyed = true;
            break;
        }
    }

    assert!(destroyed, "turret creep should clear the nest");
    sim.validate()
        .expect("simulation should stay valid after nest destruction");
}

#[test]
fn gun_turret_range_reaches_nearest_spawner_footprint_edge() {
    let mut sim = Simulation::new_test_world(123);
    let turret = entity_id_by_name(&sim.world.prototypes, "gun_turret");
    let spawner = entity_id_by_name(&sim.world.prototypes, "biter_spawner");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 17, 2);
    let turret_id = place_at(&mut sim, turret, x, y, Direction::North);
    let spawner_id = place_at(&mut sim, spawner, x + 13, y, Direction::North);
    load_turret_ammo(&mut sim, turret_id, 1);

    let health_before = sim.entity_health(spawner_id).unwrap().0;
    let mut commands = CombatCommandBuffer::default();
    sim.advance_gun_turrets(&mut commands);
    sim.resolve_combat_commands(commands);

    assert_eq!(sim.entity_health(spawner_id).unwrap().0, health_before - 5);
}

#[test]
fn walls_take_damage_and_repair_consumes_packs() {
    let mut sim = Simulation::new_test_world(123);
    let wall = entity_id_by_name(&sim.world.prototypes, "wall");
    let repair_pack = item_id_by_name(&sim.world.prototypes, "repair_pack");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 1, 2);
    let wall_id = place_at(&mut sim, wall, x, y, Direction::North);
    sim.player = PlayerState::centered_on_tile(x, y + 1);

    assert_eq!(sim.entity_health(wall_id), Some((350, 350)));
    assert!(!sim.damage_entity(wall_id, 100));
    assert_eq!(sim.entity_health(wall_id), Some((250, 350)));

    let catalog = sim.world.prototypes.clone();
    sim.player_inventory
        .insert(&catalog, repair_pack, 1)
        .expect("player inventory should accept a repair pack");

    for _ in 0..20 {
        sim.repair_entity(wall_id).expect("repair should succeed");
    }

    assert_eq!(sim.entity_health(wall_id), Some((350, 350)));
    assert_eq!(sim.player_inventory.count(repair_pack), 0);
    // Repairing a full-health entity is a no-op success.
    sim.repair_entity(wall_id)
        .expect("full-health repair should be a no-op");
    sim.validate().expect("repair should keep the state valid");
}

#[test]
fn structure_damage_is_aggregated_and_warnings_are_rate_limited_by_region() {
    let mut sim = Simulation::new_test_world(123);
    let wall = entity_id_by_name(&sim.world.prototypes, "wall");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 1, 1);
    let wall_id = place_at(&mut sim, wall, x, y, Direction::North);

    let commands = enemy_damage_commands(wall_id, [10, 20]);
    sim.resolve_combat_commands(commands);

    assert_eq!(sim.entity_health(wall_id), Some((320, 350)));
    let first_warning_sequence = sim.latest_threat_sequence();
    assert_eq!(first_warning_sequence, 1);

    sim.tick += STRUCTURE_WARNING_COOLDOWN_TICKS - 1;
    let commands = enemy_damage_commands(wall_id, [10]);
    sim.resolve_combat_commands(commands);
    assert_eq!(sim.entity_health(wall_id), Some((310, 350)));
    assert_eq!(sim.latest_threat_sequence(), first_warning_sequence);

    sim.tick += 1;
    let commands = enemy_damage_commands(wall_id, [10]);
    sim.resolve_combat_commands(commands);
    assert_eq!(sim.entity_health(wall_id), Some((300, 350)));
    assert_eq!(sim.latest_threat_sequence(), first_warning_sequence + 1);
}

#[test]
fn combat_commands_apply_resistance_per_hit_and_reject_friendly_fire() {
    let mut sim = Simulation::new_test_world(123);
    let wall = entity_id_by_name(&sim.world.prototypes, "wall");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 1, 1);
    let wall_id = place_at(&mut sim, wall, x, y, Direction::North);
    sim.entities
        .entity_health
        .get_mut(&wall_id)
        .unwrap()
        .resistances =
        ResistanceProfile::NONE.with_resistance(DamageType::Physical, Resistance::new(5, 0));

    sim.resolve_combat_commands(enemy_damage_commands(wall_id, [10, 10]));
    assert_eq!(sim.entity_health(wall_id), Some((340, 350)));

    let mut friendly_fire = CombatCommandBuffer::default();
    friendly_fire.push(CombatCommand {
        source: CombatSource::new(CombatantId::Player, Faction::Player),
        target: CombatantId::Entity(wall_id),
        damage: Damage::new(100, DamageType::Fire),
    });
    sim.resolve_combat_commands(friendly_fire);

    assert_eq!(
        sim.entity_health(wall_id),
        Some((340, 350)),
        "allied combatants must not damage one another"
    );
}

#[test]
fn player_is_a_faction_owned_combat_target() {
    let mut sim = Simulation::new_test_world(123);
    let mut commands = CombatCommandBuffer::default();
    commands.push(CombatCommand {
        source: CombatSource::new(CombatantId::Enemy(EnemyId::new(u64::MAX)), Faction::Enemy),
        target: CombatantId::Player,
        damage: Damage::new(25, DamageType::Acid),
    });

    sim.resolve_combat_commands(commands);

    assert_eq!(
        sim.player_health(),
        (PLAYER_MAX_HEALTH - 25, PLAYER_MAX_HEALTH)
    );
    assert_eq!(sim.faction_of(CombatantId::Player), Some(Faction::Player));
    sim.validate()
        .expect("a damaged player should remain a valid combatant");
}

#[test]
fn zero_structure_damage_does_not_emit_a_warning() {
    let mut sim = Simulation::new_test_world(123);
    let wall = entity_id_by_name(&sim.world.prototypes, "wall");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 1, 1);
    let wall_id = place_at(&mut sim, wall, x, y, Direction::North);

    assert!(!sim.damage_entity(wall_id, 0));
    assert_eq!(sim.entity_health(wall_id), Some((350, 350)));
    assert_eq!(sim.latest_threat_sequence(), 0);
}

#[test]
fn repair_requires_reach_and_packs() {
    let mut sim = Simulation::new_test_world(123);
    let wall = entity_id_by_name(&sim.world.prototypes, "wall");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 1, 2);
    let wall_id = place_at(&mut sim, wall, x, y, Direction::North);
    sim.damage_entity(wall_id, 100);

    sim.player = PlayerState::centered_on_tile(x + 40, y);
    assert_eq!(sim.repair_entity(wall_id), Err(RepairError::OutOfReach));

    sim.player = PlayerState::centered_on_tile(x, y + 1);
    sim.player_inventory = Inventory::player();
    assert_eq!(sim.repair_entity(wall_id), Err(RepairError::NoRepairPacks));
}

#[test]
fn destroying_wall_by_damage_drops_nothing() {
    let mut sim = Simulation::new_test_world(123);
    let wall = entity_id_by_name(&sim.world.prototypes, "wall");
    let wall_item = item_id_by_name(&sim.world.prototypes, "wall");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 1, 1);
    let wall_id = place_at(&mut sim, wall, x, y, Direction::North);
    sim.player_inventory = Inventory::player();

    assert!(sim.damage_entity(wall_id, 350));

    assert!(sim.entities.placed_entity(wall_id).is_none());
    assert_eq!(sim.player_inventory.count(wall_item), 0);
    sim.validate()
        .expect("violent destruction should keep the state valid");
}

#[test]
fn wall_and_turret_recipes_unlock_via_research() {
    let mut sim = Simulation::new_test_world(123);
    let wall_recipe = recipe_id(&sim.world.prototypes, "wall");
    let turret_recipe = recipe_id(&sim.world.prototypes, "gun_turret");
    let magazine_recipe = recipe_id(&sim.world.prototypes, "firearm_magazine");
    let repair_recipe = recipe_id(&sim.world.prototypes, "repair_pack");

    assert!(!sim.is_recipe_unlocked(wall_recipe));
    assert!(!sim.is_recipe_unlocked(turret_recipe));
    assert!(sim.is_recipe_unlocked(magazine_recipe));
    assert!(sim.is_recipe_unlocked(repair_recipe));

    complete_research_by_name(&mut sim, "logistics");
    complete_research_by_name(&mut sim, "stone_walls");
    assert!(sim.is_recipe_unlocked(wall_recipe));
    complete_research_by_name(&mut sim, "turrets");
    assert!(sim.is_recipe_unlocked(turret_recipe));
}

#[test]
fn combat_state_round_trips_through_save() {
    let mut sim = Simulation::new_test_world(123);
    let spawner_id = place_biter_spawner(&mut sim);
    let coord = chunk_of_entity(&sim, spawner_id);
    sim.add_pollution_micro(coord, 8_000_000);
    let turret = entity_id_by_name(&sim.world.prototypes, "gun_turret");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 6, 6);
    let turret_id = place_at(&mut sim, turret, x, y, Direction::North);
    load_turret_ammo(&mut sim, turret_id, 5);

    for _ in 0..300 {
        sim.tick();
    }
    // The turret may have shot down everything the spawner produced; add a
    // unit out of turret range so the save definitely covers live enemies.
    spawn_test_enemy_at(&mut sim, x + 30, y + 30);
    assert!(!sim.enemies().is_empty(), "an enemy should be alive");
    assert!(sim.pollution().total_micro() > 0);

    let before_hash = sim.state_hash();
    let bytes = save_to_bytes(&sim).expect("combat state should save");
    let mut loaded = load_from_bytes(&bytes).expect("combat state should load");

    assert_eq!(before_hash, loaded.state_hash());
    for _ in 0..60 {
        sim.tick();
        loaded.tick();
    }
    assert_eq!(
        sim.state_hash(),
        loaded.state_hash(),
        "loaded simulation should stay in lockstep"
    );
}
