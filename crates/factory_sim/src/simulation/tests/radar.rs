use super::super::*;
use super::support::*;

fn place_radar(sim: &mut Simulation, x: WorldTileCoord, y: WorldTileCoord) -> EntityId {
    let prototype_id = entity_id_by_name(&sim.world.prototypes, "radar");
    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("radar should be placeable")
}

fn radar_center_chunk(sim: &Simulation, entity_id: EntityId) -> ChunkCoord {
    let footprint = sim
        .entities
        .placed_entity(entity_id)
        .expect("radar should remain placed")
        .footprint;
    ChunkCoord::from_tile(
        footprint.x + i64::from(footprint.width / 2),
        footprint.y + i64::from(footprint.height / 2),
    )
    .expect("placed radar should have a representable center chunk")
}

fn grant_power(sim: &mut Simulation, entity_id: EntityId, satisfaction_permyriad: u32) {
    sim.power.entity_statuses.insert(
        entity_id,
        EntityPowerStatus {
            network_id: Some(0),
            satisfaction_permyriad,
            active_usage_watts: 300_000,
            drain_watts: 0,
        },
    );
}

fn set_radar_far_interval(sim: &mut Simulation, ticks: u32) {
    sim.world
        .prototypes
        .entities
        .iter_mut()
        .find(|prototype| prototype.name == "radar")
        .and_then(|prototype| prototype.radar.as_mut())
        .expect("radar metadata")
        .far_scan_interval_ticks = ticks;
}

fn set_radar_nearby_interval(sim: &mut Simulation, ticks: u32) {
    sim.world
        .prototypes
        .entities
        .iter_mut()
        .find(|prototype| prototype.name == "radar")
        .and_then(|prototype| prototype.radar.as_mut())
        .expect("radar metadata")
        .nearby_scan_interval_ticks = ticks;
}

fn radar_metadata(sim: &Simulation) -> factory_data::RadarPrototype {
    sim.world
        .prototypes
        .entities
        .iter()
        .find(|prototype| prototype.name == "radar")
        .and_then(|prototype| prototype.radar)
        .expect("radar metadata")
}

#[test]
fn radar_recipe_and_entity_are_initially_unlocked() {
    let sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "radar");
    let entity = entity_id_by_name(&sim.world.prototypes, "radar");

    assert!(sim.is_recipe_unlocked(recipe));
    assert!(sim.is_entity_unlocked(entity));
}

#[test]
fn disconnected_radar_does_not_advance() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 3, 3);
    let radar_id = place_radar(&mut sim, x, y);

    for _ in 0..120 {
        sim.tick();
    }

    let status = sim
        .entity_power_status(radar_id)
        .expect("radar should remain an electric consumer");
    assert_eq!(status.active_usage_watts, 300_000);
    assert_eq!(status.drain_watts, 0);
    assert_eq!(status.satisfaction_permyriad, 0);
    assert_eq!(
        crate::entity_access::radar_state(&sim, radar_id),
        Some(&RadarState::default())
    );
}

#[test]
fn full_and_half_power_use_the_deterministic_work_cadence() {
    let mut fully_powered = Simulation::new_test_world(123);
    set_radar_nearby_interval(&mut fully_powered, 60);
    set_radar_far_interval(&mut fully_powered, 60);
    let (x, y) = first_buildable_rect_without_resource(&fully_powered.world, 3, 3);
    let full_radar = place_radar(&mut fully_powered, x, y);
    grant_power(&mut fully_powered, full_radar, 10_000);
    for _ in 0..59 {
        fully_powered.advance_radars();
    }
    assert_eq!(
        crate::entity_access::radar_state(&fully_powered, full_radar)
            .expect("radar state")
            .nearby_scan_progress_ticks(),
        59
    );
    fully_powered.advance_radars();
    assert_eq!(
        crate::entity_access::radar_state(&fully_powered, full_radar)
            .expect("radar state")
            .nearby_scan_progress_ticks(),
        0
    );
    assert_eq!(
        crate::entity_access::radar_state(&fully_powered, full_radar)
            .expect("radar state")
            .far_scan_cursor(),
        1
    );

    let mut half_powered = Simulation::new_test_world(123);
    set_radar_nearby_interval(&mut half_powered, 60);
    set_radar_far_interval(&mut half_powered, 60);
    let (x, y) = first_buildable_rect_without_resource(&half_powered.world, 3, 3);
    let half_radar = place_radar(&mut half_powered, x, y);
    grant_power(&mut half_powered, half_radar, 5_000);
    for _ in 0..119 {
        half_powered.advance_radars();
    }
    assert_eq!(
        crate::entity_access::radar_state(&half_powered, half_radar)
            .expect("radar state")
            .nearby_scan_progress_ticks(),
        59
    );
    half_powered.advance_radars();
    assert_eq!(
        crate::entity_access::radar_state(&half_powered, half_radar)
            .expect("radar state")
            .nearby_scan_progress_ticks(),
        0
    );
    assert_eq!(
        crate::entity_access::radar_state(&half_powered, half_radar)
            .expect("radar state")
            .far_scan_cursor(),
        1
    );
}

