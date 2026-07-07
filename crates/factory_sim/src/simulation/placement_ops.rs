use super::*;

impl Simulation {
    pub fn preview_entity_placement_from_player_inventory(
        &self,
        prototype_id: EntityPrototypeId,
        item_id: ItemId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> BuildPlacementPreview {
        let mut preview = BuildPlacementPreview {
            footprint: None,
            issues: Vec::new(),
        };
        let Some(prototype) = self.world.prototypes.entity(prototype_id) else {
            preview.issues.push(BuildPlacementIssue {
                tile: None,
                kind: BuildPlacementIssueKind::MissingPrototype(prototype_id),
            });
            return preview;
        };

        let footprint =
            EntityFootprint::from_size(x, y, prototype.size.x, prototype.size.y, direction);
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
            Some(build_item) => match self.world.prototypes.item(item_id) {
                Some(item) if item.id != build_item => {
                    preview.issues.push(BuildPlacementIssue {
                        tile: None,
                        kind: BuildPlacementIssueKind::ItemDoesNotBuildEntity {
                            item_id,
                            prototype_id,
                        },
                    });
                }
                Some(_) => {}
                None => {
                    preview.issues.push(BuildPlacementIssue {
                        tile: None,
                        kind: BuildPlacementIssueKind::MissingBuildItem { prototype_id },
                    });
                }
            },
            None => {
                preview.issues.push(BuildPlacementIssue {
                    tile: None,
                    kind: BuildPlacementIssueKind::MissingBuildItem { prototype_id },
                });
            }
        }

        if !self.is_entity_unlocked(prototype_id) {
            preview.issues.push(BuildPlacementIssue {
                tile: None,
                kind: BuildPlacementIssueKind::EntityLocked { prototype_id },
            });
        }
        if self.player_inventory.count(item_id) == 0 {
            preview.issues.push(BuildPlacementIssue {
                tile: None,
                kind: BuildPlacementIssueKind::InsufficientInventory { item_id },
            });
        }

        if let Some(footprint) = preview.footprint {
            self.collect_placement_preview_issues_for_footprint(
                prototype,
                &footprint,
                direction,
                &mut preview.issues,
            );
        }

        preview
    }

