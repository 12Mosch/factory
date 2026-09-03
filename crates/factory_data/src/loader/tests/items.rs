use std::collections::BTreeSet;

use crate::catalog::PrototypeCatalog;
use crate::model::{
    AmmoCategory, DamageType, EntityKind, EquipmentEffectPrototype, TechnologyEffect,
    WeaponDeliveryPrototype,
};

use super::common::{recipe_by_id, researchable_technology_ids};

#[test]
fn coal_loads_fuel_value() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let coal = catalog
        .items
        .iter()
        .find(|prototype| prototype.name == "coal")
        .expect("base catalog should contain coal");
    let iron_ore = catalog
        .items
        .iter()
        .find(|prototype| prototype.name == "iron_ore")
        .expect("base catalog should contain iron ore");

    assert_eq!(coal.fuel_value_joules, Some(4_000_000));
    assert_eq!(iron_ore.fuel_value_joules, None);
}

/// The fuels form a ladder rather than two far-apart points, and every rung is
/// a real step: this is the thing the refined fuels exist for, so it is worth
/// asserting on the ordering and not only on the individual values.
#[test]
fn refined_fuels_sit_between_coal_and_the_uranium_fuel_cell() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let fuel_value = |name: &str| {
        catalog
            .items
            .iter()
            .find(|prototype| prototype.name == name)
            .unwrap_or_else(|| panic!("base catalog should contain {name}"))
            .fuel_value_joules
            .unwrap_or_else(|| panic!("{name} should be a fuel"))
    };

    let ladder = ["coal", "solid_fuel", "rocket_fuel", "uranium_fuel_cell"];
    let values = ladder.map(fuel_value);
    assert_eq!(values, [4_000_000, 12_000_000, 100_000_000, 8_000_000_000]);
    assert!(
        values.windows(2).all(|rungs| rungs[0] < rungs[1]),
        "the fuel ladder should be strictly increasing: {values:?}"
    );

    // Residue is what makes reprocessing a closed loop for fuel cells. Refined
    // fuels burn away, which is what lets them go in every burner slot.
    for name in ["solid_fuel", "rocket_fuel"] {
        let item = catalog
            .items
            .iter()
            .find(|prototype| prototype.name == name)
            .expect("base catalog should contain the refined fuels");
        assert_eq!(
            item.burnt_result, None,
            "{name} should burn without residue"
        );
    }
}

