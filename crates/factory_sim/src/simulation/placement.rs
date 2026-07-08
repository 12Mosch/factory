use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntityPlacementRequest {
    pub prototype_id: EntityPrototypeId,
    pub x: i32,
    pub y: i32,
    pub direction: Direction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerPlacementRequest {
    pub prototype_id: EntityPrototypeId,
    pub item_id: ItemId,
    pub x: i32,
    pub y: i32,
    pub direction: Direction,
}

pub fn preview_from_player_inventory(
    sim: &Simulation,
    request: PlayerPlacementRequest,
) -> BuildPlacementPreview {
    placement_preview_ops::preview_from_player_inventory(sim, request)
}

pub fn validate(
    sim: &Simulation,
    request: EntityPlacementRequest,
) -> Result<EntityFootprint, BuildError> {
    placement_validation_ops::validate_entity_placement(sim, request)
}

pub fn validate_from_player_inventory(
    sim: &Simulation,
    request: PlayerPlacementRequest,
) -> Result<EntityFootprint, PlayerBuildError> {
    placement_validation_ops::validate_player_inventory_placement(sim, request)
}

pub fn place(
    sim: &mut Simulation,
    request: EntityPlacementRequest,
) -> Result<EntityId, BuildError> {
    placement_mutation_ops::place_entity(sim, request)
}

pub fn place_from_player_inventory(
    sim: &mut Simulation,
    request: PlayerPlacementRequest,
) -> Result<EntityId, PlayerBuildError> {
    placement_mutation_ops::place_entity_from_player_inventory(sim, request)
}
