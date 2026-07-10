use super::*;

mod catalog;
mod construction;
mod crafting;
mod entities;
mod fluids;
mod ids;
pub(in crate::simulation) mod inventory;
pub(in crate::simulation) mod machines;
mod research;
mod world;

use self::catalog::validate_catalog;
use self::construction::validate_construction_state;
use self::crafting::validate_crafting_queue;
use self::entities::{
    validate_enemies, validate_entity_occupancy, validate_entity_state_ownership_and_kind,
};
use self::fluids::{validate_fluid_box_states, validate_fluid_network_snapshots};
use self::inventory::validate_inventory;
use self::research::validate_research_state;
use self::world::{
    validate_chart_state, validate_fluid_statistics, validate_item_statistics,
    validate_placed_entities, validate_power_statistics, validate_world_resources,
};

pub fn validate_simulation(sim: &Simulation) -> Result<(), SimValidationError> {
    validate_catalog(&sim.world.prototypes)?;
    validate_world_resources(&sim.world)?;
    validate_chart_state(sim)?;
    validate_item_statistics(sim)?;
    validate_fluid_statistics(sim)?;
    validate_power_statistics(sim)?;
    validate_placed_entities(sim)?;
    validate_entity_occupancy(&sim.entities)?;
    validate_entity_state_ownership_and_kind(sim)?;
    validate_construction_state(sim)?;
    validate_fluid_box_states(sim)?;
    validate_fluid_network_snapshots(sim)?;

    validate_inventory(&sim.world.prototypes, &sim.player_inventory)?;
    validate_crafting_queue(sim)?;
    validate_research_state(sim)?;
    validate_enemies(sim)?;

    validate_entity_states(sim)?;

    Ok(())
}

macro_rules! define_validate_entity_states {
    ($($field:ident : $ty:ty => $kind:tt),* $(,)?) => {
        /// Validates every per-kind state entry via `EntityStateBehavior`.
        fn validate_entity_states(sim: &Simulation) -> Result<(), SimValidationError> {
            $(
                for (entity_id, state) in &sim.entities.$field {
                    EntityStateBehavior::validate_state(state, sim, *entity_id)?;
                }
            )*
            Ok(())
        }
    };
}
for_each_entity_state_map!(define_validate_entity_states);

impl Simulation {
    pub fn validate(&self) -> Result<(), SimValidationError> {
        validate_simulation(self)
    }

    pub fn validate_item_conservation(&self) -> bool {
        self.validate().is_ok()
    }

    pub fn validate_state(&self) -> Result<(), SimulationValidationError> {
        self.validate()
    }
}
