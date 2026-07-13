/// Unrotated `(width, height)` footprint of an entity prototype in tiles;
/// unknown prototypes fall back to a single tile.
pub(crate) fn prototype_footprint_size(
    catalog: &factory_data::PrototypeCatalog,
    prototype_id: factory_data::EntityPrototypeId,
) -> (i32, i32) {
    catalog
        .entity(prototype_id)
        .map_or((1, 1), |prototype| (prototype.size.x, prototype.size.y))
}

pub(crate) fn compact_item_name(name: &str) -> String {
    name.split('_')
        .filter_map(|part| part.chars().next())
        .collect::<String>()
        .to_uppercase()
}

pub(crate) fn remove_previous_word(text: &mut String) {
    while text.ends_with(char::is_whitespace) {
        text.pop();
    }
    while text
        .chars()
        .last()
        .is_some_and(|character| !character.is_whitespace())
    {
        text.pop();
    }
}

#[cfg(test)]
pub(crate) fn find_entity_prototype_id(
    catalog: &factory_data::PrototypeCatalog,
    name: &str,
) -> factory_data::EntityPrototypeId {
    factory_data::entity_prototype_id_by_name(catalog, name)
}
