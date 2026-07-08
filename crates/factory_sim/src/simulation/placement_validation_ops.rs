use super::placement::{EntityPlacementRequest, PlayerPlacementRequest};
use super::topology_invalidation_ops::{EntityTopologyImpact, impact_for_prototype};
use super::*;

pub(crate) struct PlacementValidator<'a> {
    world: &'a WorldSim,
    entities: &'a EntityStore,
    player: &'a PlayerState,
    research: &'a ResearchState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedRotation {
    pub(crate) footprint: EntityFootprint,
    pub(crate) prototype_id: EntityPrototypeId,
    pub(crate) impact: EntityTopologyImpact,
}

impl<'a> PlacementValidator<'a> {
    pub(crate) fn new(
        world: &'a WorldSim,
        entities: &'a EntityStore,
        player: &'a PlayerState,
        research: &'a ResearchState,
    ) -> Self {
        Self {
            world,
            entities,
            player,
            research,
        }
    }

    fn is_entity_unlocked(&self, prototype_id: EntityPrototypeId) -> bool {
        let Some(prototype) = self.world.prototypes.entity(prototype_id) else {
            return false;
        };
        let Some(build_item) = prototype.build_item else {
            return false;
        };

        self.world.prototypes.recipes.iter().any(|recipe| {
            recipe
                .products
                .iter()
                .any(|product| product.item == build_item)
                && recipe_is_unlocked(&self.world.prototypes, self.research, recipe.id)
        })
    }

    fn validate_entity_placement(
        &self,
        request: EntityPlacementRequest,
    ) -> Result<EntityFootprint, BuildError> {
        let footprint = self.world.entity_footprint(
            request.prototype_id,
            request.x,
            request.y,
            request.direction,
        )?;
        let prototype = self
            .world
            .prototypes
            .entity(request.prototype_id)
            .ok_or(BuildError::MissingPrototype(request.prototype_id))?;
        self.world.validate_entity_footprint_for_prototype(
            prototype,
            &footprint,
            request.direction,
        )?;
        self.validate_footprint_clear_of_player(&footprint)?;
        self.entities
            .occupancy
            .validate_available(&footprint, None)?;

        Ok(footprint)
    }

    fn validate_footprint_clear_of_player(
        &self,
        footprint: &EntityFootprint,
    ) -> Result<(), BuildError> {
        let player_tile = self.player.tile_position();
        if footprint.contains_tile(player_tile.0, player_tile.1) {
            return Err(BuildError::TileBlocked {
                x: player_tile.0,
                y: player_tile.1,
            });
        }

        Ok(())
    }
}

pub(crate) fn entity_is_unlocked(sim: &Simulation, prototype_id: EntityPrototypeId) -> bool {
    PlacementValidator::new(&sim.world, &sim.entities, &sim.player, &sim.research)
        .is_entity_unlocked(prototype_id)
}

pub(crate) fn validate_entity_placement(
    sim: &Simulation,
    request: EntityPlacementRequest,
) -> Result<EntityFootprint, BuildError> {
    PlacementValidator::new(&sim.world, &sim.entities, &sim.player, &sim.research)
        .validate_entity_placement(request)
}

pub(crate) fn validate_player_inventory_placement(
    sim: &Simulation,
    request: PlayerPlacementRequest,
) -> Result<EntityFootprint, PlayerBuildError> {
    let prototype = sim
        .world
        .prototypes
        .entity(request.prototype_id)
        .ok_or(PlayerBuildError::MissingPrototype(request.prototype_id))?;
    let build_item = prototype
        .build_item
        .ok_or(PlayerBuildError::MissingBuildItem {
            prototype_id: request.prototype_id,
        })?;

    let item =
        sim.world
            .prototypes
            .item(request.item_id)
            .ok_or(PlayerBuildError::MissingBuildItem {
                prototype_id: request.prototype_id,
            })?;
    if item.id != build_item {
        return Err(PlayerBuildError::ItemDoesNotBuildEntity {
            item_id: request.item_id,
            prototype_id: request.prototype_id,
        });
    }
    if !entity_is_unlocked(sim, request.prototype_id) {
        return Err(PlayerBuildError::EntityLocked {
            prototype_id: request.prototype_id,
        });
    }
    if sim.player_inventory.count(request.item_id) == 0 {
        return Err(PlayerBuildError::InsufficientInventory {
            item_id: request.item_id,
        });
    }

    validate_entity_placement(
        sim,
        EntityPlacementRequest {
            prototype_id: request.prototype_id,
            x: request.x,
            y: request.y,
            direction: request.direction,
        },
    )
    .map_err(PlayerBuildError::Build)
}

pub(crate) fn validate_rotation(
    sim: &Simulation,
    entity_id: EntityId,
    direction: Direction,
) -> Result<Option<ValidatedRotation>, BuildError> {
    let entity = sim
        .entities
        .placed_entity(entity_id)
        .cloned()
        .ok_or(BuildError::MissingEntity(entity_id))?;
    if entity.direction == direction {
        return Ok(None);
    }

    let footprint =
        sim.world
            .entity_footprint(entity.prototype_id, entity.x, entity.y, direction)?;
    let prototype = sim
        .world
        .prototypes
        .entity(entity.prototype_id)
        .ok_or(BuildError::MissingPrototype(entity.prototype_id))?;

    sim.world
        .validate_entity_footprint_for_prototype(prototype, &footprint, direction)?;
    PlacementValidator::new(&sim.world, &sim.entities, &sim.player, &sim.research)
        .validate_footprint_clear_of_player(&footprint)?;
    sim.entities
        .occupancy
        .validate_available(&footprint, Some(entity_id))?;

    Ok(Some(ValidatedRotation {
        footprint,
        prototype_id: entity.prototype_id,
        impact: impact_for_prototype(sim, entity.prototype_id),
    }))
}
