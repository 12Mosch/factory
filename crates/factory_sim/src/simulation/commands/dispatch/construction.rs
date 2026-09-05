use super::*;

pub(super) fn apply(
    sim: &mut Simulation,
    command: &SimCommand,
) -> Result<SimCommandEffect, SimCommandError> {
    match command {
        SimCommand::PlaceEntityFromPlayerInventory {
            prototype_id,
            item_id,
            x,
            y,
            direction,
        } => {
            let entity_id = placement::place_from_player_inventory(
                sim,
                placement::PlayerPlacementRequest {
                    prototype_id: *prototype_id,
                    item_id: *item_id,
                    x: *x,
                    y: *y,
                    direction: *direction,
                },
            )
            .map_err(SimCommandError::Build)?;
            sim.record_early_game_placement(*item_id);
            Ok(SimCommandEffect::EntityPlaced(entity_id))
        }
        SimCommand::PlaceTileFromPlayerInventory { item_id, x, y } => {
            tile_placement_ops::place_tile_from_player_inventory(
                sim,
                TilePlacementRequest {
                    item_id: *item_id,
                    x: *x,
                    y: *y,
                },
            )
            .map_err(SimCommandError::TilePlacement)?;
            Ok(SimCommandEffect::TilePlaced {
                item_id: *item_id,
                x: *x,
                y: *y,
            })
        }
        SimCommand::PlaceGhost {
            prototype_id,
            x,
            y,
            direction,
        } => {
            let ghost_id = construction_ops::place_ghost(
                sim,
                GhostPlacementRequest {
                    prototype_id: *prototype_id,
                    x: *x,
                    y: *y,
                    direction: *direction,
                    recipe: None,
                },
            )
            .map_err(SimCommandError::Construction)?;
            Ok(SimCommandEffect::GhostPlaced(ghost_id))
        }
        SimCommand::CancelGhost { ghost_id } => {
            construction_ops::cancel_ghost(sim, *ghost_id)
                .map_err(SimCommandError::Construction)?;
            Ok(SimCommandEffect::None)
        }
        SimCommand::BuildGhost { ghost_id } => {
            let entity_id = construction_ops::build_ghost_from_player_inventory(sim, *ghost_id)
                .map_err(SimCommandError::Construction)?;
            let item_id = entity_recovery_ops::build_item_for_entity(
                sim,
                sim.entities
                    .placed_entity(entity_id)
                    .expect("newly built ghost should be placed")
                    .prototype_id,
            )
            .expect("placed entity should have a build item");
            sim.record_early_game_placement(item_id);
            Ok(SimCommandEffect::EntityPlaced(entity_id))
        }
        SimCommand::MarkDeconstruction {
            min_x,
            min_y,
            max_x,
            max_y,
        } => {
            let (marked, ghosts_removed) =
                construction_ops::mark_area_for_deconstruction(sim, *min_x, *min_y, *max_x, *max_y);
            Ok(SimCommandEffect::DeconstructionMarked {
                marked,
                ghosts_removed,
            })
        }
        SimCommand::CancelDeconstruction {
            min_x,
            min_y,
            max_x,
            max_y,
        } => {
            let cancelled = construction_ops::cancel_deconstruction_in_area(
                sim, *min_x, *min_y, *max_x, *max_y,
            );
            Ok(SimCommandEffect::DeconstructionCancelled { cancelled })
        }
        SimCommand::DeconstructEntity { entity_id } => {
            let item_id = sim.entities.placed_entity(*entity_id).and_then(|placed| {
                entity_recovery_ops::build_item_for_entity(sim, placed.prototype_id).ok()
            });
            let count_before = item_id.map(|item_id| sim.player_inventory.count(item_id));
            construction_ops::deconstruct_marked(sim, *entity_id)
                .map_err(SimCommandError::Construction)?;
            Ok(item_gain_effect(sim, item_id, count_before))
        }
        SimCommand::PasteBlueprint { entities, x, y } => {
            let (placed, skipped) = construction_ops::paste_blueprint_ghosts(sim, entities, *x, *y);
            Ok(SimCommandEffect::BlueprintPasted { placed, skipped })
        }
        SimCommand::SaveBlueprint {
            name,
            min_x,
            min_y,
            max_x,
            max_y,
        } => {
            let index = construction_ops::save_blueprint_from_area(
                sim, name, *min_x, *min_y, *max_x, *max_y,
            )
            .map_err(SimCommandError::Construction)?;
            Ok(SimCommandEffect::BlueprintSaved { index })
        }
        SimCommand::DeleteBlueprint { index } => {
            construction_ops::delete_blueprint(sim, *index)
                .map_err(SimCommandError::Construction)?;
            Ok(SimCommandEffect::None)
        }
        SimCommand::RenameBlueprint { index, name } => {
            construction_ops::rename_blueprint(sim, *index, name.clone())
                .map_err(SimCommandError::Construction)?;
            Ok(SimCommandEffect::None)
        }
        _ => unreachable!("non-construction command routed to construction dispatcher"),
    }
}
