use super::placement::PlayerPlacementRequest;
use super::*;

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
    if prototype.entity_kind == EntityKind::MiningDrill && prototype.mining_drill.is_some() {
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

    for (x, y) in footprint.tiles() {
        if let Some(entity_id) = sim.entities.occupancy.entity_at(x, y) {
            issues.push(BuildPlacementIssue {
                tile: Some((x, y)),
                kind: BuildPlacementIssueKind::EntityOccupied { entity_id },
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
