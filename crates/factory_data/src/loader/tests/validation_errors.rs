use crate::catalog::PrototypeCatalog;
use crate::error::PrototypeLoadError;
use crate::model::LogisticChestMode;

fn combinator_catalog(
    entity_kind: &str,
    combinator_kind: &str,
    virtual_signals: &str,
) -> Result<PrototypeCatalog, PrototypeLoadError> {
    let constant_slot_count = u8::from(combinator_kind == "Constant");
    PrototypeCatalog::from_ron_str(&format!(
        r#"(
            items: [],
            recipes: [],
            entities: [(
                id: 0,
                name: "combinator",
                entity_kind: {entity_kind},
                size: (x: 1, y: 1),
                collision_mask: (layers: ["building"]),
                circuit_connector: Some((
                    ports: InputOutput,
                    wire_reach_tiles_x2: 18,
                    reads_contents: false,
                    controllable: false,
                )),
                combinator: Some((
                    kind: {combinator_kind},
                    constant_slot_count: {constant_slot_count},
                )),
            )],
            tiles: [],
            virtual_signals: [{virtual_signals}],
        )"#
    ))
}

#[test]
fn constant_combinators_do_not_require_wildcard_signals() {
    combinator_catalog("ConstantCombinator", "Constant", "")
        .expect("constant-only catalogs do not use wildcard operands");
}

#[test]
fn operand_combinators_require_every_wildcard_signal() {
    for (signals, missing) in [
        (
            "(id: 0, name: \"anything\", kind: Anything), (id: 1, name: \"everything\", kind: Everything)",
            "Each",
        ),
        (
            "(id: 0, name: \"each\", kind: Each), (id: 1, name: \"everything\", kind: Everything)",
            "Anything",
        ),
        (
            "(id: 0, name: \"each\", kind: Each), (id: 1, name: \"anything\", kind: Anything)",
            "Everything",
        ),
    ] {
        let error = combinator_catalog("ArithmeticCombinator", "Arithmetic", signals)
            .expect_err("operand combinators require the complete wildcard vocabulary");
        assert!(
            matches!(
                error,
                PrototypeLoadError::InvalidCircuitMetadata { detail, .. }
                    if detail.contains(missing)
            ),
            "unexpected error for missing {missing}: {error}"
        );
    }
}

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
fn overflowing_repeatable_technology_cost_curve_fails() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "overflowing",
                prerequisites: [],
                science_packs: [],
                required_units: 18446744073709551615,
                level_model: Repeatable(
                    cost_curve: Linear(additional_units_per_level: 1),
                ),
                research_time_ticks: 1,
                effects: [],
            )],
        )
        "#,
    )
    .expect_err("overflowing repeatable cost curve should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidTechnologyCostCurve { technology }
            if technology == "overflowing"
    ));
}

#[test]
fn zero_mining_productivity_effect_fails() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "zero_bonus",
                prerequisites: [],
                science_packs: [],
                required_units: 1,
                research_time_ticks: 1,
                effects: [MiningDrillProductivity(bonus_permyriad: 0)],
            )],
        )
        "#,
    )
    .expect_err("zero mining productivity should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidTechnologyEffect { technology }
            if technology == "zero_bonus"
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

/// Reactor output belongs to reactors alone: a heat pipe or exchanger declaring it
/// would silently carry inert metadata that reads as if it produced heat.
#[test]
fn reactor_metadata_on_heat_pipe_fails() {
    let error = PrototypeCatalog::from_ron_str(r#"(
        items: [(id: 0, name: "heat_pipe", stack_size: 100)], recipes: [],
        entities: [(id: 0, name: "heat_pipe", entity_kind: HeatPipe, build_item: Some("heat_pipe"), building_category: Some(Power), building_menu_order: Some(90), size: (x: 1, y: 1), collision_mask: (layers: ["building"]), max_health: Some(100), nuclear_reactor: Some((heat_output_watts: 40000000, neighbour_bonus_permyriad: 10000)), heat_buffer: Some((specific_heat_joules_per_degree: 100000, max_temperature_degrees: 1000, connections: [(local_offset: (x: 0, y: 0), side: North)])))],
        tiles: [],
    )"#).expect_err("heat pipes must not declare reactor output");
    assert!(
        matches!(error, PrototypeLoadError::InvalidHeatMetadata { entity, .. } if entity == "heat_pipe")
    );
}

#[test]
fn reactor_metadata_on_heat_exchanger_fails() {
    let error = PrototypeCatalog::from_ron_str(r#"(
        items: [(id: 0, name: "heat_exchanger", stack_size: 50)], recipes: [],
        fluids: [(id: 0, name: "water"), (id: 1, name: "steam")],
        entities: [(id: 0, name: "heat_exchanger", entity_kind: HeatExchanger, build_item: Some("heat_exchanger"), building_category: Some(Power), building_menu_order: Some(100), size: (x: 3, y: 2), collision_mask: (layers: ["building"]), max_health: Some(200), boiler: Some((water_consumption_per_second_milliunits: 103000, steam_output_per_second_milliunits: 103000)), heat_energy_source: Some((energy_usage_watts: 10000000, min_working_temperature_degrees: 500)), nuclear_reactor: Some((heat_output_watts: 40000000, neighbour_bonus_permyriad: 10000)), heat_buffer: Some((specific_heat_joules_per_degree: 100000, max_temperature_degrees: 1000, connections: [(local_offset: (x: 1, y: 1), side: South)])), fluid_boxes: [(capacity_milliunits: 200000, filter: Some("water"), connections: [(local_offset: (x: 0, y: 0), side: North)]), (capacity_milliunits: 200000, filter: Some("steam"), connections: [(local_offset: (x: 2, y: 0), side: North)])])],
        tiles: [],
    )"#).expect_err("heat exchangers must not declare reactor output");
    assert!(
        matches!(error, PrototypeLoadError::InvalidHeatMetadata { entity, .. } if entity == "heat_exchanger")
    );
}

/// Builds a roboport catalog whose roboport section and energy source can be
/// overridden, so each check below differs only in the field it is about.
fn roboport_catalog(
    roboport: &str,
    extra_fields: &str,
) -> Result<PrototypeCatalog, PrototypeLoadError> {
    PrototypeCatalog::from_ron_str(&format!(
        r#"(
            items: [(id: 0, name: "roboport", stack_size: 5)],
            recipes: [],
            entities: [(
                id: 0,
                name: "roboport",
                entity_kind: Roboport,
                build_item: Some("roboport"),
                building_category: Some(Logistics),
                building_menu_order: Some(130),
                size: (x: 4, y: 4),
                collision_mask: (layers: ["building"]),
                max_health: Some(500),
                electric_energy_source: Some((energy_usage_watts: 50000, drain_watts: 3000)),
                roboport: {roboport},
                {extra_fields}
            )],
            tiles: [],
        )"#
    ))
}

