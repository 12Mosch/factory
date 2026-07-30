use super::placement::PlayerPlacementRequest;
use super::*;
use crate::rail::RailConnectionPreview;

/// The connection each end of a prospective rail placement would form.
///
/// Separate from [`BuildPlacementPreview`] because it is not a problem with the
/// placement: it is what the placement would *do*, which is the thing a player
/// laying track needs to see. Empty for anything that is not a rail.
pub(crate) fn rail_connection_preview(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    x: WorldTileCoord,
    y: WorldTileCoord,
    direction: Direction,
) -> Vec<RailConnectionPreview> {
    let Some(prototype) = sim.world.prototypes.entity(prototype_id) else {
        return Vec::new();
    };
    let footprint = EntityFootprint::from_size(x, y, prototype.size.x, prototype.size.y, direction);

    rail_ops::placement_connections(sim, prototype_id, &footprint, direction)
}

/// Cursor preview for rolling stock.
///
/// Rolling stock has no footprint to paint, so the preview covers the cursor
/// tile — the tile whose rail the piece would be put on — and the issues it
/// reports are the ones the rolling-stock placement path itself would raise.
/// Asking that path rather than re-deriving the answer is what keeps the cursor
/// from promising a placement the click would then refuse.
fn rolling_stock_preview(
    sim: &Simulation,
    request: PlayerPlacementRequest,
) -> BuildPlacementPreview {
    let mut preview = BuildPlacementPreview {
        footprint: Some(EntityFootprint::single_tile(request.x, request.y)),
        issues: Vec::new(),
    };
    let Err(error) = sim.validate_rolling_stock_placement(
        request.prototype_id,
        request.item_id,
        request.x,
        request.y,
    ) else {
        return preview;
    };

    let kind = match error {
        RollingStockPlacementError::InsufficientInventory { item_id } => {
            BuildPlacementIssueKind::InsufficientInventory { item_id }
        }
        RollingStockPlacementError::Locked(prototype_id) => {
            BuildPlacementIssueKind::EntityLocked { prototype_id }
        }
        RollingStockPlacementError::ItemDoesNotBuildStock {
            item_id,
            prototype_id,
        } => BuildPlacementIssueKind::ItemDoesNotBuildEntity {
            item_id,
            prototype_id,
        },
        RollingStockPlacementError::MissingBuildItem(prototype_id)
        | RollingStockPlacementError::NotRollingStock(prototype_id) => {
            BuildPlacementIssueKind::MissingBuildItem { prototype_id }
        }
        // No rail, too short a run, or another wagon already there: three ways
        // of saying the cursor is not over track this piece could stand on.
        RollingStockPlacementError::NoRail
        | RollingStockPlacementError::TrackTooShort
        | RollingStockPlacementError::Occupied(_) => BuildPlacementIssueKind::NeedsClearRail {
            prototype_id: request.prototype_id,
        },
    };
    preview.issues.push(BuildPlacementIssue {
        tile: Some((request.x, request.y)),
        kind,
    });
    preview
}

