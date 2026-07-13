use crate::{ids::EntityId, world::ChunkCoord};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Ticks per minute at the fixed 60 Hz simulation rate.
pub(crate) const POLLUTION_TICKS_PER_MINUTE: u64 = 3600;

/// A pollution rate converted to the fixed-tick representation used while
/// emitting. Keeping this alongside the emitter avoids repeating prototype
/// lookups and unit conversion in the hot path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PollutionEmissionRate {
    whole_micro_per_tick: u64,
    remainder_per_tick: u64,
}

impl PollutionEmissionRate {
    pub(crate) fn from_per_minute_milli(per_minute_milli: u32) -> Self {
        let numerator_per_tick = u64::from(per_minute_milli) * 1_000;
        Self {
            whole_micro_per_tick: numerator_per_tick / POLLUTION_TICKS_PER_MINUTE,
            remainder_per_tick: numerator_per_tick % POLLUTION_TICKS_PER_MINUTE,
        }
    }
}

/// Chunk-level pollution field. Amounts are stored in micro-pollution-units
/// (one millionth of a pollution unit) so per-tick emission and absorption
/// stay in integer arithmetic. Fractional micro-units are carried between
/// updates in per-source remainder maps so configured rates are conserved.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PollutionState {
    pub(crate) chunks: BTreeMap<ChunkCoord, u64>,
    pub(crate) machine_emission_remainders: BTreeMap<EntityId, u64>,
    pub(crate) terrain_absorption_remainders: BTreeMap<ChunkCoord, u64>,
}

impl PollutionState {
    pub fn amount_micro(&self, coord: ChunkCoord) -> u64 {
        self.chunks.get(&coord).copied().unwrap_or(0)
    }

    pub fn total_micro(&self) -> u64 {
        self.chunks.values().sum()
    }

    pub fn polluted_chunks(&self) -> impl Iterator<Item = (ChunkCoord, u64)> + '_ {
        self.chunks.iter().map(|(coord, amount)| (*coord, *amount))
    }

    pub(crate) fn add_micro(&mut self, coord: ChunkCoord, amount: u64) {
        if amount == 0 {
            return;
        }
        let entry = self.chunks.entry(coord).or_insert(0);
        *entry = entry.saturating_add(amount);
    }

    /// Removes up to `amount` from the chunk, returning what was actually
    /// drained. Emptied entries are dropped so the map only tracks polluted
    /// chunks.
    pub(crate) fn remove_micro(&mut self, coord: ChunkCoord, amount: u64) -> u64 {
        let Some(entry) = self.chunks.get_mut(&coord) else {
            return 0;
        };
        let removed = amount.min(*entry);
        *entry -= removed;
        if *entry == 0 {
            self.chunks.remove(&coord);
        }
        removed
    }

    pub(crate) fn accrue_machine_emission(
        &mut self,
        entity_id: EntityId,
        rate: PollutionEmissionRate,
    ) -> u64 {
        let remainder = self
            .machine_emission_remainders
            .entry(entity_id)
            .or_default();
        let numerator = rate.remainder_per_tick + *remainder;
        let amount = rate.whole_micro_per_tick + numerator / POLLUTION_TICKS_PER_MINUTE;
        *remainder = numerator % POLLUTION_TICKS_PER_MINUTE;
        if *remainder == 0 {
            self.machine_emission_remainders.remove(&entity_id);
        }
        amount
    }

    pub(crate) fn remove_machine_emission_remainder(&mut self, entity_id: EntityId) {
        self.machine_emission_remainders.remove(&entity_id);
    }

    pub(crate) fn accrue_terrain_absorption(
        &mut self,
        coord: ChunkCoord,
        per_minute_milli: u64,
        elapsed_ticks: u64,
    ) -> u64 {
        accrue_rate(
            &mut self.terrain_absorption_remainders,
            coord,
            per_minute_milli,
            elapsed_ticks,
        )
    }
}

/// Converts a milli-unit-per-minute rate to whole micro-units while retaining
/// the numerator remainder for the next update of the same source.
fn accrue_rate<K: Copy + Ord>(
    remainders: &mut BTreeMap<K, u64>,
    source: K,
    per_minute_milli: u64,
    elapsed_ticks: u64,
) -> u64 {
    let remainder = remainders.entry(source).or_default();
    let numerator = per_minute_milli * 1000 * elapsed_ticks + *remainder;
    let amount = numerator / POLLUTION_TICKS_PER_MINUTE;
    *remainder = numerator % POLLUTION_TICKS_PER_MINUTE;
    let remainder_is_zero = *remainder == 0;
    if remainder_is_zero {
        remainders.remove(&source);
    }
    amount
}
