use super::*;
use crate::construction::{
    Blueprint, BlueprintEntity, ConstructionError, ConstructionJob, ConstructionState, GhostEntity,
    GhostId,
};

/// Placement request for a ghost entity. Ghost placement does not require the
/// build item or a player-clear footprint: ghosts are plans, not buildings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GhostPlacementRequest {
    pub prototype_id: EntityPrototypeId,
    pub x: i32,
    pub y: i32,
    pub direction: Direction,
    pub recipe: Option<RecipeId>,
}

impl Simulation {
    pub fn construction(&self) -> &ConstructionState {
        &self.construction
    }

    /// Captures every placed entity and ghost whose footprint intersects the
    /// tile rectangle into a blueprint, positioned relative to the minimum
    /// captured origin tile. Read-only: used for both the copy tool and the
    /// blueprint library.
    pub fn capture_blueprint(
        &self,
        name: &str,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
    ) -> Result<Blueprint, ConstructionError> {
        capture_blueprint(self, name, min_x, min_y, max_x, max_y)
    }
}

pub(crate) fn validate_ghost_placement(
    sim: &Simulation,
    request: GhostPlacementRequest,
) -> Result<EntityFootprint, ConstructionError> {
    if !placement_validation_ops::entity_is_unlocked(sim, request.prototype_id) {
        return Err(ConstructionError::EntityLocked {
            prototype_id: request.prototype_id,
        });
    }

    let footprint = sim
        .world
        .entity_footprint(
            request.prototype_id,
            request.x,
            request.y,
            request.direction,
        )
        .map_err(ConstructionError::Build)?;
    let prototype =
        sim.world
            .prototypes
            .entity(request.prototype_id)
            .ok_or(ConstructionError::Build(BuildError::MissingPrototype(
                request.prototype_id,
            )))?;
    sim.world
        .validate_entity_footprint_for_prototype(prototype, &footprint, request.direction)
        .map_err(ConstructionError::Build)?;
    sim.entities
        .occupancy
        .validate_available(&footprint, None)
        .map_err(ConstructionError::Build)?;

    for (x, y) in footprint.tiles() {
        if let Some(&ghost_id) = sim.construction.ghost_occupancy.get(&(x, y)) {
            return Err(ConstructionError::GhostOccupied { x, y, ghost_id });
        }
    }

    Ok(footprint)
}

pub(crate) fn place_ghost(
    sim: &mut Simulation,
    request: GhostPlacementRequest,
) -> Result<GhostId, ConstructionError> {
    let footprint = validate_ghost_placement(sim, request)?;

    let ghost_id = GhostId::new(sim.construction.next_ghost_id);
    sim.construction.next_ghost_id += 1;
    for tile in footprint.tiles() {
        sim.construction.ghost_occupancy.insert(tile, ghost_id);
    }
    sim.construction.ghosts.insert(
        ghost_id,
        GhostEntity {
            id: ghost_id,
            prototype_id: request.prototype_id,
            x: request.x,
            y: request.y,
            direction: request.direction,
            footprint,
            recipe: request.recipe,
        },
    );
    sim.construction
        .queue
        .push_back(ConstructionJob::BuildGhost(ghost_id));
    sim.bump_entity_topology_revision();

    Ok(ghost_id)
}

pub(crate) fn cancel_ghost(
    sim: &mut Simulation,
    ghost_id: GhostId,
) -> Result<GhostEntity, ConstructionError> {
    let ghost = remove_ghost(&mut sim.construction, ghost_id)
        .ok_or(ConstructionError::MissingGhost(ghost_id))?;
    sim.bump_entity_topology_revision();
    Ok(ghost)
}

fn remove_ghost(construction: &mut ConstructionState, ghost_id: GhostId) -> Option<GhostEntity> {
    let ghost = construction.ghosts.remove(&ghost_id)?;
    for tile in ghost.footprint.tiles() {
        if construction.ghost_occupancy.get(&tile) == Some(&ghost_id) {
            construction.ghost_occupancy.remove(&tile);
        }
    }
    construction
        .queue
        .retain(|job| *job != ConstructionJob::BuildGhost(ghost_id));
    Some(ghost)
}

/// Builds a ghost immediately from the player inventory: the manual
/// counterpart of a construction robot completing a `BuildGhost` job. The
/// placement itself clears the ghost via the overlap hook in
/// [`placement_mutation_ops`].
pub(crate) fn build_ghost_from_player_inventory(
    sim: &mut Simulation,
    ghost_id: GhostId,
) -> Result<EntityId, ConstructionError> {
    let ghost = sim
        .construction
        .ghosts
        .get(&ghost_id)
        .cloned()
        .ok_or(ConstructionError::MissingGhost(ghost_id))?;
    let build_item = entity_recovery_ops::build_item_for_entity(sim, ghost.prototype_id)
        .map_err(ConstructionError::Destroy)?;

    let entity_id = placement_mutation_ops::place_entity_from_player_inventory(
        sim,
        placement::PlayerPlacementRequest {
            prototype_id: ghost.prototype_id,
            item_id: build_item,
            x: ghost.x,
            y: ghost.y,
            direction: ghost.direction,
        },
    )
    .map_err(ConstructionError::PlayerBuild)?;
    debug_assert!(
        !sim.construction.ghosts.contains_key(&ghost_id),
        "placement overlap hook should have cleared the built ghost"
    );

    // Best-effort: blueprints may carry recipes that are locked or that the
    // placed prototype cannot craft; the ghost still builds in that case.
    if let Some(recipe) = ghost.recipe {
        let _ = sim.select_assembler_recipe(entity_id, recipe);
    }

    Ok(entity_id)
}