pub(crate) fn preview_from_player_inventory(
    sim: &Simulation,
    request: PlayerPlacementRequest,
) -> BuildPlacementPreview {
    let mut preview = BuildPlacementPreview {
        footprint: None,
        issues: Vec::new(),
    };
    let Some(prototype) = sim.world.prototypes.entity(request.prototype_id) else {
        preview.issues.push(BuildPlacementIssue {
            tile: None,
            kind: BuildPlacementIssueKind::MissingPrototype(request.prototype_id),
        });
        return preview;
    };

    if prototype.rolling_stock.is_some() {
        return rolling_stock_preview(sim, request);
    }

    let footprint = EntityFootprint::from_size(
        request.x,
        request.y,
        prototype.size.x,
        prototype.size.y,
        request.direction,
    );
    match footprint.validate() {
        Ok(()) => {
            preview.footprint = Some(footprint);
        }
        Err(BuildError::InvalidFootprint { width, height }) => {
            preview.issues.push(BuildPlacementIssue {
                tile: None,
                kind: BuildPlacementIssueKind::InvalidFootprint { width, height },
            });
        }
        Err(_) => unreachable!("footprint validation only reports invalid dimensions"),
    }

    match prototype.build_item {
        Some(build_item) => match sim.world.prototypes.item(request.item_id) {
            Some(item) if item.id != build_item => {
                preview.issues.push(BuildPlacementIssue {
                    tile: None,
                    kind: BuildPlacementIssueKind::ItemDoesNotBuildEntity {
                        item_id: request.item_id,
                        prototype_id: request.prototype_id,
                    },
                });
            }
            Some(_) => {}
            None => {
                preview.issues.push(BuildPlacementIssue {
                    tile: None,
                    kind: BuildPlacementIssueKind::MissingBuildItem {
                        prototype_id: request.prototype_id,
                    },
                });
            }
        },
        None => {
            preview.issues.push(BuildPlacementIssue {
                tile: None,
                kind: BuildPlacementIssueKind::MissingBuildItem {
                    prototype_id: request.prototype_id,
                },
            });
        }
    }

    if !placement_validation_ops::entity_is_unlocked(sim, request.prototype_id) {
        preview.issues.push(BuildPlacementIssue {
            tile: None,
            kind: BuildPlacementIssueKind::EntityLocked {
                prototype_id: request.prototype_id,
            },
        });
    }
    if sim.player_inventory.count(request.item_id) == 0 {
        preview.issues.push(BuildPlacementIssue {
            tile: None,
            kind: BuildPlacementIssueKind::InsufficientInventory {
                item_id: request.item_id,
            },
        });
    }

    if let Some(footprint) = preview.footprint {
        collect_placement_preview_issues_for_footprint(
            sim,
            prototype,
            &footprint,
            request.direction,
            &mut preview.issues,
        );
    }

    preview
}

fn collect_placement_preview_issues_for_footprint(
    sim: &Simulation,
    prototype: &factory_data::EntityPrototype,
    footprint: &EntityFootprint,
    direction: Direction,
    issues: &mut Vec<BuildPlacementIssue>,
) {
    if prototype.entity_kind == EntityKind::Pumpjack && prototype.pumpjack.is_some() {
        collect_pumpjack_preview_issues(sim, prototype, footprint, issues);
    } else if prototype.entity_kind == EntityKind::MiningDrill && prototype.mining_drill.is_some() {
        collect_mining_drill_preview_issues(sim, prototype, footprint, issues);
    } else {
        for (x, y) in footprint.tiles() {
            match sim.world.tile_at(x, y) {
                Some(tile) if tile.collision.buildable => {}
                Some(_) => issues.push(BuildPlacementIssue {
                    tile: Some((x, y)),
                    kind: BuildPlacementIssueKind::TerrainBlocked,
                }),
                None => issues.push(BuildPlacementIssue {
                    tile: Some((x, y)),
                    kind: BuildPlacementIssueKind::OutsideGeneratedChunks,
                }),
            }
        }
    }

    if prototype.entity_kind == EntityKind::OffshorePump && prototype.offshore_pump.is_some() {
        let water_tiles = offshore_pump_water_tiles(footprint, direction);
        if !water_tiles
            .iter()
            .any(|(x, y)| sim.world.tile_at(*x, *y).is_some_and(is_water_like_tile))
        {
            for tile in water_tiles {
                issues.push(BuildPlacementIssue {
                    tile: Some(tile),
                    kind: BuildPlacementIssueKind::MissingAdjacentWater,
                });
            }
        }
    }

    let player_tile = sim.player.tile_position();
    if footprint.contains_tile(player_tile.0, player_tile.1) {
        issues.push(BuildPlacementIssue {
            tile: Some(player_tile),
            kind: BuildPlacementIssueKind::PlayerOccupied,
        });
    }

    // Track laid over track: the same rule placement validation applies, so a
    // preview never shows a rail as placeable that the placement would refuse.
    if let Err(BuildError::EntityOccupied { x, y, entity_id }) =
        placement_validation_ops::validate_rail_placement(
            sim,
            prototype.id,
            footprint,
            direction,
            None,
        )
    {
        issues.push(BuildPlacementIssue {
            tile: Some((x, y)),
            kind: BuildPlacementIssueKind::EntityOccupied { entity_id },
        });
    }

    // A signal with no aligned joint beside it, or one over a crossing another
    // signal already governs: the same rule placement validation applies, for
    // the same reason the rail case above is mirrored here.
    match placement_validation_ops::validate_rail_signal_placement(
        sim,
        prototype.id,
        footprint,
        direction,
        None,
    ) {
        Ok(()) => {}
        Err(BuildError::NeedsAlignedRail { prototype_id }) => issues.push(BuildPlacementIssue {
            tile: Some((footprint.x, footprint.y)),
            kind: BuildPlacementIssueKind::NeedsAlignedRail { prototype_id },
        }),
        Err(BuildError::EntityOccupied { x, y, entity_id }) => issues.push(BuildPlacementIssue {
            tile: Some((x, y)),
            kind: BuildPlacementIssueKind::EntityOccupied { entity_id },
        }),
        Err(_) => {}
    }

    for (x, y) in footprint.tiles() {
        if let Some(entity_id) = sim.entities.occupancy.entity_at(x, y) {
            issues.push(BuildPlacementIssue {
                tile: Some((x, y)),
                kind: BuildPlacementIssueKind::EntityOccupied { entity_id },
            });
        }
    }
}

