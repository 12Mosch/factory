pub(crate) fn compact_item_name(name: &str) -> String {
    name.split('_')
        .filter_map(|part| part.chars().next())
        .collect::<String>()
        .to_uppercase()
}

#[cfg(test)]
pub(crate) fn find_entity_prototype_id(
    catalog: &factory_data::PrototypeCatalog,
    name: &str,
) -> factory_data::EntityPrototypeId {
    factory_data::entity_prototype_id_by_name(catalog, name)
}
