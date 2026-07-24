use crate::catalog::PrototypeCatalog;
use crate::error::PrototypeLoadError;

#[test]
fn duplicate_ids_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [
                (id: 0, name: "iron_ore", stack_size: 100),
                (id: 0, name: "copper_ore", stack_size: 100),
            ],
            recipes: [],
            entities: [],
            tiles: [],
        )
        "#,
    )
    .expect_err("duplicate item ids should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::DuplicateId {
            group: "items",
            id: 0,
        }
    ));
}

#[test]
fn buildable_entity_missing_menu_metadata_fails() {
    let error = PrototypeCatalog::from_ron_str(r#"(
        items: [(id: 0, name: "chest", stack_size: 50)], recipes: [],
        entities: [(id: 0, name: "chest", entity_kind: Chest, size: (x: 1, y: 1), collision_mask: (layers: ["building"]))],
        tiles: [],
    )"#).expect_err("buildable metadata should be required");
    assert!(
        matches!(error, PrototypeLoadError::InvalidBuildingMenuMetadata { entity, .. } if entity == "chest")
    );
}

#[test]
fn buildable_entity_with_only_category_fails() {
    let error = PrototypeCatalog::from_ron_str(r#"(
        items: [(id: 0, name: "chest", stack_size: 50)], recipes: [],
        entities: [(id: 0, name: "chest", entity_kind: Chest, build_item: Some("chest"), building_category: Some(Storage), size: (x: 1, y: 1), collision_mask: (layers: ["building"]))],
        tiles: [],
    )"#).expect_err("building menu order should be required with a category");
    assert!(
        matches!(error, PrototypeLoadError::InvalidBuildingMenuMetadata { entity, .. } if entity == "chest")
    );
}

#[test]
fn buildable_entity_with_only_menu_order_fails() {
    let error = PrototypeCatalog::from_ron_str(r#"(
        items: [(id: 0, name: "chest", stack_size: 50)], recipes: [],
        entities: [(id: 0, name: "chest", entity_kind: Chest, build_item: Some("chest"), building_menu_order: Some(1), size: (x: 1, y: 1), collision_mask: (layers: ["building"]))],
        tiles: [],
    )"#).expect_err("building category should be required with a menu order");
    assert!(
        matches!(error, PrototypeLoadError::InvalidBuildingMenuMetadata { entity, .. } if entity == "chest")
    );
}

#[test]
fn non_buildable_entity_with_menu_metadata_fails() {
    let error = PrototypeCatalog::from_ron_str(r#"(
        items: [], recipes: [],
        entities: [(id: 0, name: "ore_patch", entity_kind: ResourcePatch, building_category: Some(Production), building_menu_order: Some(1), size: (x: 1, y: 1), collision_mask: (layers: ["resource"]))],
        tiles: [],
    )"#).expect_err("non-buildable metadata should be rejected");
    assert!(
        matches!(error, PrototypeLoadError::InvalidBuildingMenuMetadata { entity, .. } if entity == "ore_patch")
    );
}

#[test]
fn duplicate_names_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [
                (id: 0, name: "iron_ore", stack_size: 100),
                (id: 1, name: "iron_ore", stack_size: 100),
            ],
            recipes: [],
            entities: [],
            tiles: [],
        )
        "#,
    )
    .expect_err("duplicate item names should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::DuplicateName {
            group: "items",
            name,
        } if name == "iron_ore"
    ));
}

#[test]
fn missing_item_references_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [(id: 0, name: "iron_plate", stack_size: 100)],
            recipes: [(
                id: 0,
                name: "missing_recipe",
                category: Crafting,
                crafting_time_ticks: 30,
                ingredients: [(item: "missing_item", amount: 1)],
                products: [(item: "iron_plate", amount: 1)],
            )],
            entities: [],
            tiles: [],
        )
        "#,
    )
    .expect_err("missing item references should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingItemReference { recipe, item }
            if recipe == "missing_recipe" && item == "missing_item"
    ));
}

#[test]
fn missing_fluid_references_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            fluids: [(id: 0, name: "water")],
            recipes: [],
            entities: [(
                id: 0,
                name: "bad_pipe",
                entity_kind: Pipe,
                size: (x: 1, y: 1),
                collision_mask: (layers: ["ground", "building"]),
                fluid_boxes: [(
                    capacity_milliunits: 100000,
                    filter: Some("missing_fluid"),
                    connections: [(local_offset: (x: 0, y: 0), side: North)],
                )],
            )],
            tiles: [],
        )
        "#,
    )
    .expect_err("missing fluid references should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingFluidReference { owner, fluid }
            if owner == "bad_pipe" && fluid == "missing_fluid"
    ));
}

