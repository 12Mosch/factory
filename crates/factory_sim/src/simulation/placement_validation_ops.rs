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

/// Rejects a rail whose end would land on an existing rail end facing the same
/// way — two pieces of track laid over each other rather than a join. See
/// [`rail_ops::conflicting_rail_end`] for why this rule belongs to the rail
/// graph rather than to the occupancy grid.
pub(crate) fn validate_rail_placement(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    footprint: &EntityFootprint,
    direction: Direction,
    ignored_entity_id: Option<EntityId>,
) -> Result<(), BuildError> {
    let Some(ends) =
        rail_ops::rail_ends_for_placement(&sim.world, prototype_id, footprint, direction)
    else {
        return Ok(());
    };
    let Some((end, entity_id)) = rail_ops::conflicting_rail_end(sim, ends, ignored_entity_id)
    else {
        return Ok(());
    };

    let (x, y) = end.position.tile();
    Err(BuildError::EntityOccupied { x, y, entity_id })
}

/// Rejects a signal that would govern nothing, or that would govern a crossing
/// another signal already does.
///
/// A signal is a rail end together with a heading through it. It binds to the
/// nearest end to its own tile, and both a rail leaving that end the way the
/// signal faces and a rail entering it from the other side have to be there:
/// with either missing there is no crossing to govern, and the block partition
/// would still be cut by a signal that did nothing. Which is worse than refusing
/// it, because a railway silently split in two looks exactly like a railway.
pub(crate) fn validate_rail_signal_placement(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    footprint: &EntityFootprint,
    direction: Direction,
    ignored_entity_id: Option<EntityId>,
) -> Result<(), BuildError> {
    if sim
        .world
        .prototypes
        .entity(prototype_id)
        .is_none_or(|prototype| !prototype.entity_kind.is_rail_signal())
    {
        return Ok(());
    }

    let needs_rail = || BuildError::NeedsAlignedRail { prototype_id };
    let position =
        rail_ops::signal_binding(sim, footprint.x, footprint.y).ok_or_else(needs_rail)?;
    // The approach and the far side, in that order: a train travelling
    // `direction` leaves the rail whose end here faces that way and enters the
    // one whose end here faces back.
    if rail_ops::rail_end_at(sim, position, direction).is_none()
        || rail_ops::rail_end_at(sim, position, direction.opposite()).is_none()
    {
        return Err(needs_rail());
    }
    if let Some(entity_id) =
        rail_ops::signal_governing_crossing(sim, position, direction, ignored_entity_id)
    {
        let (x, y) = position.tile();
        return Err(BuildError::EntityOccupied { x, y, entity_id });
    }

    Ok(())
}

pub(crate) fn validate_entity_placement(
    sim: &Simulation,
    request: EntityPlacementRequest,
) -> Result<EntityFootprint, BuildError> {
    // Rolling stock never reaches the tile grid: it has a position along a rail
    // edge instead of a footprint, and goes through
    // [`Simulation::place_rolling_stock_from_player_inventory`]. Refusing it
    // here is what keeps every other path into placement — ghosts, blueprints,
    // construction robots — from inventing a wagon that occupies tiles.
    if sim
        .world
        .prototypes
        .entity(request.prototype_id)
        .is_some_and(|prototype| prototype.rolling_stock.is_some())
    {
        return Err(BuildError::RunsOnRails {
            prototype_id: request.prototype_id,
        });
    }
    let footprint = PlacementValidator::new(&sim.world, &sim.entities, &sim.player, &sim.research)
        .validate_entity_placement(request)?;
    validate_rail_placement(
        sim,
        request.prototype_id,
        &footprint,
        request.direction,
        None,
    )?;
    validate_rail_signal_placement(
        sim,
        request.prototype_id,
        &footprint,
        request.direction,
        None,
    )?;

    Ok(footprint)
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
    // A rotating rail may not turn into a duplicate of a neighbour's track, but
    // its own current ends are not in its way.
    validate_rail_placement(
        sim,
        entity.prototype_id,
        &footprint,
        direction,
        Some(entity_id),
    )?;
    // Rotating a signal is what changes which way it governs, so the same rule
    // applies: the new heading has to be one the track under it actually runs,
    // and it may not turn into a second signal over a crossing that has one.
    validate_rail_signal_placement(
        sim,
        entity.prototype_id,
        &footprint,
        direction,
        Some(entity_id),
    )?;

    Ok(Some(ValidatedRotation {
        footprint,
        prototype_id: entity.prototype_id,
        impact: impact_for_prototype(sim, entity.prototype_id),
    }))
}