const VALID_ROBOPORT: &str = r#"Some((
    construction_radius_tiles: 55,
    logistic_radius_tiles: 25,
    robot_slot_count: 6,
    material_slot_count: 4,
    charging_energy_buffer_joules: 100000000,
    charging_pad_count: 4,
    charging_pad_watts: 1200000,
))"#;

#[test]
fn valid_roboport_loads() {
    let catalog = roboport_catalog(VALID_ROBOPORT, "").expect("roboport should load");
    let roboport = catalog.entities[0]
        .roboport
        .expect("roboport metadata should survive loading");

    assert_eq!(roboport.construction_radius_tiles, 55);
    assert_eq!(roboport.logistic_radius_tiles, 25);
}

#[test]
fn roboport_without_metadata_fails() {
    let error = roboport_catalog("None", "").expect_err("roboports require roboport metadata");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRoboportMetadata { entity, .. } if entity == "roboport")
    );
}

#[test]
fn roboport_with_zero_radius_fails() {
    let error = roboport_catalog(
        r#"Some((
            construction_radius_tiles: 55,
            logistic_radius_tiles: 0,
            robot_slot_count: 6,
            material_slot_count: 4,
            charging_energy_buffer_joules: 100000000,
            charging_pad_count: 4,
            charging_pad_watts: 1200000,
        ))"#,
        "",
    )
    .expect_err("a roboport with no logistic radius could never join a network");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRoboportMetadata { entity, .. } if entity == "roboport")
    );
}

#[test]
fn roboport_without_slots_fails() {
    let error = roboport_catalog(
        r#"Some((
            construction_radius_tiles: 55,
            logistic_radius_tiles: 25,
            robot_slot_count: 0,
            material_slot_count: 4,
            charging_energy_buffer_joules: 100000000,
            charging_pad_count: 4,
            charging_pad_watts: 1200000,
        ))"#,
        "",
    )
    .expect_err("roboports require robot slots");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRoboportMetadata { entity, .. } if entity == "roboport")
    );
}

/// A roboport that also declared a burner would have two answers for what
/// powers robot charging.
#[test]
fn roboport_with_second_energy_source_fails() {
    let error = roboport_catalog(VALID_ROBOPORT, "burner: Some((energy_usage_watts: 90000)),")
        .expect_err("roboports run on electricity alone");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRoboportMetadata { entity, .. } if entity == "roboport")
    );
}

#[test]
fn roboport_metadata_on_other_kinds_fails() {
    let error = PrototypeCatalog::from_ron_str(&format!(
        r#"(
            items: [(id: 0, name: "chest", stack_size: 50)],
            recipes: [],
            entities: [(
                id: 0,
                name: "chest",
                entity_kind: Chest,
                build_item: Some("chest"),
                building_category: Some(Storage),
                building_menu_order: Some(1),
                size: (x: 1, y: 1),
                collision_mask: (layers: ["building"]),
                inventory_slot_count: Some(16),
                roboport: {VALID_ROBOPORT},
            )],
            tiles: [],
        )"#
    ))
    .expect_err("roboport metadata only valid on roboports");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRoboportMetadata { entity, .. } if entity == "chest")
    );
}