#[test]
fn empty_fluid_box_connections_fail_loading() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            fluids: [(id: 0, name: "water")],
            recipes: [],
            entities: [(
                id: 0,
                name: "bad_pipe",
                entity_kind: Pipe,
                size: (x: 1, y: 1),
                collision_mask: (layers: ["ground", "building"]),
                fluid_boxes: [(
                    capacity_milliunits: 100000,
                    connections: [],
                )],
            )],
            tiles: [],
        )
        "#,
    )
    .expect_err("empty fluid box connections should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidFluidBox { entity, box_index }
            if entity == "bad_pipe" && box_index == 0
    ));
}

#[test]
fn fluid_connection_offsets_outside_entity_fail_loading() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            fluids: [(id: 0, name: "water")],
            recipes: [],
            entities: [(
                id: 0,
                name: "bad_pipe",
                entity_kind: Pipe,
                size: (x: 1, y: 1),
                collision_mask: (layers: ["ground", "building"]),
                fluid_boxes: [(
                    capacity_milliunits: 100000,
                    connections: [(local_offset: (x: 1, y: 0), side: East)],
                )],
            )],
            tiles: [],
        )
        "#,
    )
    .expect_err("outside fluid connection offsets should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidFluidConnection {
            entity,
            box_index: 0,
            connection_index: 0,
        } if entity == "bad_pipe"
    ));
}

#[test]
fn fluid_connection_side_must_be_on_matching_outer_edge() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            fluids: [(id: 0, name: "water")],
            recipes: [],
            entities: [(
                id: 0,
                name: "bad_tank",
                entity_kind: StorageTank,
                size: (x: 3, y: 3),
                collision_mask: (layers: ["ground", "building"]),
                fluid_boxes: [(
                    capacity_milliunits: 100000,
                    connections: [(local_offset: (x: 1, y: 1), side: North)],
                )],
            )],
            tiles: [],
        )
        "#,
    )
    .expect_err("interior fluid connection side should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidFluidConnection {
            entity,
            box_index: 0,
            connection_index: 0,
        } if entity == "bad_tank"
    ));
}

#[test]
fn machine_fluid_box_roles_are_validated_during_load() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            fluids: [
                (id: 0, name: "water"),
                (id: 1, name: "steam"),
            ],
            recipes: [],
            entities: [(
                id: 0,
                name: "bad_boiler",
                entity_kind: Boiler,
                size: (x: 2, y: 3),
                collision_mask: (layers: ["ground", "building"]),
                burner: Some((energy_usage_watts: 1800000)),
                boiler: Some((
                    water_consumption_per_second_milliunits: 6000,
                    steam_output_per_second_milliunits: 60000,
                )),
                fluid_boxes: [
                    (
                        capacity_milliunits: 100000,
                        filter: Some("steam"),
                        connections: [(local_offset: (x: 0, y: 0), side: North)],
                    ),
                    (
                        capacity_milliunits: 100000,
                        filter: Some("water"),
                        connections: [(local_offset: (x: 1, y: 1), side: East)],
                    ),
                ],
            )],
            tiles: [],
        )
        "#,
    )
    .expect_err("swapped boiler fluid roles should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidFluidBox { entity, box_index: 0 }
            if entity == "bad_boiler"
    ));
}

#[test]
fn invalid_collision_layers_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [(
                id: 0,
                name: "bad_entity",
                entity_kind: Chest,
                size: (x: 2, y: 2),
                collision_mask: (layers: ["invalid"]),
            )],
            tiles: [],
        )
        "#,
    )
    .expect_err("invalid collision layers should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidCollisionLayer { owner, layer }
            if owner == "bad_entity" && layer == "invalid"
    ));
}

#[test]
fn furnace_without_furnace_section_fails() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [(
                id: 0,
                name: "bad_furnace",
                entity_kind: Furnace,
                size: (x: 2, y: 2),
                collision_mask: (layers: ["ground", "building"]),
                burner: Some((energy_usage_watts: 90000)),
            )],
            tiles: [],
        )
        "#,
    )
    .expect_err("furnace without a furnace section should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidMachineEnergySource { entity, .. }
            if entity == "bad_furnace"
    ));
}

