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

macro_rules! ownership_check {
    // Auxiliary state maps (`_` entries) are shared across kinds, so they
    // only get a generic orphan check here; maps with kind-specific owner
    // rules additionally need a dedicated check below.
    ($sim:ident, $field:ident, _) => {
        for entity_id in $sim.entities.$field.keys() {
            if !$sim.entities.placed_entities.contains_key(entity_id) {
                return Err(SimValidationError::OrphanEntityState(*entity_id));
            }
        }
    };
    ($sim:ident, $field:ident, $kind:ident) => {
        for entity_id in $sim.entities.$field.keys() {
            validate_entity_state_kind($sim, *entity_id, EntityKind::$kind)?;
        }
    };
}

macro_rules! define_validate_entity_state_ownership {
    ($($field:ident : $ty:ty => $kind:tt),* $(,)?) => {
        pub(super) fn validate_entity_state_ownership_and_kind(
            sim: &Simulation,
        ) -> Result<(), SimValidationError> {
            $(ownership_check!(sim, $field, $kind);)*
            for entity_id in sim.entities.electric_consumers.keys() {
                validate_electric_consumer_owner(sim, *entity_id)?;
            }
            for entity_id in sim.entities.fluid_boxes.keys() {
                validate_fluid_box_owner(sim, *entity_id)?;
            }

            Ok(())
        }
    };
}
for_each_entity_state_map!(define_validate_entity_state_ownership);

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
