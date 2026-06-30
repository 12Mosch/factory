use super::*;

mod catalog;
mod crafting;
mod entities;
mod fluids;
mod ids;
mod inventory;
mod machines;
mod research;
mod world;

use self::catalog::validate_catalog;
use self::crafting::validate_crafting_queue;
use self::entities::{validate_entity_occupancy, validate_entity_state_ownership_and_kind};
use self::fluids::{validate_fluid_box_states, validate_fluid_network_snapshots};
use self::inventory::validate_inventory;
use self::machines::{
    validate_assembler, validate_belt_segment, validate_boiler, validate_burner_mining_drill,
    validate_furnace, validate_inserter, validate_lab, validate_splitter_state,
};
use self::research::validate_research_state;
use self::world::{
    validate_chart_state, validate_item_statistics, validate_placed_entities,
    validate_world_resources,
};

pub fn validate_simulation(sim: &Simulation) -> Result<(), SimValidationError> {
    validate_catalog(&sim.world.prototypes)?;
    validate_world_resources(&sim.world)?;
    validate_chart_state(sim)?;
    validate_item_statistics(sim)?;
    validate_placed_entities(sim)?;
    validate_entity_occupancy(&sim.entities)?;
    validate_entity_state_ownership_and_kind(sim)?;
    validate_fluid_box_states(sim)?;
    validate_fluid_network_snapshots(sim)?;

    validate_inventory(&sim.world.prototypes, &sim.player_inventory)?;
    validate_crafting_queue(sim)?;
    validate_research_state(sim)?;

    for inventory in sim.entities.entity_inventories.values() {
        validate_inventory(&sim.world.prototypes, inventory)?;
    }
    for (entity_id, state) in &sim.entities.burner_mining_drills {
        validate_burner_mining_drill(sim, *entity_id, state)?;
    }
    for (entity_id, state) in &sim.entities.furnaces {
        validate_furnace(sim, *entity_id, state)?;
    }
    for (entity_id, state) in &sim.entities.assembling_machines {
        validate_assembler(sim, *entity_id, state)?;
    }
    for (entity_id, state) in &sim.entities.labs {
        validate_lab(sim, *entity_id, state)?;
    }
    for (entity_id, state) in &sim.entities.electric_consumers {
        if state.work_remainder_permyriad >= POWER_SATISFACTION_FULL_PERMYRIAD {
            return Err(SimValidationError::InvalidEntityState {
                entity_id: *entity_id,
            });
        }
    }
    for (entity_id, state) in &sim.entities.boilers {
        validate_boiler(sim, *entity_id, state)?;
    }
    for (entity_id, segment) in &sim.entities.transport_belts {
        validate_belt_segment(sim, *entity_id, segment)?;
    }
    for (entity_id, state) in &sim.entities.splitters {
        validate_splitter_state(sim, *entity_id, state)?;
    }
    for (entity_id, state) in &sim.entities.inserters {
        validate_inserter(sim, *entity_id, state)?;
    }

    Ok(())
}

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