#[test]
fn furnace_with_both_energy_sources_fails() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [(
                id: 0,
                name: "bad_furnace",
                entity_kind: Furnace,
                size: (x: 2, y: 2),
                collision_mask: (layers: ["ground", "building"]),
                furnace: Some((crafting_speed_numerator: 1, crafting_speed_denominator: 1)),
                burner: Some((energy_usage_watts: 90000)),
                electric_energy_source: Some((energy_usage_watts: 180000, drain_watts: 0)),
            )],
            tiles: [],
        )
        "#,
    )
    .expect_err("furnace with two energy sources should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidMachineEnergySource { entity, .. }
            if entity == "bad_furnace"
    ));
}

#[test]
fn mining_drill_without_energy_source_fails() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [(
                id: 0,
                name: "bad_drill",
                entity_kind: MiningDrill,
                size: (x: 2, y: 2),
                collision_mask: (layers: ["ground", "building"]),
                mining_drill: Some((mining_area: (x: 2, y: 2), ticks_per_item: 240)),
            )],
            tiles: [],
        )
        "#,
    )
    .expect_err("mining drill without an energy source should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidMachineEnergySource { entity, .. }
            if entity == "bad_drill"
    ));
}

#[test]
fn non_furnace_with_furnace_section_fails() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [(
                id: 0,
                name: "bad_chest",
                entity_kind: Chest,
                size: (x: 1, y: 1),
                collision_mask: (layers: ["ground", "building"]),
                furnace: Some((crafting_speed_numerator: 1, crafting_speed_denominator: 1)),
            )],
            tiles: [],
        )
        "#,
    )
    .expect_err("furnace section on a non-furnace entity should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidMachineEnergySource { entity, .. }
            if entity == "bad_chest"
    ));
}

#[test]
fn missing_technology_prerequisites_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "automation",
                prerequisites: ["missing"],
                science_packs: [],
                required_units: 1,
                research_time_ticks: 1,
                effects: [],
            )],
        )
        "#,
    )
    .expect_err("missing technology prerequisites should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingTechnologyPrerequisite {
            technology,
            prerequisite,
        } if technology == "automation" && prerequisite == "missing"
    ));
}

#[test]
fn missing_technology_science_pack_items_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "automation",
                prerequisites: [],
                science_packs: [(item: "missing_pack", amount: 1)],
                required_units: 1,
                research_time_ticks: 1,
                effects: [],
            )],
        )
        "#,
    )
    .expect_err("missing science pack item should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingTechnologySciencePackItem {
            technology,
            item,
        } if technology == "automation" && item == "missing_pack"
    ));
}

#[test]
fn missing_technology_unlock_recipes_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "automation",
                prerequisites: [],
                science_packs: [],
                required_units: 1,
                research_time_ticks: 1,
                effects: [UnlockRecipe("missing_recipe")],
            )],
        )
        "#,
    )
    .expect_err("missing unlock recipe should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingTechnologyUnlockRecipe {
            technology,
            recipe,
        } if technology == "automation" && recipe == "missing_recipe"
    ));
}

#[test]
fn duplicate_technology_ids_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [
                (
                    id: 0,
                    name: "automation",
                    prerequisites: [],
                    science_packs: [],
                    required_units: 1,
                    research_time_ticks: 1,
                    effects: [],
                ),
                (
                    id: 0,
                    name: "logistics",
                    prerequisites: [],
                    science_packs: [],
                    required_units: 1,
                    research_time_ticks: 1,
                    effects: [],
                ),
            ],
        )
        "#,
    )
    .expect_err("duplicate technology ids should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::DuplicateId {
            group: "technologies",
            id: 0,
        }
    ));
}

#[test]
fn duplicate_technology_names_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [
                (
                    id: 0,
                    name: "automation",
                    prerequisites: [],
                    science_packs: [],
                    required_units: 1,
                    research_time_ticks: 1,
                    effects: [],
                ),
                (
                    id: 1,
                    name: "automation",
                    prerequisites: [],
                    science_packs: [],
                    required_units: 1,
                    research_time_ticks: 1,
                    effects: [],
                ),
            ],
        )
        "#,
    )
    .expect_err("duplicate technology names should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::DuplicateName {
            group: "technologies",
            name,
        } if name == "automation"
    ));
}

