use super::placement::{EntityPlacementRequest, PlayerPlacementRequest};
use super::topology_invalidation_ops::{apply_entity_topology_change, impact_for_prototype};
use super::*;

pub(crate) fn place_entity(
    sim: &mut Simulation,
    request: EntityPlacementRequest,
) -> Result<EntityId, BuildError> {
    let footprint = placement_validation_ops::validate_entity_placement(sim, request)?;
    Ok(place_validated_entity(sim, request, footprint))
}

pub(crate) fn place_entity_from_player_inventory(
    sim: &mut Simulation,
    request: PlayerPlacementRequest,
) -> Result<EntityId, PlayerBuildError> {
    let footprint = placement_validation_ops::validate_player_inventory_placement(sim, request)?;
    let entity_id = place_validated_entity(
        sim,
        EntityPlacementRequest {
            prototype_id: request.prototype_id,
            x: request.x,
            y: request.y,
            direction: request.direction,
        },
        footprint,
    );
    sim.player_inventory
        .remove(request.item_id, 1)
        .expect("validated player build item should remain removable");

    Ok(entity_id)
}

fn place_validated_entity(
    sim: &mut Simulation,
    request: EntityPlacementRequest,
    footprint: EntityFootprint,
) -> EntityId {
    let prototype = &sim.world.prototypes.entities[request.prototype_id.index()];
    let reservation = reservation_for_prototype(
        prototype,
        request.prototype_id,
        request.x,
        request.y,
        request.direction,
        footprint,
    );
    let impact = impact_for_prototype(sim, request.prototype_id);
    construction_ops::clear_ghosts_overlapping_footprint(sim, &footprint);
    let entity_id = sim.entities.reserve_entity(reservation);
    apply_entity_topology_change(sim, impact);
    entity_id
}
