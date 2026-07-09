use glam::IVec2;

use crate::catalog::PrototypeCatalog;
use crate::model::{
    AssemblingMachinePrototype, CraftingCategory, ElectricEnergySourcePrototype, EntityKind,
    UndergroundBeltPart,
};

#[test]
fn chest_entity_loads_inventory_slot_count() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let chest = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "chest")
        .expect("base catalog should contain chest entity");

    assert_eq!(chest.inventory_slot_count, Some(16));
}

#[test]
fn lab_entity_loads_inventory_slot_count() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let lab = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "lab")
        .expect("base catalog should contain lab entity");

    assert_eq!(lab.inventory_slot_count, Some(16));
    assert_eq!(
        lab.electric_energy_source,
        Some(ElectricEnergySourcePrototype {
            energy_usage_watts: 60_000,
            drain_watts: 0,
        })
    );
}

#[test]
fn burner_mining_drill_loads_energy_and_mining_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let drill = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "burner_mining_drill")
        .expect("base catalog should contain burner mining drill");

    assert_eq!(
        drill
            .burner
            .as_ref()
            .map(|burner| burner.energy_usage_watts),
        Some(150_000)
    );
    assert_eq!(
        drill.mining_drill.as_ref().map(|mining| mining.mining_area),
        Some(IVec2::new(2, 2))
    );
    assert_eq!(
        drill
            .mining_drill
            .as_ref()
            .map(|mining| mining.ticks_per_item),
        Some(240)
    );
}

#[test]
fn stone_furnace_loads_burner_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let furnace = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "stone_furnace")
        .expect("base catalog should contain stone furnace");

    assert_eq!(
        furnace
            .burner
            .as_ref()
            .map(|burner| burner.energy_usage_watts),
        Some(90_000)
    );
}

#[test]
fn assembling_machine_loads_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let assembler = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "assembling_machine")
        .expect("base catalog should contain assembling machine");

    assert_eq!(assembler.entity_kind, EntityKind::AssemblingMachine);
    assert_eq!(assembler.size, IVec2::new(3, 3));
    assert_eq!(
        assembler.assembling_machine,
        Some(AssemblingMachinePrototype {
            crafting_speed_numerator: 1,
            crafting_speed_denominator: 2,
            input_slot_count: 4,
            output_slot_count: 1,
            crafting_category: CraftingCategory::Crafting,
        })
    );
    assert_eq!(
        assembler.electric_energy_source,
        Some(ElectricEnergySourcePrototype {
            energy_usage_watts: 75_000,
            drain_watts: 2_500,
        })
    );
}

#[test]
fn inserter_variants_load_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    for (
        entity_name,
        expected_pickup_offset,
        expected_drop_offset,
        expected_pickup_ticks,
        expected_drop_ticks,
        expected_energy_usage_watts,
        expected_drain_watts,
    ) in [
        (
            "inserter",
            IVec2::new(0, -1),
            IVec2::new(0, 1),
            35,
            35,
            15_100,
            400,
        ),
        (
            "fast_inserter",
            IVec2::new(0, -1),
            IVec2::new(0, 1),
            12,
            12,
            59_300,
            500,
        ),
        (
            "long_handed_inserter",
            IVec2::new(0, -2),
            IVec2::new(0, 2),
            25,
            25,
            21_400,
            400,
        ),
    ] {
        let item = catalog
            .items
            .iter()
            .find(|prototype| prototype.name == entity_name)
            .unwrap_or_else(|| panic!("base catalog should contain {entity_name} item"));
        let entity = catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == entity_name)
            .unwrap_or_else(|| panic!("base catalog should contain {entity_name} entity"));
        let inserter = entity
            .inserter
            .as_ref()
            .expect("inserter entity should define inserter metadata");

        assert_eq!(entity.entity_kind, EntityKind::Inserter);
        assert_eq!(entity.build_item, Some(item.id));
        assert_eq!(inserter.pickup_offset, expected_pickup_offset);
        assert_eq!(inserter.drop_offset, expected_drop_offset);
        assert_eq!(inserter.pickup_ticks, expected_pickup_ticks);
        assert_eq!(inserter.drop_ticks, expected_drop_ticks);
        assert_eq!(
            entity.electric_energy_source,
            Some(ElectricEnergySourcePrototype {
                energy_usage_watts: expected_energy_usage_watts,
                drain_watts: expected_drain_watts,
            })
        );
    }
}

