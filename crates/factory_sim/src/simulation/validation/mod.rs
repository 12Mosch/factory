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
    crate::day_night::validate_day_night_cycle_state(sim)?;
    validate_world_resources(&sim.world)?;
    validate_chart_state(sim)?;
    validate_item_statistics(sim)?;
    validate_fluid_statistics(sim)?;
    validate_power_statistics(sim)?;
    validate_pollution_state(sim)?;
    validate_placed_entities(sim)?;
    validate_entity_occupancy(&sim.entities)?;
    validate_entity_state_ownership_and_kind(sim)?;
    validate_construction_state(sim)?;
    validate_fluid_box_states(sim)?;
    validate_fluid_network_snapshots(sim)?;

    validate_inventory(&sim.world.prototypes, &sim.player_inventory)?;
    if !sim.player.health.is_valid()
        || sim.player.health.maximum != PLAYER_MAX_HEALTH
        || sim.player.health.faction != Faction::Player
    {
        return Err(SimValidationError::InvalidPlayerState);
    }
    equipment_ops::validate_player_equipment(sim)?;
    validate_crafting_queue(sim)?;
    validate_research_state(sim)?;
    validate_enemies(sim)?;

    validate_entity_states(sim)?;

    Ok(())
}

fn validate_pollution_state(sim: &Simulation) -> Result<(), SimValidationError> {
    for (coord, amount) in &sim.pollution.chunks {
        if *amount > MAX_POLLUTION_PER_CHUNK_MICRO {
            return Err(SimValidationError::PollutionCapacityExceeded {
                chunk: Some(*coord),
            });
        }
    }
    if sim
        .pollution
        .checked_total_micro()
        .is_none_or(|total| total > MAX_TOTAL_POLLUTION_MICRO)
    {
        return Err(SimValidationError::PollutionCapacityExceeded { chunk: None });
    }
    for (entity_id, remainder) in &sim.pollution.machine_emission_remainders {
        if *remainder == 0
            || *remainder >= crate::pollution::POLLUTION_TICKS_PER_MINUTE
            || !sim.entities.placed_entities.contains_key(entity_id)
        {
            return Err(SimValidationError::InvalidPollutionState {
                source: PollutionRemainderSource::MachineEmission(*entity_id),
            });
        }
    }
    for (coord, remainder) in &sim.pollution.terrain_absorption_remainders {
        if *remainder == 0
            || *remainder >= crate::pollution::POLLUTION_TICKS_PER_MINUTE
            || !sim.world.chunks.contains_key(coord)
        {
            return Err(SimValidationError::InvalidPollutionState {
                source: PollutionRemainderSource::TerrainAbsorption(*coord),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod pollution_tests {
    use super::*;

    #[test]
    fn invalid_machine_pollution_remainder_reports_entity() {
        let mut sim = Simulation::new_test_world(123);
        let entity_id = EntityId::new(u64::MAX);
        sim.pollution
            .machine_emission_remainders
            .insert(entity_id, 1);

        assert_eq!(
            validate_pollution_state(&sim),
            Err(SimValidationError::InvalidPollutionState {
                source: PollutionRemainderSource::MachineEmission(entity_id),
            })
        );
    }

    #[test]
    fn invalid_terrain_pollution_remainder_reports_chunk() {
        let mut sim = Simulation::new_test_world(123);
        let coord = ChunkCoord {
            x: i32::MAX,
            y: i32::MAX,
        };
        sim.pollution.terrain_absorption_remainders.insert(coord, 1);

        assert_eq!(
            validate_pollution_state(&sim),
            Err(SimValidationError::InvalidPollutionState {
                source: PollutionRemainderSource::TerrainAbsorption(coord),
            })
        );
    }

    #[test]
    fn pollution_above_practical_chunk_limit_is_rejected() {
        let mut sim = Simulation::new_test_world(123);
        let coord = ChunkCoord { x: 0, y: 0 };
        sim.pollution
            .chunks
            .insert(coord, MAX_POLLUTION_PER_CHUNK_MICRO + 1);

        assert_eq!(
            validate_pollution_state(&sim),
            Err(SimValidationError::PollutionCapacityExceeded { chunk: Some(coord) })
        );
    }

    #[test]
    fn pollution_addition_overflow_is_exposed_by_capacity_diagnostics() {
        let mut sim = Simulation::new_test_world(123);
        let coord = ChunkCoord { x: 0, y: 0 };
        sim.add_pollution_micro(coord, u64::MAX);
        sim.add_pollution_micro(coord, 1);

        let diagnostics = sim.capacity_diagnostics();
        assert_eq!(diagnostics.pollution_addition_overflows, 1);
        assert_eq!(diagnostics.pollution_chunks_over_practical_limit, 1);
        assert!(diagnostics.pollution_total_over_practical_limit);
        assert!(diagnostics.has_capacity_failures());
    }

    #[test]
    fn pollution_above_practical_total_limit_is_rejected() {
        let mut sim = Simulation::new_test_world(123);
        let chunk_count = MAX_TOTAL_POLLUTION_MICRO / MAX_POLLUTION_PER_CHUNK_MICRO + 1;
        for x in 0..chunk_count {
            sim.pollution.chunks.insert(
                ChunkCoord {
                    x: i32::try_from(x).unwrap(),
                    y: 0,
                },
                MAX_POLLUTION_PER_CHUNK_MICRO,
            );
        }

        assert_eq!(
            validate_pollution_state(&sim),
            Err(SimValidationError::PollutionCapacityExceeded { chunk: None })
        );
    }
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
