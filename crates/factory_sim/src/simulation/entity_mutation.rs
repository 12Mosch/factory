use super::topology_invalidation_ops::{apply_entity_topology_change, impact_for_prototype};
use super::*;

pub fn rotate(
    sim: &mut Simulation,
    entity_id: EntityId,
    direction: Direction,
) -> Result<(), BuildError> {
    let Some(rotation) = placement_validation_ops::validate_rotation(sim, entity_id, direction)?
    else {
        return Ok(());
    };
    let old_footprint = sim
        .entities
        .placed_entities
        .get(&entity_id)
        .map(|placed| placed.footprint);

    sim.entities
        .update_entity_footprint(entity_id, direction, rotation.footprint)?;
    construction_ops::clear_ghosts_overlapping_footprint(sim, &rotation.footprint);
    if let Some(old_footprint) = old_footprint
        && old_footprint != rotation.footprint
    {
        if rotation.impact.affects_transport_lane_graph {
            sim.invalidate_transport_lane_graph_region(entity_id, old_footprint);
        }
        if let Some(radius) = rotation.impact.beacon_effect_radius_tiles {
            sim.refresh_machines_in_beacon_region(old_footprint, radius);
        }
    }
    apply_entity_topology_change(sim, rotation.impact, entity_id, rotation.footprint);
    Ok(())
}

pub fn remove(sim: &mut Simulation, entity_id: EntityId) -> Option<PlacedEntity> {
    let removed = sim.entities.remove_placed_entity(entity_id);
    if let Some(removed) = &removed {
        sim.unregister_pollution_emitter(entity_id);
        if sim
            .world
            .prototypes
            .entity(removed.prototype_id)
            .is_some_and(|prototype| prototype.entity_kind == EntityKind::EnemySpawner)
        {
            sim.on_enemy_spawner_removed(entity_id, removed.x, removed.y);
        }
        construction_ops::clear_construction_state_for_removed_entity(sim, entity_id);
        let impact = impact_for_prototype(sim, removed.prototype_id);
        apply_entity_topology_change(sim, impact, entity_id, removed.footprint);
    }
    removed
}

pub fn destroy_to_player_inventory(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<PlacedEntity, EntityDestroyError> {
    entity_recovery_ops::destroy_to_player_inventory(sim, entity_id)
}