/// A roboport with no pad accepts robots it can never finish charging, which
/// would leave a queue that only ever grows.
#[test]
fn roboport_without_charging_pads_fails() {
    let error = roboport_catalog(
        r#"Some((
            construction_radius_tiles: 55,
            logistic_radius_tiles: 25,
            robot_slot_count: 6,
            material_slot_count: 4,
            charging_energy_buffer_joules: 100000000,
            charging_pad_count: 0,
            charging_pad_watts: 1200000,
        ))"#,
        "",
    )
    .expect_err("roboports require a charging pad");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRoboportMetadata { entity, .. } if entity == "roboport")
    );
}

/// Builds an item-only catalog whose single item carries the given robot
/// section, so each robot check differs only in the field it is about.
fn robot_item_catalog(fields: &str) -> Result<PrototypeCatalog, PrototypeLoadError> {
    PrototypeCatalog::from_ron_str(&format!(
        r#"(
            items: [(id: 0, name: "construction_robot", stack_size: 50, {fields})],
            recipes: [],
            entities: [],
            tiles: [],
        )"#
    ))
}

const VALID_ROBOT: &str = r#"robot: Some((
    kind: Construction,
    speed_fixed_per_tick: 60,
    energy_capacity_joules: 1500000,
    flight_energy_usage_watts: 21000,
))"#;

#[test]
fn valid_robot_item_loads() {
    let catalog = robot_item_catalog(VALID_ROBOT).expect("robot item should load");
    let robot = catalog.items[0]
        .robot
        .expect("robot metadata should survive loading");

    assert_eq!(robot.speed_fixed_per_tick, 60);
    assert_eq!(robot.energy_capacity_joules, 1_500_000);
}

#[test]
fn robot_without_flight_energy_fails() {
    let error = robot_item_catalog(
        r#"robot: Some((
            kind: Construction,
            speed_fixed_per_tick: 60,
            energy_capacity_joules: 1500000,
            flight_energy_usage_watts: 0,
        ))"#,
    )
    .expect_err("a robot that spends nothing to fly would never need a roboport");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRobotMetadata { item, .. } if item == "construction_robot")
    );
}

/// The roboport's two inventories accept disjoint item sets derived from the
/// item prototype, so an item claiming to be both a robot and repair material
/// would be accepted by whichever half was tried first.
#[test]
fn robot_that_is_also_repair_material_fails() {
    let error = robot_item_catalog(&format!(
        "{VALID_ROBOT}, repair: Some((restore_health: 200))"
    ))
    .expect_err("a robot cannot double as repair material");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRobotMetadata { item, .. } if item == "construction_robot")
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

/// Builds a chest catalog whose kind and logistic section can be overridden, so
/// each check below differs only in the field it is about.
fn logistic_chest_catalog(
    entity_kind: &str,
    inventory_slot_count: &str,
    logistic_chest: &str,
) -> Result<PrototypeCatalog, PrototypeLoadError> {
    PrototypeCatalog::from_ron_str(&format!(
        r#"(
            items: [(id: 0, name: "requester_chest", stack_size: 50)],
            recipes: [],
            entities: [(
                id: 0,
                name: "requester_chest",
                entity_kind: {entity_kind},
                build_item: Some("requester_chest"),
                building_category: Some(Storage),
                building_menu_order: Some(17),
                size: (x: 1, y: 1),
                collision_mask: (layers: ["building"]),
                inventory_slot_count: {inventory_slot_count},
                logistic_chest: {logistic_chest},
                max_health: Some(350),
            )],
            tiles: [],
        )"#
    ))
}

#[test]
fn valid_logistic_chest_loads() {
    let catalog = logistic_chest_catalog(
        "Chest",
        "Some(48)",
        "Some((mode: Requester, request_slot_count: 12))",
    )
    .expect("a requester chest should load");
    let logistic_chest = catalog.entities[0]
        .logistic_chest
        .expect("logistic chest metadata should survive loading");

    assert_eq!(logistic_chest.mode, LogisticChestMode::Requester);
    assert_eq!(logistic_chest.request_slot_count, 12);
}

#[test]
fn logistic_chest_metadata_on_a_non_chest_fails() {
    let error = logistic_chest_catalog(
        "Wall",
        "Some(48)",
        "Some((mode: Requester, request_slot_count: 12))",
    )
    .expect_err("only chests can carry a logistic role");
    assert!(
        matches!(error, PrototypeLoadError::InvalidLogisticChestMetadata { entity, .. } if entity == "requester_chest")
    );
}

#[test]
fn logistic_chest_without_inventory_fails() {
    let error = logistic_chest_catalog(
        "Chest",
        "None",
        "Some((mode: Requester, request_slot_count: 12))",
    )
    .expect_err("a logistic chest with nowhere to put items is not a chest");
    assert!(
        matches!(error, PrototypeLoadError::InvalidLogisticChestMetadata { entity, .. } if entity == "requester_chest")
    );
}

