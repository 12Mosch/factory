use factory_data::{EntityPrototypeId, PrototypeCatalog};

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
    catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required entity prototype {name:?}"))
}