fn collect_pumpjack_preview_issues(
    sim: &Simulation,
    prototype: &factory_data::EntityPrototype,
    footprint: &EntityFootprint,
    issues: &mut Vec<BuildPlacementIssue>,
) {
    for (x, y) in footprint.tiles() {
        match sim.world.tile_at(x, y) {
            Some(tile) if tile.collision.walkable => {}
            Some(_) => issues.push(BuildPlacementIssue {
                tile: Some((x, y)),
                kind: BuildPlacementIssueKind::TerrainBlocked,
            }),
            None => issues.push(BuildPlacementIssue {
                tile: Some((x, y)),
                kind: BuildPlacementIssueKind::OutsideGeneratedChunks,
            }),
        }
    }

    let pumpjack = prototype
        .pumpjack
        .as_ref()
        .expect("pumpjack prototype should have pumpjack metadata");
    if footprint.tiles().into_iter().all(|(x, y)| {
        sim.world
            .tile_at(x, y)
            .and_then(|tile| tile.resource)
            .is_none_or(|resource| resource.resource_item != pumpjack.resource_item)
    }) {
        for tile in footprint.tiles() {
            issues.push(BuildPlacementIssue {
                tile: Some(tile),
                kind: BuildPlacementIssueKind::MissingRequiredResource,
            });
        }
    }
}

fn collect_mining_drill_preview_issues(
    sim: &Simulation,
    prototype: &factory_data::EntityPrototype,
    footprint: &EntityFootprint,
    issues: &mut Vec<BuildPlacementIssue>,
) {
    for (x, y) in footprint.tiles() {
        match sim.world.tile_at(x, y) {
            Some(tile) if tile.collision.walkable => {}
            Some(_) => issues.push(BuildPlacementIssue {
                tile: Some((x, y)),
                kind: BuildPlacementIssueKind::TerrainBlocked,
            }),
            None => issues.push(BuildPlacementIssue {
                tile: Some((x, y)),
                kind: BuildPlacementIssueKind::OutsideGeneratedChunks,
            }),
        }
    }

    let mining_drill = prototype
        .mining_drill
        .as_ref()
        .expect("mining drill prototype should have mining metadata");
    let mining_tiles = mining_area_tiles(footprint, mining_drill);
    if mining_tiles.iter().all(|(x, y)| {
        sim.world
            .tile_at(*x, *y)
            .and_then(|tile| tile.resource)
            .is_none()
    }) {
        for tile in mining_tiles {
            issues.push(BuildPlacementIssue {
                tile: Some(tile),
                kind: BuildPlacementIssueKind::MissingRequiredResource,
            });
        }
    }
}