/// The row count has to match what the mode does with rows, because the
/// simulation reads them positionally without re-asking what the mode is.
#[test]
fn logistic_chest_row_counts_must_match_the_mode() {
    for (mode, rows) in [
        ("PassiveProvider", 1),
        ("ActiveProvider", 2),
        ("Storage", 0),
        ("Storage", 3),
        ("Buffer", 0),
        ("Requester", 0),
    ] {
        let error = logistic_chest_catalog(
            "Chest",
            "Some(48)",
            &format!("Some((mode: {mode}, request_slot_count: {rows}))"),
        )
        .expect_err("mismatched row counts should fail");
        assert!(
            matches!(&error, PrototypeLoadError::InvalidLogisticChestMetadata { entity, .. } if entity == "requester_chest"),
            "unexpected error for {mode} with {rows} rows: {error}"
        );
    }
}

/// Builds a one-piece rail catalog whose kind, footprint, and geometry can be
/// overridden, so each check below differs only in the field it is about.
fn rail_catalog(
    entity_kind: &str,
    size: &str,
    rail_piece: &str,
) -> Result<PrototypeCatalog, PrototypeLoadError> {
    PrototypeCatalog::from_ron_str(&format!(
        r#"(
            items: [(id: 0, name: "rail", stack_size: 100)],
            recipes: [],
            entities: [(
                id: 0,
                name: "rail",
                entity_kind: {entity_kind},
                build_item: Some("rail"),
                building_category: Some(Logistics),
                building_menu_order: Some(140),
                size: {size},
                collision_mask: (layers: ["transport"]),
                max_health: Some(100),
                rail_piece: {rail_piece},
            )],
            tiles: [],
        )"#
    ))
}

const VALID_STRAIGHT_PIECE: &str = r#"Some((
    start: (position: (x: 512, y: 0), heading: South),
    end: (position: (x: 512, y: 2048), heading: North),
    curve: Straight,
))"#;

#[test]
fn valid_rail_geometry_loads() {
    let catalog = rail_catalog("RailStraight", "(x: 1, y: 2)", VALID_STRAIGHT_PIECE)
        .expect("a well-formed rail piece should load");

    assert_eq!(
        catalog.entities[0]
            .rail_piece
            .expect("rail geometry should be present")
            .length(),
        2_048
    );
}

#[test]
fn rail_entities_require_geometry() {
    let error = rail_catalog("RailStraight", "(x: 1, y: 2)", "None")
        .expect_err("a rail without geometry has no path to run on");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidRailMetadata { entity, .. } if entity == "rail"
    ));
}

#[test]
fn rail_geometry_on_other_kinds_fails() {
    let error = rail_catalog("Wall", "(x: 1, y: 2)", VALID_STRAIGHT_PIECE)
        .expect_err("only rails declare rail geometry");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidRailMetadata { entity, .. } if entity == "rail"
    ));
}

/// Each of these breaks one geometric rule the rail graph and the renderer rely
/// on, and every one of them would otherwise produce track that connects to
/// nothing, leaves its own footprint, or has no well-defined length.
#[test]
fn malformed_rail_geometry_is_rejected() {
    let cases = [
        (
            "end off the footprint edge it faces",
            "RailStraight",
            "(x: 1, y: 2)",
            r#"Some((
                start: (position: (x: 512, y: 256), heading: South),
                end: (position: (x: 512, y: 2048), heading: North),
                curve: Straight,
            ))"#,
        ),
        (
            "end outside the footprint",
            "RailStraight",
            "(x: 1, y: 2)",
            r#"Some((
                start: (position: (x: 512, y: 0), heading: South),
                end: (position: (x: 512, y: 4096), heading: North),
                curve: Straight,
            ))"#,
        ),
        (
            "straight whose ends do not face opposite ways",
            "RailStraight",
            "(x: 2, y: 2)",
            r#"Some((
                start: (position: (x: 512, y: 0), heading: South),
                end: (position: (x: 2048, y: 512), heading: East),
                curve: Straight,
            ))"#,
        ),
        (
            "curve whose ends are not on one circle",
            "RailCurved",
            "(x: 2, y: 2)",
            r#"Some((
                start: (position: (x: 512, y: 0), heading: South),
                end: (position: (x: 2048, y: 1536), heading: East),
                curve: QuarterArc(center: (x: 2048, y: 512)),
            ))"#,
        ),
        (
            "curve that does not leave its end along the tangent",
            "RailCurved",
            "(x: 2, y: 2)",
            r#"Some((
                start: (position: (x: 512, y: 0), heading: South),
                end: (position: (x: 1536, y: 2048), heading: North),
                curve: QuarterArc(center: (x: 2048, y: 0)),
            ))"#,
        ),
        (
            "curve whose ends face the same way",
            "RailCurved",
            "(x: 2, y: 2)",
            r#"Some((
                start: (position: (x: 512, y: 0), heading: South),
                end: (position: (x: 1536, y: 0), heading: South),
                curve: QuarterArc(center: (x: 2048, y: 0)),
            ))"#,
        ),
    ];

    for (reason, entity_kind, size, rail_piece) in cases {
        let error = rail_catalog(entity_kind, size, rail_piece)
            .err()
            .unwrap_or_else(|| panic!("{reason} should be rejected"));
        assert!(
            matches!(error, PrototypeLoadError::InvalidRailMetadata { entity, .. } if entity == "rail"),
            "{reason} should be reported as invalid rail metadata"
        );
    }
}

