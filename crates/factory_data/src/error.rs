use std::fmt;

#[derive(Debug)]
pub enum PrototypeLoadError {
    Io(std::io::Error),
    Ron(ron::error::SpannedError),
    DuplicateId {
        group: &'static str,
        id: u16,
    },
    DuplicateName {
        group: &'static str,
        name: String,
    },
    NonContiguousIds {
        group: &'static str,
        expected: u16,
        actual: u16,
    },
    MissingItemReference {
        recipe: String,
        item: String,
    },
    MissingTechnologyPrerequisite {
        technology: String,
        prerequisite: String,
    },
    MissingTechnologySciencePackItem {
        technology: String,
        item: String,
    },
    MissingTechnologyUnlockRecipe {
        technology: String,
        recipe: String,
    },
    InvalidTechnologyRequiredUnits {
        technology: String,
    },
    InvalidTechnologyResearchTime {
        technology: String,
    },
    TechnologySelfPrerequisite {
        technology: String,
    },
    TechnologyPrerequisiteCycle {
        technology: String,
    },
    InvalidCollisionLayer {
        owner: String,
        layer: String,
    },
}

impl fmt::Display for PrototypeLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "failed to read prototype data: {error}"),
            Self::Ron(error) => write!(formatter, "failed to parse prototype data: {error}"),
            Self::DuplicateId { group, id } => {
                write!(formatter, "duplicate {group} prototype id {id}")
            }
            Self::DuplicateName { group, name } => {
                write!(formatter, "duplicate {group} prototype name {name:?}")
            }
            Self::NonContiguousIds {
                group,
                expected,
                actual,
            } => write!(
                formatter,
                "{group} prototype ids must be contiguous from 0: expected {expected}, got {actual}"
            ),
            Self::MissingItemReference { recipe, item } => {
                write!(
                    formatter,
                    "recipe {recipe:?} references missing item {item:?}"
                )
            }
            Self::MissingTechnologyPrerequisite {
                technology,
                prerequisite,
            } => write!(
                formatter,
                "technology {technology:?} references missing prerequisite {prerequisite:?}"
            ),
            Self::MissingTechnologySciencePackItem { technology, item } => write!(
                formatter,
                "technology {technology:?} references missing science pack item {item:?}"
            ),
            Self::MissingTechnologyUnlockRecipe { technology, recipe } => write!(
                formatter,
                "technology {technology:?} references missing unlock recipe {recipe:?}"
            ),
            Self::InvalidTechnologyRequiredUnits { technology } => write!(
                formatter,
                "technology {technology:?} must require at least one research unit"
            ),
            Self::InvalidTechnologyResearchTime { technology } => write!(
                formatter,
                "technology {technology:?} must require at least one research tick per unit"
            ),
            Self::TechnologySelfPrerequisite { technology } => write!(
                formatter,
                "technology {technology:?} cannot list itself as a prerequisite"
            ),
            Self::TechnologyPrerequisiteCycle { technology } => write!(
                formatter,
                "technology prerequisite graph contains a cycle at {technology:?}"
            ),
            Self::InvalidCollisionLayer { owner, layer } => {
                write!(
                    formatter,
                    "prototype {owner:?} uses invalid collision layer {layer:?}"
                )
            }
        }
    }
}

impl std::error::Error for PrototypeLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Ron(error) => Some(error),
            _ => None,
        }
    }
}
