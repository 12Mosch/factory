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
    let items = items
        .into_iter()
        .map(|item| {
            validate_item_metadata(&item)?;
            let id = ItemId::new(item.id);
            item_ids_by_name.insert(item.name.clone(), id);
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
                ammo: item.ammo,
                repair: item.repair,
                armor: item.armor,
                equipment: item.equipment,
                module_effect: item.module_effect,
                place_as_tile,
            })
        })
        .collect::<Result<_, PrototypeLoadError>>()?;

    Ok((items, item_ids_by_name))
}

fn validate_item_metadata(item: &RawItemPrototype) -> Result<(), PrototypeLoadError> {
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
    Ok(())
}