#[test]
fn electricity_entities_load_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    let pole = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "small_electric_pole")
        .expect("base catalog should contain small electric pole");
    assert_eq!(pole.entity_kind, EntityKind::ElectricPole);
    assert_eq!(pole.size, IVec2::new(1, 1));
    let pole_metadata = pole
        .electric_pole
        .as_ref()
        .expect("small electric pole should define pole metadata");
    assert_eq!(pole_metadata.supply_area_tiles, IVec2::new(5, 5));
    assert_eq!(pole_metadata.wire_reach_tiles_x2, 15);
    assert!(pole.electric_energy_source.is_none());
    assert!(pole.steam_engine.is_none());
    assert!(pole.boiler.is_none());
    assert!(pole.offshore_pump.is_none());
    assert!(pole.burner.is_none());

    let steam_engine = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "steam_engine")
        .expect("base catalog should contain steam engine");
    assert_eq!(steam_engine.entity_kind, EntityKind::SteamEngine);
    assert_eq!(steam_engine.size, IVec2::new(3, 5));
    let steam_engine_metadata = steam_engine
        .steam_engine
        .as_ref()
        .expect("steam engine should define steam engine metadata");
    assert_eq!(steam_engine_metadata.max_power_output_watts, 900_000);
    assert_eq!(
        steam_engine_metadata.steam_consumption_per_second_milliunits,
        30_000
    );
    assert!(steam_engine.electric_energy_source.is_none());
    assert!(steam_engine.electric_pole.is_none());
    assert!(steam_engine.boiler.is_none());
    assert!(steam_engine.offshore_pump.is_none());
    assert!(steam_engine.burner.is_none());
    assert_eq!(steam_engine.fluid_boxes.len(), 1);

    let boiler = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "boiler")
        .expect("base catalog should contain boiler");
    assert_eq!(boiler.entity_kind, EntityKind::Boiler);
    assert_eq!(boiler.size, IVec2::new(2, 3));
    let boiler_burner = boiler
        .burner
        .as_ref()
        .expect("boiler should define burner metadata");
    assert_eq!(boiler_burner.energy_usage_watts, 1_800_000);
    let boiler_metadata = boiler
        .boiler
        .as_ref()
        .expect("boiler should define boiler metadata");
    assert_eq!(
        boiler_metadata.water_consumption_per_second_milliunits,
        6_000
    );
    assert_eq!(boiler_metadata.steam_output_per_second_milliunits, 60_000);
    assert!(boiler.electric_energy_source.is_none());
    assert!(boiler.electric_pole.is_none());
    assert!(boiler.steam_engine.is_none());
    assert!(boiler.offshore_pump.is_none());
    assert_eq!(boiler.fluid_boxes.len(), 2);

    let pump = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "offshore_pump")
        .expect("base catalog should contain offshore pump");
    assert_eq!(pump.entity_kind, EntityKind::OffshorePump);
    assert_eq!(pump.size, IVec2::new(2, 1));
    let pump_metadata = pump
        .offshore_pump
        .as_ref()
        .expect("offshore pump should define pump metadata");
    assert_eq!(pump_metadata.pumping_speed_per_second_milliunits, 1_200_000);
    assert!(pump.electric_energy_source.is_none());
    assert!(pump.electric_pole.is_none());
    assert!(pump.steam_engine.is_none());
    assert!(pump.boiler.is_none());
    assert!(pump.burner.is_none());
    assert_eq!(pump.fluid_boxes.len(), 1);
}

