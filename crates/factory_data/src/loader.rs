mod catalog;
mod circuits;
mod entities;
mod fluids;
mod items;
mod recipes;
mod technologies;
mod tiles;
mod world_generation;

use std::collections::HashMap;
use std::path::Path;

use catalog::ValidatedRawCatalog;
use circuits::{load_virtual_signals, validate_circuit_content};
use entities::load_entities;
use fluids::load_fluids;
use items::load_items;
use recipes::load_recipes;
use technologies::load_technologies;
use tiles::load_tiles;
use world_generation::load_world_generation;

use crate::catalog::PrototypeCatalog;
use crate::error::PrototypeLoadError;
use crate::raw::RawPrototypeCatalog;
use crate::validation::validate_technology_prerequisite_graph;

#[cfg(test)]
mod tests;

impl PrototypeCatalog {
    pub fn load_base() -> Result<Self, PrototypeLoadError> {
        Self::from_ron_str(include_str!("../data/base.ron"))
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, PrototypeLoadError> {
        let data = std::fs::read_to_string(path).map_err(PrototypeLoadError::Io)?;
        Self::from_ron_str(&data)
    }

    pub fn from_ron_str(data: &str) -> Result<Self, PrototypeLoadError> {
        let raw: RawPrototypeCatalog = ron::from_str(data).map_err(PrototypeLoadError::Ron)?;
        let raw = ValidatedRawCatalog::from_raw(raw)?;

        // Tiles load first because items may pave a tile by name.
        let tiles = load_tiles(raw.tiles)?;
        let tile_ids_by_name = tiles
            .iter()
            .map(|tile| (tile.name.clone(), tile.id))
            .collect::<HashMap<_, _>>();
        let (items, item_ids_by_name) = load_items(raw.items, &tile_ids_by_name)?;
        let (fluids, fluid_ids_by_name) = load_fluids(raw.fluids);
        let (recipes, recipe_ids_by_name) =
            load_recipes(raw.recipes, &item_ids_by_name, &fluid_ids_by_name)?;
        let entities = load_entities(raw.entities, &item_ids_by_name, &fluid_ids_by_name)?;
        let technologies =
            load_technologies(raw.technologies, &item_ids_by_name, &recipe_ids_by_name)?;
        validate_technology_prerequisite_graph(&technologies)?;
        let virtual_signals = load_virtual_signals(raw.virtual_signals);
        validate_circuit_content(&entities, &virtual_signals)?;
        let world_generation =
            load_world_generation(raw.world_generation, &item_ids_by_name, &tiles, &entities)?;

        Ok(Self {
            items,
            fluids,
            recipes,
            entities,
            tiles,
            technologies,
            virtual_signals,
            world_generation,
            enemy_gameplay: raw.enemy_gameplay,
            day_night_cycle: raw.day_night_cycle,
        })
    }
}
