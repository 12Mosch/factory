use super::*;
use std::collections::btree_map::Entry;

impl Simulation {
    pub fn pollution(&self) -> &PollutionState {
        &self.pollution
    }

    /// Test and scenario hook: injects pollution directly into a chunk.
    pub fn add_pollution_micro(&mut self, coord: ChunkCoord, amount: u64) {
        let before = self.pollution.amount_micro(coord);
        let overflowed = self.pollution.add_micro(coord, amount);
        if self.pollution.amount_micro(coord) != before {
            self.pollution_map_revision = self.pollution_map_revision.wrapping_add(1);
        }
        self.record_pollution_addition_overflows(u64::from(overflowed));
    }

    pub fn capacity_diagnostics(&self) -> CapacityDiagnostics {
        let exact_total = self.pollution.checked_total_micro();
        CapacityDiagnostics {
            pollution_addition_overflows: self.capacity_overflows.pollution_additions,
            attack_budget_addition_overflows: self.capacity_overflows.attack_budget_additions,
            pollution_total_overflowed: exact_total.is_none(),
            pollution_chunks_over_practical_limit: self
                .pollution
                .chunks
                .values()
                .filter(|amount| **amount > MAX_POLLUTION_PER_CHUNK_MICRO)
                .count(),
            pollution_total_over_practical_limit: exact_total
                .is_none_or(|total| total > MAX_TOTAL_POLLUTION_MICRO),
            attack_budgets_over_practical_limit: self
                .enemies
                .bases
                .iter()
                .filter(|(base_id, base)| {
                    self.attack_budget_cap(**base_id)
                        .is_some_and(|cap| base.attack_budget_micro > cap)
                })
                .count(),
        }
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
        let multiplier = entity_resolved_module_effects(&self.entities, entity_id).map_or(
            10_000,
            ResolvedModuleEffects::pollution_multiplier_permyriad,
        );
        let scaled_per_minute_milli = u64::from(per_minute_milli)
            .saturating_mul(multiplier)
            .saturating_add(9_999)
            / 10_000;
        self.pollution_emitters.emitters.insert(
            entity_id,
            PollutionEmitter {
                chunk,
                rate: PollutionEmissionRate::from_per_minute_milli(
                    scaled_per_minute_milli.min(u64::from(u32::MAX)) as u32,
                ),
                active: false,
            },
        );
    }