    pub fn can_place_entity_from_player_inventory(
        &self,
        prototype_id: EntityPrototypeId,
        item_id: ItemId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> Result<EntityFootprint, PlayerBuildError> {
        let prototype = self
            .world
            .prototypes
            .entity(prototype_id)
            .ok_or(PlayerBuildError::MissingPrototype(prototype_id))?;
        let build_item = prototype
            .build_item
            .ok_or(PlayerBuildError::MissingBuildItem { prototype_id })?;

        let item = self
            .world
            .prototypes
            .item(item_id)
            .ok_or(PlayerBuildError::MissingBuildItem { prototype_id })?;
        if item.id != build_item {
            return Err(PlayerBuildError::ItemDoesNotBuildEntity {
                item_id,
                prototype_id,
            });
        }
        if !self.is_entity_unlocked(prototype_id) {
            return Err(PlayerBuildError::EntityLocked { prototype_id });
        }
        if self.player_inventory.count(item_id) == 0 {
            return Err(PlayerBuildError::InsufficientInventory { item_id });
        }

        self.can_place_entity(prototype_id, x, y, direction)
            .map_err(PlayerBuildError::Build)
    }

    pub fn place_entity_from_player_inventory(
        &mut self,
        prototype_id: EntityPrototypeId,
        item_id: ItemId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> Result<EntityId, PlayerBuildError> {
        self.can_place_entity_from_player_inventory(prototype_id, item_id, x, y, direction)?;

        let entity_id = self
            .place_entity(prototype_id, x, y, direction)
            .map_err(PlayerBuildError::Build)?;
        self.player_inventory
            .remove(item_id, 1)
            .expect("validated player build item should remain removable");

        Ok(entity_id)
    }

    pub fn can_place_entity(
        &self,
        prototype_id: EntityPrototypeId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> Result<EntityFootprint, BuildError> {
        let footprint = self.world.entity_footprint(prototype_id, x, y, direction)?;
        let prototype = self
            .world
            .prototypes
            .entity(prototype_id)
            .ok_or(BuildError::MissingPrototype(prototype_id))?;
        self.world
            .validate_entity_footprint_for_prototype(prototype, &footprint, direction)?;
        self.validate_footprint_clear_of_player(&footprint)?;
        self.entities
            .occupancy
            .validate_available(&footprint, None)?;

        Ok(footprint)
    }

    pub fn place_entity(
        &mut self,
        prototype_id: EntityPrototypeId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> Result<EntityId, BuildError> {
        let footprint = self.can_place_entity(prototype_id, x, y, direction)?;
        let prototype = &self.world.prototypes.entities[prototype_id.index()];
        let reservation =
            reservation_for_prototype(prototype, prototype_id, x, y, direction, footprint);
        let affects_power_topology = self.prototype_affects_power_topology(prototype);
        let affects_transport_lane_graph = self.prototype_affects_transport_lane_graph(prototype);
        let entity_id = self.entities.reserve_entity(reservation);
        if affects_power_topology {
            self.invalidate_power_state();
        }
        if affects_transport_lane_graph {
            self.invalidate_transport_lane_graph();
        }
        self.invalidate_fluid_state();
        self.bump_entity_topology_revision();
        Ok(entity_id)
    }

    pub fn rotate_entity(
        &mut self,
        entity_id: EntityId,
        direction: Direction,
    ) -> Result<(), BuildError> {
        let entity = self
            .entities
            .placed_entity(entity_id)
            .cloned()
            .ok_or(BuildError::MissingEntity(entity_id))?;
        if entity.direction == direction {
            return Ok(());
        }
        let footprint =
            self.world
                .entity_footprint(entity.prototype_id, entity.x, entity.y, direction)?;
        let prototype = self
            .world
            .prototypes
            .entity(entity.prototype_id)
            .ok_or(BuildError::MissingPrototype(entity.prototype_id))?;

        self.world
            .validate_entity_footprint_for_prototype(prototype, &footprint, direction)?;
        self.validate_footprint_clear_of_player(&footprint)?;
        self.entities
            .occupancy
            .validate_available(&footprint, Some(entity_id))?;
        let affects_power_topology = self.prototype_affects_power_topology(prototype);
        let affects_transport_lane_graph = self.prototype_affects_transport_lane_graph(prototype);
        self.entities
            .update_entity_footprint(entity_id, direction, footprint)?;
        if affects_power_topology {
            self.invalidate_power_state();
        }
        if affects_transport_lane_graph {
            self.invalidate_transport_lane_graph();
        }
        self.invalidate_fluid_state();
        self.bump_entity_topology_revision();
        Ok(())
    }

    pub fn remove_entity(&mut self, entity_id: EntityId) -> Option<PlacedEntity> {
        let removed = self.entities.remove_placed_entity(entity_id);
        if let Some(removed) = &removed {
            self.invalidate_after_entity_removal(removed);
        }
        removed
    }

    pub(super) fn invalidate_after_entity_removal(&mut self, removed: &PlacedEntity) {
        let prototype = self.world.prototypes.entity(removed.prototype_id);
        let affects_power =
            prototype.is_some_and(|prototype| self.prototype_affects_power_topology(prototype));
        let affects_lanes = prototype
            .is_some_and(|prototype| self.prototype_affects_transport_lane_graph(prototype));
        if affects_power {
            self.invalidate_power_state();
        }
        if affects_lanes {
            self.invalidate_transport_lane_graph();
        }
        self.invalidate_fluid_state();
        self.bump_entity_topology_revision();
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

    fn collect_placement_preview_issues_for_footprint(
        &self,
        prototype: &factory_data::EntityPrototype,
        footprint: &EntityFootprint,
        direction: Direction,
        issues: &mut Vec<BuildPlacementIssue>,
    ) {
        if prototype.entity_kind == EntityKind::MiningDrill && prototype.mining_drill.is_some() {
            self.collect_mining_drill_preview_issues(prototype, footprint, issues);
        } else {
            for (x, y) in footprint.tiles() {
                match self.world.tile_at(x, y) {
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
                .any(|(x, y)| self.world.tile_at(*x, *y).is_some_and(is_water_like_tile))
            {
                for tile in water_tiles {
                    issues.push(BuildPlacementIssue {
                        tile: Some(tile),
                        kind: BuildPlacementIssueKind::MissingAdjacentWater,
                    });
                }
            }
        }

        let player_tile = self.player.tile_position();
        if footprint.contains_tile(player_tile.0, player_tile.1) {
            issues.push(BuildPlacementIssue {
                tile: Some(player_tile),
                kind: BuildPlacementIssueKind::PlayerOccupied,
            });
        }

        for (x, y) in footprint.tiles() {
            if let Some(entity_id) = self.entities.occupancy.entity_at(x, y) {
                issues.push(BuildPlacementIssue {
                    tile: Some((x, y)),
                    kind: BuildPlacementIssueKind::EntityOccupied { entity_id },
                });
            }
        }
    }

    fn collect_mining_drill_preview_issues(
        &self,
        prototype: &factory_data::EntityPrototype,
        footprint: &EntityFootprint,
        issues: &mut Vec<BuildPlacementIssue>,
    ) {
        for (x, y) in footprint.tiles() {
            match self.world.tile_at(x, y) {
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
            self.world
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
}
