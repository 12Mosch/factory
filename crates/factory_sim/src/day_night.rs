use factory_data::DayNightCycleConfig;
use serde::{Deserialize, Serialize};
use std::hash::Hash;

use crate::{SimValidationError, Simulation};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub(crate) struct DayNightCycleState {
    tick_in_cycle: u64,
}

impl DayNightCycleState {
    pub(crate) const fn new() -> Self {
        Self { tick_in_cycle: 0 }
    }

    fn advance(&mut self, cycle_length_ticks: u64) {
        if cycle_length_ticks == 0 {
            return;
        }
        self.tick_in_cycle = if self.tick_in_cycle == cycle_length_ticks - 1 {
            0
        } else {
            self.tick_in_cycle + 1
        };
    }

    fn daylight(self, config: DayNightCycleConfig) -> f32 {
        let cycle = config.cycle_length_ticks;
        let ramp = config.dawn_dusk_ticks;
        let dusk_start = cycle / 2;
        let dusk_end = dusk_start + ramp;
        let dawn_start = cycle - ramp;
        let tick = self.tick_in_cycle;

        let daylight = if tick < dusk_start {
            1.0
        } else if tick < dusk_end {
            1.0 - (tick - dusk_start) as f64 / ramp as f64
        } else if tick < dawn_start {
            0.0
        } else {
            (tick - dawn_start) as f64 / ramp as f64
        };
        daylight as f32
    }
}

impl Simulation {
    /// Normalized daylight available at the current deterministic simulation
    /// phase. Disabled cycles remain at full daylight.
    pub fn daylight(&self) -> f32 {
        let Some(config) = self.catalog().day_night_cycle else {
            return 1.0;
        };
        self.day_night_cycle
            .map_or(1.0, |state| state.daylight(config))
    }

    pub(crate) fn advance_day_night_cycle(&mut self) {
        let Some(config) = self.catalog().day_night_cycle else {
            return;
        };
        if let Some(state) = self.day_night_cycle.as_mut() {
            state.advance(config.cycle_length_ticks);
        }
    }
}

