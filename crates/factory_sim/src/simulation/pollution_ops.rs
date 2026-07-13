use super::*;

impl Simulation {
    pub fn pollution(&self) -> &PollutionState {
        &self.pollution
    }

    /// Test and scenario hook: injects pollution directly into a chunk.
    pub fn add_pollution_micro(&mut self, coord: ChunkCoord, amount: u64) {
        self.pollution.add_micro(coord, amount);
    }

    pub(super) fn register_pollution_emitter(
        &mut self,
        entity_id: EntityId,
        prototype_id: EntityPrototypeId,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) {
        let Some(per_minute_milli) = self
            .world
            .prototypes
            .entity(prototype_id)
            .and_then(|prototype| prototype.pollution_per_minute_milli)
            .filter(|rate| *rate > 0)
        else {
            return;
        };
        let Some(chunk) = ChunkCoord::from_tile(x, y) else {
            return;
        };
        self.pollution_emitters.emitters.insert(
            entity_id,
            PollutionEmitter {
                chunk,
                rate: PollutionEmissionRate::from_per_minute_milli(per_minute_milli),
                active: false,
            },
        );
    }

    pub(super) fn unregister_pollution_emitter(&mut self, entity_id: EntityId) {
        self.pollution_emitters.emitters.remove(&entity_id);
        self.pollution_emitters
            .active_emitters
            .retain(|active_id| *active_id != entity_id);
        self.pollution.remove_machine_emission_remainder(entity_id);
    }

    pub(super) fn rebuild_pollution_emitter_index(&mut self) {
        self.pollution_emitters.emitters.clear();
        self.pollution_emitters.active_emitters.clear();
        let placed = self
            .entities
            .placed_entities
            .values()
            .map(|placed| (placed.id, placed.prototype_id, placed.x, placed.y))
            .collect::<Vec<_>>();
        for (entity_id, prototype_id, x, y) in placed {
            self.register_pollution_emitter(entity_id, prototype_id, x, y);
        }

        // The active flag is derived state. Reconstruct it once after loading
        // so test/scenario hooks that emit before the next tick retain their
        // pre-save behavior; normal ticks update it from actual machine work.
        let active = self
            .pollution_emitters
            .emitters
            .keys()
            .copied()
            .filter(|entity_id| {
                self.machine_status_for_entity(*entity_id) == Some(MachineStatus::Working)
            })
            .collect::<Vec<_>>();
        for entity_id in active {
            self.pollution_emitters.mark_active(entity_id);
        }
    }

    /// Adds each active emitter's cached per-tick emission to its chunk.
    pub(super) fn emit_pollution_from_machines(&mut self) {
        let emissions: SmallVec<[(EntityId, PollutionEmitter); 32]> = self
            .pollution_emitters
            .active_emitters
            .iter()
            .filter_map(|entity_id| {
                self.pollution_emitters
                    .emitters
                    .get(entity_id)
                    .map(|emitter| (*entity_id, *emitter))
            })
            .collect();

        for (entity_id, emitter) in emissions {
            let amount = self
                .pollution
                .accrue_machine_emission(entity_id, emitter.rate);
            self.pollution.add_micro(emitter.chunk, amount);
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
            let share = amount / 1000 * POLLUTION_SPREAD_PER_NEIGHBOR_PERMILLE;
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

    pub(super) fn absorb_pollution_by_terrain(&mut self) {
        // Absorption rate per tile id, in milli-units per minute. Conversion
        // happens after summing a chunk so fractional output is carried once
        // per terrain source rather than discarded once per tile.
        let per_tile_absorption: Vec<u64> = self
            .world
            .prototypes
            .tiles
            .iter()
            .map(|tile| u64::from(tile.pollution_absorption_per_minute_milli))
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
            let per_minute_milli: u64 = chunk
                .tiles
                .iter()
                .map(|tile| {
                    per_tile_absorption
                        .get(tile.tile_id.index())
                        .copied()
                        .unwrap_or(0)
                })
                .sum();
            let absorption = self.pollution.accrue_terrain_absorption(
                coord,
                per_minute_milli,
                POLLUTION_SPREAD_INTERVAL_TICKS,
            );
            self.pollution.remove_micro(coord, absorption);
        }
    }
}
