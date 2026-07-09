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

    sim.entities
        .update_entity_footprint(entity_id, direction, rotation.footprint)?;
    construction_ops::clear_ghosts_overlapping_footprint(sim, &rotation.footprint);
    apply_entity_topology_change(sim, rotation.impact);
    Ok(())
}

pub fn remove(sim: &mut Simulation, entity_id: EntityId) -> Option<PlacedEntity> {
    let removed = sim.entities.remove_placed_entity(entity_id);
    if let Some(removed) = &removed {
        construction_ops::clear_construction_state_for_removed_entity(sim, entity_id);
        let impact = impact_for_prototype(sim, removed.prototype_id);
        apply_entity_topology_change(sim, impact);
    }
    removed
}

pub fn destroy_to_player_inventory(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<PlacedEntity, EntityDestroyError> {
    entity_recovery_ops::destroy_to_player_inventory(sim, entity_id)
}
