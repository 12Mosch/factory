use super::*;
use factory_data::{ResourceExtraction, item_id_by_name};

pub(in crate::simulation) fn item_id(prototypes: &PrototypeCatalog, name: &str) -> ItemId {
    item_id_by_name(prototypes, name)
}

#[cfg(test)]
pub(in crate::simulation) fn recipe_id(prototypes: &PrototypeCatalog, name: &str) -> RecipeId {
    factory_data::recipe_id_by_name(prototypes, name)
}

#[cfg(test)]
pub(in crate::simulation) fn technology_id(
    prototypes: &PrototypeCatalog,
    name: &str,
) -> TechnologyId {
    factory_data::technology_id_by_name(prototypes, name)
}

/// Whether `item_id` marks a fluid resource cell: the world generation config
/// declares its extraction type as [`ResourceExtraction::Fluid`]. Fluid
/// resources are extracted by pumpjacks and excluded from solid mining by
/// drills and the player.
pub(in crate::simulation) fn is_fluid_resource_item(
    prototypes: &PrototypeCatalog,
    item_id: ItemId,
) -> bool {
    prototypes
        .world_generation
        .resources
        .iter()
        .any(|resource| {
            resource.resource_item == item_id && resource.extraction == ResourceExtraction::Fluid
        })
}

pub(in crate::simulation) fn item_stack_size(
    prototypes: &PrototypeCatalog,
    item_id: ItemId,
) -> Option<u16> {
    prototypes
        .item(item_id)
        .map(|prototype| prototype.stack_size)
}

pub(in crate::simulation) fn fuel_value_joules(
    prototypes: &PrototypeCatalog,
    item_id: ItemId,
) -> Option<u64> {
    prototypes
        .item(item_id)
        .and_then(|prototype| prototype.fuel_value_joules)
}

pub(in crate::simulation) fn is_science_pack_item(
    catalog: &PrototypeCatalog,
    item_id: ItemId,
) -> bool {
    catalog
        .technologies
        .iter()
        .flat_map(|technology| &technology.science_packs)
        .any(|science_pack| science_pack.item == item_id)
}

pub(in crate::simulation) fn lab_can_accept_item(
    catalog: &PrototypeCatalog,
    item_id: ItemId,
) -> bool {
    is_science_pack_item(catalog, item_id)
}
