use std::collections::{HashMap, HashSet};

use crate::error::PrototypeLoadError;
use crate::ids::{ItemId, TileId};
use crate::model::{ItemPrototype, TilePlacementPrototype};
use crate::raw::RawItemPrototype;

pub(super) fn load_items(
    items: Vec<RawItemPrototype>,
    tile_ids_by_name: &HashMap<String, TileId>,
) -> Result<(Vec<ItemPrototype>, HashMap<String, ItemId>), PrototypeLoadError> {
    let mut item_ids_by_name = HashMap::with_capacity(items.len());
    // Burnt results are resolved after this pass so a fuel may name residue
    // declared later in the list.
    let mut burnt_result_names = Vec::new();
    let mut items = items
        .into_iter()
        .map(|item| {
            validate_item_metadata(&item)?;
            let id = ItemId::new(item.id);
            item_ids_by_name.insert(item.name.clone(), id);
            if let Some(burnt_result) = item.burnt_result.clone() {
                burnt_result_names.push((item.name.clone(), burnt_result));
            }
            let place_as_tile = item
                .place_as_tile
                .map(|placement| {
                    let tile = tile_ids_by_name
                        .get(&placement.tile)
                        .copied()
                        .ok_or_else(|| PrototypeLoadError::MissingItemPlacementTile {
                            item: item.name.clone(),
                            tile: placement.tile.clone(),
                        })?;
                    if placement.building_menu_order == 0 {
                        return Err(PrototypeLoadError::InvalidTilePlacementMetadata {
                            item: item.name.clone(),
                            detail: "building_menu_order must be at least 1",
                        });
                    }
                    Ok::<_, PrototypeLoadError>(TilePlacementPrototype {
                        tile,
                        fills_water: placement.fills_water,
                        building_category: placement.building_category,
                        building_menu_order: placement.building_menu_order,
                    })
                })
                .transpose()?;
            Ok(ItemPrototype {
                id,
                name: item.name,
                stack_size: item.stack_size,
                fuel_value_joules: item.fuel_value_joules,
                burnt_result: None,
                ammo: item.ammo,
                repair: item.repair,
                armor: item.armor,
                equipment: item.equipment,
                module_effect: item.module_effect,
                place_as_tile,
                robot: item.robot,
            })
        })
        .collect::<Result<Vec<ItemPrototype>, PrototypeLoadError>>()?;

    for (item_name, burnt_result_name) in burnt_result_names {
        let burnt_result = item_ids_by_name
            .get(&burnt_result_name)
            .copied()
            .ok_or_else(|| PrototypeLoadError::MissingBurntResultItem {
                item: item_name.clone(),
                burnt_result: burnt_result_name,
            })?;
        let item = items
            .iter_mut()
            .find(|item| item.name == item_name)
            .expect("burnt results are collected from the items being loaded");
        item.burnt_result = Some(burnt_result);
    }

    Ok((items, item_ids_by_name))
}

fn validate_item_metadata(item: &RawItemPrototype) -> Result<(), PrototypeLoadError> {
    // A residue is what remains after burning, so it is meaningless without a
    // fuel value, and self-reference would make a fuel burn into itself.
    if let Some(burnt_result) = item.burnt_result.as_deref() {
        if item.fuel_value_joules.is_none() {
            return Err(PrototypeLoadError::InvalidItemFuelMetadata {
                item: item.name.clone(),
                detail: "burnt results require a fuel value",
            });
        }
        if burnt_result == item.name {
            return Err(PrototypeLoadError::InvalidItemFuelMetadata {
                item: item.name.clone(),
                detail: "a fuel cannot burn into itself",
            });
        }
    }
    if let Some(effect) = item.module_effect {
        if effect.speed_delta_permyriad == 0
            && effect.productivity_permyriad == 0
            && effect.energy_delta_permyriad == 0
            && effect.pollution_delta_permyriad == 0
        {
            return Err(PrototypeLoadError::InvalidModuleMetadata {
                item: item.name.clone(),
                detail: "at least one effect must be non-zero",
            });
        }
        if item.fuel_value_joules.is_some()
            || item.ammo.is_some()
            || item.repair.is_some()
            || item.armor.is_some()
            || item.equipment.is_some()
        {
            return Err(PrototypeLoadError::InvalidModuleMetadata {
                item: item.name.clone(),
                detail: "modules cannot also be fuel, ammunition, repair tools, armor, or equipment",
            });
        }
    }
    if item
        .ammo
        .is_some_and(|ammo| ammo.damage_per_shot == 0 || ammo.shots_per_item == 0)
    {
        return Err(PrototypeLoadError::InvalidAmmoMetadata {
            item: item.name.clone(),
            detail: "damage and shots per item must be positive",
        });
    }
    if let Some(armor) = item.armor.as_ref() {
        if armor.grid_width == 0 || armor.grid_height == 0 {
            return Err(PrototypeLoadError::InvalidArmorMetadata {
                item: item.name.clone(),
                detail: "grid dimensions must be positive",
            });
        }
        let mut types = HashSet::new();
        for resistance in &armor.resistances {
            if resistance.percent_reduction_permyriad > 10_000 {
                return Err(PrototypeLoadError::InvalidArmorMetadata {
                    item: item.name.clone(),
                    detail: "resistance percentages cannot exceed 100%",
                });
            }
            if !types.insert(resistance.damage_type) {
                return Err(PrototypeLoadError::InvalidArmorMetadata {
                    item: item.name.clone(),
                    detail: "resistance damage types must be unique",
                });
            }
        }
    }
    if let Some(equipment) = item.equipment {
        use crate::model::EquipmentEffectPrototype;
        let effect_is_valid = match equipment.effect {
            EquipmentEffectPrototype::PowerGeneration { power_watts } => power_watts > 0,
            EquipmentEffectPrototype::Battery { capacity_joules } => capacity_joules > 0,
            EquipmentEffectPrototype::EnergyShield {
                capacity_points,
                max_recharge_watts,
            } => capacity_points > 0 && max_recharge_watts > 0,
        };
        if equipment.width == 0 || equipment.height == 0 || !effect_is_valid {
            return Err(PrototypeLoadError::InvalidEquipmentMetadata {
                item: item.name.clone(),
                detail: "dimensions and effect power/capacity values must be positive",
            });
        }
    }
    if let Some(robot) = item.robot {
        if robot.speed_fixed_per_tick == 0
            || robot.energy_capacity_joules == 0
            || robot.flight_energy_usage_watts == 0
        {
            return Err(PrototypeLoadError::InvalidRobotMetadata {
                item: item.name.clone(),
                detail: "speed, energy capacity, and flight draw must be positive",
            });
        }
        // A roboport's two inventories accept disjoint item sets, and both
        // answers are derived from the item prototype: robots go in the robot
        // slots, repair material in the material slots. An item that claimed
        // both would be accepted by whichever half was tried first.
        if item.repair.is_some()
            || item.fuel_value_joules.is_some()
            || item.ammo.is_some()
            || item.armor.is_some()
            || item.equipment.is_some()
            || item.module_effect.is_some()
            || item.place_as_tile.is_some()
        {
            return Err(PrototypeLoadError::InvalidRobotMetadata {
                item: item.name.clone(),
                detail: "robots cannot also be fuel, ammunition, repair tools, armor, equipment, modules, or tiles",
            });
        }
    }
    Ok(())
}