/// The arc's length is computed from a whole-unit radius, so a curve whose ends
/// sit at an irrational distance from its center would have a length that does
/// not match the curve it declares.
#[test]
fn a_curve_with_a_fractional_radius_is_rejected() {
    let error = rail_catalog(
        "RailCurved",
        "(x: 2, y: 2)",
        r#"Some((
            start: (position: (x: 0, y: 1000), heading: West),
            end: (position: (x: 1000, y: 0), heading: South),
            curve: QuarterArc(center: (x: 0, y: 0)),
        ))"#,
    )
    .err();

    // Radius 1000 is whole here, so this case must load; the guard is exercised
    // by the mismatched pair below.
    assert!(error.is_none(), "a whole-unit radius is fine");

    let error = rail_catalog(
        "RailCurved",
        "(x: 2, y: 2)",
        r#"Some((
            start: (position: (x: 0, y: 1001), heading: West),
            end: (position: (x: 1001, y: 0), heading: South),
            curve: QuarterArc(center: (x: 3, y: 4)),
        ))"#,
    )
    .expect_err("a curve whose ends are not a whole radius from its center is not a circle");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidRailMetadata { entity, .. } if entity == "rail"
    ));
}

/// Builds a one-piece rolling-stock catalog whose kind, cargo declarations, and
/// motion metadata can be overridden, so each check below differs only in the
/// field it is about.
fn rolling_stock_catalog(
    entity_kind: &str,
    extra_fields: &str,
    rolling_stock: &str,
) -> Result<PrototypeCatalog, PrototypeLoadError> {
    PrototypeCatalog::from_ron_str(&format!(
        r#"(
            items: [(id: 0, name: "locomotive", stack_size: 5)],
            recipes: [],
            entities: [(
                id: 0,
                name: "locomotive",
                entity_kind: {entity_kind},
                build_item: Some("locomotive"),
                building_category: Some(Logistics),
                building_menu_order: Some(142),
                size: (x: 2, y: 7),
                collision_mask: (layers: ["transport"]),
                max_health: Some(1000),
                {extra_fields}
                rolling_stock: {rolling_stock},
            )],
            tiles: [],
        )"#
    ))
}

const VALID_LOCOMOTIVE_STOCK: &str = r#"Some((
    length_fixed: 7168,
    weight_kilograms: 2000,
    braking_force_newtons: 24000,
    max_speed_fixed_per_tick: 1229,
    locomotive: Some((tractive_force_newtons: 12000)),
))"#;

#[test]
fn rolling_stock_without_motion_metadata_fails() {
    let error = rolling_stock_catalog(
        "Locomotive",
        "burner: Some((energy_usage_watts: 600000)),",
        "None",
    )
    .expect_err("rolling stock with no weight or length could not be stepped");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidRollingStockMetadata { entity, .. } if entity == "locomotive"
    ));
}

#[test]
fn rolling_stock_metadata_on_other_kinds_fails() {
    let error = rolling_stock_catalog("Wall", "", VALID_LOCOMOTIVE_STOCK)
        .expect_err("only rolling stock runs on rails");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidRollingStockMetadata { entity, .. } if entity == "locomotive"
    ));
}

/// Each of these breaks something the motion model treats as a fact: a zero
/// weight is a division by zero, a zero length is a train of pieces stacked on
/// one point, and a zero top speed is a train that can never move.
#[test]
fn malformed_rolling_stock_metadata_is_rejected() {
    let cases = [
        (
            "zero length",
            r#"Some((
                length_fixed: 0,
                weight_kilograms: 2000,
                braking_force_newtons: 24000,
                max_speed_fixed_per_tick: 1229,
                locomotive: Some((tractive_force_newtons: 12000)),
            ))"#,
        ),
        (
            "zero weight",
            r#"Some((
                length_fixed: 7168,
                weight_kilograms: 0,
                braking_force_newtons: 24000,
                max_speed_fixed_per_tick: 1229,
                locomotive: Some((tractive_force_newtons: 12000)),
            ))"#,
        ),
        (
            "zero top speed",
            r#"Some((
                length_fixed: 7168,
                weight_kilograms: 2000,
                braking_force_newtons: 24000,
                max_speed_fixed_per_tick: 0,
                locomotive: Some((tractive_force_newtons: 12000)),
            ))"#,
        ),
        (
            "a locomotive that pulls with no force",
            r#"Some((
                length_fixed: 7168,
                weight_kilograms: 2000,
                braking_force_newtons: 24000,
                max_speed_fixed_per_tick: 1229,
                locomotive: Some((tractive_force_newtons: 0)),
            ))"#,
        ),
        (
            "a locomotive with no locomotive section",
            r#"Some((
                length_fixed: 7168,
                weight_kilograms: 2000,
                braking_force_newtons: 24000,
                max_speed_fixed_per_tick: 1229,
            ))"#,
        ),
    ];

    for (reason, rolling_stock) in cases {
        let error = rolling_stock_catalog(
            "Locomotive",
            "burner: Some((energy_usage_watts: 600000)),",
            rolling_stock,
        )
        .err()
        .unwrap_or_else(|| panic!("{reason} should be rejected"));
        assert!(
            matches!(
                error,
                PrototypeLoadError::InvalidRollingStockMetadata { entity, .. }
                    if entity == "locomotive"
            ),
            "{reason} should be reported as invalid rolling stock metadata"
        );
    }
}