    pub(super) fn refresh_pollution_emitter(&mut self, entity_id: EntityId) {
        let Some(placed) = self.entities.placed_entity(entity_id).cloned() else {
            return;
        };
        let was_active = self
            .pollution_emitters
            .emitters
            .get(&entity_id)
            .is_some_and(|emitter| emitter.active);
        self.pollution_emitters.emitters.remove(&entity_id);
        self.register_pollution_emitter(entity_id, placed.prototype_id, placed.x, placed.y);
        if was_active {
            self.pollution_emitters.mark_active(entity_id);
        }
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
            let overflowed = self.pollution.add_micro(emitter.chunk, amount);
            self.record_pollution_addition_overflows(u64::from(overflowed));
        }
    }

    /// Every spread interval: diffuses a share of each sufficiently polluted
    /// chunk to its generated neighbors, then lets terrain absorb, then
    /// evaporates residue so the map stays bounded.
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

    pub(super) fn spread_pollution_to_neighbors(&mut self) {
        // Accumulate against the unchanged field so every outflow uses the
        // pre-spread amount. The scratch allocations are retained between
        // passes and each affected BTreeMap entry is updated only once.
        self.pollution_diffusion.deltas.clear();
        self.pollution_diffusion.ordered_deltas.clear();
        let mut overflow_count = 0_u64;

        for (&coord, &amount) in self
            .pollution
            .chunks
            .iter()
            .filter(|(_, amount)| **amount >= POLLUTION_MIN_TO_SPREAD_MICRO)
        {
            let share = amount / 1000 * POLLUTION_SPREAD_PER_NEIGHBOR_PERMILLE;
            if share == 0 {
                continue;
            }
            let mut moved = 0;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (Some(x), Some(y)) = (coord.x.checked_add(dx), coord.y.checked_add(dy)) else {
                    continue;
                };
                let destination = ChunkCoord { x, y };
                // Pollution does not materialize terrain or exist beyond the
                // generated world. Treat its edge as a closed boundary: only
                // shares with generated destinations leave the source.
                if !self.world.chunks.contains_key(&destination) {
                    continue;
                }
                let delta = self
                    .pollution_diffusion
                    .deltas
                    .entry(destination)
                    .or_default();
                if coord < destination {
                    let (sum, overflowed) =
                        saturating_add_with_overflow(delta.incoming_before_outflow, share);
                    delta.incoming_before_outflow = sum;
                    overflow_count = overflow_count.saturating_add(u64::from(overflowed));
                } else {
                    let (sum, overflowed) =
                        saturating_add_with_overflow(delta.incoming_after_outflow, share);
                    delta.incoming_after_outflow = sum;
                    overflow_count = overflow_count.saturating_add(u64::from(overflowed));
                }
                moved += share;
            }
            self.pollution_diffusion
                .deltas
                .entry(coord)
                .or_default()
                .outgoing = moved;
        }

        self.pollution_diffusion
            .ordered_deltas
            .extend(self.pollution_diffusion.deltas.drain());
        self.pollution_diffusion
            .ordered_deltas
            .sort_unstable_by_key(|(coord, _)| *coord);

        for (coord, delta) in self.pollution_diffusion.ordered_deltas.drain(..) {
            match self.pollution.chunks.entry(coord) {
                Entry::Occupied(mut entry) => {
                    let (before_outflow, before_overflowed) =
                        saturating_add_with_overflow(*entry.get(), delta.incoming_before_outflow);
                    let (amount, after_overflowed) = saturating_add_with_overflow(
                        before_outflow.saturating_sub(delta.outgoing),
                        delta.incoming_after_outflow,
                    );
                    overflow_count = overflow_count
                        .saturating_add(u64::from(before_overflowed))
                        .saturating_add(u64::from(after_overflowed));
                    if amount == 0 {
                        entry.remove();
                    } else {
                        entry.insert(amount);
                    }
                }
                Entry::Vacant(entry) => {
                    debug_assert_eq!(delta.outgoing, 0);
                    let (amount, overflowed) = saturating_add_with_overflow(
                        delta.incoming_before_outflow,
                        delta.incoming_after_outflow,
                    );
                    overflow_count = overflow_count.saturating_add(u64::from(overflowed));
                    if amount != 0 {
                        entry.insert(amount);
                    }
                }
            }
        }
        self.record_pollution_addition_overflows(overflow_count);
    }

    pub(super) fn absorb_pollution_by_terrain(&mut self) {
        if !self.world.generator.has_pollution_absorption() {
            return;
        }

        let polluted: Vec<ChunkCoord> = self.pollution.chunks.keys().copied().collect();
        for coord in polluted {
            // Ungenerated chunks have no terrain yet; they start absorbing
            // once the world generates them.
            let Some(chunk) = self.world.chunks.get(&coord) else {
                continue;
            };
            let absorption = self.pollution.accrue_terrain_absorption(
                coord,
                chunk.pollution_absorption_per_minute_milli,
                POLLUTION_SPREAD_INTERVAL_TICKS,
            );
            self.pollution.remove_micro(coord, absorption);
        }
    }

    fn record_pollution_addition_overflows(&mut self, count: u64) {
        self.capacity_overflows.pollution_additions = self
            .capacity_overflows
            .pollution_additions
            .saturating_add(count);
    }
}

fn entity_resolved_module_effects(
    entities: &EntityStore,
    entity_id: EntityId,
) -> Option<ResolvedModuleEffects> {
    entities
        .machine_module_state(entity_id)
        .map(|modules| modules.resolved_effects)
}