#[test]
fn military_items_load_typed_ammo_armor_and_powered_equipment() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let firearm = catalog
        .items
        .iter()
        .find(|item| item.name == "firearm_magazine")
        .and_then(|item| item.ammo)
        .unwrap();
    let piercing = catalog
        .items
        .iter()
        .find(|item| item.name == "piercing_rounds_magazine")
        .and_then(|item| item.ammo)
        .unwrap();
    assert_eq!((firearm.shots_per_item, firearm.damage_per_shot), (10, 5));
    assert_eq!((piercing.shots_per_item, piercing.damage_per_shot), (10, 8));
    assert_eq!(firearm.damage_type, DamageType::Physical);
    assert_eq!(piercing.damage_type, DamageType::Physical);
    assert_eq!(firearm.category, AmmoCategory::Bullet);
    assert_eq!(piercing.category, AmmoCategory::Bullet);

    let pistol = catalog
        .items
        .iter()
        .find(|item| item.name == "pistol")
        .and_then(|item| item.weapon)
        .unwrap();
    let submachine_gun = catalog
        .items
        .iter()
        .find(|item| item.name == "submachine_gun")
        .and_then(|item| item.weapon)
        .unwrap();
    assert_eq!((pistol.range_tiles, pistol.cooldown_ticks), (10, 20));
    assert_eq!(
        (submachine_gun.range_tiles, submachine_gun.cooldown_ticks),
        (11, 8)
    );
    assert_eq!(pistol.ammo_category, AmmoCategory::Bullet);
    assert_eq!(pistol.delivery, WeaponDeliveryPrototype::Hitscan);

    let armor = catalog
        .items
        .iter()
        .find(|item| item.name == "modular_armor")
        .and_then(|item| item.armor.as_ref())
        .unwrap();
    assert_eq!((armor.grid_width, armor.grid_height), (5, 5));
    assert_eq!(armor.resistances.len(), 1);
    assert_eq!(armor.resistances[0].damage_type, DamageType::Physical);
    assert_eq!(armor.resistances[0].flat_reduction, 2);
    assert_eq!(armor.resistances[0].percent_reduction_permyriad, 2_000);

    let effects = [
        "portable_solar_panel",
        "battery_equipment",
        "energy_shield_equipment",
    ]
    .map(|name| {
        catalog
            .items
            .iter()
            .find(|item| item.name == name)
            .and_then(|item| item.equipment)
            .unwrap()
            .effect
    });
    assert_eq!(
        effects,
        [
            EquipmentEffectPrototype::PowerGeneration {
                power_watts: 60_000,
            },
            EquipmentEffectPrototype::Battery {
                capacity_joules: 500_000,
            },
            EquipmentEffectPrototype::EnergyShield {
                capacity_points: 50,
                max_recharge_watts: 60_000,
            },
        ]
    );

    let personal = catalog
        .items
        .iter()
        .find(|item| item.name == "personal_roboport_equipment")
        .and_then(|item| item.equipment)
        .expect("base catalog should define personal roboport equipment");
    assert_eq!((personal.width, personal.height), (2, 2));
    assert_eq!(
        personal.effect,
        EquipmentEffectPrototype::PersonalRoboport {
            energy_capacity_joules: 3_000_000,
            energy_input_watts: 200_000,
            charging_pad_count: 2,
            charging_pad_watts: 100_000,
            construction_radius_tiles: 15,
        }
    );
}

#[test]
fn advanced_personal_weapons_load_their_deterministic_delivery_rules() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let item = |name: &str| {
        catalog
            .items
            .iter()
            .find(|item| item.name == name)
            .unwrap_or_else(|| panic!("missing {name}"))
    };

    assert_eq!(
        item("shotgun_shell").ammo.unwrap().category,
        AmmoCategory::ShotgunShell
    );
    assert_eq!(
        item("shotgun").weapon.unwrap().delivery,
        WeaponDeliveryPrototype::Shotgun {
            pellet_count: 8,
            cone_half_width_permyriad: 3_500,
        }
    );
    assert_eq!(
        item("rocket_launcher").weapon.unwrap().delivery,
        WeaponDeliveryPrototype::Rocket {
            speed_fixed_per_tick: 256,
            explosion_radius_tiles: 3,
        }
    );
    assert_eq!(
        item("flamethrower").weapon.unwrap().delivery,
        WeaponDeliveryPrototype::Flame {
            cone_half_width_permyriad: 5_000,
            burn_duration_ticks: 180,
            burn_interval_ticks: 30,
        }
    );
    assert_eq!(
        item("personal_laser_equipment").equipment.unwrap().effect,
        EquipmentEffectPrototype::PersonalLaser {
            damage: 15,
            range_tiles: 15,
            cooldown_ticks: 30,
            energy_per_shot_joules: 50_000,
        }
    );
}

