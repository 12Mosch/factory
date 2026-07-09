use super::*;
use crate::construction::ConstructionJob;
use std::collections::BTreeMap;

/// Validates the construction planning state: ghosts and their occupancy
/// index, deconstruction marks, the job queue, and the blueprint library.
pub(super) fn validate_construction_state(sim: &Simulation) -> Result<(), SimValidationError> {
    let construction = &sim.construction;
    let mut expected_occupancy = BTreeMap::new();

    for (ghost_id, ghost) in &construction.ghosts {
        if ghost.id != *ghost_id {
            return Err(SimValidationError::InvalidGhostFootprint {
                ghost_id: *ghost_id,
            });
        }
        let Some(prototype) = sim.world.prototypes.entity(ghost.prototype_id) else {
            return Err(SimValidationError::InvalidGhostPrototype {
                ghost_id: *ghost_id,
                prototype_id: ghost.prototype_id,
            });
        };

        let expected_footprint = EntityFootprint::from_size(
            ghost.x,
            ghost.y,
            prototype.size.x,
            prototype.size.y,
            ghost.direction,
        );
        if ghost.footprint != expected_footprint || ghost.footprint.validate().is_err() {
            return Err(SimValidationError::InvalidGhostFootprint {
                ghost_id: *ghost_id,
            });
        }

        if let Some(recipe_id) = ghost.recipe
            && sim.world.prototypes.recipe(recipe_id).is_none()
        {
            return Err(SimValidationError::InvalidGhostRecipe {
                ghost_id: *ghost_id,
                recipe_id,
            });
        }

        for tile in ghost.footprint.tiles() {
            if let Some(entity_id) = sim.entities.occupancy.entity_at(tile.0, tile.1) {
                return Err(SimValidationError::GhostOverlapsEntity {
                    ghost_id: *ghost_id,
                    entity_id,
                });
            }
            if expected_occupancy.insert(tile, *ghost_id).is_some() {
                return Err(SimValidationError::GhostOccupancyMismatch);
            }
        }
    }

    if expected_occupancy != construction.ghost_occupancy {
        return Err(SimValidationError::GhostOccupancyMismatch);
    }

    for entity_id in &construction.deconstruction_marks {
        if sim.entities.placed_entity(*entity_id).is_none() {
            return Err(SimValidationError::InvalidDeconstructionMark(*entity_id));
        }
    }

    // The queue must hold exactly one job per ghost and per deconstruction
    // mark, in any order.
    let mut queued_ghosts = std::collections::BTreeSet::new();
    let mut queued_deconstructions = std::collections::BTreeSet::new();
    for job in &construction.queue {
        let unique = match job {
            ConstructionJob::BuildGhost(ghost_id) => {
                construction.ghosts.contains_key(ghost_id) && queued_ghosts.insert(*ghost_id)
            }
            ConstructionJob::Deconstruct(entity_id) => {
                construction.deconstruction_marks.contains(entity_id)
                    && queued_deconstructions.insert(*entity_id)
            }
        };
        if !unique {
            return Err(SimValidationError::InvalidConstructionQueue);
        }
    }
    if queued_ghosts.len() != construction.ghosts.len()
        || queued_deconstructions.len() != construction.deconstruction_marks.len()
    {
        return Err(SimValidationError::InvalidConstructionQueue);
    }

    for (blueprint_index, blueprint) in construction.blueprints.iter().enumerate() {
        for entity in &blueprint.entities {
            if sim.world.prototypes.entity(entity.prototype_id).is_none() {
                return Err(SimValidationError::InvalidBlueprintPrototype {
                    blueprint_index,
                    prototype_id: entity.prototype_id,
                });
            }
            if let Some(recipe_id) = entity.recipe
                && sim.world.prototypes.recipe(recipe_id).is_none()
            {
                return Err(SimValidationError::InvalidBlueprintRecipe {
                    blueprint_index,
                    recipe_id,
                });
            }
        }
    }

    Ok(())
}