#[test]
fn nearby_pulse_reveals_generated_chunks_and_queues_missing_chunks() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 3, 3);
    let radar_id = place_radar(&mut sim, x, y);
    grant_power(&mut sim, radar_id, 10_000);
    let center = radar_center_chunk(&sim, radar_id);
    let metadata = radar_metadata(&sim);
    let radius = i32::from(metadata.nearby_reveal_radius_chunks);
    let candidates = (-radius..=radius)
        .flat_map(|dy| {
            (-radius..=radius).map(move |dx| ChunkCoord {
                x: center.x + dx,
                y: center.y + dy,
            })
        })
        .collect::<BTreeSet<_>>();
    let previously_revealed = sim.revealed_chunks().clone();

    for _ in 0..metadata.nearby_scan_interval_ticks {
        sim.advance_radars();
    }

    let newly_revealed = sim
        .revealed_chunks()
        .difference(&previously_revealed)
        .copied()
        .collect::<BTreeSet<_>>();
    let covered = newly_revealed
        .union(&sim.chunk_generation_queue.radar_reveal)
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        covered,
        candidates
            .difference(&previously_revealed)
            .copied()
            .collect()
    );
    assert!(!sim.chunk_generation_queue.radar_reveal.is_empty());
}

#[test]
fn far_scan_reveals_generated_target_immediately_and_exactly() {
    let mut catalog = PrototypeCatalog::load_base().expect("base catalog");
    let radar = catalog
        .entities
        .iter_mut()
        .find(|prototype| prototype.name == "radar")
        .expect("radar prototype");
    radar
        .radar
        .as_mut()
        .expect("radar metadata")
        .far_scan_interval_ticks = 1;
    let mut sim = Simulation::new(123, catalog);
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 3, 3);
    let radar_id = place_radar(&mut sim, x, y);
    grant_power(&mut sim, radar_id, 10_000);
    let center = radar_center_chunk(&sim, radar_id);
    let target = ChunkCoord {
        x: center.x - 4,
        y: center.y + 4,
    };
    sim.ensure_chunk_generated(target);
    let revision = sim.revealed_revision();

    sim.advance_radars();

    assert!(sim.is_chunk_revealed(target));
    assert_eq!(
        sim.revealed_chunks_since(revision)
            .expect("recent history")
            .collect::<Vec<_>>(),
        vec![target]
    );
    assert_eq!(
        crate::entity_access::radar_state(&sim, radar_id)
            .expect("radar state")
            .far_scan_cursor(),
        1
    );
}

#[test]
fn completed_far_sweep_uses_and_preserves_the_fast_path() {
    let mut sim = Simulation::new_test_world(123);
    set_radar_nearby_interval(&mut sim, u32::MAX);
    set_radar_far_interval(&mut sim, 1);
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 3, 3);
    let radar_id = place_radar(&mut sim, x, y);
    grant_power(&mut sim, radar_id, 10_000);
    let center = radar_center_chunk(&sim, radar_id);
    let metadata = radar_metadata(&sim);
    let nearby = i32::from(metadata.nearby_reveal_radius_chunks);
    let far = i32::from(metadata.far_scan_radius_chunks);
    let initially_revealed = sim.chart.revealed_chunks.clone();
    for dy in -far..=far {
        for dx in -far..=far {
            if dx.abs().max(dy.abs()) > nearby {
                sim.chart.revealed_chunks.insert(ChunkCoord {
                    x: center.x + dx,
                    y: center.y + dy,
                });
            }
        }
    }

    sim.advance_radars();
    let state = crate::entity_access::radar_state(&sim, radar_id).expect("radar state");
    assert!(state.far_scan_complete());
    let completed_cursor = state.far_scan_cursor();

    sim.advance_radars();
    let state = crate::entity_access::radar_state(&sim, radar_id).expect("radar state");
    assert!(state.far_scan_complete());
    assert_eq!(state.far_scan_cursor(), completed_cursor);

    sim.chart.revealed_chunks = initially_revealed;
    let loaded = load_from_bytes(&save_to_bytes(&sim).expect("save completed radar"))
        .expect("load completed radar");
    assert!(
        crate::entity_access::radar_state(&loaded, radar_id)
            .expect("loaded radar state")
            .far_scan_complete()
    );
}