/// Removes every ghost whose reserved tiles intersect `footprint`. Called
/// whenever a real entity claims tiles (placement, rotation) so ghosts never
/// overlap placed entities.
pub(crate) fn clear_ghosts_overlapping_footprint(
    sim: &mut Simulation,
    footprint: &EntityFootprint,
) {
    let mut cleared = false;
    for tile in footprint.tiles() {
        let Some(&ghost_id) = sim.construction.ghost_occupancy.get(&tile) else {
            continue;
        };
        remove_ghost(&mut sim.construction, ghost_id);
        cleared = true;
    }
    if cleared {
        sim.bump_entity_topology_revision();
    }
}

/// Drops construction bookkeeping that referenced a now-removed entity.
/// Called from every entity removal path.
pub(crate) fn clear_construction_state_for_removed_entity(
    sim: &mut Simulation,
    entity_id: EntityId,
) {
    if sim.construction.deconstruction_marks.remove(&entity_id) {
        sim.construction
            .queue
            .retain(|job| *job != ConstructionJob::Deconstruct(entity_id));
    }
}

/// Marks every placed entity intersecting the rectangle for deconstruction
/// and removes ghosts in the area (deconstructing a plan cancels it).
/// Returns `(entities_marked, ghosts_removed)`.
pub(crate) fn mark_area_for_deconstruction(
    sim: &mut Simulation,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
) -> (usize, usize) {
    let entity_ids = sim
        .entities
        .occupancy
        .entity_ids_in_tile_rect(min_x, max_x, min_y, max_y);
    let mut marked = 0;
    for entity_id in entity_ids {
        if sim.construction.deconstruction_marks.insert(entity_id) {
            sim.construction
                .queue
                .push_back(ConstructionJob::Deconstruct(entity_id));
            marked += 1;
        }
    }

    let ghost_ids = sim
        .construction
        .ghost_ids_in_tile_rect(min_x, max_x, min_y, max_y);
    let ghosts_removed = ghost_ids.len();
    for ghost_id in ghost_ids {
        remove_ghost(&mut sim.construction, ghost_id);
    }

    if marked > 0 || ghosts_removed > 0 {
        sim.bump_entity_topology_revision();
    }

    (marked, ghosts_removed)
}

/// Unmarks every entity intersecting the rectangle. Returns how many marks
/// were removed.
pub(crate) fn cancel_deconstruction_in_area(
    sim: &mut Simulation,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
) -> usize {
    let entity_ids = sim
        .entities
        .occupancy
        .entity_ids_in_tile_rect(min_x, max_x, min_y, max_y);
    let mut cancelled = 0;
    for entity_id in entity_ids {
        if sim.construction.deconstruction_marks.remove(&entity_id) {
            sim.construction
                .queue
                .retain(|job| *job != ConstructionJob::Deconstruct(entity_id));
            cancelled += 1;
        }
    }

    if cancelled > 0 {
        sim.bump_entity_topology_revision();
    }

    cancelled
}

