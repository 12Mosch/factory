use super::*;

use factory_data::TilePlacementPrototype;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TilePlacementRequest {
    pub item_id: ItemId,
    pub x: WorldTileCoord,
    pub y: WorldTileCoord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TilePlacementError {
    UnknownItem(ItemId),
    /// The item has no `place_as_tile` metadata, so it builds an entity (or
    /// nothing) rather than paving.
    ItemDoesNotPlaceTile {
        item_id: ItemId,
    },
    InsufficientInventory {
        item_id: ItemId,
    },
    OutsideGeneratedChunks {
        x: WorldTileCoord,
        y: WorldTileCoord,
    },
    /// A fill item (landfill) was aimed at solid ground.
    RequiresWater {
        x: WorldTileCoord,
        y: WorldTileCoord,
    },
    /// A paving item (stone path, concrete) was aimed at water.
    RequiresSolidGround {
        x: WorldTileCoord,
        y: WorldTileCoord,
    },
    /// The target already carries the requested tile.
    AlreadyPlaced {
        x: WorldTileCoord,
        y: WorldTileCoord,
    },
}

/// Tile-paving metadata for an item, if it has any.
pub fn tile_placement_for_item(
    prototypes: &PrototypeCatalog,
    item_id: ItemId,
) -> Option<TilePlacementPrototype> {
    prototypes.item(item_id).and_then(|item| item.place_as_tile)
}

/// Checks every rule that governs paving a tile, without mutating anything.
/// Shared by the placement command and the build preview so the cursor never
/// disagrees with what a click will do.
pub fn validate_tile_placement(
    sim: &Simulation,
    request: TilePlacementRequest,
) -> Result<TilePlacementPrototype, TilePlacementError> {
    let TilePlacementRequest { item_id, x, y } = request;
    if sim.world.prototypes.item(item_id).is_none() {
        return Err(TilePlacementError::UnknownItem(item_id));
    }
    let placement = tile_placement_for_item(&sim.world.prototypes, item_id)
        .ok_or(TilePlacementError::ItemDoesNotPlaceTile { item_id })?;

    let tile = sim
        .world
        .tile_at(x, y)
        .ok_or(TilePlacementError::OutsideGeneratedChunks { x, y })?;

    // Fill and paving items cover disjoint terrain: landfill is only useful on
    // water, and paving must not bridge it.
    if placement.fills_water {
        if tile.collision.walkable {
            return Err(TilePlacementError::RequiresWater { x, y });
        }
    } else if !tile.collision.walkable {
        return Err(TilePlacementError::RequiresSolidGround { x, y });
    }

    if tile.tile_id == placement.tile {
        return Err(TilePlacementError::AlreadyPlaced { x, y });
    }

    Ok(placement)
}

pub fn place_tile_from_player_inventory(
    sim: &mut Simulation,
    request: TilePlacementRequest,
) -> Result<(), TilePlacementError> {
    let placement = validate_tile_placement(sim, request)?;
    if sim.player_inventory.count(request.item_id) == 0 {
        return Err(TilePlacementError::InsufficientInventory {
            item_id: request.item_id,
        });
    }

    // Validation already proved the chunk is generated, the tile differs, and
    // the id came from the catalog, so the write cannot be rejected.
    sim.world
        .set_tile(request.x, request.y, placement.tile)
        .expect("validated tile placement should be writable");
    sim.player_inventory
        .remove(request.item_id, 1)
        .expect("validated placement item should remain removable");
    sim.record_item_consumed(request.item_id, 1);

    Ok(())
}
