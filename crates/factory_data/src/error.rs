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
    MissingEntityBuildItem {
        entity: String,
        item: String,
    },
    MissingFluidReference {
        owner: String,
        fluid: String,
    },
    InvalidRecipeFluidAmount {
        recipe: String,
        fluid: String,
    },
    MissingPumpjackResourceItem {
        entity: String,
        item: String,
    },
    InvalidFluidBox {
        entity: String,
        box_index: usize,
    },
    InvalidFluidConnection {
        entity: String,
        box_index: usize,
        connection_index: usize,
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
    UnsupportedWorldGenerationVersion {
        found: u32,
        supported: u32,
    },
    MissingWorldGenerationTile {
        tile: String,
    },
    MissingWorldGenerationResourceItem {
        item: String,
    },
    MissingWorldGenerationSpawnerEntity {
        entity: String,
    },
    DuplicateWorldGenerationResource {
        item: String,
    },
    InvalidWorldGenerationConfig {
        detail: &'static str,
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
            Self::MissingEntityBuildItem { entity, item } => {
                write!(
                    formatter,
                    "entity {entity:?} references missing build item {item:?}"
                )
            }
            Self::MissingFluidReference { owner, fluid } => {
                write!(
                    formatter,
                    "prototype {owner:?} references missing fluid {fluid:?}"
                )
            }
            Self::InvalidRecipeFluidAmount { recipe, fluid } => {
                write!(
                    formatter,
                    "recipe {recipe:?} requires a non-zero amount of fluid {fluid:?}"
                )
            }
            Self::MissingPumpjackResourceItem { entity, item } => {
                write!(
                    formatter,
                    "pumpjack {entity:?} references missing resource item {item:?}"
                )
            }
            Self::InvalidFluidBox { entity, box_index } => {
                write!(
                    formatter,
                    "entity {entity:?} has invalid fluid box {box_index}"
                )
            }
            Self::InvalidFluidConnection {
                entity,
                box_index,
                connection_index,
            } => write!(
                formatter,
                "entity {entity:?} has invalid fluid connection {connection_index} in fluid box {box_index}"
            ),
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
            Self::UnsupportedWorldGenerationVersion { found, supported } => write!(
                formatter,
                "world generation config version {found} is not supported (expected {supported})"
            ),
            Self::MissingWorldGenerationTile { tile } => {
                write!(
                    formatter,
                    "world generation config references missing tile {tile:?}"
                )
            }
            Self::MissingWorldGenerationResourceItem { item } => write!(
                formatter,
                "world generation config references missing resource item {item:?}"
            ),
            Self::MissingWorldGenerationSpawnerEntity { entity } => write!(
                formatter,
                "world generation config references missing spawner entity {entity:?}"
            ),
            Self::DuplicateWorldGenerationResource { item } => write!(
                formatter,
                "world generation config defines resource item {item:?} more than once"
            ),
            Self::InvalidWorldGenerationConfig { detail } => {
                write!(formatter, "invalid world generation config: {detail}")
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
