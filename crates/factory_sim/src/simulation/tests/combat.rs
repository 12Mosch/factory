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

pub(super) fn spawn_test_enemy_at(
    sim: &mut Simulation,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> EnemyId {
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

/// Inserts a named weapon and compatible ammunition into the test player inventory.
fn give_player_weapon_and_ammo(sim: &mut Simulation, weapon_name: &str, magazines: u16) -> ItemId {
    let weapon = item_id_by_name(&sim.world.prototypes, weapon_name);
    let category = sim
        .world
        .prototypes
        .item(weapon)
        .unwrap()
        .weapon
        .unwrap()
        .ammo_category;
    let magazine = sim
        .world
        .prototypes
        .items()
        .iter()
        .find(|item| item.ammo.is_some_and(|ammo| ammo.category == category))
        .expect("base catalog should contain compatible ammunition")
        .id;
    let catalog = sim.world.prototypes.clone();
    sim.player_inventory
        .insert(&catalog, weapon, 1)
        .expect("player inventory should accept a weapon");
    sim.player_inventory
        .insert(&catalog, magazine, magazines)
        .expect("player inventory should accept ammunition");
    weapon
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
    for prototype in prototypes.entities_mut() {
        prototype.pollution_per_minute_milli = None;
    }
    prototypes.entities_mut()[assembler.index()].pollution_per_minute_milli = Some(1);
    let mut sim = Simulation::new(123, prototypes).unwrap();
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
    let area = sim.world.prototypes.world_generation().starting_area;
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
    assert_eq!(
        sim.ensure_chunk_generated(outside[0]).generated_chunks(),
        &[outside[0]]
    );
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
    for tile in prototypes.tiles_mut() {
        tile.pollution_absorption_per_minute_milli = 1;
    }
    let mut sim = Simulation::new(123, prototypes).unwrap();
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
fn enemy_map_revision_changes_only_on_first_pollution_contact() {
    let mut sim = Simulation::new_test_world(123);
    let spawner_id = place_biter_spawner(&mut sim);
    let coord = chunk_of_entity(&sim, spawner_id);
    sim.add_pollution_micro(coord, 12_000_000);
    let before_contact = sim.enemy_map_revision();

    sim.advance_enemy_spawners();
    let after_contact = sim.enemy_map_revision();
    assert!(after_contact > before_contact);

    sim.advance_enemy_spawners();
    assert_eq!(
        sim.enemy_map_revision(),
        after_contact,
        "continued absorption does not change the contacted-sector marker"
    );
}

#[test]
fn blocked_spawner_preserves_attack_budget_when_enemy_spawn_fails() {
    let mut sim = Simulation::new_test_world(123);
    let spawner_id = place_biter_spawner(&mut sim);
    let footprint = sim
        .entities
        .placed_entity(spawner_id)
        .expect("spawner should be placed")
        .footprint;
    let attack_cost = spawner_attack_cost(&sim, spawner_id);
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
    let max_alive = spawner_max_alive(&sim, spawner_id);
    let attack_cost = spawner_attack_cost(&sim, spawner_id);
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
fn expansion_dispatch_respects_spawner_alive_cap_with_partial_party() {
    let mut sim = Simulation::new_test_world(123);
    let spawner_id = place_biter_spawner(&mut sim);
    let placed = sim.entities.placed_entities[&spawner_id].clone();
    let max_alive = spawner_max_alive(&sim, spawner_id);
    let base_id = sim.enemies.spawner_bases[&spawner_id];
    assert!(max_alive >= 4, "fixture needs room for a 14/15 case");

    // One slot below the cap: a 3-member expansion must be truncated to 1.
    fill_spawner(&mut sim, spawner_id, max_alive - 1, placed.x, placed.y + 8);

    let expansions_before = sim.enemies.expansions.len();
    assert!(
        sim.dispatch_expansion(base_id, (placed.x + 100, placed.y + 100)),
        "a partially formed party still departs"
    );

    assert_eq!(
        alive_for_spawner(&sim, spawner_id),
        max_alive as usize,
        "expansion must stop at max_alive_units"
    );
    assert_eq!(
        sim.enemies.expansions.len(),
        expansions_before + 1,
        "a partially formed party still departs"
    );
    assert_eq!(
        sim.enemies
            .expansions
            .values()
            .last()
            .unwrap()
            .members
            .len(),
        1,
        "only the single remaining slot may be filled"
    );
}

#[test]
fn expansion_dispatch_uses_sibling_spawner_when_first_is_saturated() {
    let mut sim = Simulation::new_test_world(123);
    let first_id = place_biter_spawner(&mut sim);
    let first = sim.entities.placed_entities[&first_id].clone();
    let max_alive = spawner_max_alive(&sim, first_id);
    let base_id = sim.enemies.spawner_bases[&first_id];

    // A second spawner joins the same colony; whichever spawner sorts first
    // is the one the old dispatch always selected.
    let second_id = join_colony(&mut sim, base_id, first.x + 6, first.y, 1)
        .into_iter()
        .next()
        .expect("the colony should accept a second spawner");
    assert_eq!(sim.enemies.bases[&base_id].spawners.len(), 2);

    let saturated_id = *sim.enemies.bases[&base_id]
        .spawners
        .iter()
        .next()
        .expect("the colony should list its spawners");
    let spare_id = if saturated_id == first_id {
        second_id
    } else {
        first_id
    };
    fill_spawner(&mut sim, saturated_id, max_alive, first.x, first.y + 8);

    let expansions_before = sim.enemies.expansions.len();
    assert!(
        sim.dispatch_expansion(base_id, (first.x + 100, first.y + 100)),
        "a colony with a free sibling spawner still expands"
    );

    assert_eq!(
        alive_for_spawner(&sim, saturated_id),
        max_alive as usize,
        "the saturated spawner must not gain expansion members"
    );
    let party = sim
        .enemies
        .expansions
        .values()
        .last()
        .expect("a party should have departed");
    assert_eq!(
        sim.enemies.expansions.len(),
        expansions_before + 1,
        "the sibling spawner should launch the party"
    );
    assert!(
        !party.members.is_empty(),
        "the sibling spawner should launch the party"
    );
    for member in &party.members {
        assert_eq!(
            sim.enemies.enemies[member].home_spawner,
            Some(spare_id),
            "expansion members must belong to the spawner with capacity"
        );
    }
}

#[test]
fn staging_uses_sibling_spawner_when_first_is_saturated() {
    let mut sim = Simulation::new_test_world(123);
    let first_id = place_biter_spawner(&mut sim);
    let first = sim.entities.placed_entities[&first_id].clone();
    let base_id = sim.enemies.spawner_bases[&first_id];
    let attack_cost = spawner_attack_cost(&sim, first_id);

    join_colony(&mut sim, base_id, first.x + 6, first.y, 2);
    assert_eq!(sim.enemies.bases[&base_id].spawners.len(), 3);

    // Deterministic colony order: saturate the lowest-ID spawner so only
    // siblings have capacity.
    let ordered: Vec<EntityId> = sim.enemies.bases[&base_id]
        .spawners
        .iter()
        .copied()
        .collect();
    let cap = spawner_max_alive(&sim, ordered[0]);
    fill_spawner(&mut sim, ordered[0], cap, first.x, first.y + 8);
    // Suppress free guard spawns so the staging decision observes exactly
    // the filled counts.
    for &spawner_id in &ordered {
        sim.entities
            .enemy_spawners
            .get_mut(&spawner_id)
            .expect("colony spawner should track guard timing")
            .next_free_spawn_tick = u64::MAX;
    }
    sim.enemies
        .bases
        .get_mut(&base_id)
        .unwrap()
        .attack_budget_micro = attack_cost;

    sim.advance_enemy_spawners();

    assert_eq!(
        sim.enemies.bases[&base_id].staged_units.len(),
        1,
        "a free sibling spawner must still stage the raid unit"
    );
    let staged_id = *sim.enemies.bases[&base_id]
        .staged_units
        .iter()
        .next()
        .unwrap();
    assert_eq!(
        sim.enemies.enemies[&staged_id].home_spawner,
        Some(ordered[1]),
        "staging must pick the lowest-ID spawner with capacity"
    );
    assert_eq!(
        sim.enemies.bases[&base_id].attack_budget_micro, 0,
        "staging consumes the unit cost once"
    );
}

#[test]
fn staging_stalls_when_all_spawners_saturated() {
    let mut sim = Simulation::new_test_world(123);
    let first_id = place_biter_spawner(&mut sim);
    let first = sim.entities.placed_entities[&first_id].clone();
    let base_id = sim.enemies.spawner_bases[&first_id];
    let attack_cost = spawner_attack_cost(&sim, first_id);

    join_colony(&mut sim, base_id, first.x + 6, first.y, 1);
    assert_eq!(sim.enemies.bases[&base_id].spawners.len(), 2);

    let ordered: Vec<EntityId> = sim.enemies.bases[&base_id]
        .spawners
        .iter()
        .copied()
        .collect();
    for &spawner_id in &ordered {
        let cap = spawner_max_alive(&sim, spawner_id);
        fill_spawner(&mut sim, spawner_id, cap, first.x, first.y + 8);
        sim.entities
            .enemy_spawners
            .get_mut(&spawner_id)
            .expect("colony spawner should track guard timing")
            .next_free_spawn_tick = u64::MAX;
    }
    sim.enemies
        .bases
        .get_mut(&base_id)
        .unwrap()
        .attack_budget_micro = attack_cost;

    sim.advance_enemy_spawners();

    assert!(
        sim.enemies.bases[&base_id].staged_units.is_empty(),
        "a fully saturated colony must not stage additional units"
    );
    assert_eq!(
        sim.enemies.bases[&base_id].attack_budget_micro, attack_cost,
        "a suppressed staging spawn must not consume attack budget"
    );
}

/// Places `extra` spawners into an existing colony near
/// `(from_x, from_y)`, returning their ids in placement order.
fn join_colony(
    sim: &mut Simulation,
    base_id: EnemyBaseId,
    from_x: WorldTileCoord,
    from_y: WorldTileCoord,
    extra: usize,
) -> Vec<EntityId> {
    let prototype = entity_id_by_name(&sim.world.prototypes, "biter_spawner");
    sim.enemies.placement_base = Some(base_id);
    let mut spawners = Vec::new();
    let mut cursor = 0;
    while spawners.len() < extra && cursor < 400 {
        let request = crate::placement::EntityPlacementRequest {
            prototype_id: prototype,
            x: from_x + (cursor % 20),
            y: from_y + (cursor / 20),
            direction: Direction::North,
        };
        if let Ok(id) = crate::placement::place(sim, request) {
            spawners.push(id);
        }
        cursor += 1;
    }
    sim.enemies.placement_base = None;
    assert_eq!(
        spawners.len(),
        extra,
        "the colony should accept more spawners"
    );
    spawners
}

/// Spawns `count` units homed to `spawner_id` on tiles east of the origin.
fn fill_spawner(
    sim: &mut Simulation,
    spawner_id: EntityId,
    count: u32,
    origin_x: WorldTileCoord,
    origin_y: WorldTileCoord,
) {
    for offset in 0..count {
        let id = spawn_test_enemy_at(sim, origin_x + i64::from(offset), origin_y);
        sim.enemies.enemies.get_mut(&id).unwrap().home_spawner = Some(spawner_id);
    }
}

/// Live units homed to one spawner.
fn alive_for_spawner(sim: &Simulation, spawner_id: EntityId) -> usize {
    sim.enemies
        .enemies
        .values()
        .filter(|unit| unit.home_spawner == Some(spawner_id))
        .count()
}

/// Retry deadline tuning for a due colony.
fn expansion_retry_ticks(sim: &Simulation) -> u64 {
    u64::from(
        sim.gameplay()
            .expect("the catalog should tune enemy expansion")
            .expansion_retry_ticks,
    )
}

/// Builds the scheduler minimum of three spawners in one colony and
/// generates the surrounding chunk ring, so the expansion site search has
/// deterministic candidates to evaluate.
fn colony_with_three_spawners(sim: &mut Simulation) -> (EnemyBaseId, Vec<EntityId>) {
    let first_id = place_biter_spawner(sim);
    let first = sim.entities.placed_entities[&first_id].clone();
    let base_id = sim.enemies.spawner_bases[&first_id];

    let mut spawners = vec![first_id];
    spawners.extend(join_colony(sim, base_id, first.x + 6, first.y, 2));
    assert_eq!(
        spawners.len(),
        3,
        "the expansion scheduler needs three spawners"
    );

    let anchor = sim.enemies.bases[&base_id].anchor;
    for dx in -5_i32..=5 {
        for dy in -5_i32..=5 {
            if (3..=5).contains(&dx.abs().max(dy.abs())) {
                sim.ensure_chunk_generated(ChunkCoord {
                    x: anchor.x + dx,
                    y: anchor.y + dy,
                });
            }
        }
    }
    (base_id, spawners)
}

fn spawner_max_alive(sim: &Simulation, spawner_id: EntityId) -> u32 {
    let placed = &sim.entities.placed_entities[&spawner_id];
    sim.world.prototypes.entities()[placed.prototype_id.index()]
        .enemy_spawner
        .as_ref()
        .expect("test spawner should define a live-unit ceiling")
        .max_alive_units
}

/// Attack-budget cost in micro units for staging one unit from `spawner_id`.
fn spawner_attack_cost(sim: &Simulation, spawner_id: EntityId) -> u64 {
    let placed = &sim.entities.placed_entities[&spawner_id];
    u64::from(
        sim.world.prototypes.entities()[placed.prototype_id.index()]
            .enemy_spawner
            .as_ref()
            .expect("test spawner should define a unit cost")
            .unit_spawn_pollution_cost_milli,
    ) * 1_000
}

/// Makes the colony old enough and due for expansion without touching the
/// growth schedule.
fn arm_expansion_due(sim: &mut Simulation, base_id: EnemyBaseId) {
    let minimum_age = sim
        .gameplay()
        .expect("the catalog should tune enemy expansion")
        .expansion_minimum_age_ticks;
    sim.tick = u64::from(minimum_age) + 1000;
    let base = sim.enemies.bases.get_mut(&base_id).unwrap();
    base.creation_tick = 0;
    base.next_expansion_tick = 0;
    base.next_growth_tick = u64::MAX;
}

#[test]
fn saturated_colony_defers_expansion_to_retry_ticks() {
    let mut sim = Simulation::new_test_world(123);
    let (base_id, spawners) = colony_with_three_spawners(&mut sim);
    let first = sim.entities.placed_entities[&spawners[0]].clone();
    for &spawner_id in &spawners {
        let cap = spawner_max_alive(&sim, spawner_id);
        fill_spawner(&mut sim, spawner_id, cap, first.x, first.y + 8);
    }
    arm_expansion_due(&mut sim, base_id);
    let retry = expansion_retry_ticks(&sim);
    let tick = sim.tick;

    let expansions_before = sim.enemies.expansions.len();
    sim.advance_enemy_spawners();

    assert_eq!(
        sim.enemies.expansions.len(),
        expansions_before,
        "a saturated colony must not launch an expansion"
    );
    assert_eq!(
        sim.enemies.bases[&base_id].next_expansion_tick,
        tick + retry,
        "a saturated dispatch must defer to the retry deadline"
    );
}

#[test]
fn guard_spawn_fills_last_slot_before_expansion_dispatch() {
    let mut sim = Simulation::new_test_world(123);
    let (base_id, spawners) = colony_with_three_spawners(&mut sim);
    let first = sim.entities.placed_entities[&spawners[0]].clone();
    // Two spawners saturated; the last sits exactly one slot below the cap
    // with attack-mode units only, so this tick's free guard spawn fills its
    // last slot before the expansion scheduler runs.
    for (index, &spawner_id) in spawners.iter().enumerate() {
        let cap = spawner_max_alive(&sim, spawner_id);
        let fill = if index + 1 == spawners.len() {
            cap - 1
        } else {
            cap
        };
        fill_spawner(&mut sim, spawner_id, fill, first.x, first.y + 8);
    }
    arm_expansion_due(&mut sim, base_id);
    let retry = expansion_retry_ticks(&sim);
    let tick = sim.tick;
    let last = *spawners.last().unwrap();
    let cap = spawner_max_alive(&sim, last);

    let expansions_before = sim.enemies.expansions.len();
    sim.advance_enemy_spawners();

    assert_eq!(
        alive_for_spawner(&sim, last),
        cap as usize,
        "the guard fills the last slot and no expansion member may follow"
    );
    assert_eq!(
        sim.enemies.expansions.len(),
        expansions_before,
        "scheduler dispatch must observe the same-tick guard spawn and defer"
    );
    assert_eq!(
        sim.enemies.bases[&base_id].next_expansion_tick,
        tick + retry,
        "a colony saturated mid-tick must defer to the retry deadline"
    );
}

#[test]
fn colony_with_capacity_dispatches_expansion_on_schedule() {
    let mut sim = Simulation::new_test_world(123);
    let (base_id, _spawners) = colony_with_three_spawners(&mut sim);
    arm_expansion_due(&mut sim, base_id);
    let cfg = *sim
        .gameplay()
        .expect("the catalog should tune enemy expansion");
    let percent = sim.config.runtime.expansion_frequency_percent;
    assert_ne!(percent, 0, "the test preset should scale expansion time");
    let expected = sim.tick
        + (u64::from(cfg.expansion_interval_ticks) * 100)
            .div_ceil(u64::from(percent))
            .max(1);
    let tick = sim.tick;

    let expansions_before = sim.enemies.expansions.len();
    sim.advance_enemy_spawners();

    assert_eq!(
        sim.enemies.expansions.len(),
        expansions_before + 1,
        "a colony with capacity needs a reachable site to dispatch"
    );
    assert_eq!(
        sim.enemies.bases[&base_id].next_expansion_tick, expected,
        "a successful dispatch must schedule the normal interval, not the retry"
    );
    assert_ne!(
        expected,
        tick + u64::from(cfg.expansion_retry_ticks),
        "the fixture must distinguish the normal interval from the retry"
    );
}

/// Regression test for https://github.com/12Mosch/factory/issues/307:
/// expansion parties must keep their destination authoritative and never pick
/// up ordinary global attack targets en route, even once every member has
/// passed its initial decision stagger (`enemy.id % 16` ticks).
#[test]
fn expansion_members_ignore_global_attack_targets_en_route() {
    let mut sim = Simulation::new_test_world(123);
    let spawner = place_biter_spawner(&mut sim);
    let base_id = sim.enemies.spawner_bases[&spawner];
    let origin = sim.entities.placed_entities[&spawner].clone();
    // Global attack candidate far enough west that idle guards (aggro radius
    // 12 around the spawner) never engage it, yet visible to world-wide
    // `Attack` targeting: only the expansion party's behavior may touch it.
    // The eastern expansion route stays clear.
    let chest_prototype = entity_id_by_name(&sim.world.prototypes, "chest");
    let mut chest_spot: Option<(WorldTileCoord, WorldTileCoord)> = None;
    for dy in -20..=20_i64 {
        for dx in -22..=-18_i64 {
            let (x, y) = (origin.x + dx, origin.y + dy);
            if let Some(chunk) = ChunkCoord::from_tile(x, y) {
                sim.ensure_chunk_generated(chunk);
            }
            if crate::placement::validate(
                &sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: chest_prototype,
                    x,
                    y,
                    direction: Direction::North,
                },
            )
            .is_ok()
            {
                chest_spot = Some((x, y));
                break;
            }
        }
        if chest_spot.is_some() {
            break;
        }
    }
    let (chest_x, chest_y) =
        chest_spot.expect("the fixture needs a chest tile west of the spawner");
    let chest = place_at(
        &mut sim,
        chest_prototype,
        chest_x,
        chest_y,
        Direction::North,
    );
    assert!(
        sim.entities.entity_health.contains_key(&chest),
        "the chest must be damageable so global attack targeting can see it"
    );
    // Nearby destination on a straight, unobstructed row: close enough to
    // stay en route for the whole window at 40 fixed units per tick, far
    // enough that no member can arrive within 64 ticks. Several rows are
    // tried because the spawner footprint, the chest, water, or resources
    // may block any single one.
    let mut route: Option<(WorldTileCoord, WorldTileCoord)> = None;
    for dy in 0..8_i64 {
        let y = origin.y + dy;
        for dx in 0..=24_i64 {
            if let Some(chunk) = ChunkCoord::from_tile(origin.x + dx, y) {
                sim.ensure_chunk_generated(chunk);
            }
        }
        // The 2x2 spawner footprint covers the row start, so candidate
        // segments begin two tiles east of the spawner anchor.
        let clear = |x: WorldTileCoord| {
            sim.world.tile_at(x, y).is_some_and(|tile| {
                tile.collision.walkable
                    && tile.resource.is_none()
                    && sim.entities.occupancy.entity_at(x, y).is_none()
            })
        };
        if let Some(dx_end) = (12..24).find(|&dx_end| (2..=dx_end).all(|dx| clear(origin.x + dx))) {
            route = Some((origin.x + dx_end, y));
            break;
        }
    }
    let destination = route.expect("the fixture needs a clear row east of the spawner");
    assert!(
        sim.dispatch_expansion(base_id, destination),
        "the fixture must dispatch an expansion"
    );
    let (expansion_id, members): (ExpansionId, Vec<EnemyId>) = sim
        .enemies
        .expansions
        .iter()
        .find(|(_, party)| party.base_id == base_id)
        .map(|(&id, party)| (id, party.members.iter().copied().collect()))
        .expect("the fixture must dispatch an expansion");
    assert!(!members.is_empty());
    let start = members
        .iter()
        .map(|id| {
            let unit = &sim.enemies.enemies[id];
            (unit.tile().0 - destination.0).abs() + (unit.tile().1 - destination.1).abs()
        })
        .min()
        .expect("the party must have members");

    let chest_health = sim.entities.entity_health[&chest].current;
    // Past the 0-15 tick decision stagger: any ordinary global targeting
    // would have retargeted the party by now. The target is checked every
    // tick (not just at the end) because a diverted party can destroy the
    // chest and end up targetless again.
    for _ in 0..64 {
        sim.tick();
        for id in &members {
            let unit = sim
                .enemies
                .enemies
                .get(id)
                .expect("expansion members face no damage on this route");
            assert_eq!(
                unit.target, None,
                "expansion member {id:?} must not acquire a global attack target"
            );
        }
    }
    assert_eq!(
        sim.entities
            .entity_health
            .get(&chest)
            .map(|health| health.current),
        Some(chest_health),
        "the expansion party must leave the nearby player structure alone"
    );

    let party = sim
        .enemies
        .expansions
        .get(&expansion_id)
        .expect("the expansion must still be active en route");
    assert_eq!(
        party.destination, destination,
        "the expansion destination must stay authoritative"
    );
    for id in &members {
        assert!(
            party.members.contains(id),
            "expansion member {id:?} must not abandon the party"
        );
        let unit = sim
            .enemies
            .enemies
            .get(id)
            .expect("expansion members face no damage on this route");
        assert_eq!(
            unit.mission,
            EnemyMission::Expansion(expansion_id),
            "expansion member {id:?} must keep the expansion mission"
        );
        assert_eq!(
            unit.target, None,
            "expansion member {id:?} must not acquire a global attack target"
        );
    }
    let closest = members
        .iter()
        .map(|id| {
            let unit = &sim.enemies.enemies[id];
            (unit.tile().0 - destination.0).abs() + (unit.tile().1 - destination.1).abs()
        })
        .min()
        .expect("the party must have members");
    assert!(
        closest < start,
        "the unobstructed party must keep moving toward its destination"
    );
}

/// Places a chest on the first validated tile ringing `(cx, cy)`, ensuring
/// the chunk first so fixed offsets near rect edges cannot leave generated
/// area.
fn place_chest_near(sim: &mut Simulation, cx: WorldTileCoord, cy: WorldTileCoord) -> EntityId {
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    for ring in 1..=10_i64 {
        for dy in -ring..=ring {
            for dx in -ring..=ring {
                if dx.abs().max(dy.abs()) != ring {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if let Some(chunk) = ChunkCoord::from_tile(x, y) {
                    sim.ensure_chunk_generated(chunk);
                }
                if crate::placement::validate(
                    sim,
                    crate::placement::EntityPlacementRequest {
                        prototype_id: chest,
                        x,
                        y,
                        direction: Direction::North,
                    },
                )
                .is_ok()
                {
                    return place_at(sim, chest, x, y, Direction::North);
                }
            }
        }
    }
    panic!("expected a placeable chest tile near {cx},{cy}");
}

fn assert_converted_to_guard(sim: &Simulation, id: EnemyId, context: &str) {
    let unit = &sim.enemies.enemies[&id];
    assert_eq!(
        unit.mission,
        EnemyMission::Guard,
        "{context} must stand the unit down to guard"
    );
    assert_eq!(
        unit.mode,
        EnemyMode::Guard,
        "{context} must reset guard stance"
    );
    assert_eq!(
        unit.target, None,
        "{context} must drop the stale attack target so guard aggro rules apply"
    );
    assert!(
        unit.path.is_empty(),
        "{context} must drop mission-specific navigation state"
    );
}

/// Makes spawner placement at `destination` fail while the expansion site
/// itself still reads clear: only off-center footprint tiles are marked
/// occupied under `occupant`, and no placed entities are added, so the site
/// checks that inspect the center tile and nearby structures keep passing.
fn block_spawner_footprint(
    sim: &mut Simulation,
    occupant: EntityId,
    spawner_prototype: EntityPrototypeId,
    destination: (WorldTileCoord, WorldTileCoord),
) {
    let footprint = sim
        .world
        .entity_footprint(
            spawner_prototype,
            destination.0,
            destination.1,
            Direction::North,
        )
        .expect("destination should fit a spawner footprint");
    for (x, y) in footprint.tiles() {
        if (x, y) != destination {
            sim.entities
                .occupancy
                .occupied_tiles
                .insert((x, y), occupant);
        }
    }
}

/// Dispatches a real expansion through the scheduler, then gives every member
/// a stale attack target plus a pending path and teleports the first member
/// onto the destination so the party counts as arrived. The destination is
/// legitimate by construction: production search picked it while the stale
/// target chest below was already placed, so every site rule held at dispatch.
fn dispatched_arrived_expansion() -> (
    Simulation,
    ExpansionId,
    (WorldTileCoord, WorldTileCoord),
    Vec<EnemyId>,
    EntityId,
) {
    let mut sim = Simulation::new_test_world(123);
    let (base_id, spawners) = colony_with_three_spawners(&mut sim);
    let origin = sim.entities.placed_entities[&spawners[0]].clone();
    let target = place_chest_near(&mut sim, origin.x, origin.y);
    arm_expansion_due(&mut sim, base_id);
    sim.advance_enemy_spawners();
    let (expansion_id, destination, members): (
        ExpansionId,
        (WorldTileCoord, WorldTileCoord),
        Vec<EnemyId>,
    ) = sim
        .enemies
        .expansions
        .iter()
        .find(|(_, party)| party.base_id == base_id)
        .map(|(&id, party)| {
            (
                id,
                party.destination,
                party.members.iter().copied().collect(),
            )
        })
        .expect("the fixture must dispatch an expansion");
    assert!(!members.is_empty());
    for &id in &members {
        let unit = sim.enemies.enemies.get_mut(&id).unwrap();
        unit.target = Some(target);
        unit.path.push_back((destination.0 + 1, destination.1));
    }
    // Teleport the first member onto the destination so the party has arrived.
    let founder = sim.enemies.enemies.get_mut(&members[0]).unwrap();
    founder.x = destination.0 * POSITION_SCALE + POSITION_SCALE / 2;
    founder.y = destination.1 * POSITION_SCALE + POSITION_SCALE / 2;
    (sim, expansion_id, destination, members, target)
}

/// Regression tests for https://github.com/12Mosch/factory/issues/310: every
/// expansion-to-guard transition must drop the previous attack target so the
/// new guard reacquires victims through guard aggro rules only.
#[test]
fn blocked_expansion_stands_down_to_guard_without_stale_targets() {
    let (mut sim, expansion_id, destination, members, _target) = dispatched_arrived_expansion();
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    place_at(
        &mut sim,
        chest,
        destination.0,
        destination.1,
        Direction::North,
    );
    assert!(
        sim.entities
            .occupancy
            .entity_at(destination.0, destination.1)
            .is_some(),
        "the fixture must occupy the destination"
    );
    let bases_before = sim.enemies.bases.len();

    sim.resolve_arrived_expansions();

    assert!(!sim.enemies.expansions.contains_key(&expansion_id));
    assert_eq!(sim.enemies.bases.len(), bases_before);
    for id in members {
        assert_converted_to_guard(&sim, id, "blocked expansion");
    }
}

#[test]
fn successful_expansion_founds_colony_and_clears_survivor_targets() {
    let (mut sim, expansion_id, _destination, members, _target) = dispatched_arrived_expansion();
    let bases_before = sim.enemies.bases.len();

    sim.resolve_arrived_expansions();

    assert!(!sim.enemies.expansions.contains_key(&expansion_id));
    assert_eq!(
        sim.enemies.bases.len(),
        bases_before + 1,
        "successful expansion must found a colony"
    );
    assert!(
        !sim.enemies.enemies.contains_key(&members[0]),
        "the founder is consumed by the new spawner"
    );
    for id in members.into_iter().skip(1) {
        assert_converted_to_guard(&sim, id, "successful expansion");
    }
}

#[test]
fn failed_expansion_placement_stands_down_to_guard_without_stale_targets() {
    let (mut sim, expansion_id, destination, members, target) = dispatched_arrived_expansion();
    let spawner_prototype = sim.enemies.expansions[&expansion_id].spawner_prototype;
    block_spawner_footprint(&mut sim, target, spawner_prototype, destination);
    assert!(
        crate::placement::validate(
            &sim,
            crate::placement::EntityPlacementRequest {
                prototype_id: spawner_prototype,
                x: destination.0,
                y: destination.1,
                direction: Direction::North,
            },
        )
        .is_err(),
        "the fixture must make spawner placement fail while the site reads clear"
    );
    let bases_before = sim.enemies.bases.len();

    sim.resolve_arrived_expansions();

    assert!(!sim.enemies.expansions.contains_key(&expansion_id));
    assert_eq!(
        sim.enemies.bases.len(),
        bases_before,
        "failed placement must remove the half-founded base"
    );
    assert!(
        sim.enemies.enemies.contains_key(&members[0]),
        "failed placement must retain the founder as a guard"
    );
    for id in members {
        assert_converted_to_guard(&sim, id, "failed expansion placement");
    }
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
    let utility_x = spawner.x - 3;
    let utility_y = spawner.y - 3;
    let chest_id = place_at(&mut sim, chest, chest_x, chest_y, Direction::North);
    // Destroying any entity invalidates these networks, even when the attacked
    // building itself has no fluid or heat state.
    let pipe = entity_id_by_name(&sim.world.prototypes, "pipe");
    let heat_pipe = entity_id_by_name(&sim.world.prototypes, "heat_pipe");
    place_at(&mut sim, pipe, utility_x, utility_y, Direction::North);
    place_at(
        &mut sim,
        heat_pipe,
        utility_x + 1,
        utility_y,
        Direction::North,
    );

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
    let bytes = save_to_bytes(&sim).unwrap();
    let loaded = load_from_bytes(&bytes).unwrap();
    assert_eq!(sim.state_hash(), loaded.state_hash());
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
        .filter_map(|slot| slot.stack())
        .map(|stack| u32::from(stack.count()))
        .sum();
    assert!(
        remaining_magazines < 2 || state.loaded_shots < 10,
        "firing should consume ammo"
    );
}

/// Ensures every turret insertion and validation path applies its declared
/// ammunition category.
#[test]
fn gun_turret_rejects_incompatible_ammunition_category() {
    let mut sim = Simulation::new_test_world(123);
    let turret = entity_id_by_name(&sim.world.prototypes, "gun_turret");
    let magazine = item_id_by_name(&sim.world.prototypes, "firearm_magazine");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 3, 3);
    let turret_id = place_at(&mut sim, turret, x, y, Direction::North);
    sim.world.prototypes.items_mut()[magazine.index()]
        .ammo
        .as_mut()
        .expect("firearm magazine should have ammunition metadata")
        .category = factory_data::AmmoCategory::Rocket;
    let catalog = sim.world.prototypes.clone();
    sim.player_inventory
        .insert(&catalog, magazine, 1)
        .expect("player inventory should accept test ammunition");
    let player_slot = sim
        .player_inventory
        .slots()
        .iter()
        .position(|slot| {
            slot.stack()
                .is_some_and(|stack| stack.item_id() == magazine)
        })
        .expect("inserted magazine should occupy a player slot");

    assert_eq!(
        crate::entity_transfer::player_slot_to_entity(&mut sim, turret_id, player_slot),
        Err(ContainerError::InvalidItem(magazine))
    );

    crate::entity_access::inventory_mut(&mut sim, turret_id)
        .expect("turret should expose its ammo inventory")
        .insert(&catalog, magazine, 1)
        .expect("direct inventory mutation deliberately bypasses slot policy");
    assert_eq!(
        sim.validate(),
        Err(SimValidationError::InvalidMachineItem {
            entity_id: turret_id,
            item_id: magazine,
        })
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
fn disconnected_laser_turret_engages_with_active_demand_but_cannot_fire() {
    let mut sim = Simulation::new_test_world(123);
    let laser = entity_id_by_name(&sim.world.prototypes, "laser_turret");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 8, 8);
    let laser_id = place_at(&mut sim, laser, x, y, Direction::North);
    let enemy_id = spawn_test_enemy_at(&mut sim, x + 4, y + 4);

    sim.refresh_power_state();
    let idle = sim.entity_power_status(laser_id).unwrap();
    assert_eq!(idle.active_usage_watts, 0);
    assert_eq!(idle.drain_watts, 24_000);

    let health_before = sim.enemies().get(enemy_id).unwrap().health.current;
    let mut commands = CombatCommandBuffer::default();
    sim.advance_defensive_turrets(&mut commands);
    assert!(
        commands.is_empty(),
        "acquisition waits for power accounting"
    );
    assert!(sim.entities.laser_turrets[&laser_id].engaged);

    sim.refresh_power_state();
    let engaged = sim.entity_power_status(laser_id).unwrap();
    assert_eq!(engaged.active_usage_watts, 600_000);
    assert_eq!(engaged.drain_watts, 24_000);
    assert_eq!(engaged.satisfaction_permyriad, 0);
    sim.advance_defensive_turrets(&mut commands);
    sim.resolve_combat_commands(commands);
    assert_eq!(
        sim.enemies().get(enemy_id).unwrap().health.current,
        health_before
    );
}

#[test]
fn laser_turret_fires_every_thirty_powered_ticks_after_accounting() {
    let mut sim = Simulation::new_test_world(123);
    let laser = entity_id_by_name(&sim.world.prototypes, "laser_turret");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 8, 8);
    let laser_id = place_at(&mut sim, laser, x, y, Direction::North);
    let enemy_id = spawn_test_enemy_at(&mut sim, x + 4, y + 4);
    sim.enemies
        .enemies
        .get_mut(&enemy_id)
        .unwrap()
        .health
        .maximum = 200;
    sim.enemies
        .enemies
        .get_mut(&enemy_id)
        .unwrap()
        .health
        .current = 200;

    let mut commands = CombatCommandBuffer::default();
    sim.advance_defensive_turrets(&mut commands);
    assert!(commands.is_empty());
    sim.power.entity_statuses.insert(
        laser_id,
        EntityPowerStatus {
            satisfaction_permyriad: POWER_SATISFACTION_FULL_PERMYRIAD,
            active_usage_watts: 600_000,
            drain_watts: 24_000,
            ..EntityPowerStatus::default()
        },
    );

    sim.advance_defensive_turrets(&mut commands);
    assert_eq!(commands.len(), 1);
    sim.resolve_combat_commands(commands);
    assert_eq!(sim.enemies().get(enemy_id).unwrap().health.current, 180);

    for powered_tick in 1..30 {
        let mut commands = CombatCommandBuffer::default();
        sim.advance_defensive_turrets(&mut commands);
        assert!(
            commands.is_empty(),
            "laser fired early on powered cooldown tick {powered_tick}"
        );
    }
    let mut commands = CombatCommandBuffer::default();
    sim.advance_defensive_turrets(&mut commands);
    assert_eq!(commands.len(), 1);
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
    let pistol_recipe = recipe_id(&sim.world.prototypes, "pistol");
    let submachine_gun_recipe = recipe_id(&sim.world.prototypes, "submachine_gun");

    assert!(!sim.is_recipe_unlocked(wall_recipe));
    assert!(!sim.is_recipe_unlocked(turret_recipe));
    assert!(sim.is_recipe_unlocked(magazine_recipe));
    assert!(sim.is_recipe_unlocked(repair_recipe));
    assert!(sim.is_recipe_unlocked(pistol_recipe));
    assert!(!sim.is_recipe_unlocked(submachine_gun_recipe));

    complete_research_by_name(&mut sim, "logistics");
    complete_research_by_name(&mut sim, "stone_walls");
    assert!(sim.is_recipe_unlocked(wall_recipe));
    complete_research_by_name(&mut sim, "turrets");
    assert!(sim.is_recipe_unlocked(turret_recipe));
    complete_research_by_name(&mut sim, "automation");
    complete_research_by_name(&mut sim, "electric_power");
    complete_research_by_name(&mut sim, "logistic_science_pack");
    complete_research_by_name(&mut sim, "advanced_material_processing");
    complete_research_by_name(&mut sim, "advanced_ammunition");
    assert!(sim.is_recipe_unlocked(submachine_gun_recipe));
}

/// Ensures an out-of-range attack has no side effects and the exact range
/// boundary commits damage, ammunition, and cooldown together.
#[test]
fn player_weapon_range_boundary_and_failed_attack_preserve_ammo_and_cooldown() {
    let mut sim = Simulation::new_test_world(123);
    let weapon = give_player_weapon_and_ammo(&mut sim, "pistol", 1);
    assert_eq!(sim.cycle_player_weapon(), Ok(weapon));
    let (x, y) = sim.player.tile_position();
    let in_range = spawn_test_enemy_at(&mut sim, x + 10, y);
    let out_of_range = spawn_test_enemy_at(&mut sim, x + 11, y);

    assert_eq!(
        sim.attack_with_player_weapon(x + 11, y),
        Err(PlayerWeaponError::OutOfRange { range_tiles: 10 })
    );
    let untouched = sim.player_weapon_status();
    assert_eq!(untouched.loaded_shots, 0);
    assert_eq!(untouched.reserve_shots, 10);
    assert_eq!(untouched.cooldown_remaining_ticks, 0);
    assert_eq!(sim.enemies.get(out_of_range).unwrap().health.current, 30);

    assert_eq!(
        sim.attack_with_player_weapon(x + 10, y),
        Ok(CombatantId::Enemy(in_range))
    );
    assert_eq!(sim.enemies.get(in_range).unwrap().health.current, 25);
    let fired = sim.player_weapon_status();
    assert_eq!(fired.loaded_shots, 9);
    assert_eq!(fired.reserve_shots, 0);
    assert_eq!(fired.cooldown_remaining_ticks, 20);
}

/// Ensures cooldown cadence and magazine exhaustion use exact fixed ticks.
#[test]
fn player_weapon_cadence_and_ammunition_exhaustion_are_exact() {
    let mut sim = Simulation::new_test_world(123);
    give_player_weapon_and_ammo(&mut sim, "pistol", 1);
    sim.cycle_player_weapon().unwrap();
    let (x, y) = sim.player.tile_position();
    let enemy_id = spawn_test_enemy_at(&mut sim, x + 2, y);
    sim.enemies.enemies.get_mut(&enemy_id).unwrap().health =
        HealthState::new(1_000, Faction::Enemy);

    for shot in 0..10 {
        let target_health = sim.enemies.get(enemy_id).unwrap().health.current;
        assert_eq!(
            sim.attack_with_player_weapon(x + 2, y),
            Ok(CombatantId::Enemy(enemy_id))
        );
        assert_eq!(
            sim.enemies.get(enemy_id).unwrap().health.current,
            target_health - 5
        );
        if shot == 0 {
            assert_eq!(
                sim.attack_with_player_weapon(x + 2, y),
                Err(PlayerWeaponError::CoolingDown {
                    remaining_ticks: 20
                })
            );
            assert_eq!(sim.player_weapon_status().loaded_shots, 9);
        }
        sim.tick = sim.player_weapon.next_ready_tick;
    }

    assert_eq!(
        sim.attack_with_player_weapon(x + 2, y),
        Err(PlayerWeaponError::NoAmmunition)
    );
    assert_eq!(sim.player_weapon_status().cooldown_remaining_ticks, 0);
    assert_eq!(sim.enemies.get(enemy_id).unwrap().health.current, 950);
}

/// Ensures personal weapon damage uses the target's typed resistance profile.
#[test]
fn player_weapon_damage_uses_typed_resistance_per_shot() {
    let mut sim = Simulation::new_test_world(123);
    give_player_weapon_and_ammo(&mut sim, "pistol", 1);
    sim.cycle_player_weapon().unwrap();
    let (x, y) = sim.player.tile_position();
    let enemy_id = spawn_test_enemy_at(&mut sim, x + 2, y);
    sim.enemies
        .enemies
        .get_mut(&enemy_id)
        .unwrap()
        .health
        .resistances =
        ResistanceProfile::NONE.with_resistance(DamageType::Physical, Resistance::new(2, 2_000));

    sim.attack_with_player_weapon(x + 2, y).unwrap();

    assert_eq!(sim.enemies.get(enemy_id).unwrap().health.current, 28);
}

#[test]
fn shotgun_spreads_individually_resisted_pellets_inside_its_cone() {
    let mut sim = Simulation::new_test_world(123);
    give_player_weapon_and_ammo(&mut sim, "shotgun", 1);
    sim.cycle_player_weapon().unwrap();
    let (x, y) = sim.player.tile_position();
    let primary = spawn_test_enemy_at(&mut sim, x + 4, y);
    let secondary = spawn_test_enemy_at(&mut sim, x + 4, y + 1);
    let outside = spawn_test_enemy_at(&mut sim, x + 4, y + 3);
    for enemy_id in [primary, secondary] {
        let enemy = sim.enemies.enemies.get_mut(&enemy_id).unwrap();
        enemy.health = HealthState::new(100, Faction::Enemy);
        enemy.health.resistances =
            ResistanceProfile::NONE.with_resistance(DamageType::Physical, Resistance::new(2, 0));
    }

    sim.attack_with_player_weapon(x + 4, y).unwrap();

    // Eight pellets alternate across the two in-cone targets. Applying the
    // flat resistance to each 8-damage pellet yields four 6-damage hits each.
    assert_eq!(sim.enemies.get(primary).unwrap().health.current, 76);
    assert_eq!(sim.enemies.get(secondary).unwrap().health.current, 76);
    assert_eq!(sim.enemies.get(outside).unwrap().health.current, 30);
}

#[test]
fn rocket_travel_and_area_impact_are_delayed_and_deterministic() {
    let mut sim = Simulation::new_test_world(123);
    give_player_weapon_and_ammo(&mut sim, "rocket_launcher", 1);
    sim.cycle_player_weapon().unwrap();
    let (x, y) = sim.player.tile_position();
    let primary = spawn_test_enemy_at(&mut sim, x + 4, y);
    let nearby = spawn_test_enemy_at(&mut sim, x + 6, y + 2);
    let outside = spawn_test_enemy_at(&mut sim, x + 8, y);
    for enemy_id in [primary, nearby, outside] {
        sim.enemies.enemies.get_mut(&enemy_id).unwrap().health =
            HealthState::new(200, Faction::Enemy);
    }

    sim.attack_with_player_weapon(x + 4, y).unwrap();
    assert_eq!(sim.delayed_combat_state().projectiles().count(), 1);
    assert_eq!(sim.enemies.get(primary).unwrap().health.current, 200);
    let impact_tick = sim
        .delayed_combat_state()
        .projectiles()
        .next()
        .unwrap()
        .impact_tick;
    while sim.tick < impact_tick {
        sim.tick += 1;
        sim.advance_day_night_cycle();
        sim.advance_statistics_to_current_tick();
        let mut commands = CombatCommandBuffer::default();
        sim.advance_delayed_combat(&mut commands);
        sim.resolve_combat_commands(commands);
    }

    assert_eq!(sim.delayed_combat_state().projectiles().count(), 0);
    assert_eq!(sim.enemies.get(primary).unwrap().health.current, 120);
    assert_eq!(sim.enemies.get(nearby).unwrap().health.current, 120);
    assert_eq!(sim.enemies.get(outside).unwrap().health.current, 200);
    sim.validate().unwrap();
}

#[test]
fn flamethrower_cone_applies_durable_interval_fire_damage() {
    let mut sim = Simulation::new_test_world(123);
    give_player_weapon_and_ammo(&mut sim, "flamethrower", 1);
    sim.cycle_player_weapon().unwrap();
    let (x, y) = sim.player.tile_position();
    let primary = spawn_test_enemy_at(&mut sim, x + 4, y);
    let secondary = spawn_test_enemy_at(&mut sim, x + 4, y + 1);
    for enemy_id in [primary, secondary] {
        sim.enemies.enemies.get_mut(&enemy_id).unwrap().health =
            HealthState::new(100, Faction::Enemy);
    }

    sim.attack_with_player_weapon(x + 4, y).unwrap();
    assert_eq!(sim.enemies.get(primary).unwrap().health.current, 98);
    assert!(
        sim.delayed_combat_state()
            .status(CombatantId::Enemy(primary))
            .unwrap()
            .burning
            .is_some()
    );
    for _ in 0..180 {
        sim.tick += 1;
        sim.advance_day_night_cycle();
        sim.advance_statistics_to_current_tick();
        let mut commands = CombatCommandBuffer::default();
        sim.advance_delayed_combat(&mut commands);
        sim.resolve_combat_commands(commands);
    }

    assert_eq!(sim.enemies.get(primary).unwrap().health.current, 86);
    assert_eq!(sim.enemies.get(secondary).unwrap().health.current, 86);
    assert!(
        sim.delayed_combat_state()
            .status(CombatantId::Enemy(primary))
            .is_none()
    );
    sim.validate().unwrap();
}

#[test]
fn in_flight_rocket_round_trips_and_remains_lockstep_deterministic() {
    let mut original = Simulation::new_test_world(123);
    give_player_weapon_and_ammo(&mut original, "rocket_launcher", 1);
    original.cycle_player_weapon().unwrap();
    let (x, y) = original.player.tile_position();
    spawn_test_enemy_at(&mut original, x + 4, y);
    original.attack_with_player_weapon(x + 4, y).unwrap();

    let bytes = save_to_bytes(&original).unwrap();
    let mut loaded = load_from_bytes(&bytes).unwrap();
    assert_eq!(loaded.state_hash(), original.state_hash());
    for _ in 0..30 {
        original.tick();
        loaded.tick();
    }
    assert_eq!(loaded.state_hash(), original.state_hash());
    assert_eq!(loaded.delayed_combat_state().projectiles().count(), 0);
}

/// Ensures selected weapon state survives saves and remains lockstep-identical.
#[test]
fn player_weapon_state_round_trips_and_stays_deterministic() {
    let mut original = Simulation::new_test_world(123);
    give_player_weapon_and_ammo(&mut original, "pistol", 2);
    let before_selection = original.state_hash();
    original.cycle_player_weapon().unwrap();
    assert_ne!(original.state_hash(), before_selection);
    let (x, y) = original.player.tile_position();
    spawn_test_enemy_at(&mut original, x + 2, y);
    original.attack_with_player_weapon(x + 2, y).unwrap();
    original
        .validate()
        .expect("loaded personal weapon state should validate");

    let bytes = save_to_bytes(&original).expect("weapon state should save");
    let mut loaded = load_from_bytes(&bytes).expect("weapon state should load");
    assert_eq!(
        loaded.player_weapon_status(),
        original.player_weapon_status()
    );
    assert_eq!(loaded.state_hash(), original.state_hash());

    for _ in 0..40 {
        original.tick();
        loaded.tick();
    }
    assert_eq!(loaded.state_hash(), original.state_hash());
}

/// Ensures cycling between compatible weapons preserves both a partial
/// magazine and the active cooldown without invalidating simulation state.
#[test]
fn cycling_weapon_during_cooldown_preserves_valid_state() {
    let mut sim = Simulation::new_test_world(123);
    let pistol = give_player_weapon_and_ammo(&mut sim, "pistol", 1);
    let submachine_gun = item_id_by_name(&sim.world.prototypes, "submachine_gun");
    let catalog = sim.world.prototypes.clone();
    sim.player_inventory
        .insert(&catalog, submachine_gun, 1)
        .expect("player inventory should accept the second weapon");
    assert_eq!(sim.cycle_player_weapon(), Ok(pistol));
    let (x, y) = sim.player.tile_position();
    spawn_test_enemy_at(&mut sim, x + 2, y);
    sim.attack_with_player_weapon(x + 2, y).unwrap();
    let pistol_status = sim.player_weapon_status();

    assert_eq!(sim.cycle_player_weapon(), Ok(submachine_gun));
    let switched_status = sim.player_weapon_status();
    assert_eq!(switched_status.loaded_ammo, pistol_status.loaded_ammo);
    assert_eq!(switched_status.loaded_shots, pistol_status.loaded_shots);
    assert_eq!(
        switched_status.cooldown_remaining_ticks,
        pistol_status.cooldown_remaining_ticks
    );
    assert_eq!(sim.player_weapon.cooldown_origin, Some(pistol));
    sim.validate()
        .expect("a cooldown originating from the previous weapon remains valid");
}

/// Ensures a loaded magazine is discarded canonically when the next weapon
/// belongs to a different ammunition category.
#[test]
fn cycling_to_incompatible_weapon_clears_loaded_magazine() {
    let mut sim = Simulation::new_test_world(123);
    let pistol = give_player_weapon_and_ammo(&mut sim, "pistol", 1);
    let submachine_gun = item_id_by_name(&sim.world.prototypes, "submachine_gun");
    let catalog = sim.world.prototypes.clone();
    sim.player_inventory
        .insert(&catalog, submachine_gun, 1)
        .expect("player inventory should accept the second weapon");
    assert_eq!(sim.cycle_player_weapon(), Ok(pistol));
    let (x, y) = sim.player.tile_position();
    spawn_test_enemy_at(&mut sim, x + 2, y);
    sim.attack_with_player_weapon(x + 2, y).unwrap();
    sim.world.prototypes.items_mut()[submachine_gun.index()]
        .weapon
        .as_mut()
        .expect("submachine gun should have weapon metadata")
        .ammo_category = factory_data::AmmoCategory::Rocket;

    assert_eq!(sim.cycle_player_weapon(), Ok(submachine_gun));
    let status = sim.player_weapon_status();
    assert_eq!(status.loaded_ammo, None);
    assert_eq!(status.loaded_shots, 0);
    sim.validate()
        .expect("switching categories should leave canonical unloaded state");
}

/// Ensures malformed combinations of weapon, magazine, damage, and cooldown
/// state cannot enter the durable simulation.
#[test]
fn invalid_player_weapon_state_is_rejected() {
    let mut sim = Simulation::new_test_world(123);
    give_player_weapon_and_ammo(&mut sim, "pistol", 1);
    sim.cycle_player_weapon().unwrap();
    sim.player_weapon.loaded_shots = 1;

    assert_eq!(
        sim.validate(),
        Err(SimValidationError::InvalidPlayerWeaponState)
    );

    let mut no_selection = Simulation::new_test_world(123);
    no_selection.player_weapon.loaded_damage = Damage::new(0, DamageType::Fire);
    assert_eq!(
        no_selection.validate(),
        Err(SimValidationError::InvalidPlayerWeaponState)
    );

    let mut selected_unloaded = Simulation::new_test_world(123);
    give_player_weapon_and_ammo(&mut selected_unloaded, "pistol", 0);
    selected_unloaded.cycle_player_weapon().unwrap();
    selected_unloaded.player_weapon.loaded_damage = Damage::new(0, DamageType::Fire);
    assert_eq!(
        selected_unloaded.validate(),
        Err(SimValidationError::InvalidPlayerWeaponState)
    );
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

#[test]
fn corpse_recovers_opened_magazine_and_full_inventory_without_duplication() {
    let mut sim = Simulation::new_test_world(123);
    give_player_weapon_and_ammo(&mut sim, "pistol", 2);
    sim.cycle_player_weapon().unwrap();
    let (x, y) = sim.player.tile_position();
    spawn_test_enemy_at(&mut sim, x + 10, y);
    sim.attack_with_player_weapon(x + 10, y).unwrap();
    let plate = item_id_by_name(sim.catalog(), "iron_plate");
    let slots = sim.player_inventory.slots().len();
    for slot in 0..slots {
        if sim.player_inventory.slot(slot).is_none() {
            set_inventory_slot(&mut sim.player_inventory, slot, plate, 100);
        }
    }
    let inventory = sim.player_inventory.clone();
    let weapon = sim.player_weapon;
    assert_eq!(weapon.loaded_shots, 9);
    super::player_death::damage_player(&mut sim, &[u32::MAX]);
    let mut restored = load_from_bytes(&save_to_bytes(&sim).unwrap()).unwrap();
    restored.apply_command(&SimCommand::RespawnPlayer).unwrap();
    restored.tick();
    assert!(
        restored
            .player_inventory
            .slots()
            .iter()
            .all(|slot| slot.is_empty())
    );
    assert_eq!(restored.player_weapon, PlayerWeaponState::default());
    restored
        .apply_command(&SimCommand::RecoverCorpse { corpse_id: 1 })
        .unwrap();
    assert_eq!(restored.player_inventory, inventory);
    assert_eq!(restored.player_weapon, weapon);
    assert!(restored.corpse(1).is_none());
    assert_eq!(
        restored.recover_corpse(1),
        Err(CorpseRecoveryError::MissingCorpse)
    );
    restored.validate().unwrap();
}

#[test]
fn corpse_keeps_opened_magazine_until_player_has_its_weapon() {
    let mut sim = Simulation::new_test_world(123);
    sim.player_inventory = Inventory::player();
    give_player_weapon_and_ammo(&mut sim, "pistol", 2);
    sim.cycle_player_weapon().unwrap();
    let (x, y) = sim.player.tile_position();
    spawn_test_enemy_at(&mut sim, x + 10, y);
    sim.attack_with_player_weapon(x + 10, y).unwrap();
    let weapon = sim.player_weapon;
    super::player_death::damage_player(&mut sim, &[u32::MAX]);
    sim.apply_command(&SimCommand::RespawnPlayer).unwrap();
    sim.tick();
    let plate = item_id_by_name(sim.catalog(), "iron_plate");
    for slot in 0..sim.player_inventory.slots().len() {
        set_inventory_slot(&mut sim.player_inventory, slot, plate, 100);
    }
    assert_eq!(sim.recover_corpse(1), Err(CorpseRecoveryError::NoCapacity));
    assert_eq!(sim.player_weapon, PlayerWeaponState::default());
    assert_eq!(sim.corpse(1).unwrap().weapon, weapon);
    sim.validate().unwrap();
    sim.player_inventory.remove(plate, 200).unwrap();
    sim.recover_corpse(1).unwrap();
    assert_eq!(sim.player_weapon, weapon);
    assert!(sim.corpse(1).is_none());
    sim.validate().unwrap();
}
