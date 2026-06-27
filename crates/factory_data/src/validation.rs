use std::collections::{HashMap, HashSet};

use crate::error::PrototypeLoadError;
use crate::ids::ItemId;
use crate::model::{CollisionLayer, CollisionMask, ItemAmount, TechnologyPrototype};
use crate::raw::{RawCollisionMask, RawItemAmount};

pub(crate) trait RawPrototype {
    fn id(&self) -> u16;
    fn name(&self) -> &str;
}

pub(crate) fn validate_group<T>(
    prototypes: &mut [T],
    group: &'static str,
) -> Result<(), PrototypeLoadError>
where
    T: RawPrototype,
{
    {
        let mut seen_ids = HashSet::new();
        let mut seen_names = HashSet::new();

        for prototype in prototypes.iter() {
            if !seen_ids.insert(prototype.id()) {
                return Err(PrototypeLoadError::DuplicateId {
                    group,
                    id: prototype.id(),
                });
            }

            if !seen_names.insert(prototype.name()) {
                return Err(PrototypeLoadError::DuplicateName {
                    group,
                    name: prototype.name().to_string(),
                });
            }
        }
    }

    prototypes.sort_by_key(RawPrototype::id);

    for (expected, prototype) in prototypes.iter().enumerate() {
        let expected = u16::try_from(expected).expect("prototype group exceeds u16 id range");
        let actual = prototype.id();
        if actual != expected {
            return Err(PrototypeLoadError::NonContiguousIds {
                group,
                expected,
                actual,
            });
        }
    }

    Ok(())
}

pub(crate) fn resolve_item_amounts(
    recipe: &str,
    amounts: Vec<RawItemAmount>,
    item_ids_by_name: &HashMap<String, ItemId>,
) -> Result<Vec<ItemAmount>, PrototypeLoadError> {
    amounts
        .into_iter()
        .map(|amount| {
            let item = *item_ids_by_name.get(&amount.item).ok_or_else(|| {
                PrototypeLoadError::MissingItemReference {
                    recipe: recipe.to_string(),
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

pub(crate) fn resolve_collision_mask(
    owner: String,
    raw: RawCollisionMask,
) -> Result<CollisionMask, PrototypeLoadError> {
    let layers = raw
        .layers
        .into_iter()
        .map(|layer| match layer.as_str() {
            "ground" => Ok(CollisionLayer::Ground),
            "water" => Ok(CollisionLayer::Water),
            "resource" => Ok(CollisionLayer::Resource),
            "building" => Ok(CollisionLayer::Building),
            "transport" => Ok(CollisionLayer::Transport),
            _ => Err(PrototypeLoadError::InvalidCollisionLayer {
                owner: owner.clone(),
                layer,
            }),
        })
        .collect::<Result<_, _>>()?;

    Ok(CollisionMask { layers })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TechnologyVisitState {
    Visiting,
    Visited,
}

pub(crate) fn validate_technology_prerequisite_graph(
    technologies: &[TechnologyPrototype],
) -> Result<(), PrototypeLoadError> {
    let mut states = vec![None; technologies.len()];

    for technology in technologies {
        visit_technology_prerequisites(technology.id.index(), technologies, &mut states)?;
    }

    Ok(())
}

fn visit_technology_prerequisites(
    index: usize,
    technologies: &[TechnologyPrototype],
    states: &mut [Option<TechnologyVisitState>],
) -> Result<(), PrototypeLoadError> {
    match states[index] {
        Some(TechnologyVisitState::Visited) => return Ok(()),
        Some(TechnologyVisitState::Visiting) => {
            return Err(PrototypeLoadError::TechnologyPrerequisiteCycle {
                technology: technologies[index].name.clone(),
            });
        }
        None => {}
    }

    states[index] = Some(TechnologyVisitState::Visiting);

    for prerequisite in &technologies[index].prerequisites {
        let prerequisite_index = prerequisite.index();
        if prerequisite_index >= technologies.len()
            || technologies[prerequisite_index].id != *prerequisite
        {
            return Err(PrototypeLoadError::MissingTechnologyPrerequisite {
                technology: technologies[index].name.clone(),
                prerequisite: format!("<id:{}>", prerequisite.raw()),
            });
        }
        visit_technology_prerequisites(prerequisite_index, technologies, states)?;
    }

    states[index] = Some(TechnologyVisitState::Visited);
    Ok(())
}
