use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeneratedChunkReveal {
    Missing,
    AlreadyRevealed,
    NewlyRevealed,
}

const CHART_REVEAL_RADIUS_CHUNKS: i32 = 1;
const REVEALED_CHUNK_HISTORY_LIMIT: usize = 4096;

impl Default for ItemStatistics {
    fn default() -> Self {
        Self {
            buckets: vec![ItemStatisticsBucket::default(); ITEM_STATISTICS_WINDOW_TICKS as usize],
            last_advanced_tick: 0,
            rolling_produced: BTreeMap::new(),
            rolling_consumed: BTreeMap::new(),
            total_produced: BTreeMap::new(),
            total_consumed: BTreeMap::new(),
        }
    }
}

impl Default for FluidStatistics {
    fn default() -> Self {
        Self {
            buckets: vec![FluidStatisticsBucket::default(); ITEM_STATISTICS_WINDOW_TICKS as usize],
            last_advanced_tick: 0,
            rolling_produced: BTreeMap::new(),
            rolling_consumed: BTreeMap::new(),
            total_produced: BTreeMap::new(),
            total_consumed: BTreeMap::new(),
        }
    }
}

impl Default for PowerStatistics {
    fn default() -> Self {
        Self {
            samples: vec![PowerStatisticsSample::default(); ITEM_STATISTICS_WINDOW_TICKS as usize],
            last_advanced_tick: 0,
        }
    }
}

impl Simulation {
    pub fn revealed_chunks(&self) -> &BTreeSet<ChunkCoord> {
        &self.chart.revealed_chunks
    }

    pub fn is_chunk_revealed(&self, coord: ChunkCoord) -> bool {
        self.chart.revealed_chunks.contains(&coord)
    }

    /// Returns exact chunk coordinates revealed after `revision`.
    ///
    /// `None` means the caller fell behind the bounded runtime history and
    /// must rebuild its derived state from `revealed_chunks`.
    pub fn revealed_chunks_since(
        &self,
        revision: u64,
    ) -> Option<impl Iterator<Item = ChunkCoord> + '_> {
        if revision > self.revealed_revision {
            return None;
        }
        if revision < self.revealed_revision
            && self
                .revealed_chunk_history
                .0
                .front()
                .is_none_or(|batch| batch.revision > revision.saturating_add(1))
        {
            return None;
        }

