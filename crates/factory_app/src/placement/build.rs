use factory_data::{EntityKind, EntityPrototypeId, ItemId, PrototypeCatalog};
use factory_sim::{
    BuildError, BuildPlacementIssue, BuildPlacementIssueKind, BuildPlacementPreview,
    ConstructionError, Direction, EntityDestroyError, PlayerBuildError, Simulation,
};

use crate::build::resources::{BuildPlacementStatus, BuildSelection, HOTBAR_SLOT_COUNT};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildablePrototype {
    pub prototype_id: EntityPrototypeId,
    pub item_id: ItemId,
    pub display_name: String,
}

impl BuildablePrototype {
    pub fn selection(&self) -> BuildSelection {
        BuildSelection {
            prototype_id: self.prototype_id,
            item_id: self.item_id,
        }
    }
}

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
            prototype_id: entity.id,
            item_id,
            display_name: display_name(&entity.name),
        });
    }

    buildables
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

pub fn place_selected_building_at_tile(
    sim: &mut Simulation,
    selection: BuildSelection,
    direction: Direction,
    x: i32,
    y: i32,
) -> BuildPlacementStatus {
    match factory_sim::placement::place_from_player_inventory(
        sim,
        factory_sim::placement::PlayerPlacementRequest {
            prototype_id: selection.prototype_id,
            item_id: selection.item_id,
            x,
            y,
            direction,
        },
    ) {
        Ok(_) => BuildPlacementStatus::Placed(format!(
            "Placed {}",
            entity_display_name(sim.catalog(), selection.prototype_id)
                .unwrap_or_else(|| "Building".to_string())
        )),
        Err(error) => build_status_from_error(sim.catalog(), error),
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
        PlayerBuildError::MissingPrototype(_)
        | PlayerBuildError::MissingBuildItem { .. }
        | PlayerBuildError::ItemDoesNotBuildEntity { .. }
        | PlayerBuildError::Build(BuildError::MissingPrototype(_))
        | PlayerBuildError::Build(BuildError::MissingEntity(_)) => {
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
        ConstructionError::MissingBlueprint { .. } => {
            BuildPlacementStatus::CannotPlace("Blueprint no longer exists".to_string())
        }
    }
}

pub(crate) fn build_status_from_preview(
    catalog: &PrototypeCatalog,
    preview: &BuildPlacementPreview,
) -> Option<BuildPlacementStatus> {
    preview
        .issues
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

fn display_name(name: &str) -> String {
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
}
