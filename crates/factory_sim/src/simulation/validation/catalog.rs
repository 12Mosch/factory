use super::super::*;
use super::ids::*;
use factory_data::EquipmentEffectPrototype;
use std::collections::HashSet;

pub(super) fn validate_catalog(catalog: &PrototypeCatalog) -> Result<(), SimValidationError> {
    for (index, item) in catalog.items.iter().enumerate() {
        if item.id.index() != index {
            return Err(SimValidationError::UnknownItem(item.id));
        }
        if let Some(ammo) = item.ammo
            && (ammo.damage_per_shot == 0 || ammo.shots_per_item == 0)
        {
            return Err(SimValidationError::UnknownItem(item.id));
        }
        if let Some(repair) = item.repair
            && repair.restore_health == 0
        {
            return Err(SimValidationError::UnknownItem(item.id));
        }
        if let Some(armor) = item.armor.as_ref() {
            let mut types = HashSet::new();
            if armor.grid_width == 0
                || armor.grid_height == 0
                || armor.resistances.iter().any(|resistance| {
                    resistance.percent_reduction_permyriad > 10_000
                        || !types.insert(resistance.damage_type)
                })
            {
                return Err(SimValidationError::UnknownItem(item.id));
            }
        }
        if let Some(equipment) = item.equipment {
            let valid_effect = match equipment.effect {
                EquipmentEffectPrototype::PowerGeneration { power_watts } => power_watts > 0,
                EquipmentEffectPrototype::Battery { capacity_joules } => capacity_joules > 0,
                EquipmentEffectPrototype::EnergyShield {
                    capacity_points,
                    max_recharge_watts,
                } => capacity_points > 0 && max_recharge_watts > 0,
            };
            if equipment.width == 0 || equipment.height == 0 || !valid_effect {
                return Err(SimValidationError::UnknownItem(item.id));
            }
        }
    }

    for (index, fluid) in catalog.fluids.iter().enumerate() {
        if fluid.id.index() != index {
            return Err(SimValidationError::InvalidFluidId(fluid.id));
        }
    }

    for (index, recipe) in catalog.recipes.iter().enumerate() {
        if recipe.id.index() != index {
            return Err(SimValidationError::InvalidCraftingRecipe {
                recipe_id: recipe.id,
            });
        }

        for amount in recipe.ingredients.iter().chain(recipe.products.iter()) {
            if !item_exists(catalog, amount.item) {
                return Err(SimValidationError::InvalidRecipeItem {
                    recipe_id: recipe.id,
                    item_id: amount.item,
                });
            }
        }
        for amount in recipe
            .fluid_ingredients
            .iter()
            .chain(recipe.fluid_products.iter())
        {
            if !fluid_exists(catalog, amount.fluid) {
                return Err(SimValidationError::InvalidFluidId(amount.fluid));
            }
            if amount.amount_milliunits == 0 {
                return Err(SimValidationError::InvalidCraftingRecipe {
                    recipe_id: recipe.id,
                });
            }
        }
    }

    for (index, technology) in catalog.technologies.iter().enumerate() {
        if technology.id.index() != index {
            return Err(SimValidationError::InvalidResearchTechnology {
                technology_id: technology.id,
            });
        }

        for science_pack in &technology.science_packs {
            if !item_exists(catalog, science_pack.item) {
                return Err(SimValidationError::InvalidTechnologyItem {
                    technology_id: technology.id,
                    item_id: science_pack.item,
                });
            }
        }
        for prerequisite_id in &technology.prerequisites {
            if catalog.technology(*prerequisite_id).is_none() {
                return Err(SimValidationError::InvalidTechnologyPrerequisite {
                    technology_id: technology.id,
                    prerequisite_id: *prerequisite_id,
                });
            }
        }
        for effect in &technology.effects {
            let TechnologyEffect::UnlockRecipe(recipe_id) = *effect;
            if catalog.recipe(recipe_id).is_none() {
                return Err(SimValidationError::InvalidTechnologyRecipe {
                    technology_id: technology.id,
                    recipe_id,
                });
            }
        }
    }

    for prototype in &catalog.entities {
        if prototype.size.x <= 0 || prototype.size.y <= 0 {
            return Err(SimValidationError::InvalidCatalogEntityPrototype {
                prototype_id: prototype.id,
            });
        }
        if let Some(item_id) = prototype.build_item
            && !item_exists(catalog, item_id)
        {
            return Err(SimValidationError::UnknownItem(item_id));
        }
        for fluid_box in &prototype.fluid_boxes {
            if fluid_box.capacity_milliunits == 0 {
                return Err(SimValidationError::InvalidCatalogEntityPrototype {
                    prototype_id: prototype.id,
                });
            }
            if let Some(fluid_id) = fluid_box.filter
                && !fluid_exists(catalog, fluid_id)
            {
                return Err(SimValidationError::InvalidFluidId(fluid_id));
            }
        }

        match prototype.entity_kind {
            EntityKind::ElectricPole => {
                let Some(electric_pole) = prototype.electric_pole.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if electric_pole.supply_area_tiles.x <= 0
                    || electric_pole.supply_area_tiles.y <= 0
                    || electric_pole.wire_reach_tiles_x2 == 0
                {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::SteamEngine => {
                let Some(steam_engine) = prototype.steam_engine.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if steam_engine.max_power_output_watts == 0
                    || steam_engine.steam_consumption_per_second_milliunits == 0
                    || prototype.fluid_boxes.len() != 1
                {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::Boiler => {
                let Some(boiler) = prototype.boiler.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if prototype.burner.is_none()
                    || boiler.water_consumption_per_second_milliunits == 0
                    || boiler.steam_output_per_second_milliunits == 0
                    || prototype.fluid_boxes.len() != 2
                {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::OffshorePump => {
                let Some(offshore_pump) = prototype.offshore_pump.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if offshore_pump.pumping_speed_per_second_milliunits == 0 {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
                if prototype.fluid_boxes.len() != 1 {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::Pump => {
                let Some(pump) = prototype.pump.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if pump.pumping_speed_per_second_milliunits == 0
                    || prototype.electric_energy_source.is_none()
                    || prototype.fluid_boxes.len() != 2
                    || prototype.fluid_boxes[0].io != factory_data::FluidBoxIo::Input
                    || prototype.fluid_boxes[1].io != factory_data::FluidBoxIo::Output
                {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::Pipe | EntityKind::StorageTank => {
                if prototype.fluid_boxes.len() != 1 {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
                if prototype
                    .underground_pipe
                    .as_ref()
                    .is_some_and(|underground| underground.max_distance == 0)
                {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::Pumpjack => {
                let Some(pumpjack) = prototype.pumpjack.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if pumpjack.pumping_speed_per_second_milliunits == 0
                    || !item_exists(catalog, pumpjack.resource_item)
                    || !fluid_exists(catalog, pumpjack.output_fluid)
                    || prototype.fluid_boxes.len() != 1
                {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::TransportBelt => {
                let Some(transport_belt) = prototype.transport_belt.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if transport_belt.speed_subtiles_per_tick == 0 {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::Splitter => {
                let Some(splitter) = prototype.splitter.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if splitter.speed_subtiles_per_tick == 0 {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::Wall => {
                if prototype.max_health.is_none() {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::GunTurret => {
                let Some(gun_turret) = prototype.gun_turret.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if gun_turret.range_tiles == 0
                    || gun_turret.cooldown_ticks == 0
                    || prototype.max_health.is_none()
                {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::LaserTurret => {
                let Some(laser_turret) = prototype.laser_turret.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if laser_turret.range_tiles == 0
                    || laser_turret.damage == 0
                    || laser_turret.cooldown_ticks == 0
                    || prototype.max_health.is_none()
                    || prototype.electric_energy_source.is_none()
                    || prototype
                        .electric_energy_source
                        .as_ref()
                        .is_some_and(|source| source.drain_watts == 0)
                {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::EnemySpawner => {
                let Some(spawner) = prototype.enemy_spawner.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if spawner.max_alive_units == 0
                    || spawner.free_spawn_interval_ticks == 0
                    || spawner.unit.max_health == 0
                    || spawner.unit.attack_cooldown_ticks == 0
                    || spawner.unit.speed_fixed_per_tick == 0
                    || prototype.max_health.is_none()
                {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            EntityKind::Inserter => {
                let Some(inserter) = prototype.inserter.as_ref() else {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                };
                if inserter.pickup_ticks == 0
                    || inserter.drop_ticks == 0
                    || (inserter.pickup_offset.x == 0
                        && inserter.pickup_offset.y == 0
                        && inserter.drop_offset.x == 0
                        && inserter.drop_offset.y == 0)
                {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
                if prototype.burner.is_some() == prototype.electric_energy_source.is_some() {
                    return Err(SimValidationError::InvalidCatalogEntityPrototype {
                        prototype_id: prototype.id,
                    });
                }
            }
            _ => {}
        }

        if let Some(electric_energy_source) = prototype.electric_energy_source.as_ref()
            && electric_energy_source.energy_usage_watts == 0
        {
            return Err(SimValidationError::InvalidCatalogEntityPrototype {
                prototype_id: prototype.id,
            });
        }
    }

    Ok(())
}