#[test]
fn invalid_technology_required_units_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "automation",
                prerequisites: [],
                science_packs: [],
                required_units: 0,
                research_time_ticks: 1,
                effects: [],
            )],
        )
        "#,
    )
    .expect_err("zero required units should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidTechnologyRequiredUnits { technology }
            if technology == "automation"
    ));
}

#[test]
fn invalid_technology_research_time_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "automation",
                prerequisites: [],
                science_packs: [],
                required_units: 1,
                research_time_ticks: 0,
                effects: [],
            )],
        )
        "#,
    )
    .expect_err("zero research time should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidTechnologyResearchTime { technology }
            if technology == "automation"
    ));
}

#[test]
fn technology_self_prerequisites_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "automation",
                prerequisites: ["automation"],
                science_packs: [],
                required_units: 1,
                research_time_ticks: 1,
                effects: [],
            )],
        )
        "#,
    )
    .expect_err("self prerequisites should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::TechnologySelfPrerequisite { technology }
            if technology == "automation"
    ));
}

#[test]
fn technology_prerequisite_cycles_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [
                (
                    id: 0,
                    name: "automation",
                    prerequisites: ["logistics"],
                    science_packs: [],
                    required_units: 1,
                    research_time_ticks: 1,
                    effects: [],
                ),
                (
                    id: 1,
                    name: "logistics",
                    prerequisites: ["automation"],
                    science_packs: [],
                    required_units: 1,
                    research_time_ticks: 1,
                    effects: [],
                ),
            ],
        )
        "#,
    )
    .expect_err("technology prerequisite cycles should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::TechnologyPrerequisiteCycle { .. }
    ));
}

#[test]
fn enemy_spawner_without_enemy_gameplay_section_fails() {
    let error = PrototypeCatalog::from_ron_str(
        r#"(
        items: [], recipes: [],
        entities: [(
            id: 0, name: "spawner", entity_kind: EnemySpawner,
            size: (x: 2, y: 2), collision_mask: (layers: ["building"]),
            max_health: Some(300),
            enemy_spawner: Some((
                max_alive_units: 15,
                guard_units: 3,
                free_spawn_interval_ticks: 1800,
                unit_spawn_pollution_cost_milli: 4000,
                pollution_absorption_per_tick_milli: 20,
                unit: (
                    max_health: 30,
                    damage: 15,
                    attack_cooldown_ticks: 60,
                    speed_fixed_per_tick: 40,
                    aggro_radius_tiles: 12,
                ),
            )),
        )],
        tiles: [],
    )"#,
    )
    .expect_err("enemy content without enemy_gameplay should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingEnemyGameplayConfig
    ));
}

#[test]
fn invalid_armor_and_equipment_metadata_fail() {
    let armor_error = PrototypeCatalog::from_ron_str(
        r#"(
            items: [(
                id: 0, name: "armor", stack_size: 1,
                armor: Some((
                    grid_width: 0, grid_height: 5,
                    resistances: [],
                )),
            )],
            recipes: [], entities: [], tiles: [],
        )"#,
    )
    .expect_err("zero-sized armor grids should fail");
    assert!(matches!(
        armor_error,
        PrototypeLoadError::InvalidArmorMetadata { .. }
    ));

    let equipment_error = PrototypeCatalog::from_ron_str(
        r#"(
            items: [(
                id: 0, name: "battery", stack_size: 1,
                equipment: Some((
                    width: 1, height: 2,
                    effect: Battery(capacity_joules: 0),
                )),
            )],
            recipes: [], entities: [], tiles: [],
        )"#,
    )
    .expect_err("zero equipment capacity should fail");
    assert!(matches!(
        equipment_error,
        PrototypeLoadError::InvalidEquipmentMetadata { .. }
    ));
}

#[test]
fn duplicate_or_over_one_hundred_percent_armor_resistance_fails() {
    for resistances in [
        "[(damage_type: Physical, flat_reduction: 0, percent_reduction_permyriad: 10001)]",
        "[(damage_type: Physical, flat_reduction: 0, percent_reduction_permyriad: 1), (damage_type: Physical, flat_reduction: 2, percent_reduction_permyriad: 2)]",
    ] {
        let data = format!(
            r#"(
                items: [(
                    id: 0, name: "armor", stack_size: 1,
                    armor: Some((grid_width: 5, grid_height: 5, resistances: {resistances})),
                )],
                recipes: [], entities: [], tiles: [],
            )"#
        );
        assert!(matches!(
            PrototypeCatalog::from_ron_str(&data),
            Err(PrototypeLoadError::InvalidArmorMetadata { .. })
        ));
    }
}