/// Tractive force is what burnt fuel buys, so a locomotive without a burner
/// would pull for free.
#[test]
fn a_locomotive_without_a_burner_is_rejected() {
    let error = rolling_stock_catalog("Locomotive", "", VALID_LOCOMOTIVE_STOCK)
        .expect_err("a locomotive needs something to burn");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidRollingStockMetadata { entity, .. } if entity == "locomotive"
    ));
}

/// A wagon whose whole purpose is missing: the cargo declarations are checked
/// against the kind for the same reason the rail geometry is.
#[test]
fn wagons_without_the_cargo_their_kind_implies_are_rejected() {
    const WAGON_STOCK: &str = r#"Some((
        length_fixed: 6144,
        weight_kilograms: 1000,
        braking_force_newtons: 12000,
        max_speed_fixed_per_tick: 1536,
    ))"#;

    for entity_kind in ["CargoWagon", "FluidWagon"] {
        let error = rolling_stock_catalog(entity_kind, "", WAGON_STOCK)
            .err()
            .unwrap_or_else(|| panic!("a {entity_kind} with nowhere to put cargo should fail"));
        assert!(
            matches!(
                error,
                PrototypeLoadError::InvalidRollingStockMetadata { entity, .. }
                    if entity == "locomotive"
            ),
            "a {entity_kind} with nowhere to put cargo should be reported as invalid"
        );
    }

    assert!(
        rolling_stock_catalog("CargoWagon", "inventory_slot_count: Some(40),", WAGON_STOCK).is_ok(),
        "a cargo wagon with inventory slots is well formed"
    );
    // A fluid wagon's tank has no pipe openings on purpose: it is filled at a
    // station rather than by touching a pipe, and it is never in the occupancy
    // grid a pipe network is built from.
    assert!(
        rolling_stock_catalog(
            "FluidWagon",
            r#"fluid_boxes: [(capacity_milliunits: 25000000, connections: [])],"#,
            WAGON_STOCK
        )
        .is_ok(),
        "a fluid wagon's unconnected tank is well formed"
    );
}

/// Builds a rocket silo catalog whose silo section and surrounding fields can be
/// overridden, so each check below differs only in the field it is about.
fn rocket_silo_catalog(
    entity_kind: &str,
    rocket_silo: &str,
    extra_fields: &str,
) -> Result<PrototypeCatalog, PrototypeLoadError> {
    const ELECTRIC: &str =
        "electric_energy_source: Some((energy_usage_watts: 4000000, drain_watts: 250000)),";
    let extra_fields = if extra_fields.contains("electric_energy_source") {
        extra_fields.to_string()
    } else {
        format!("{ELECTRIC}{extra_fields}")
    };
    PrototypeCatalog::from_ron_str(&format!(
        r#"(
            items: [
                (id: 0, name: "rocket_silo", stack_size: 1),
                (id: 1, name: "satellite", stack_size: 1),
                (id: 2, name: "space_science_pack", stack_size: 200),
            ],
            recipes: [],
            entities: [(
                id: 0,
                name: "rocket_silo",
                entity_kind: {entity_kind},
                build_item: Some("rocket_silo"),
                building_category: Some(Production),
                building_menu_order: Some(80),
                size: (x: 9, y: 9),
                collision_mask: (layers: ["building"]),
                max_health: Some(5000),
                rocket_silo: {rocket_silo},
                {extra_fields}
            )],
            tiles: [],
        )"#
    ))
}

const VALID_ROCKET_SILO: &str = r#"Some((
    crafting_speed_numerator: 1,
    crafting_speed_denominator: 1,
    input_slot_count: 4,
    parts_per_rocket: 100,
    launch_payload: "satellite",
    launch_product: (item: "space_science_pack", amount: 1000),
    output_slot_count: 5,
))"#;