        Some(
            self.revealed_chunk_history
                .0
                .iter()
                .filter(move |batch| batch.revision > revision)
                .flat_map(|batch| batch.chunks.iter().copied()),
        )
    }

    pub fn item_statistics(&self) -> ItemStatisticsSnapshot {
        let mut item_ids = BTreeSet::new();
        item_ids.extend(self.statistics.items.rolling_produced.keys().copied());
        item_ids.extend(self.statistics.items.rolling_consumed.keys().copied());
        item_ids.extend(self.statistics.items.total_produced.keys().copied());
        item_ids.extend(self.statistics.items.total_consumed.keys().copied());

        ItemStatisticsSnapshot {
            rows: item_ids
                .into_iter()
                .map(|item_id| ItemStatisticsRow {
                    item_id,
                    produced_last_minute: self
                        .statistics
                        .items
                        .rolling_produced
                        .get(&item_id)
                        .copied()
                        .unwrap_or(0),
                    consumed_last_minute: self
                        .statistics
                        .items
                        .rolling_consumed
                        .get(&item_id)
                        .copied()
                        .unwrap_or(0),
                    produced_total: self
                        .statistics
                        .items
                        .total_produced
                        .get(&item_id)
                        .copied()
                        .unwrap_or(0),
                    consumed_total: self
                        .statistics
                        .items
                        .total_consumed
                        .get(&item_id)
                        .copied()
                        .unwrap_or(0),
                })
                .collect(),
        }
    }

    pub fn fluid_statistics(&self) -> FluidStatisticsSnapshot {
        let mut fluid_ids = BTreeSet::new();
        fluid_ids.extend(self.statistics.fluids.rolling_produced.keys().copied());
        fluid_ids.extend(self.statistics.fluids.rolling_consumed.keys().copied());
        fluid_ids.extend(self.statistics.fluids.total_produced.keys().copied());
        fluid_ids.extend(self.statistics.fluids.total_consumed.keys().copied());

        FluidStatisticsSnapshot {
            rows: fluid_ids
                .into_iter()
                .map(|fluid_id| FluidStatisticsRow {
                    fluid_id,
                    produced_last_minute: self
                        .statistics
                        .fluids
                        .rolling_produced
                        .get(&fluid_id)
                        .copied()
                        .unwrap_or(0),
                    consumed_last_minute: self
                        .statistics
                        .fluids
                        .rolling_consumed
                        .get(&fluid_id)
                        .copied()
                        .unwrap_or(0),
                    produced_total: self
                        .statistics
                        .fluids
                        .total_produced
                        .get(&fluid_id)
                        .copied()
                        .unwrap_or(0),
                    consumed_total: self
                        .statistics
                        .fluids
                        .total_consumed
                        .get(&fluid_id)
                        .copied()
                        .unwrap_or(0),
                })
                .collect(),
        }
    }

    pub fn power_statistics(&self) -> PowerStatisticsSnapshot {
        let mut samples = self
            .statistics
            .power
            .samples
            .iter()
            .copied()
            .filter(|sample| {
                power_sample_is_recorded(*sample)
                    && sample.tick <= self.tick
                    && sample.tick.saturating_add(ITEM_STATISTICS_WINDOW_TICKS) > self.tick
            })
            .collect::<Vec<_>>();
        samples.sort_by_key(|sample| sample.tick);
        PowerStatisticsSnapshot { samples }
    }

    pub(super) fn request_chunks_around_player(&mut self) {
        let (tile_x, tile_y) = self.player.tile_position();
        let Some(player_chunk) = ChunkCoord::from_tile(tile_x, tile_y) else {
            return;
        };

        // Chart requests describe the player's current reveal neighborhood.
        // Drop obsolete work after a teleport instead of streaming terrain
        // that will no longer be revealed.
        self.chunk_generation_queue.chart.clear();
        let Ok((min_x, max_x, min_y, max_y)) =
            chunk_neighborhood_bounds(player_chunk, CHART_REVEAL_RADIUS_CHUNKS)
        else {
            self.request_chunk_generation(player_chunk, ChunkGenerationPriority::Required);
            return;
        };
        let mut revealed_chunks = Vec::new();
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let coord = ChunkCoord { x, y };
                match self.reveal_generated_chunk(coord) {
                    GeneratedChunkReveal::Missing => {
                        let priority = if coord == player_chunk {
                            ChunkGenerationPriority::Required
                        } else {
                            ChunkGenerationPriority::Chart
                        };
                        self.request_chunk_generation(coord, priority);
                    }
                    GeneratedChunkReveal::NewlyRevealed => revealed_chunks.push(coord),
                    GeneratedChunkReveal::AlreadyRevealed => {}
                }
            }
        }
        self.finish_chunk_reveal(revealed_chunks);
    }

    pub(super) fn reveal_generated_chunks_around_player(&mut self, chunks: &[ChunkCoord]) {
        let (tile_x, tile_y) = self.player.tile_position();
        let Some(player_chunk) = ChunkCoord::from_tile(tile_x, tile_y) else {
            return;
        };
        let Ok((min_x, max_x, min_y, max_y)) =
            chunk_neighborhood_bounds(player_chunk, CHART_REVEAL_RADIUS_CHUNKS)
        else {
            return;
        };

        let mut revealed_chunks = Vec::new();
        for &coord in chunks {
            if (min_x..=max_x).contains(&coord.x)
                && (min_y..=max_y).contains(&coord.y)
                && matches!(
                    self.reveal_generated_chunk(coord),
                    GeneratedChunkReveal::NewlyRevealed
                )
            {
                revealed_chunks.push(coord);
            }
        }
        self.finish_chunk_reveal(revealed_chunks);
    }

    fn reveal_generated_chunk(&mut self, coord: ChunkCoord) -> GeneratedChunkReveal {
        if !self.world.chunks.contains_key(&coord) {
            GeneratedChunkReveal::Missing
        } else if self.chart.revealed_chunks.insert(coord) {
            GeneratedChunkReveal::NewlyRevealed
        } else {
            GeneratedChunkReveal::AlreadyRevealed
        }
    }

    fn finish_chunk_reveal(&mut self, chunks: Vec<ChunkCoord>) {
        if chunks.is_empty() {
            return;
        }

        self.revealed_revision = self.revealed_revision.wrapping_add(1);
        self.revealed_chunk_history.0.push_back(RevealedChunkBatch {
            revision: self.revealed_revision,
            chunks,
        });
        while self.revealed_chunk_history.0.len() > REVEALED_CHUNK_HISTORY_LIMIT {
            self.revealed_chunk_history.0.pop_front();
        }
    }

    pub(super) fn advance_statistics_to_current_tick(&mut self) {
        StatisticsContext::new(self.tick, &mut self.statistics).advance_to_current_tick();
    }

    pub(super) fn record_item_produced(&mut self, item_id: ItemId, amount: u64) {
        StatisticsContext::new(self.tick, &mut self.statistics)
            .record_item_produced(item_id, amount);
        let base = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes);
        self.onboarding_progress
            .record_item_produced(&base, item_id, amount);
    }

    pub(super) fn record_item_consumed(&mut self, item_id: ItemId, amount: u64) {
        StatisticsContext::new(self.tick, &mut self.statistics)
            .record_item_consumed(item_id, amount);
    }

    pub(super) fn record_fluid_produced(&mut self, fluid_id: FluidId, amount: u64) {
        StatisticsContext::new(self.tick, &mut self.statistics)
            .record_fluid_produced(fluid_id, amount);
        let petroleum = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes)
            .fluids
            .petroleum_gas;
        if fluid_id == petroleum {
            self.onboarding_progress.record_counter(
                |progress| &mut progress.petroleum_gas_produced,
                amount / 1_000,
            );
        }
    }

    pub(super) fn record_fluid_consumed(&mut self, fluid_id: FluidId, amount: u64) {
        StatisticsContext::new(self.tick, &mut self.statistics)
            .record_fluid_consumed(fluid_id, amount);
    }

    pub(super) fn record_power_sample(&mut self) {
        let summary = self.power.summary;
        StatisticsContext::new(self.tick, &mut self.statistics).record_power_sample(summary);
    }
}

#[derive(Clone, Copy)]
pub(super) enum ItemStatisticDirection {
    Produced,
    Consumed,
}

#[derive(Clone, Copy)]
pub(super) enum StatisticDirection {
    Produced,
    Consumed,
}

pub(super) fn power_sample_is_recorded(sample: PowerStatisticsSample) -> bool {
    sample.tick != 0
        || sample.production_watts != 0
        || sample.available_production_watts != 0
        || sample.consumption_watts != 0
}

pub(super) fn add_stat<K: Ord>(stats: &mut BTreeMap<K, u64>, key: K, amount: u64) {
    let current = stats.entry(key).or_default();
    *current = current.saturating_add(amount);
}

pub(super) fn subtract_stat<K: Ord>(stats: &mut BTreeMap<K, u64>, key: K, amount: u64) {
    let Some(current) = stats.get_mut(&key) else {
        return;
    };
    *current = current.saturating_sub(amount);
    if *current == 0 {
        stats.remove(&key);
    }
}
