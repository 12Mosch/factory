use super::super::*;
use super::ids::*;

pub(super) fn validate_entity_occupancy(entities: &EntityStore) -> Result<(), SimValidationError> {
    let mut expected = BTreeMap::new();

    for placed in entities.placed_entities.values() {
        for (x, y) in placed.footprint.tiles() {
            if let Some(first) = expected.insert((x, y), placed.id) {
                return Err(SimValidationError::EntityOverlap {
                    x,
                    y,
                    first,
                    second: placed.id,
                });
            }
        }
    }

    if expected != entities.occupancy.occupied_tiles {
        return Err(SimValidationError::OccupancyMismatch);
    }

    Ok(())
}

pub(super) fn validate_entity_state_ownership_and_kind(
    sim: &Simulation,
) -> Result<(), SimValidationError> {
    for entity_id in sim.entities.entity_inventories.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::Chest)?;
    }
    for entity_id in sim.entities.burner_mining_drills.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::MiningDrill)?;
    }
    for entity_id in sim.entities.furnaces.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::Furnace)?;
    }
    for entity_id in sim.entities.assembling_machines.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::AssemblingMachine)?;
    }
    for entity_id in sim.entities.labs.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::Lab)?;
    }
    for entity_id in sim.entities.electric_poles.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::ElectricPole)?;
    }
    for entity_id in sim.entities.electric_consumers.keys() {
        validate_electric_consumer_owner(sim, *entity_id)?;
    }
    for entity_id in sim.entities.steam_engines.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::SteamEngine)?;
    }
    for entity_id in sim.entities.boilers.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::Boiler)?;
    }
    for entity_id in sim.entities.offshore_pumps.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::OffshorePump)?;
    }
    for entity_id in sim.entities.fluid_boxes.keys() {
        validate_fluid_box_owner(sim, *entity_id)?;
    }
    for entity_id in sim.entities.transport_belts.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::TransportBelt)?;
    }
    for entity_id in sim.entities.splitters.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::Splitter)?;
    }
    for entity_id in sim.entities.inserters.keys() {
        validate_entity_state_kind(sim, *entity_id, EntityKind::Inserter)?;
    }

    Ok(())
}

fn validate_fluid_box_owner(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<(), SimValidationError> {
    let placed = sim
        .entities
        .placed_entities
        .get(&entity_id)
        .ok_or(SimValidationError::OrphanEntityState(entity_id))?;
    let prototype = entity_prototype_by_id(&sim.world.prototypes, placed.prototype_id).ok_or(
        SimValidationError::InvalidEntityPrototype {
            entity_id,
            prototype_id: placed.prototype_id,
        },
    )?;

    if prototype.fluid_boxes.is_empty() {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }

    Ok(())
}

fn validate_electric_consumer_owner(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<(), SimValidationError> {
    let placed = sim
        .entities
        .placed_entities
        .get(&entity_id)
        .ok_or(SimValidationError::OrphanEntityState(entity_id))?;
    let prototype = entity_prototype_by_id(&sim.world.prototypes, placed.prototype_id).ok_or(
        SimValidationError::InvalidEntityPrototype {
            entity_id,
            prototype_id: placed.prototype_id,
        },
    )?;

    if prototype.electric_energy_source.is_none() {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }

    Ok(())
}

fn validate_entity_state_kind(
    sim: &Simulation,
    entity_id: EntityId,
    expected_kind: EntityKind,
) -> Result<(), SimValidationError> {
    let placed = sim
        .entities
        .placed_entities
        .get(&entity_id)
        .ok_or(SimValidationError::OrphanEntityState(entity_id))?;
    let prototype = entity_prototype_by_id(&sim.world.prototypes, placed.prototype_id).ok_or(
        SimValidationError::InvalidEntityPrototype {
            entity_id,
            prototype_id: placed.prototype_id,
        },
    )?;

    if prototype.entity_kind != expected_kind {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }

    Ok(())
}
