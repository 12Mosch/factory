use super::super::super::*;

pub(in crate::simulation::tests) fn entity_id_by_name(
    catalog: &PrototypeCatalog,
    name: &str,
) -> EntityPrototypeId {
    factory_data::entity_prototype_id_by_name(catalog, name)
}

pub(in crate::simulation::tests) fn fluid_id(catalog: &PrototypeCatalog, name: &str) -> FluidId {
    factory_data::fluid_id_by_name(catalog, name)
}

pub(in crate::simulation::tests) fn item_id_by_name(
    catalog: &PrototypeCatalog,
    name: &str,
) -> ItemId {
    factory_data::item_id_by_name(catalog, name)
}