#[test]
fn underground_belt_endpoints_load_shared_build_item_and_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let underground_belt = catalog
        .items
        .iter()
        .find(|prototype| prototype.name == "underground_belt")
        .expect("base catalog should contain underground belt item")
        .id;
    let entrance = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "underground_belt_entrance")
        .expect("base catalog should contain underground belt entrance");
    let exit = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "underground_belt_exit")
        .expect("base catalog should contain underground belt exit");

    assert_eq!(entrance.entity_kind, EntityKind::TransportBelt);
    assert_eq!(exit.entity_kind, EntityKind::TransportBelt);
    assert_eq!(entrance.build_item, Some(underground_belt));
    assert_eq!(exit.build_item, Some(underground_belt));
    assert_eq!(
        entrance
            .transport_belt
            .as_ref()
            .and_then(|belt| belt.underground.as_ref())
            .map(|underground| (underground.part, underground.max_distance)),
        Some((UndergroundBeltPart::Entrance, 4))
    );
    assert_eq!(
        exit.transport_belt
            .as_ref()
            .and_then(|belt| belt.underground.as_ref())
            .map(|underground| (underground.part, underground.max_distance)),
        Some((UndergroundBeltPart::Exit, 4))
    );
}

#[test]
fn transport_belt_tiers_load_speed_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    for (entity_name, expected_speed) in [
        ("transport_belt", 8),
        ("fast_transport_belt", 16),
        ("express_transport_belt", 24),
    ] {
        let entity = catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == entity_name)
            .unwrap_or_else(|| panic!("base catalog should contain {entity_name}"));

        assert_eq!(entity.entity_kind, EntityKind::TransportBelt);
        assert_eq!(
            entity
                .transport_belt
                .as_ref()
                .map(|belt| (belt.speed_subtiles_per_tick, belt.underground.as_ref())),
            Some((expected_speed, None)),
            "{entity_name} should define straight belt metadata"
        );
    }
}

#[test]
fn underground_belt_tiers_load_speed_and_distance_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    for (base_name, item_name, expected_speed, expected_distance) in [
        ("underground_belt", "underground_belt", 8, 4),
        ("fast_underground_belt", "fast_underground_belt", 16, 6),
        (
            "express_underground_belt",
            "express_underground_belt",
            24,
            8,
        ),
    ] {
        let item = catalog
            .items
            .iter()
            .find(|prototype| prototype.name == item_name)
            .unwrap_or_else(|| panic!("base catalog should contain {item_name} item"))
            .id;
        for (suffix, expected_part) in [
            ("entrance", UndergroundBeltPart::Entrance),
            ("exit", UndergroundBeltPart::Exit),
        ] {
            let entity_name = format!("{base_name}_{suffix}");
            let entity = catalog
                .entities
                .iter()
                .find(|prototype| prototype.name == entity_name)
                .unwrap_or_else(|| panic!("base catalog should contain {entity_name}"));
            let belt = entity
                .transport_belt
                .as_ref()
                .expect("underground endpoint should define belt metadata");
            let underground = belt
                .underground
                .as_ref()
                .expect("underground endpoint should define underground metadata");

            assert_eq!(entity.entity_kind, EntityKind::TransportBelt);
            assert_eq!(entity.build_item, Some(item));
            assert_eq!(belt.speed_subtiles_per_tick, expected_speed);
            assert_eq!(underground.part, expected_part);
            assert_eq!(underground.max_distance, expected_distance);
        }
    }
}

#[test]
fn splitter_tiers_load_speed_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    for (entity_name, expected_speed) in [
        ("splitter", 8),
        ("fast_splitter", 16),
        ("express_splitter", 24),
    ] {
        let entity = catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == entity_name)
            .unwrap_or_else(|| panic!("base catalog should contain {entity_name}"));

        assert_eq!(entity.entity_kind, EntityKind::Splitter);
        assert_eq!(
            entity
                .splitter
                .as_ref()
                .map(|splitter| splitter.speed_subtiles_per_tick),
            Some(expected_speed)
        );
    }
}
