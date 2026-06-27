use factory_data::{EntityKind, EntityPrototypeId, ItemId, PrototypeCatalog};
use factory_sim::{BuildError, Direction, PlayerBuildError, Simulation};

use crate::resources::{BuildPlacementStatus, BuildSelection};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildablePrototype {
    pub slot_index: usize,
    pub prototype_id: EntityPrototypeId,
    pub item_id: ItemId,
    pub display_name: String,
}

pub fn buildable_prototypes(catalog: &PrototypeCatalog) -> Vec<BuildablePrototype> {
    let mut buildables = Vec::new();

    for entity in &catalog.entities {
        if entity.entity_kind == EntityKind::ResourcePatch {
            continue;
        }
        let Some(item) = catalog.items.iter().find(|item| item.name == entity.name) else {
            continue;
        };

        buildables.push(BuildablePrototype {
            slot_index: buildables.len(),
            prototype_id: entity.id,
            item_id: item.id,
            display_name: display_name(&entity.name),
        });
    }

    buildables
}

pub fn buildable_prototype_at_slot(
    catalog: &PrototypeCatalog,
    slot_index: usize,
) -> Option<BuildablePrototype> {
    buildable_prototypes(catalog)
        .into_iter()
        .find(|buildable| buildable.slot_index == slot_index)
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
    match sim.place_entity_from_player_inventory(
        selection.prototype_id,
        selection.item_id,
        x,
        y,
        direction,
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

fn build_status_from_error(
    catalog: &PrototypeCatalog,
    error: PlayerBuildError,
) -> BuildPlacementStatus {
    match error {
        PlayerBuildError::InsufficientInventory { item_id } => {
            BuildPlacementStatus::MissingInventory(short_inventory_need(catalog, item_id))
        }
        PlayerBuildError::Build(BuildError::EntityOccupied { .. })
        | PlayerBuildError::Build(BuildError::TileBlocked { .. }) => {
            BuildPlacementStatus::CannotPlace("Blocked".to_string())
        }
        PlayerBuildError::Build(_) => {
            BuildPlacementStatus::CannotPlace("Cannot build here".to_string())
        }
        PlayerBuildError::MissingPrototype(_)
        | PlayerBuildError::MissingBuildItem { .. }
        | PlayerBuildError::ItemDoesNotBuildEntity { .. } => {
            BuildPlacementStatus::CannotPlace("Cannot build here".to_string())
        }
    }
}

fn entity_display_name(
    catalog: &PrototypeCatalog,
    prototype_id: EntityPrototypeId,
) -> Option<String> {
    catalog
        .entities
        .get(prototype_id.index())
        .filter(|prototype| prototype.id == prototype_id)
        .map(|prototype| display_name(&prototype.name))
}

fn item_display_name(catalog: &PrototypeCatalog, item_id: ItemId) -> Option<String> {
    catalog
        .items
        .get(item_id.index())
        .filter(|prototype| prototype.id == item_id)
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