#[test]
fn higher_priority_generation_preserves_radar_reveal_intent() {
    let mut sim = Simulation::new_test_world(123);
    let target = ChunkCoord { x: 30, y: -30 };
    sim.request_chunk_generation(target, ChunkGenerationPriority::RadarReveal);
    sim.request_chunk_generation(target, ChunkGenerationPriority::Required);
    let revision = sim.revealed_revision();

    assert_eq!(sim.process_chunk_generation_queue(1), 1);

    assert!(sim.is_chunk_revealed(target));
    assert_eq!(
        sim.revealed_chunks_since(revision)
            .expect("recent history")
            .collect::<Vec<_>>(),
        vec![target]
    );
}

#[test]
fn direct_generation_preserves_radar_reveal_intent() {
    let mut sim = Simulation::new_test_world(123);
    let target = ChunkCoord { x: -31, y: 29 };
    sim.request_chunk_generation(target, ChunkGenerationPriority::RadarReveal);
    let revision = sim.revealed_revision();

    sim.ensure_chunk_generated(target);

    assert!(sim.is_chunk_revealed(target));
    assert_eq!(
        sim.revealed_chunks_since(revision)
            .expect("recent history")
            .collect::<Vec<_>>(),
        vec![target]
    );
}

#[test]
fn overlapping_radars_choose_distinct_pending_far_targets() {
    let mut sim = Simulation::new_test_world(123);
    set_radar_far_interval(&mut sim, 1);
    let prototype_id = entity_id_by_name(&sim.world.prototypes, "radar");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 3, 3);
    let first = place_radar(&mut sim, x, y);
    let center = radar_center_chunk(&sim, first);
    let (second_x, second_y) = all_tile_coords(&sim.world)
        .into_iter()
        .find(|&(candidate_x, candidate_y)| {
            let center_coord = ChunkCoord::from_tile(candidate_x + 1, candidate_y + 1);
            center_coord == Some(center)
                && crate::placement::validate(
                    &sim,
                    crate::placement::EntityPlacementRequest {
                        prototype_id,
                        x: candidate_x,
                        y: candidate_y,
                        direction: Direction::North,
                    },
                )
                .is_ok()
        })
        .expect("same chunk should have room for a second radar");
    let second = place_radar(&mut sim, second_x, second_y);
    grant_power(&mut sim, first, 10_000);
    grant_power(&mut sim, second, 10_000);

    sim.advance_radars();

    assert_eq!(sim.chunk_generation_queue.radar_reveal.len(), 2);
    assert_eq!(
        crate::entity_access::radar_state(&sim, first)
            .expect("first radar state")
            .far_scan_cursor(),
        1
    );
    assert_eq!(
        crate::entity_access::radar_state(&sim, second)
            .expect("second radar state")
            .far_scan_cursor(),
        2
    );
}

#[test]
fn already_revealed_radar_batch_does_not_increment_revision() {
    let mut sim = Simulation::new_test_world(123);
    let coord = *sim
        .revealed_chunks()
        .first()
        .expect("initial chart contains chunks");
    let revision = sim.revealed_revision();

    sim.reveal_generated_chunks(&[coord]);

    assert_eq!(sim.revealed_revision(), revision);
}

#[test]
fn save_load_preserves_radar_progress_cursor_and_pending_reveals() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 3, 3);
    let radar_id = place_radar(&mut sim, x, y);
    let state = sim.entities.radars.get_mut(&radar_id).expect("radar state");
    state.nearby_scan_progress_ticks = 17;
    state.far_scan_progress_ticks = 901;
    state.far_scan_cursor = 77;
    let pending = ChunkCoord { x: 31, y: -29 };
    sim.request_chunk_generation(pending, ChunkGenerationPriority::RadarReveal);
    let hash = sim.state_hash();

    let bytes = save_to_bytes(&sim).expect("save radar state");
    let mut loaded = load_from_bytes(&bytes).expect("load radar state");

    assert_eq!(loaded.state_hash(), hash);
    assert_eq!(
        crate::entity_access::radar_state(&loaded, radar_id),
        crate::entity_access::radar_state(&sim, radar_id)
    );
    assert!(
        loaded
            .chunk_generation_queue
            .radar_reveal
            .contains(&pending)
    );
    sim.tick();
    loaded.tick();
    assert_eq!(loaded.state_hash(), sim.state_hash());
}
