use crate::error::PrototypeLoadError;
use crate::model::{DayNightCycleConfig, EnemyGameplayConfig};
use crate::raw::{
    RawEntityPrototype, RawFluidPrototype, RawItemPrototype, RawPrototypeCatalog,
    RawRecipePrototype, RawTechnologyPrototype, RawTilePrototype, RawVirtualSignalPrototype,
    RawWorldGenerationConfig,
};
use crate::validation::validate_group;

pub(super) struct ValidatedRawCatalog {
    pub(super) items: Vec<RawItemPrototype>,
    pub(super) fluids: Vec<RawFluidPrototype>,
    pub(super) recipes: Vec<RawRecipePrototype>,
    pub(super) entities: Vec<RawEntityPrototype>,
    pub(super) tiles: Vec<RawTilePrototype>,
    pub(super) technologies: Vec<RawTechnologyPrototype>,
    pub(super) virtual_signals: Vec<RawVirtualSignalPrototype>,
    pub(super) world_generation: Option<RawWorldGenerationConfig>,
    pub(super) enemy_gameplay: Option<EnemyGameplayConfig>,
    pub(super) day_night_cycle: Option<DayNightCycleConfig>,
}

impl ValidatedRawCatalog {
    pub(super) fn from_raw(raw: RawPrototypeCatalog) -> Result<Self, PrototypeLoadError> {
        match raw.enemy_gameplay.as_ref() {
            Some(config) => validate_enemy_gameplay(config)?,
            // A catalog with enemy content but no gameplay section would
            // silently run without any enemy simulation; fail loudly instead.
            None => {
                let has_enemy_content = raw
                    .entities
                    .iter()
                    .any(|entity| entity.enemy_spawner.is_some())
                    || raw
                        .world_generation
                        .as_ref()
                        .is_some_and(|config| config.enemy_bases.is_some());
                if has_enemy_content {
                    return Err(PrototypeLoadError::MissingEnemyGameplayConfig);
                }
            }
        }
        if raw.day_night_cycle.is_some_and(|config| !config.is_valid()) {
            return Err(PrototypeLoadError::InvalidDayNightCycleConfig);
        }
        let mut items = raw.items;
        validate_group(&mut items, "items")?;

        let mut fluids = raw.fluids;
        validate_group(&mut fluids, "fluids")?;

        let mut recipes = raw.recipes;
        validate_group(&mut recipes, "recipes")?;

        let mut entities = raw.entities;
        validate_group(&mut entities, "entities")?;

        let mut tiles = raw.tiles;
        validate_group(&mut tiles, "tiles")?;

        let mut technologies = raw.technologies;
        validate_group(&mut technologies, "technologies")?;

        let mut virtual_signals = raw.virtual_signals;
        validate_group(&mut virtual_signals, "virtual_signals")?;

        Ok(Self {
            items,
            fluids,
            recipes,
            entities,
            tiles,
            technologies,
            virtual_signals,
            world_generation: raw.world_generation,
            enemy_gameplay: raw.enemy_gameplay,
            day_night_cycle: raw.day_night_cycle,
        })
    }
}

fn validate_enemy_gameplay(config: &EnemyGameplayConfig) -> Result<(), PrototypeLoadError> {
    let valid = config.generated_colony_min_spawners > 0
        && config.generated_colony_min_spawners <= config.generated_colony_max_spawners
        && config.generated_colony_max_spawners <= config.max_spawners_per_colony
        && config.colony_spawner_radius_tiles > 0
        && config.outpost_growth_interval_ticks > 0
        && config.raid_staging_timeout_ticks > 0
        && config.raid_cooldown_ticks > 0
        && config.expansion_minimum_age_ticks > 0
        && config.expansion_interval_ticks > 0
        && config.expansion_retry_ticks > 0
        && config.expansion_min_distance_chunks > 0
        && config.expansion_min_distance_chunks <= config.expansion_max_distance_chunks
        && config.expansion_candidate_limit > 0
        && config.expansion_colony_spacing_chunks > 0
        && config.expansion_player_spacing_tiles > 0
        && config.evolution_time_interval_ticks > 0
        && config.evolution_time_points > 0
        && config.evolution_pollution_units_per_point > 0
        && config.evolution_spawner_destroyed_points > 0
        && config.evolution_colony_destroyed_points > 0;
    if valid {
        Ok(())
    } else {
        Err(PrototypeLoadError::InvalidEnemyGameplayConfig {
            detail: "enemy gameplay intervals and ranges must be non-zero and ordered",
        })
    }
}
