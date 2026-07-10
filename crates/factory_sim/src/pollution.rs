use crate::world::ChunkCoord;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Chunk-level pollution field. Amounts are stored in micro-pollution-units
/// (one millionth of a pollution unit) so per-tick emission and absorption
/// stay in exact integer arithmetic.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PollutionState {
    pub(crate) chunks: BTreeMap<ChunkCoord, u64>,
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
}