#[test]
fn laser_turret_requires_health_electric_and_positive_turret_metadata() {
    let error = PrototypeCatalog::from_ron_str(
        r#"(
            items: [(id: 0, name: "laser_turret", stack_size: 50)],
            recipes: [],
            entities: [(
                id: 0, name: "laser_turret", entity_kind: LaserTurret,
                building_category: Some(Defense), building_menu_order: Some(30),
                size: (x: 2, y: 2), collision_mask: (layers: ["building"]),
                laser_turret: Some((range_tiles: 15, damage: 20, cooldown_ticks: 30)),
            )],
            tiles: [],
        )"#,
    )
    .expect_err("laser turrets without health and electric metadata should fail");
    assert!(matches!(
        error,
        PrototypeLoadError::InvalidLaserTurretMetadata { .. }
    ));
}

#[test]
fn solar_panel_without_metadata_fails() {
    let error = PrototypeCatalog::from_ron_str(r#"(
        items: [(id: 0, name: "solar_panel", stack_size: 50)], recipes: [],
        entities: [(id: 0, name: "solar_panel", entity_kind: SolarPanel, build_item: Some("solar_panel"), building_category: Some(Power), building_menu_order: Some(60), size: (x: 3, y: 3), collision_mask: (layers: ["building"]), max_health: Some(200))],
        tiles: [],
    )"#).expect_err("solar panels require solar metadata");
    assert!(
        matches!(error, PrototypeLoadError::InvalidSolarStorageMetadata { entity, .. } if entity == "solar_panel")
    );
}

#[test]
fn accumulator_with_zero_rates_fails() {
    let error = PrototypeCatalog::from_ron_str(r#"(
        items: [(id: 0, name: "accumulator", stack_size: 50)], recipes: [],
        entities: [(id: 0, name: "accumulator", entity_kind: Accumulator, build_item: Some("accumulator"), building_category: Some(Power), building_menu_order: Some(70), size: (x: 2, y: 2), collision_mask: (layers: ["building"]), max_health: Some(150), accumulator: Some((capacity_joules: 5000000, max_charge_watts: 0, max_discharge_watts: 300000)))],
        tiles: [],
    )"#).expect_err("accumulators require positive charge rate");
    assert!(
        matches!(error, PrototypeLoadError::InvalidSolarStorageMetadata { entity, .. } if entity == "accumulator")
    );
}

#[test]
fn solar_metadata_on_other_kind_fails() {
    let error = PrototypeCatalog::from_ron_str(r#"(
        items: [(id: 0, name: "chest", stack_size: 50)], recipes: [],
        entities: [(id: 0, name: "chest", entity_kind: Chest, build_item: Some("chest"), building_category: Some(Storage), building_menu_order: Some(1), size: (x: 1, y: 1), collision_mask: (layers: ["building"]), inventory_slot_count: Some(16), solar_panel: Some((max_power_output_watts: 60000)))],
        tiles: [],
    )"#).expect_err("solar metadata only valid on solar panels");
    assert!(
        matches!(error, PrototypeLoadError::InvalidSolarStorageMetadata { entity, .. } if entity == "chest")
    );
}

#[test]
fn radar_without_metadata_fails() {
    let error = PrototypeCatalog::from_ron_str(r#"(
        items: [(id: 0, name: "radar", stack_size: 50)], recipes: [],
        entities: [(id: 0, name: "radar", entity_kind: Radar, build_item: Some("radar"), building_category: Some(Defense), building_menu_order: Some(40), size: (x: 3, y: 3), collision_mask: (layers: ["building"]), max_health: Some(250), electric_energy_source: Some((energy_usage_watts: 300000, drain_watts: 0)))],
        tiles: [],
    )"#).expect_err("radars require scan metadata");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRadarMetadata { entity, .. } if entity == "radar")
    );
}

