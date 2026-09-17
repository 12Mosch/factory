use super::super::*;
use super::support::*;
use std::sync::{Arc, Barrier};

/// Exercises mutations whose entity-state indexes occupy different 256-entry
/// pages while a detached generation is being encoded. Moving the player to a
/// different terrain chunk in the same interval covers spatial ownership too.
#[test]
fn snapshot_bytes_ignore_cross_page_transfer_and_cross_chunk_movement() {
    let mut sim = Simulation::new_test_world(294);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let mut source = None;
    let mut destination = None;
    let mut source_page = 0;

    for (x, y) in all_tile_coords(&sim.world) {
        let request = crate::placement::EntityPlacementRequest {
            prototype_id: chest,
            x,
            y,
            direction: Direction::North,
        };
        let Ok(id) = crate::placement::place(&mut sim, request) else {
            continue;
        };
        if source.is_none() {
            source_page = id.raw() >> 8;
            source = Some(id);
        } else if id.raw() >> 8 != source_page {
            destination = Some(id);
            break;
        }
    }

    let source = source.expect("fixture should place a source chest");
    let destination = destination.expect("fixture should cross an entity-state page boundary");
    assert_ne!(source.raw() >> 8, destination.raw() >> 8);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let catalog = sim.world.prototypes.clone();
    crate::entity_access::inventory_mut(&mut sim, source)
        .unwrap()
        .insert(&catalog, iron_plate, 37)
        .unwrap();
    sim.player_inventory = Inventory::with_slot_count(1);
    sim.validate_state().unwrap();

    let captured_tick = sim.tick_count();
    let captured_hash = sim.state_hash();
    let snapshot = try_capture_save_snapshot(&sim, 17).unwrap();
    assert_eq!(
        snapshot.identity(),
        SaveSnapshotIdentity {
            world_generation: 17,
            tick: captured_tick,
        }
    );

    let barrier = Arc::new(Barrier::new(2));
    let worker_barrier = Arc::clone(&barrier);
    let encoder = std::thread::spawn(move || {
        worker_barrier.wait();
        save_snapshot_to_bytes(&snapshot).unwrap()
    });
    barrier.wait();

    crate::entity_transfer::entity_slot_to_player(&mut sim, source, 0).unwrap();
    crate::entity_transfer::player_slot_to_entity(&mut sim, destination, 0).unwrap();
    let far_chunk = ChunkCoord { x: 8, y: -7 };
    sim.ensure_chunk_generated(far_chunk);
    let (far_x, far_y) = far_chunk.tile_at(1, 1);
    sim.teleport_player_to_tile(far_x, far_y);
    sim.tick();
    sim.validate_state().unwrap();

    let captured = load_from_bytes(&encoder.join().unwrap()).unwrap();
    assert_eq!(captured.tick_count(), captured_tick);
    assert_eq!(captured.state_hash(), captured_hash);
    assert_ne!(captured.state_hash(), sim.state_hash());
    assert_eq!(
        crate::entity_access::inventory(&captured, source)
            .unwrap()
            .count(iron_plate),
        37
    );
    assert_eq!(
        crate::entity_access::inventory(&captured, destination)
            .unwrap()
            .count(iron_plate),
        0
    );
}