#[test]
fn valid_rocket_silo_loads_with_module_slots() {
    let catalog = rocket_silo_catalog("RocketSilo", VALID_ROCKET_SILO, " module_slot_count: 4,")
        .expect("rocket silo should load");
    let rocket_silo = catalog.entities[0]
        .rocket_silo
        .expect("rocket silo metadata should survive loading");

    assert_eq!(rocket_silo.parts_per_rocket, 100);
    assert_eq!(rocket_silo.launch_product.amount, 1_000);
    assert_eq!(rocket_silo.output_slot_count, 5);
    assert_eq!(catalog.entities[0].module_slot_count, 4);
}

#[test]
fn rocket_silo_launch_items_resolve_and_output_capacity_is_validated() {
    for (metadata, expected_role) in [
        (
            VALID_ROCKET_SILO.replace(
                "launch_payload: \"satellite\"",
                "launch_payload: \"missing\"",
            ),
            "payload",
        ),
        (
            VALID_ROCKET_SILO.replace("item: \"space_science_pack\"", "item: \"missing\""),
            "product",
        ),
    ] {
        let error = rocket_silo_catalog("RocketSilo", &metadata, "")
            .expect_err("missing launch items should fail");
        assert!(matches!(
            error,
            PrototypeLoadError::MissingRocketSiloLaunchItem { role, .. }
                if role == expected_role
        ));
    }

    let undersized = VALID_ROCKET_SILO.replace("output_slot_count: 5", "output_slot_count: 4");
    let error = rocket_silo_catalog("RocketSilo", &undersized, "")
        .expect_err("four stacks cannot hold one thousand packs of stack size two hundred");
    assert!(matches!(
        error,
        PrototypeLoadError::InvalidRocketSiloMetadata { entity, .. }
            if entity == "rocket_silo"
    ));
}

/// Each of these leaves the silo unable to derive the thing the simulation asks
/// it for: the section itself, the power it runs on, somewhere to hold
/// ingredients, a speed to craft at, or a rocket size to count toward.
#[test]
fn incoherent_rocket_silo_metadata_fails() {
    let cases = [
        (
            "None",
            "",
            "a silo without its section has no recipe to run",
        ),
        (
            VALID_ROCKET_SILO,
            "electric_energy_source: None,",
            "a silo with no energy source could never work",
        ),
        (
            r#"Some((crafting_speed_numerator: 1, crafting_speed_denominator: 0, input_slot_count: 4, parts_per_rocket: 100, launch_payload: "satellite", launch_product: (item: "space_science_pack", amount: 1000), output_slot_count: 5))"#,
            "",
            "a zero crafting-speed denominator is not a fraction",
        ),
        (
            r#"Some((crafting_speed_numerator: 1, crafting_speed_denominator: 1, input_slot_count: 0, parts_per_rocket: 100, launch_payload: "satellite", launch_product: (item: "space_science_pack", amount: 1000), output_slot_count: 5))"#,
            "",
            "a silo with no ingredient slots could never be fed",
        ),
        (
            r#"Some((crafting_speed_numerator: 1, crafting_speed_denominator: 1, input_slot_count: 4, parts_per_rocket: 0, launch_payload: "satellite", launch_product: (item: "space_science_pack", amount: 1000), output_slot_count: 5))"#,
            "",
            "a rocket of no parts would be finished before it started",
        ),
    ];

    for (rocket_silo, extra_fields, reason) in cases {
        let error = rocket_silo_catalog("RocketSilo", rocket_silo, extra_fields)
            .err()
            .unwrap_or_else(|| panic!("{reason}"));
        assert!(
            matches!(
                error,
                PrototypeLoadError::InvalidRocketSiloMetadata { entity, .. }
                    if entity == "rocket_silo"
            ),
            "{reason}"
        );
    }
}

#[test]
fn rocket_silo_metadata_on_another_kind_fails() {
    let error = rocket_silo_catalog("AssemblingMachine", VALID_ROCKET_SILO, "")
        .expect_err("only a rocket silo builds rockets");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRocketSiloMetadata { entity, .. } if entity == "rocket_silo")
    );
}

