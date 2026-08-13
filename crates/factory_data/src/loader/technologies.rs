use std::collections::HashMap;

use crate::error::PrototypeLoadError;
use crate::ids::{ItemId, RecipeId, TechnologyId};
use crate::model::{
    ItemAmount, TechnologyCostCurve, TechnologyEffect, TechnologyLevelModel, TechnologyPrototype,
};
use crate::raw::{
    RawTechnologyCostCurve, RawTechnologyEffect, RawTechnologyLevelModel, RawTechnologyPrototype,
};

pub(super) fn load_technologies(
    technologies: Vec<RawTechnologyPrototype>,
    item_ids_by_name: &HashMap<String, ItemId>,
    recipe_ids_by_name: &HashMap<String, RecipeId>,
) -> Result<Vec<TechnologyPrototype>, PrototypeLoadError> {
    let technology_ids_by_name = technologies
        .iter()
        .map(|technology| (technology.name.clone(), TechnologyId::new(technology.id)))
        .collect::<HashMap<_, _>>();

    technologies
        .into_iter()
        .map(|technology| {
            if technology.required_units == 0 {
                return Err(PrototypeLoadError::InvalidTechnologyRequiredUnits {
                    technology: technology.name,
                });
            }
            if technology.research_time_ticks == 0 {
                return Err(PrototypeLoadError::InvalidTechnologyResearchTime {
                    technology: technology.name,
                });
            }

            let level_model = resolve_level_model(&technology)?;

            let id = TechnologyId::new(technology.id);
            let prerequisites =
                resolve_technology_prerequisites(&technology, id, &technology_ids_by_name)?;
            let science_packs = resolve_technology_science_packs(&technology, item_ids_by_name)?;
            let effects = resolve_technology_effects(&technology, recipe_ids_by_name)?;

            Ok(TechnologyPrototype {
                id,
                name: technology.name,
                prerequisites,
                science_packs,
                required_units: technology.required_units,
                level_model,
                research_time_ticks: technology.research_time_ticks,
                effects,
            })
        })
        .collect()
}

fn resolve_technology_prerequisites(
    technology: &RawTechnologyPrototype,
    technology_id: TechnologyId,
    technology_ids_by_name: &HashMap<String, TechnologyId>,
) -> Result<Vec<TechnologyId>, PrototypeLoadError> {
    technology
        .prerequisites
        .iter()
        .map(|prerequisite| {
            let prerequisite_id = *technology_ids_by_name.get(prerequisite).ok_or_else(|| {
                PrototypeLoadError::MissingTechnologyPrerequisite {
                    technology: technology.name.clone(),
                    prerequisite: prerequisite.clone(),
                }
            })?;
            if prerequisite_id == technology_id {
                return Err(PrototypeLoadError::TechnologySelfPrerequisite {
                    technology: technology.name.clone(),
                });
            }
            Ok(prerequisite_id)
        })
        .collect()
}

fn resolve_technology_science_packs(
    technology: &RawTechnologyPrototype,
    item_ids_by_name: &HashMap<String, ItemId>,
) -> Result<Vec<ItemAmount>, PrototypeLoadError> {
    technology
        .science_packs
        .iter()
        .map(|amount| {
            let item = *item_ids_by_name.get(&amount.item).ok_or_else(|| {
                PrototypeLoadError::MissingTechnologySciencePackItem {
                    technology: technology.name.clone(),
                    item: amount.item.clone(),
                }
            })?;
            Ok(ItemAmount {
                item,
                amount: amount.amount,
            })
        })
        .collect()
}

fn resolve_technology_effects(
    technology: &RawTechnologyPrototype,
    recipe_ids_by_name: &HashMap<String, RecipeId>,
) -> Result<Vec<TechnologyEffect>, PrototypeLoadError> {
    technology
        .effects
        .iter()
        .map(|effect| match effect {
            RawTechnologyEffect::UnlockRecipe(recipe) => {
                let recipe_id = *recipe_ids_by_name.get(recipe).ok_or_else(|| {
                    PrototypeLoadError::MissingTechnologyUnlockRecipe {
                        technology: technology.name.clone(),
                        recipe: recipe.clone(),
                    }
                })?;
                Ok(TechnologyEffect::UnlockRecipe(recipe_id))
            }
            RawTechnologyEffect::MiningDrillProductivity { bonus_permyriad } => {
                if *bonus_permyriad == 0 {
                    return Err(PrototypeLoadError::InvalidTechnologyEffect {
                        technology: technology.name.clone(),
                    });
                }
                Ok(TechnologyEffect::MiningDrillProductivity {
                    bonus_permyriad: *bonus_permyriad,
                })
            }
        })
        .collect()
}

fn resolve_level_model(
    technology: &RawTechnologyPrototype,
) -> Result<TechnologyLevelModel, PrototypeLoadError> {
    let model = match technology.level_model {
        RawTechnologyLevelModel::Finite => TechnologyLevelModel::Finite,
        RawTechnologyLevelModel::Repeatable {
            cost_curve:
                RawTechnologyCostCurve::Linear {
                    additional_units_per_level,
                },
        } => TechnologyLevelModel::Repeatable {
            cost_curve: TechnologyCostCurve::Linear {
                additional_units_per_level,
            },
        },
    };
    let max_cost_is_valid = match model {
        TechnologyLevelModel::Finite => true,
        TechnologyLevelModel::Repeatable {
            cost_curve:
                TechnologyCostCurve::Linear {
                    additional_units_per_level,
                },
        } => additional_units_per_level
            .checked_mul(u64::from(u32::MAX - 1))
            .and_then(|additional| technology.required_units.checked_add(additional))
            .is_some(),
    };
    if !max_cost_is_valid {
        return Err(PrototypeLoadError::InvalidTechnologyCostCurve {
            technology: technology.name.clone(),
        });
    }
    Ok(model)
}
