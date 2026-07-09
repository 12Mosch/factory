use super::super::*;
use super::ids::*;

pub(super) fn validate_catalog(catalog: &PrototypeCatalog) -> Result<(), SimValidationError> {
    for (index, item) in catalog.items.iter().enumerate() {
        if item.id.index() != index {
            return Err(SimValidationError::UnknownItem(item.id));
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
            EntityKind::Pipe | EntityKind::StorageTank => {
                if prototype.fluid_boxes.len() != 1 {
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