/// Deconstructs a marked entity into the player inventory: the manual
/// counterpart of a construction robot completing a `Deconstruct` job.
pub(crate) fn deconstruct_marked(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<PlacedEntity, ConstructionError> {
    if !sim.construction.deconstruction_marks.contains(&entity_id) {
        return Err(ConstructionError::NotMarkedForDeconstruction(entity_id));
    }

    entity_recovery_ops::destroy_to_player_inventory(sim, entity_id)
        .map_err(ConstructionError::Destroy)
}

fn capture_blueprint(
    sim: &Simulation,
    name: &str,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
) -> Result<Blueprint, ConstructionError> {
    let mut captured: Vec<(i32, i32, EntityPrototypeId, Direction, Option<RecipeId>)> = Vec::new();

    for entity_id in sim
        .entities
        .occupancy
        .entity_ids_in_tile_rect(min_x, max_x, min_y, max_y)
    {
        let placed = sim
            .entities
            .placed_entity(entity_id)
            .expect("occupancy grid should only reference placed entities");
        let recipe = sim
            .entities
            .assembling_machines
            .get(&entity_id)
            .and_then(|state| state.selected_recipe);
        captured.push((
            placed.x,
            placed.y,
            placed.prototype_id,
            placed.direction,
            recipe,
        ));
    }

    for ghost_id in sim
        .construction
        .ghost_ids_in_tile_rect(min_x, max_x, min_y, max_y)
    {
        let ghost = sim
            .construction
            .ghost(ghost_id)
            .expect("ghost occupancy should only reference existing ghosts");
        captured.push((
            ghost.x,
            ghost.y,
            ghost.prototype_id,
            ghost.direction,
            ghost.recipe,
        ));
    }

    if captured.is_empty() {
        return Err(ConstructionError::EmptyBlueprintArea);
    }

    let origin_x = captured.iter().map(|entry| entry.0).min().unwrap();
    let origin_y = captured.iter().map(|entry| entry.1).min().unwrap();
    captured.sort_by_key(|&(x, y, prototype_id, ..)| (y, x, prototype_id));

    Ok(Blueprint {
        name: name.to_string(),
        entities: captured
            .into_iter()
            .map(|(x, y, prototype_id, direction, recipe)| BlueprintEntity {
                prototype_id,
                dx: x - origin_x,
                dy: y - origin_y,
                direction,
                recipe,
            })
            .collect(),
    })
}

pub(crate) fn save_blueprint_from_area(
    sim: &mut Simulation,
    name: &str,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
) -> Result<usize, ConstructionError> {
    let blueprint = capture_blueprint(sim, name, min_x, min_y, max_x, max_y)?;
    sim.construction.blueprints.push(blueprint);
    Ok(sim.construction.blueprints.len() - 1)
}

pub(crate) fn delete_blueprint(
    sim: &mut Simulation,
    index: usize,
) -> Result<Blueprint, ConstructionError> {
    if index >= sim.construction.blueprints.len() {
        return Err(ConstructionError::MissingBlueprint { index });
    }

    Ok(sim.construction.blueprints.remove(index))
}

/// Places one ghost per blueprint entry with the blueprint origin at
/// `(x, y)`. Entries that cannot be placed (occupied tiles, invalid terrain,
/// locked or unknown prototypes) are skipped. Returns `(placed, skipped)`.
pub(crate) fn paste_blueprint_ghosts(
    sim: &mut Simulation,
    entities: &[BlueprintEntity],
    x: i32,
    y: i32,
) -> (usize, usize) {
    let mut placed = 0;
    let mut skipped = 0;

    for entity in entities {
        let request = GhostPlacementRequest {
            prototype_id: entity.prototype_id,
            x: x + entity.dx,
            y: y + entity.dy,
            direction: entity.direction,
            recipe: entity.recipe,
        };
        match place_ghost(sim, request) {
            Ok(_) => placed += 1,
            Err(_) => skipped += 1,
        }
    }

    (placed, skipped)
}

/// Placement preview for a ghost at the cursor: like the player build
/// preview, but without the item, unlock-by-inventory, and player-in-the-way
/// checks that do not apply to plans.
pub fn preview_ghost_placement(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    x: i32,
    y: i32,
    direction: Direction,
) -> BuildPlacementPreview {
    let mut preview = BuildPlacementPreview {
        footprint: None,
        issues: Vec::new(),
    };
    let Some(prototype) = sim.world.prototypes.entity(prototype_id) else {
        preview.issues.push(BuildPlacementIssue {
            tile: None,
            kind: BuildPlacementIssueKind::MissingPrototype(prototype_id),
        });
        return preview;
    };

    if !placement_validation_ops::entity_is_unlocked(sim, prototype_id) {
        preview.issues.push(BuildPlacementIssue {
            tile: None,
            kind: BuildPlacementIssueKind::EntityLocked { prototype_id },
        });
    }

    let footprint = EntityFootprint::from_size(x, y, prototype.size.x, prototype.size.y, direction);
    if footprint.validate().is_err() {
        preview.issues.push(BuildPlacementIssue {
            tile: None,
            kind: BuildPlacementIssueKind::InvalidFootprint {
                width: footprint.width,
                height: footprint.height,
            },
        });
        return preview;
    }
    preview.footprint = Some(footprint);

    if sim
        .world
        .validate_entity_footprint_for_prototype(prototype, &footprint, direction)
        .is_err()
    {
        preview.issues.push(BuildPlacementIssue {
            tile: None,
            kind: BuildPlacementIssueKind::TerrainBlocked,
        });
    }
    for (tile_x, tile_y) in footprint.tiles() {
        if let Some(entity_id) = sim.entities.occupancy.entity_at(tile_x, tile_y) {
            preview.issues.push(BuildPlacementIssue {
                tile: Some((tile_x, tile_y)),
                kind: BuildPlacementIssueKind::EntityOccupied { entity_id },
            });
        }
        if sim
            .construction
            .ghost_occupancy
            .contains_key(&(tile_x, tile_y))
        {
            preview.issues.push(BuildPlacementIssue {
                tile: Some((tile_x, tile_y)),
                kind: BuildPlacementIssueKind::GhostOccupied,
            });
        }
    }

    preview
}
