use glam::IVec2;

use crate::catalog::PrototypeCatalog;
use crate::model::{EntityKind, FluidBoxIo, FluidConnectionSide, UndergroundBeltPart};

#[test]
fn fluid_ids_are_stable_and_contiguous() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    assert_eq!(catalog.fluids[0].name, "water");
    assert_eq!(catalog.fluids[0].id.index(), 0);
    assert_eq!(catalog.fluids[1].name, "steam");
    assert_eq!(catalog.fluids[1].id.index(), 1);
}

#[test]
fn fluid_metadata_resolves_to_valid_fluid_ids() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let water = catalog
        .fluids
        .iter()
        .find(|prototype| prototype.name == "water")
        .expect("base catalog should contain water")
        .id;
    let steam = catalog
        .fluids
        .iter()
        .find(|prototype| prototype.name == "steam")
        .expect("base catalog should contain steam")
        .id;

    for entity_name in [
        "offshore_pump",
        "boiler",
        "steam_engine",
        "pipe",
        "storage_tank",
    ] {
        let entity = catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == entity_name)
            .unwrap_or_else(|| panic!("base catalog should contain {entity_name}"));
        for fluid_box in &entity.fluid_boxes {
            if let Some(fluid_id) = fluid_box.filter {
                assert!(fluid_id.index() < catalog.fluids.len());
            }
            assert!(fluid_box.capacity_milliunits > 0);
            assert!(!fluid_box.connections.is_empty());
        }
    }

    let offshore_pump = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "offshore_pump")
        .expect("base catalog should contain offshore pump");
    assert_eq!(offshore_pump.fluid_boxes[0].filter, Some(water));

    let boiler = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "boiler")
        .expect("base catalog should contain boiler");
    assert_eq!(boiler.fluid_boxes[0].filter, Some(water));
    assert_eq!(boiler.fluid_boxes[1].filter, Some(steam));

    let steam_engine = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "steam_engine")
        .expect("base catalog should contain steam engine");
    assert_eq!(steam_engine.fluid_boxes[0].filter, Some(steam));

    let pipe = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "pipe")
        .expect("base catalog should contain pipe");
    assert_eq!(pipe.entity_kind, EntityKind::Pipe);
    assert_eq!(pipe.size, IVec2::new(1, 1));
    assert_eq!(pipe.fluid_boxes.len(), 1);
    assert_eq!(pipe.fluid_boxes[0].filter, None);
    assert_eq!(pipe.fluid_boxes[0].connections.len(), 4);

    let storage_tank = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "storage_tank")
        .expect("base catalog should contain storage tank");
    assert_eq!(storage_tank.entity_kind, EntityKind::StorageTank);
    assert_eq!(storage_tank.size, IVec2::new(3, 3));
    assert_eq!(storage_tank.fluid_boxes.len(), 1);
    assert_eq!(storage_tank.fluid_boxes[0].filter, None);
    assert_eq!(
        storage_tank.fluid_boxes[0].connections[0].side,
        FluidConnectionSide::North
    );
}

#[test]
fn underground_pipe_and_pump_metadata_loads() {
    let catalog = PrototypeCatalog::load_base().expect("base catalog should load");
    let pipe_to_ground = crate::item_id_by_name(&catalog, "pipe_to_ground");

    for (name, expected_part) in [
        ("pipe_to_ground_entrance", UndergroundBeltPart::Entrance),
        ("pipe_to_ground_exit", UndergroundBeltPart::Exit),
    ] {
        let endpoint = catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == name)
            .unwrap_or_else(|| panic!("base catalog should contain {name}"));
        assert_eq!(endpoint.entity_kind, EntityKind::Pipe);
        assert_eq!(endpoint.build_item, Some(pipe_to_ground));
        assert_eq!(
            endpoint
                .underground_pipe
                .as_ref()
                .map(|underground| (underground.part, underground.max_distance)),
            Some((expected_part, 10))
        );
    }

    let pump = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "pump")
        .expect("base catalog should contain pump");
    assert_eq!(pump.entity_kind, EntityKind::Pump);
    assert_eq!(pump.fluid_boxes.len(), 2);
    assert_eq!(pump.fluid_boxes[0].io, FluidBoxIo::Input);
    assert_eq!(pump.fluid_boxes[1].io, FluidBoxIo::Output);
    assert_eq!(
        pump.pump
            .as_ref()
            .map(|metadata| metadata.pumping_speed_per_second_milliunits),
        Some(1_200_000)
    );
}
