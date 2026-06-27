use factory_data::{EntityPrototypeId, PrototypeCatalog, entity_prototype_id_by_name};

pub(crate) fn compact_item_name(name: &str) -> String {
    name.split('_')
        .filter_map(|part| part.chars().next())
        .collect::<String>()
        .to_uppercase()
}

pub(crate) fn find_entity_prototype_id(
    catalog: &PrototypeCatalog,
    name: &str,
) -> EntityPrototypeId {
    entity_prototype_id_by_name(catalog, name)
}