/// The fixed recipe must fit all of its ingredients at once because crafting
/// checks and consumes them atomically. This covers both distinct item stacks
/// and a single ingredient whose amount spans multiple stacks.
#[test]
fn undersized_rocket_silo_input_inventory_fails() {
    let catalog = |input_slot_count, ingredients: &str| {
        PrototypeCatalog::from_ron_str(&format!(
            r#"(
                items: [
                    (id: 0, name: "rocket_part", stack_size: 1),
                    (id: 1, name: "steel_plate", stack_size: 100),
                    (id: 2, name: "processing_unit", stack_size: 100),
                    (id: 3, name: "satellite", stack_size: 1),
                    (id: 4, name: "space_science_pack", stack_size: 200),
                ],
                recipes: [(
                    id: 0, name: "rocket_part", category: RocketBuilding,
                    crafting_time_ticks: 180, ingredients: [{ingredients}],
                    products: [(item: "rocket_part", amount: 1)],
                )],
                entities: [(
                    id: 0, name: "rocket_silo", entity_kind: RocketSilo,
                    size: (x: 9, y: 9), collision_mask: (layers: ["building"]),
                    electric_energy_source: Some((
                        energy_usage_watts: 4000000, drain_watts: 250000,
                    )),
                    rocket_silo: Some((
                        crafting_speed_numerator: 1, crafting_speed_denominator: 1,
                        input_slot_count: {input_slot_count}, parts_per_rocket: 100,
                        launch_payload: "satellite",
                        launch_product: (item: "space_science_pack", amount: 1000),
                        output_slot_count: 5,
                    )),
                )],
                tiles: [],
            )"#
        ))
    };

    let two_items = r#"(item: "steel_plate", amount: 1),
                       (item: "processing_unit", amount: 1)"#;
    let oversized_stack = r#"(item: "steel_plate", amount: 101)"#;
    for ingredients in [two_items, oversized_stack] {
        let error = catalog(1, ingredients).expect_err("one slot cannot hold this recipe");
        assert!(
            matches!(error, PrototypeLoadError::InvalidRocketSiloMetadata { entity, .. } if entity == "rocket_silo")
        );
        catalog(2, ingredients).expect("two slots can hold this recipe");
    }
}

/// Builds a catalog whose single rocket-building recipe can be overridden, so
/// each check below differs only in the recipe shape it is about.
fn rocket_building_recipe_catalog(
    recipe_body: &str,
) -> Result<PrototypeCatalog, PrototypeLoadError> {
    PrototypeCatalog::from_ron_str(&format!(
        r#"(
            items: [
                (id: 0, name: "rocket_part", stack_size: 1),
                (id: 1, name: "steel_plate", stack_size: 100),
            ],
            fluids: [(id: 0, name: "water")],
            recipes: [(
                id: 0,
                name: "rocket_part",
                category: RocketBuilding,
                crafting_time_ticks: 180,
                ingredients: [(item: "steel_plate", amount: 1)],
                {recipe_body}
            )],
            entities: [],
            tiles: [],
        )"#
    ))
}

#[test]
fn unit_output_rocket_building_recipe_loads() {
    let catalog =
        rocket_building_recipe_catalog(r#"products: [(item: "rocket_part", amount: 1)],"#)
            .expect("one part a craft is what a silo counts");

    assert_eq!(catalog.recipes[0].products[0].amount, 1);
}

/// A silo has no fluid boxes and counts whole crafts, so each of these shapes
/// would be quietly mishandled: fluid amounts never drawn or emitted, or a part
/// counter disagreeing with the production recorded beside it.
#[test]
fn rocket_building_recipes_a_silo_cannot_build_fail() {
    let cases = [
        (
            r#"products: [(item: "rocket_part", amount: 2)],"#,
            "two parts a craft would outrun the counter",
        ),
        (
            "products: [],",
            "a craft that yields no part would never fill a rocket",
        ),
        (
            r#"products: [(item: "rocket_part", amount: 1), (item: "steel_plate", amount: 1)],"#,
            "a second product has nowhere to go in a silo",
        ),
        (
            r#"products: [(item: "rocket_part", amount: 1)], fluid_ingredients: [(fluid: "water", amount: 10)],"#,
            "a fluid ingredient a silo cannot hold would be drawn for free",
        ),
        (
            r#"products: [(item: "rocket_part", amount: 1)], fluid_products: [(fluid: "water", amount: 10)],"#,
            "a fluid product a silo cannot hold would vanish",
        ),
    ];

    for (recipe_body, reason) in cases {
        let error = rocket_building_recipe_catalog(recipe_body)
            .err()
            .unwrap_or_else(|| panic!("{reason}"));
        assert!(
            matches!(
                error,
                PrototypeLoadError::InvalidRocketBuildingRecipe { recipe, .. }
                    if recipe == "rocket_part"
            ),
            "{reason}"
        );
    }
}

/// A silo has nowhere to record which recipe it is building, so two candidates
/// would mean a later research silently switching every silo mid-build.
#[test]
fn a_second_rocket_building_recipe_fails() {
    let error = PrototypeCatalog::from_ron_str(
        r#"(
            items: [
                (id: 0, name: "rocket_part", stack_size: 1),
                (id: 1, name: "steel_plate", stack_size: 100),
            ],
            recipes: [
                (id: 0, name: "rocket_part", category: RocketBuilding, crafting_time_ticks: 180,
                 ingredients: [(item: "steel_plate", amount: 1)],
                 products: [(item: "rocket_part", amount: 1)]),
                (id: 1, name: "cheap_rocket_part", category: RocketBuilding, crafting_time_ticks: 60,
                 ingredients: [(item: "steel_plate", amount: 1)],
                 products: [(item: "rocket_part", amount: 1)]),
            ],
            entities: [],
            tiles: [],
        )"#,
    )
    .expect_err("a silo builds one recipe, so the category holds one");
    assert!(
        matches!(error, PrototypeLoadError::InvalidRocketBuildingRecipe { recipe, .. } if recipe == "cheap_rocket_part")
    );
}