#[test]
fn module_items_and_beacon_load_exact_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let expected = [
        ("speed_module_1", 2_000, 0, 5_000, 0),
        ("speed_module_2", 3_000, 0, 6_000, 0),
        ("speed_module_3", 5_000, 0, 7_000, 0),
        ("productivity_module_1", -500, 400, 4_000, 500),
        ("productivity_module_2", -1_000, 600, 6_000, 700),
        ("productivity_module_3", -1_500, 1_000, 8_000, 1_000),
        ("efficiency_module_1", 0, 0, -3_000, 0),
        ("efficiency_module_2", 0, 0, -4_000, 0),
        ("efficiency_module_3", 0, 0, -5_000, 0),
    ];
    for (name, speed, productivity, energy, pollution) in expected {
        let item = catalog
            .items
            .iter()
            .find(|item| item.name == name)
            .unwrap_or_else(|| panic!("missing {name}"));
        let effect = item.module_effect.expect("module metadata should resolve");
        assert_eq!(item.stack_size, 50);
        assert_eq!(effect.speed_delta_permyriad, speed);
        assert_eq!(effect.productivity_permyriad, productivity);
        assert_eq!(effect.energy_delta_permyriad, energy);
        assert_eq!(effect.pollution_delta_permyriad, pollution);
    }

    let beacon = catalog
        .entities
        .iter()
        .find(|entity| entity.name == "beacon")
        .expect("beacon entity should load");
    assert_eq!(beacon.entity_kind, EntityKind::Beacon);
    assert_eq!(beacon.module_slot_count, 2);
    assert_eq!(beacon.size, glam::IVec2::new(3, 3));
    assert_eq!(beacon.beacon.unwrap().effect_radius_tiles, 3);
    assert_eq!(beacon.beacon.unwrap().transmission_permyriad, 5_000);
}

#[test]
fn placeable_items_have_acquisition_paths() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let item_ids = catalog
        .items
        .iter()
        .map(|item| item.id)
        .collect::<BTreeSet<_>>();
    let recipe_products = catalog
        .recipes
        .iter()
        .flat_map(|recipe| recipe.products.iter().map(|product| product.item))
        .collect::<BTreeSet<_>>();
    let research_unlocked_recipes = catalog
        .technologies
        .iter()
        .flat_map(|technology| technology.effects.iter())
        .filter_map(|effect| match effect {
            TechnologyEffect::UnlockRecipe(recipe_id) => Some(*recipe_id),
            TechnologyEffect::MiningDrillProductivity { .. } => None,
        })
        .collect::<BTreeSet<_>>();
    let starting_inventory_items = ["burner_mining_drill", "stone_furnace"]
        .into_iter()
        .map(|name| crate::item_id_by_name(&catalog, name))
        .collect::<BTreeSet<_>>();
    let mineable_resource_items = ["iron_ore", "copper_ore", "coal", "stone"]
        .into_iter()
        .map(|name| crate::item_id_by_name(&catalog, name))
        .collect::<BTreeSet<_>>();

    for entity in catalog
        .entities
        .iter()
        .filter(|entity| entity.entity_kind != EntityKind::ResourcePatch)
    {
        let Some(build_item) = entity.build_item else {
            continue;
        };

        assert!(
            item_ids.contains(&build_item),
            "{} build item should exist",
            entity.name
        );
        assert!(
            recipe_products.contains(&build_item)
                || starting_inventory_items.contains(&build_item)
                || mineable_resource_items.contains(&build_item),
            "{} build item should have an acquisition path",
            entity.name
        );
    }

    let researchable_technologies = researchable_technology_ids(&catalog);
    assert_eq!(
        researchable_technologies.len(),
        catalog.technologies.len(),
        "every technology should be reachable from prerequisite roots"
    );

    for technology in &catalog.technologies {
        assert!(
            researchable_technologies.contains(&technology.id),
            "{} should be reachable through research",
            technology.name
        );

        for effect in &technology.effects {
            let TechnologyEffect::UnlockRecipe(recipe_id) = *effect else {
                continue;
            };
            let recipe = recipe_by_id(&catalog, recipe_id);

            assert!(
                research_unlocked_recipes.contains(&recipe_id),
                "{} should be unlocked by a technology",
                recipe.name
            );
            assert!(
                !recipe.products.is_empty() || !recipe.fluid_products.is_empty(),
                "{} should produce at least one item or fluid",
                recipe.name
            );
            assert!(
                recipe
                    .products
                    .iter()
                    .all(|product| recipe_products.contains(&product.item)),
                "{} locked products should resolve to catalog recipe products",
                recipe.name
            );
        }
    }
}
