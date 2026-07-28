use factory_data::{
    BuildingCategory, EntityKind, EntityPrototypeId, ItemId, PrototypeCatalog, TechnologyEffect,
    TechnologyId,
};
use factory_sim::{
    BuildError, BuildPlacementIssue, BuildPlacementIssueKind, BuildPlacementPreview,
    ConstructionError, Direction, EntityDestroyError, EntityFootprint, PlayerBuildError,
    RollingStockPlacementError, Simulation, TilePlacementError, TilePlacementRequest,
};

use crate::build::resources::{
    BuildPlacementStatus, BuildSelection, BuildTarget, HOTBAR_SLOT_COUNT,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildablePrototype {
    pub target: BuildTarget,
    pub item_id: ItemId,
    pub display_name: String,
    pub category: BuildingCategory,
    pub menu_order: u16,
    pub required_technology: Option<TechnologyId>,
}

impl BuildablePrototype {
    pub fn selection(&self) -> BuildSelection {
        BuildSelection {
            target: self.target,
            item_id: self.item_id,
        }
    }
}

/// Everything the player can put down from the build menu: entities with a
/// build item, plus items that rewrite terrain. Both flow through the same
/// menu, hotbar, and selection state.
pub fn buildable_prototypes(catalog: &PrototypeCatalog) -> Vec<BuildablePrototype> {
    let mut buildables = Vec::new();

    for entity in &catalog.entities {
        if entity.entity_kind == EntityKind::ResourcePatch {
            continue;
        }
        let Some(item_id) = entity.build_item else {
            continue;
        };
        if catalog.item(item_id).is_none() {
            continue;
        }

        buildables.push(BuildablePrototype {
            target: BuildTarget::Entity(entity.id),
            item_id,
            display_name: display_name(&entity.name),
            category: entity
                .building_category
                .expect("validated buildable has a building category"),
            menu_order: entity
                .building_menu_order
                .expect("validated buildable has a building menu order"),
            required_technology: required_technology(catalog, item_id),
        });
    }

    for item in &catalog.items {
        let Some(placement) = item.place_as_tile else {
            continue;
        };

        buildables.push(BuildablePrototype {
            target: BuildTarget::Tile(placement.tile),
            item_id: item.id,
            display_name: display_name(&item.name),
            category: placement.building_category,
            menu_order: placement.building_menu_order,
            required_technology: required_technology(catalog, item.id),
        });
    }

    buildables
}

fn required_technology(catalog: &PrototypeCatalog, item_id: ItemId) -> Option<TechnologyId> {
    catalog
        .recipes
        .iter()
        .filter(|recipe| {
            recipe
                .products
                .iter()
                .any(|product| product.item == item_id)
        })
        .flat_map(|recipe| {
            catalog.technologies.iter().filter_map(move |technology| {
                technology.effects.iter().any(|effect| {
                    matches!(effect, TechnologyEffect::UnlockRecipe(id) if *id == recipe.id)
                }).then_some(technology.id)
            })
        })
        .min()
}

pub fn default_hotbar_slots(
    catalog: &PrototypeCatalog,
) -> [Option<BuildSelection>; HOTBAR_SLOT_COUNT] {
    let mut slots = [None; HOTBAR_SLOT_COUNT];
    for (slot, buildable) in slots.iter_mut().zip(buildable_prototypes(catalog)) {
        *slot = Some(buildable.selection());
    }
    slots
}

pub fn next_direction(direction: Direction) -> Direction {
    match direction {
        Direction::North => Direction::East,
        Direction::East => Direction::South,
        Direction::South => Direction::West,
        Direction::West => Direction::North,
    }
}

pub fn place_selection_at_tile(
    sim: &mut Simulation,
    selection: BuildSelection,
    direction: Direction,
    x: factory_sim::WorldTileCoord,
    y: factory_sim::WorldTileCoord,
) -> BuildPlacementStatus {
    let prototype_id = match selection.target {
        BuildTarget::Entity(prototype_id) => prototype_id,
        BuildTarget::Tile(_) => {
            return match factory_sim::tile_placement_ops::place_tile_from_player_inventory(
                sim,
                TilePlacementRequest {
                    item_id: selection.item_id,
                    x,
                    y,
                },
            ) {
                Ok(()) => BuildPlacementStatus::Placed(format!(
                    "Placed {}",
                    item_display_name(sim.catalog(), selection.item_id)
                        .unwrap_or_else(|| "Tile".to_string())
                )),
                Err(error) => tile_status_from_error(sim.catalog(), error),
            };
        }
    };

    // Rolling stock goes on rails rather than on tiles, so it takes the
    // rolling-stock path instead of the footprint one. Routing here rather than
    // inside the simulation keeps both paths honest about what they place: one
    // returns an entity that occupies tiles, the other a train that does not.
    if sim
        .catalog()
        .entity(prototype_id)
        .is_some_and(|prototype| prototype.rolling_stock.is_some())
    {
        return match sim.place_rolling_stock_from_player_inventory(
            prototype_id,
            selection.item_id,
            x,
            y,
        ) {
            Ok(_) => BuildPlacementStatus::Placed(format!(
                "Placed {}",
                entity_display_name(sim.catalog(), prototype_id)
                    .unwrap_or_else(|| "Rolling stock".to_string())
            )),
            Err(error) => rolling_stock_status_from_error(sim.catalog(), error),
        };
    }

    match factory_sim::placement::place_from_player_inventory(
        sim,
        factory_sim::placement::PlayerPlacementRequest {
            prototype_id,
            item_id: selection.item_id,
            x,
            y,
            direction,
        },
    ) {
        Ok(_) => BuildPlacementStatus::Placed(format!(
            "Placed {}",
            entity_display_name(sim.catalog(), prototype_id)
                .unwrap_or_else(|| "Building".to_string())
        )),
        Err(error) => build_status_from_error(sim.catalog(), error),
    }
}

/// Cursor preview for a terrain selection. Terrain has no entity footprint,
/// so this reports a single-tile footprint plus the same issue vocabulary the
/// entity preview uses, letting the existing preview renderer and status line
/// handle both without a second code path.
pub(crate) fn tile_placement_preview(
    sim: &Simulation,
    item_id: ItemId,
    x: factory_sim::WorldTileCoord,
    y: factory_sim::WorldTileCoord,
) -> (BuildPlacementPreview, BuildPlacementStatus) {
    let error =
        match factory_sim::validate_tile_placement(sim, TilePlacementRequest { item_id, x, y }) {
            Ok(_) if sim.player_inventory().count(item_id) == 0 => {
                Some(TilePlacementError::InsufficientInventory { item_id })
            }
            Ok(_) => None,
            Err(error) => Some(error),
        };

    let footprint = Some(EntityFootprint::single_tile(x, y));
    let Some(error) = error else {
        return (
            BuildPlacementPreview {
                footprint,
                issues: Vec::new(),
            },
            BuildPlacementStatus::Ready,
        );
    };

    let kind = match error {
        TilePlacementError::InsufficientInventory { item_id } => {
            BuildPlacementIssueKind::InsufficientInventory { item_id }
        }
        TilePlacementError::OutsideGeneratedChunks { .. } => {
            BuildPlacementIssueKind::OutsideGeneratedChunks
        }
        // The remaining cases mean the click cannot change this tile. The
        // precise reason still reaches the player through the status text; the
        // preview only needs to paint the tile as blocked.
        TilePlacementError::RequiresWater { .. }
        | TilePlacementError::RequiresSolidGround { .. }
        | TilePlacementError::AlreadyPlaced { .. }
        | TilePlacementError::SupportsOffshorePump { .. }
        | TilePlacementError::UnknownItem(_)
        | TilePlacementError::ItemDoesNotPlaceTile { .. } => {
            BuildPlacementIssueKind::TerrainBlocked
        }
    };

    (
        BuildPlacementPreview {
            footprint,
            issues: vec![BuildPlacementIssue {
                tile: Some((x, y)),
                kind,
            }],
        },
        tile_status_from_error(sim.catalog(), error),
    )
}

pub(crate) fn tile_status_from_error(
    catalog: &PrototypeCatalog,
    error: TilePlacementError,
) -> BuildPlacementStatus {
    match error {
        TilePlacementError::InsufficientInventory { item_id } => {
            BuildPlacementStatus::MissingInventory(short_inventory_need(catalog, item_id))
        }
        TilePlacementError::RequiresWater { .. } => {
            BuildPlacementStatus::CannotPlace("Landfill needs water".to_string())
        }
        TilePlacementError::RequiresSolidGround { .. } => {
            BuildPlacementStatus::CannotPlace("Paving needs solid ground".to_string())
        }
        TilePlacementError::AlreadyPlaced { .. } => {
            BuildPlacementStatus::CannotPlace("Tile already placed".to_string())
        }
        TilePlacementError::SupportsOffshorePump { .. } => {
            BuildPlacementStatus::CannotPlace("An offshore pump needs this water".to_string())
        }
        TilePlacementError::OutsideGeneratedChunks { .. } => {
            BuildPlacementStatus::CannotPlace("Outside generated area".to_string())
        }
        TilePlacementError::UnknownItem(_) | TilePlacementError::ItemDoesNotPlaceTile { .. } => {
            BuildPlacementStatus::CannotPlace("Cannot place this item".to_string())
        }
    }
}

pub fn short_inventory_need(catalog: &PrototypeCatalog, item_id: ItemId) -> String {
    format!(
        "Need {}",
        item_display_name(catalog, item_id).unwrap_or_else(|| "item".to_string())
    )
}

pub(crate) fn build_status_from_error(
    catalog: &PrototypeCatalog,
    error: PlayerBuildError,
) -> BuildPlacementStatus {
    match error {
        PlayerBuildError::InsufficientInventory { item_id } => {
            BuildPlacementStatus::MissingInventory(short_inventory_need(catalog, item_id))
        }
        PlayerBuildError::EntityLocked { prototype_id } => BuildPlacementStatus::Locked(format!(
            "{} locked",
            entity_display_name(catalog, prototype_id).unwrap_or_else(|| "Building".to_string())
        )),
        PlayerBuildError::Build(BuildError::EntityOccupied { .. }) => {
            BuildPlacementStatus::CannotPlace("Entity already there".to_string())
        }
        PlayerBuildError::Build(BuildError::TileBlocked { .. }) => {
            BuildPlacementStatus::CannotPlace("Tile blocked".to_string())
        }
        PlayerBuildError::Build(BuildError::OutsideGeneratedChunks { .. }) => {
            BuildPlacementStatus::CannotPlace("Outside generated area".to_string())
        }
        PlayerBuildError::Build(BuildError::InvalidFootprint { .. }) => {
            BuildPlacementStatus::CannotPlace("Invalid building footprint".to_string())
        }
        // Rolling stock reaches the simulation through its own command, so a
        // tile placement carrying one is a routing mistake rather than
        // something the player did wrong. Saying what it needs is more use than
        // saying it cannot be built.
        PlayerBuildError::Build(BuildError::RunsOnRails { .. }) => {
            BuildPlacementStatus::CannotPlace("Needs a clear run of rail".to_string())
        }
        PlayerBuildError::MissingPrototype(_)
        | PlayerBuildError::MissingBuildItem { .. }
        | PlayerBuildError::ItemDoesNotBuildEntity { .. }
        | PlayerBuildError::Build(BuildError::MissingPrototype(_))
        | PlayerBuildError::Build(BuildError::MissingEntity(_)) => {
            BuildPlacementStatus::CannotPlace("Cannot build this item".to_string())
        }
    }
}

pub(crate) fn rolling_stock_status_from_error(
    catalog: &PrototypeCatalog,
    error: RollingStockPlacementError,
) -> BuildPlacementStatus {
    match error {
        RollingStockPlacementError::InsufficientInventory { item_id } => {
            BuildPlacementStatus::MissingInventory(short_inventory_need(catalog, item_id))
        }
        RollingStockPlacementError::Locked(prototype_id) => BuildPlacementStatus::Locked(format!(
            "{} locked",
            entity_display_name(catalog, prototype_id)
                .unwrap_or_else(|| "Rolling stock".to_string())
        )),
        RollingStockPlacementError::NoRail => {
            BuildPlacementStatus::CannotPlace("Needs a rail here".to_string())
        }
        RollingStockPlacementError::TrackTooShort => {
            BuildPlacementStatus::CannotPlace("Track is too short for this".to_string())
        }
        RollingStockPlacementError::Occupied(_) => {
            BuildPlacementStatus::CannotPlace("Rolling stock already there".to_string())
        }
        RollingStockPlacementError::NotRollingStock(_)
        | RollingStockPlacementError::MissingBuildItem(_) => {
            BuildPlacementStatus::CannotPlace("Cannot build this item".to_string())
        }
    }
}

pub(crate) fn construction_status_from_error(
    catalog: &PrototypeCatalog,
    error: ConstructionError,
) -> BuildPlacementStatus {
    match error {
        ConstructionError::Build(BuildError::EntityOccupied { .. }) => {
            BuildPlacementStatus::CannotPlace("Entity already there".to_string())
        }
        ConstructionError::Build(BuildError::TileBlocked { .. }) => {
            BuildPlacementStatus::CannotPlace("Tile blocked".to_string())
        }
        ConstructionError::Build(BuildError::OutsideGeneratedChunks { .. }) => {
            BuildPlacementStatus::CannotPlace("Outside generated area".to_string())
        }
        ConstructionError::Build(_) => {
            BuildPlacementStatus::CannotPlace("Cannot plan this here".to_string())
        }
        ConstructionError::PlayerBuild(error) => build_status_from_error(catalog, error),
        ConstructionError::Destroy(EntityDestroyError::InsufficientInventory { item_id }) => {
            BuildPlacementStatus::CannotPlace(format!(
                "No inventory space for {}",
                item_display_name(catalog, item_id).unwrap_or_else(|| "item".to_string())
            ))
        }
        ConstructionError::Destroy(_) => {
            BuildPlacementStatus::CannotPlace("Cannot deconstruct this".to_string())
        }
        ConstructionError::EntityLocked { prototype_id } => BuildPlacementStatus::Locked(format!(
            "{} locked",
            entity_display_name(catalog, prototype_id).unwrap_or_else(|| "Building".to_string())
        )),
        ConstructionError::GhostOccupied { .. } => {
            BuildPlacementStatus::CannotPlace("Ghost already planned there".to_string())
        }
        ConstructionError::MissingGhost(_) => {
            BuildPlacementStatus::CannotPlace("Ghost no longer exists".to_string())
        }
        ConstructionError::NotMarkedForDeconstruction(_) => {
            BuildPlacementStatus::CannotPlace("Not marked for deconstruction".to_string())
        }
        ConstructionError::EmptyBlueprintArea => {
            BuildPlacementStatus::CannotPlace("Nothing to capture".to_string())
        }
        ConstructionError::BlueprintOffsetOutOfRange => {
            BuildPlacementStatus::CannotPlace("Blueprint area is too large".to_string())
        }
        ConstructionError::MissingBlueprint { .. } => {
            BuildPlacementStatus::CannotPlace("Blueprint no longer exists".to_string())
        }
    }
}

pub(crate) fn build_status_from_preview(
    catalog: &PrototypeCatalog,
    preview: &BuildPlacementPreview,
) -> Option<BuildPlacementStatus> {
    build_status_from_issues(catalog, &preview.issues)
}

/// Picks the worst issue across a set of placement issues (e.g. every entity
/// of a blueprint paste preview) and maps it to a status message.
pub(crate) fn build_status_from_issues(
    catalog: &PrototypeCatalog,
    issues: &[BuildPlacementIssue],
) -> Option<BuildPlacementStatus> {
    issues
        .iter()
        .min_by_key(|issue| preview_issue_priority(issue))
        .map(|issue| build_status_from_preview_issue(catalog, issue))
}

pub(crate) fn build_status_from_preview_issue(
    catalog: &PrototypeCatalog,
    issue: &BuildPlacementIssue,
) -> BuildPlacementStatus {
    match &issue.kind {
        BuildPlacementIssueKind::EntityLocked { prototype_id } => {
            BuildPlacementStatus::Locked(format!(
                "{} locked",
                entity_display_name(catalog, *prototype_id)
                    .unwrap_or_else(|| "Building".to_string())
            ))
        }
        BuildPlacementIssueKind::InsufficientInventory { item_id } => {
            BuildPlacementStatus::MissingInventory(short_inventory_need(catalog, *item_id))
        }
        BuildPlacementIssueKind::ItemDoesNotBuildEntity { .. }
        | BuildPlacementIssueKind::MissingBuildItem { .. }
        | BuildPlacementIssueKind::MissingPrototype(_) => {
            BuildPlacementStatus::CannotPlace("Cannot build this item".to_string())
        }
        BuildPlacementIssueKind::EntityOccupied { .. } => {
            BuildPlacementStatus::CannotPlace("Entity already there".to_string())
        }
        BuildPlacementIssueKind::GhostOccupied => {
            BuildPlacementStatus::CannotPlace("Ghost already planned there".to_string())
        }
        BuildPlacementIssueKind::PlayerOccupied => {
            BuildPlacementStatus::CannotPlace("Player in the way".to_string())
        }
        BuildPlacementIssueKind::TerrainBlocked => {
            BuildPlacementStatus::CannotPlace("Tile blocked".to_string())
        }
        BuildPlacementIssueKind::OutsideGeneratedChunks => {
            BuildPlacementStatus::CannotPlace("Outside generated area".to_string())
        }
        BuildPlacementIssueKind::MissingRequiredResource => {
            BuildPlacementStatus::CannotPlace("Mining drill needs a resource patch".to_string())
        }
        BuildPlacementIssueKind::MissingAdjacentWater => {
            BuildPlacementStatus::CannotPlace("Offshore pump needs adjacent water".to_string())
        }
        BuildPlacementIssueKind::InvalidFootprint { .. } => {
            BuildPlacementStatus::CannotPlace("Invalid building footprint".to_string())
        }
        BuildPlacementIssueKind::NeedsClearRail { .. } => {
            BuildPlacementStatus::CannotPlace("Needs a clear run of rail".to_string())
        }
    }
}

fn preview_issue_priority(issue: &BuildPlacementIssue) -> usize {
    match issue.kind {
        BuildPlacementIssueKind::EntityLocked { .. } => 0,
        BuildPlacementIssueKind::InsufficientInventory { .. } => 1,
        BuildPlacementIssueKind::ItemDoesNotBuildEntity { .. } => 2,
        BuildPlacementIssueKind::MissingBuildItem { .. } => 3,
        BuildPlacementIssueKind::MissingPrototype(_) => 4,
        BuildPlacementIssueKind::EntityOccupied { .. } => 5,
        BuildPlacementIssueKind::GhostOccupied => 6,
        BuildPlacementIssueKind::PlayerOccupied => 7,
        BuildPlacementIssueKind::TerrainBlocked => 8,
        BuildPlacementIssueKind::OutsideGeneratedChunks => 9,
        BuildPlacementIssueKind::MissingRequiredResource => 10,
        BuildPlacementIssueKind::MissingAdjacentWater => 11,
        BuildPlacementIssueKind::InvalidFootprint { .. } => 12,
        BuildPlacementIssueKind::NeedsClearRail { .. } => 13,
    }
}

pub(crate) fn entity_display_name(
    catalog: &PrototypeCatalog,
    prototype_id: EntityPrototypeId,
) -> Option<String> {
    catalog
        .entity(prototype_id)
        .map(|prototype| display_name(&prototype.name))
}

fn item_display_name(catalog: &PrototypeCatalog, item_id: ItemId) -> Option<String> {
    catalog
        .item(item_id)
        .map(|prototype| display_name(&prototype.name))
}

pub(crate) fn display_name(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::EntityId;

    #[test]
    fn preview_mapper_reports_occupied_entity() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let status = build_status_from_preview_issue(
            &catalog,
            &BuildPlacementIssue {
                tile: Some((1, 2)),
                kind: BuildPlacementIssueKind::EntityOccupied {
                    entity_id: EntityId::new(1),
                },
            },
        );

        assert_eq!(
            status,
            BuildPlacementStatus::CannotPlace("Entity already there".to_string())
        );
    }

    #[test]
    fn preview_mapper_reports_player_collision() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let status = build_status_from_preview_issue(
            &catalog,
            &BuildPlacementIssue {
                tile: Some((1, 2)),
                kind: BuildPlacementIssueKind::PlayerOccupied,
            },
        );

        assert_eq!(
            status,
            BuildPlacementStatus::CannotPlace("Player in the way".to_string())
        );
    }

    #[test]
    fn preview_mapper_reports_missing_drill_resource() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let status = build_status_from_preview_issue(
            &catalog,
            &BuildPlacementIssue {
                tile: Some((1, 2)),
                kind: BuildPlacementIssueKind::MissingRequiredResource,
            },
        );

        assert_eq!(
            status,
            BuildPlacementStatus::CannotPlace("Mining drill needs a resource patch".to_string())
        );
    }

    #[test]
    fn preview_mapper_reports_missing_offshore_pump_water() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let status = build_status_from_preview_issue(
            &catalog,
            &BuildPlacementIssue {
                tile: Some((1, 2)),
                kind: BuildPlacementIssueKind::MissingAdjacentWater,
            },
        );

        assert_eq!(
            status,
            BuildPlacementStatus::CannotPlace("Offshore pump needs adjacent water".to_string())
        );
    }

    #[test]
    fn buildables_derive_starter_and_unlocking_technologies() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let buildables = buildable_prototypes(&catalog);
        let technology_name = |entity_name: &str| {
            let buildable = buildables
                .iter()
                .find(|buildable| match buildable.target {
                    BuildTarget::Entity(prototype_id) => catalog
                        .entity(prototype_id)
                        .is_some_and(|entity| entity.name == entity_name),
                    BuildTarget::Tile(_) => false,
                })
                .expect("buildable should exist");
            buildable
                .required_technology
                .and_then(|id| catalog.technology(id))
                .map(|technology| technology.name.as_str())
        };
        assert_eq!(technology_name("stone_furnace"), None);
        assert_eq!(technology_name("splitter"), Some("logistics"));
        assert_eq!(technology_name("assembling_machine"), Some("automation"));
        assert_eq!(technology_name("storage_tank"), Some("fluid_handling"));
    }

    #[test]
    fn multiple_unlocking_technologies_choose_lowest_id() {
        let catalog = PrototypeCatalog::from_ron_str(r#"(
            items: [(id: 0, name: "machine", stack_size: 50)],
            recipes: [(id: 0, name: "machine", category: Crafting, crafting_time_ticks: 1, products: [(item: "machine", amount: 1)])],
            entities: [(id: 0, name: "machine", entity_kind: AssemblingMachine, build_item: Some("machine"), building_category: Some(Production), building_menu_order: Some(1), size: (x: 1, y: 1), collision_mask: (layers: ["building"]))],
            tiles: [],
            technologies: [
                (id: 0, name: "first", prerequisites: [], science_packs: [], required_units: 1, research_time_ticks: 1, effects: [UnlockRecipe("machine")]),
                (id: 1, name: "second", prerequisites: [], science_packs: [], required_units: 1, research_time_ticks: 1, effects: [UnlockRecipe("machine")]),
            ],
        )"#).expect("catalog should load");
        assert_eq!(
            buildable_prototypes(&catalog)[0].required_technology,
            Some(TechnologyId::new(0))
        );
    }
}
