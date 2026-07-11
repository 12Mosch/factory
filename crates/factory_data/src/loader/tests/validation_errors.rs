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
                entity_kind: Furnace,
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