pub(crate) fn validate_day_night_cycle_state(sim: &Simulation) -> Result<(), SimValidationError> {
    let config = sim.catalog().day_night_cycle;
    match (config, sim.day_night_cycle) {
        (None, None) => Ok(()),
        (Some(_), None) | (None, Some(_)) => {
            Err(SimValidationError::DayNightCycleStatePresenceMismatch)
        }
        (Some(config), Some(state)) => {
            if state.tick_in_cycle >= config.cycle_length_ticks {
                return Err(SimValidationError::InvalidDayNightCycleTick {
                    tick_in_cycle: state.tick_in_cycle,
                    cycle_length_ticks: config.cycle_length_ticks,
                });
            }
            let expected = sim.tick_count() % config.cycle_length_ticks;
            if state.tick_in_cycle != expected {
                return Err(SimValidationError::DayNightCyclePhaseMismatch {
                    expected,
                    actual: state.tick_in_cycle,
                });
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_from_bytes, save_to_bytes};
    use factory_data::PrototypeCatalog;

    fn simulation_with_cycle(cycle_length_ticks: u64, dawn_dusk_ticks: u64) -> Simulation {
        let mut catalog = PrototypeCatalog::load_base().expect("base catalog should load");
        catalog.day_night_cycle = Some(DayNightCycleConfig {
            cycle_length_ticks,
            dawn_dusk_ticks,
        });
        Simulation::new(17, catalog)
    }

    fn simulation_without_cycle() -> Simulation {
        let mut catalog = PrototypeCatalog::load_base().expect("base catalog should load");
        catalog.day_night_cycle = None;
        Simulation::new(17, catalog)
    }

    #[test]
    fn curve_hits_expected_phases_and_wraps() {
        let mut sim = simulation_with_cycle(20, 4);
        assert_eq!(sim.daylight(), 1.0);

        for expected_tick in 1..=20 {
            sim.tick();
            let expected = match expected_tick {
                1..=10 => 1.0,
                12 => 0.5,
                14..=16 => 0.0,
                18 => 0.5,
                20 => 1.0,
                _ => sim.daylight(),
            };
            assert_eq!(sim.daylight(), expected, "tick {expected_tick}");
        }
    }

    #[test]
    fn complete_cycle_repeats_identically() {
        let mut sim = simulation_with_cycle(20, 4);
        let first = (0..20)
            .map(|_| {
                let value = sim.daylight();
                sim.tick();
                value
            })
            .collect::<Vec<_>>();
        let second = (0..20)
            .map(|_| {
                let value = sim.daylight();
                sim.tick();
                value
            })
            .collect::<Vec<_>>();

        assert_eq!(first, second);
    }

    #[test]
    fn disabled_cycle_stays_at_full_daylight() {
        let mut sim = simulation_without_cycle();
        for _ in 0..100 {
            assert_eq!(sim.daylight(), 1.0);
            sim.tick();
        }
    }

    #[test]
    fn identical_simulations_keep_matching_daylight_and_hashes() {
        let mut first = simulation_with_cycle(20, 4);
        let mut second = simulation_with_cycle(20, 4);

        for _ in 0..75 {
            assert_eq!(first.daylight(), second.daylight());
            assert_eq!(first.state_hash(), second.state_hash());
            first.tick();
            second.tick();
        }
    }

    #[test]
    fn ramp_save_round_trip_continues_across_wrap() {
        let mut original = simulation_with_cycle(20, 4);
        for _ in 0..12 {
            original.tick();
        }
        assert_eq!(original.daylight(), 0.5);

        let bytes = save_to_bytes(&original).expect("simulation should save");
        let mut loaded = load_from_bytes(&bytes).expect("simulation should load");
        assert_eq!(loaded.daylight(), original.daylight());
        assert_eq!(loaded.state_hash(), original.state_hash());

        for _ in 0..12 {
            original.tick();
            loaded.tick();
            assert_eq!(loaded.daylight(), original.daylight());
            assert_eq!(loaded.state_hash(), original.state_hash());
        }
    }

    #[test]
    fn validation_rejects_invalid_cycle_configuration() {
        let mut catalog = PrototypeCatalog::load_base().expect("base catalog should load");
        catalog.day_night_cycle = Some(DayNightCycleConfig {
            cycle_length_ticks: 16,
            dawn_dusk_ticks: 4,
        });
        let sim = Simulation::new(17, catalog);

        assert_eq!(
            sim.validate(),
            Err(SimValidationError::InvalidDayNightCycleConfig)
        );
    }

    #[test]
    fn validation_rejects_cycle_state_presence_mismatches() {
        let mut configured = simulation_with_cycle(20, 4);
        configured.day_night_cycle = None;
        assert_eq!(
            configured.validate(),
            Err(SimValidationError::DayNightCycleStatePresenceMismatch)
        );

        let mut disabled = simulation_without_cycle();
        disabled.day_night_cycle = Some(DayNightCycleState::new());
        assert_eq!(
            disabled.validate(),
            Err(SimValidationError::DayNightCycleStatePresenceMismatch)
        );
    }

    #[test]
    fn validation_rejects_out_of_range_and_inconsistent_phases() {
        let mut out_of_range = simulation_with_cycle(20, 4);
        out_of_range.day_night_cycle = Some(DayNightCycleState { tick_in_cycle: 20 });
        assert_eq!(
            out_of_range.validate(),
            Err(SimValidationError::InvalidDayNightCycleTick {
                tick_in_cycle: 20,
                cycle_length_ticks: 20,
            })
        );

        let mut inconsistent = simulation_with_cycle(20, 4);
        inconsistent.day_night_cycle = Some(DayNightCycleState { tick_in_cycle: 1 });
        assert_eq!(
            inconsistent.validate(),
            Err(SimValidationError::DayNightCyclePhaseMismatch {
                expected: 0,
                actual: 1,
            })
        );
    }
}
