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
    MissingRocketSiloLaunchItem {
        entity: String,
        item: String,
        role: &'static str,
    },
    InvalidBuildingMenuMetadata {
        entity: String,
        detail: &'static str,
    },
    InvalidEntityMetadata {
        entity: String,
        detail: &'static str,
    },
    InvalidAmmoMetadata {
        item: String,
        detail: &'static str,
    },
    InvalidArmorMetadata {
        item: String,
        detail: &'static str,
    },
    InvalidEquipmentMetadata {
        item: String,
        detail: &'static str,
    },
    InvalidModuleMetadata {
        item: String,
        detail: &'static str,
    },
    InvalidRobotMetadata {
        item: String,
        detail: &'static str,
    },
    InvalidTileMetadata {
        tile: String,
        detail: &'static str,
    },
    MissingItemPlacementTile {
        item: String,
        tile: String,
    },
    InvalidTilePlacementMetadata {
        item: String,
        detail: &'static str,
    },
    InvalidModuleSlotMetadata {
        entity: String,
        detail: &'static str,
    },
    InvalidBeaconMetadata {
        entity: String,
        detail: &'static str,
    },
    InvalidLaserTurretMetadata {
        entity: String,
        detail: &'static str,
    },
    MissingFluidReference {
        owner: String,
        fluid: String,
    },
    InvalidRecipeFluidAmount {
        recipe: String,
        fluid: String,
    },
    InvalidRocketBuildingRecipe {
        recipe: String,
        detail: &'static str,
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
    MissingEnemyGameplayConfig,
    InvalidEnemyGameplayConfig {
        detail: &'static str,
    },
    InvalidDayNightCycleConfig,
    InvalidMachineEnergySource {
        entity: String,
        detail: &'static str,
    },
    InvalidSolarStorageMetadata {
        entity: String,
        detail: &'static str,
    },
    InvalidRadarMetadata {
        entity: String,
        detail: &'static str,
    },
    InvalidCircuitMetadata {
        entity: String,
        detail: &'static str,
    },
    InvalidHeatMetadata {
        entity: String,
        detail: &'static str,
    },
    InvalidRoboportMetadata {
        entity: String,
        detail: &'static str,
    },
    InvalidLogisticChestMetadata {
        entity: String,
        detail: &'static str,
    },
    InvalidRocketSiloMetadata {
        entity: String,
        detail: &'static str,
    },
    InvalidHeatConnection {
        entity: String,
        connection_index: usize,
    },
    InvalidRailMetadata {
        entity: String,
        detail: &'static str,
    },
    InvalidRollingStockMetadata {
        entity: String,
        detail: &'static str,
    },
    MissingBurntResultItem {
        item: String,
        burnt_result: String,
    },
    InvalidItemFuelMetadata {
        item: String,
        detail: &'static str,
    },
    /// Content data does not define a prototype the engine hard-codes a
    /// dependency on, so the catalog cannot drive a simulation.
    MissingRequiredPrototype(crate::base_ids::MissingBasePrototype),
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
            Self::MissingRocketSiloLaunchItem { entity, item, role } => write!(
                formatter,
                "rocket silo {entity:?} references missing {role} item {item:?}"
            ),
            Self::InvalidBuildingMenuMetadata { entity, detail } => write!(
                formatter,
                "entity {entity:?} has invalid building menu metadata: {detail}"
            ),
            Self::InvalidEntityMetadata { entity, detail } => {
                write!(
                    formatter,
                    "entity {entity:?} has invalid metadata: {detail}"
                )
            }
            Self::InvalidAmmoMetadata { item, detail } => {
                write!(
                    formatter,
                    "item {item:?} has invalid ammunition metadata: {detail}"
                )
            }
            Self::InvalidArmorMetadata { item, detail } => {
                write!(
                    formatter,
                    "item {item:?} has invalid armor metadata: {detail}"
                )
            }
            Self::InvalidEquipmentMetadata { item, detail } => {
                write!(
                    formatter,
                    "item {item:?} has invalid equipment metadata: {detail}"
                )
            }
            Self::InvalidModuleMetadata { item, detail } => write!(
                formatter,
                "item {item:?} has invalid module metadata: {detail}"
            ),
            Self::InvalidRobotMetadata { item, detail } => write!(
                formatter,
                "item {item:?} has invalid robot metadata: {detail}"
            ),
            Self::InvalidTileMetadata { tile, detail } => {
                write!(formatter, "tile {tile:?} has invalid metadata: {detail}")
            }
            Self::MissingItemPlacementTile { item, tile } => {
                write!(formatter, "item {item:?} paves missing tile {tile:?}")
            }
            Self::InvalidTilePlacementMetadata { item, detail } => write!(
                formatter,
                "item {item:?} has invalid tile placement metadata: {detail}"
            ),
            Self::InvalidModuleSlotMetadata { entity, detail } => write!(
                formatter,
                "entity {entity:?} has invalid module-slot metadata: {detail}"
            ),
            Self::InvalidBeaconMetadata { entity, detail } => write!(
                formatter,
                "entity {entity:?} has invalid beacon metadata: {detail}"
            ),
            Self::InvalidLaserTurretMetadata { entity, detail } => write!(
                formatter,
                "entity {entity:?} has invalid laser turret metadata: {detail}"
            ),
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
            Self::InvalidRocketBuildingRecipe { recipe, detail } => {
                write!(
                    formatter,
                    "rocket-building recipe {recipe:?} is not a recipe that a silo can build: {detail}"
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
            Self::MissingEnemyGameplayConfig => write!(
                formatter,
                "catalog defines enemy spawners or enemy base generation but no enemy_gameplay section"
            ),
            Self::InvalidEnemyGameplayConfig { detail } => {
                write!(formatter, "invalid enemy gameplay config: {detail}")
            }
            Self::InvalidDayNightCycleConfig => write!(
                formatter,
                "invalid day/night cycle config: cycle and ramp lengths must be non-zero and four ramp lengths must fit strictly within one cycle"
            ),
            Self::InvalidMachineEnergySource { entity, detail } => {
                write!(
                    formatter,
                    "entity {entity:?} has an invalid energy source: {detail}"
                )
            }
            Self::InvalidSolarStorageMetadata { entity, detail } => {
                write!(
                    formatter,
                    "entity {entity:?} has invalid solar/storage metadata: {detail}"
                )
            }
            Self::InvalidRadarMetadata { entity, detail } => {
                write!(
                    formatter,
                    "entity {entity:?} has invalid radar metadata: {detail}"
                )
            }
            Self::InvalidCircuitMetadata { entity, detail } => {
                write!(
                    formatter,
                    "entity {entity:?} has invalid circuit metadata: {detail}"
                )
            }
            Self::InvalidHeatMetadata { entity, detail } => {
                write!(
                    formatter,
                    "entity {entity:?} has invalid heat metadata: {detail}"
                )
            }
            Self::InvalidRoboportMetadata { entity, detail } => {
                write!(
                    formatter,
                    "entity {entity:?} has invalid roboport metadata: {detail}"
                )
            }
            Self::InvalidLogisticChestMetadata { entity, detail } => {
                write!(
                    formatter,
                    "entity {entity:?} has invalid logistic chest metadata: {detail}"
                )
            }
            Self::InvalidRocketSiloMetadata { entity, detail } => {
                write!(
                    formatter,
                    "entity {entity:?} has invalid rocket silo metadata: {detail}"
                )
            }
            Self::InvalidHeatConnection {
                entity,
                connection_index,
            } => write!(
                formatter,
                "entity {entity:?} has invalid heat connection {connection_index}"
            ),
            Self::InvalidRailMetadata { entity, detail } => {
                write!(
                    formatter,
                    "entity {entity:?} has invalid rail metadata: {detail}"
                )
            }
            Self::InvalidRollingStockMetadata { entity, detail } => {
                write!(
                    formatter,
                    "entity {entity:?} has invalid rolling stock metadata: {detail}"
                )
            }
            Self::MissingBurntResultItem { item, burnt_result } => write!(
                formatter,
                "item {item:?} references unknown burnt result item {burnt_result:?}"
            ),
            Self::InvalidItemFuelMetadata { item, detail } => {
                write!(
                    formatter,
                    "item {item:?} has invalid fuel metadata: {detail}"
                )
            }
            Self::MissingRequiredPrototype(missing) => write!(formatter, "{missing}"),
        }
    }
}

impl std::error::Error for PrototypeLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Ron(error) => Some(error),
            Self::MissingRequiredPrototype(error) => Some(error),
            _ => None,
        }
    }
}
