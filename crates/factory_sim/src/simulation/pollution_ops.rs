use super::*;

/// Ticks per minute at the fixed 60 Hz simulation rate; converts
/// per-minute prototype rates into per-tick amounts.
const TICKS_PER_MINUTE: u64 = 3600;

impl Simulation {
    pub fn pollution(&self) -> &PollutionState {
        &self.pollution
    }

    /// Test and scenario hook: injects pollution directly into a chunk.
    pub fn add_pollution_micro(&mut self, coord: ChunkCoord, amount: u64) {
        self.pollution.add_micro(coord, amount);
    }

    /// Adds each working machine's per-tick emission to the chunk containing
    /// its anchor tile.
    pub(super) fn emit_pollution_from_machines(&mut self) {
        let mut emissions: SmallVec<[(ChunkCoord, u64); 32]> = SmallVec::new();
        for placed in self.entities.placed_entities.values() {
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };
            let Some(per_minute_milli) = prototype.pollution_per_minute_milli else {
                continue;
            };
            let per_tick_micro = u64::from(per_minute_milli) * 1000 / TICKS_PER_MINUTE;
            if per_tick_micro == 0 {
                continue;
            }
            if self.machine_status_for_entity(placed.id) != Some(MachineStatus::Working) {
                continue;
            }
            let Some(coord) = ChunkCoord::from_tile(placed.x, placed.y) else {
                continue;
            };
            emissions.push((coord, per_tick_micro));
        }

        for (coord, amount) in emissions {
            self.pollution.add_micro(coord, amount);
        }
    }

    /// Every spread interval: diffuses a share of each sufficiently polluted
    /// chunk to its four neighbors, then lets terrain absorb, then evaporates
    /// residue so the map stays bounded.
    pub(super) fn spread_and_absorb_pollution(&mut self) {
        if !self.tick.is_multiple_of(POLLUTION_SPREAD_INTERVAL_TICKS) {
            return;
        }

        self.spread_pollution_to_neighbors();
        self.absorb_pollution_by_terrain();
        self.pollution
            .chunks
            .retain(|_, amount| *amount >= POLLUTION_MIN_RETAINED_MICRO);
    }

    fn spread_pollution_to_neighbors(&mut self) {
        // Outflows are computed from a snapshot so a chunk's outgoing share
        // is based on its pre-spread amount regardless of map order.
        let snapshot: Vec<(ChunkCoord, u64)> = self
            .pollution
            .chunks
            .iter()
            .filter(|(_, amount)| **amount >= POLLUTION_MIN_TO_SPREAD_MICRO)
            .map(|(coord, amount)| (*coord, *amount))
            .collect();

        for (coord, amount) in snapshot {
            let share = amount * POLLUTION_SPREAD_PER_NEIGHBOR_PERMILLE / 1000;
            if share == 0 {
                continue;
            }
            let mut moved = 0;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (Some(x), Some(y)) = (coord.x.checked_add(dx), coord.y.checked_add(dy)) else {
                    continue;
                };
                self.pollution.add_micro(ChunkCoord { x, y }, share);
                moved += share;
            }
            self.pollution.remove_micro(coord, moved);
        }
    }

    fn absorb_pollution_by_terrain(&mut self) {
        // Absorption rate per tile id, in micro-units per spread interval.
        let per_tile_absorption: Vec<u64> = self
            .world
            .prototypes
            .tiles
            .iter()
            .map(|tile| {
                u64::from(tile.pollution_absorption_per_minute_milli)
                    * 1000
                    * POLLUTION_SPREAD_INTERVAL_TICKS
                    / TICKS_PER_MINUTE
            })
            .collect();
        if per_tile_absorption.iter().all(|rate| *rate == 0) {
            return;
        }

        let polluted: Vec<ChunkCoord> = self.pollution.chunks.keys().copied().collect();
        for coord in polluted {
            // Ungenerated chunks have no terrain yet; they start absorbing
            // once the world generates them.
            let Some(chunk) = self.world.chunks.get(&coord) else {
                continue;
            };
            let absorption: u64 = chunk
                .tiles
                .iter()
                .map(|tile| {
                    per_tile_absorption
                        .get(tile.tile_id.index())
                        .copied()
                        .unwrap_or(0)
                })
                .sum();
            self.pollution.remove_micro(coord, absorption);
        }
    }
}