#[test]
fn invalid_radar_ranges_fail() {
    let error = PrototypeCatalog::from_ron_str(r#"(
        items: [(id: 0, name: "radar", stack_size: 50)], recipes: [],
        entities: [(id: 0, name: "radar", entity_kind: Radar, build_item: Some("radar"), building_category: Some(Defense), building_menu_order: Some(40), size: (x: 3, y: 3), collision_mask: (layers: ["building"]), max_health: Some(250), electric_energy_source: Some((energy_usage_watts: 300000, drain_watts: 0)), radar: Some((nearby_reveal_radius_chunks: 3, nearby_scan_interval_ticks: 60, far_scan_radius_chunks: 3, far_scan_interval_ticks: 2000)))],
        tiles: [],
    )"#).expect_err("far radar range must exceed nearby range");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRadarMetadata { entity, .. } if entity == "radar")
    );
}

#[test]
fn radar_with_zero_nearby_radius_fails() {
    assert_invalid_radar_fields(0, 60, 14, 2_000, Some(250), true, false);
}

#[test]
fn radar_with_zero_far_radius_fails() {
    assert_invalid_radar_fields(3, 60, 0, 2_000, Some(250), true, false);
}

#[test]
fn radar_with_zero_nearby_interval_fails() {
    assert_invalid_radar_fields(3, 0, 14, 2_000, Some(250), true, false);
}

#[test]
fn radar_with_zero_far_interval_fails() {
    assert_invalid_radar_fields(3, 60, 14, 0, Some(250), true, false);
}

#[test]
fn radar_without_electric_power_fails() {
    assert_invalid_radar_fields(3, 60, 14, 2_000, Some(250), false, false);
}

#[test]
fn radar_with_burner_power_fails() {
    assert_invalid_radar_fields(3, 60, 14, 2_000, Some(250), true, true);
}

#[test]
fn radar_with_zero_health_fails() {
    assert_invalid_radar_fields(3, 60, 14, 2_000, Some(0), true, false);
}

#[test]
fn radar_without_health_fails() {
    assert_invalid_radar_fields(3, 60, 14, 2_000, None, true, false);
}

#[test]
fn radar_metadata_on_other_kind_fails() {
    let error = PrototypeCatalog::from_ron_str(r#"(
        items: [(id: 0, name: "chest", stack_size: 50)], recipes: [],
        entities: [(id: 0, name: "chest", entity_kind: Chest, build_item: Some("chest"), building_category: Some(Storage), building_menu_order: Some(1), size: (x: 1, y: 1), collision_mask: (layers: ["building"]), inventory_slot_count: Some(16), radar: Some((nearby_reveal_radius_chunks: 3, nearby_scan_interval_ticks: 60, far_scan_radius_chunks: 14, far_scan_interval_ticks: 2000)))],
        tiles: [],
    )"#).expect_err("radar metadata only applies to radar entities");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRadarMetadata { entity, .. } if entity == "chest")
    );
}

fn assert_invalid_radar_fields(
    nearby_radius: u16,
    nearby_interval: u32,
    far_radius: u16,
    far_interval: u32,
    health: Option<u32>,
    electric: bool,
    burner: bool,
) {
    let health = health.map_or_else(String::new, |health| format!("max_health: Some({health}),"));
    let electric = if electric {
        "electric_energy_source: Some((energy_usage_watts: 300000, drain_watts: 0)),"
    } else {
        ""
    };
    let burner = if burner {
        "burner: Some((energy_usage_watts: 300000)),"
    } else {
        ""
    };
    let ron = format!(
        r#"(
            items: [(id: 0, name: "radar", stack_size: 50)],
            recipes: [],
            entities: [(
                id: 0,
                name: "radar",
                entity_kind: Radar,
                build_item: Some("radar"),
                building_category: Some(Defense),
                building_menu_order: Some(40),
                size: (x: 3, y: 3),
                collision_mask: (layers: ["building"]),
                {health}
                {electric}
                {burner}
                radar: Some((
                    nearby_reveal_radius_chunks: {nearby_radius},
                    nearby_scan_interval_ticks: {nearby_interval},
                    far_scan_radius_chunks: {far_radius},
                    far_scan_interval_ticks: {far_interval},
                )),
            )],
            tiles: [],
        )"#
    );
    let error =
        PrototypeCatalog::from_ron_str(&ron).expect_err("invalid radar declaration should fail");
    assert!(
        matches!(&error, PrototypeLoadError::InvalidRadarMetadata { entity, .. } if entity == "radar"),
        "unexpected radar validation error: {error}"
    );
}
