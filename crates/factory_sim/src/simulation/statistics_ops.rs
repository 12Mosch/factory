use super::*;

const CHART_REVEAL_RADIUS_CHUNKS: i32 = 1;

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

    pub fn item_statistics(&self) -> ItemStatisticsSnapshot {
        let mut item_ids = BTreeSet::new();
        item_ids.extend(self.item_statistics.rolling_produced.keys().copied());
        item_ids.extend(self.item_statistics.rolling_consumed.keys().copied());
        item_ids.extend(self.item_statistics.total_produced.keys().copied());
        item_ids.extend(self.item_statistics.total_consumed.keys().copied());

        ItemStatisticsSnapshot {
            rows: item_ids
                .into_iter()
                .map(|item_id| ItemStatisticsRow {
                    item_id,
                    produced_last_minute: self
                        .item_statistics
                        .rolling_produced
                        .get(&item_id)
                        .copied()
                        .unwrap_or(0),
                    consumed_last_minute: self
                        .item_statistics
                        .rolling_consumed
                        .get(&item_id)
                        .copied()
                        .unwrap_or(0),
                    produced_total: self
                        .item_statistics
                        .total_produced
                        .get(&item_id)
                        .copied()
                        .unwrap_or(0),
                    consumed_total: self
                        .item_statistics
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
        fluid_ids.extend(self.fluid_statistics.rolling_produced.keys().copied());
        fluid_ids.extend(self.fluid_statistics.rolling_consumed.keys().copied());
        fluid_ids.extend(self.fluid_statistics.total_produced.keys().copied());
        fluid_ids.extend(self.fluid_statistics.total_consumed.keys().copied());

        FluidStatisticsSnapshot {
            rows: fluid_ids
                .into_iter()
                .map(|fluid_id| FluidStatisticsRow {
                    fluid_id,
                    produced_last_minute: self
                        .fluid_statistics
                        .rolling_produced
                        .get(&fluid_id)
                        .copied()
                        .unwrap_or(0),
                    consumed_last_minute: self
                        .fluid_statistics
                        .rolling_consumed
                        .get(&fluid_id)
                        .copied()
                        .unwrap_or(0),
                    produced_total: self
                        .fluid_statistics
                        .total_produced
                        .get(&fluid_id)
                        .copied()
                        .unwrap_or(0),
                    consumed_total: self
                        .fluid_statistics
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
            .power_statistics
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

    pub(super) fn reveal_chunks_around_player(&mut self) {
        let (tile_x, tile_y) = self.player.tile_position();
        let player_chunk = ChunkCoord {
            x: tile_x.div_euclid(CHUNK_SIZE),
            y: tile_y.div_euclid(CHUNK_SIZE),
        };
        self.world
            .ensure_chunks_around_chunk(player_chunk, CHART_REVEAL_RADIUS_CHUNKS);

        for y in player_chunk.y - CHART_REVEAL_RADIUS_CHUNKS
            ..=player_chunk.y + CHART_REVEAL_RADIUS_CHUNKS
        {
            for x in player_chunk.x - CHART_REVEAL_RADIUS_CHUNKS
                ..=player_chunk.x + CHART_REVEAL_RADIUS_CHUNKS
            {
                let coord = ChunkCoord { x, y };
                if self.world.chunks.contains_key(&coord) {
                    self.chart.revealed_chunks.insert(coord);
                }
            }
        }
    }

    pub(super) fn advance_statistics_to_current_tick(&mut self) {
        while self.item_statistics.last_advanced_tick < self.tick {
            self.item_statistics.last_advanced_tick += 1;
            self.clear_item_statistics_bucket(self.item_statistics.last_advanced_tick);
        }
        while self.fluid_statistics.last_advanced_tick < self.tick {
            self.fluid_statistics.last_advanced_tick += 1;
            self.clear_fluid_statistics_bucket(self.fluid_statistics.last_advanced_tick);
        }
        while self.power_statistics.last_advanced_tick < self.tick {
            self.power_statistics.last_advanced_tick += 1;
            self.clear_power_statistics_sample(self.power_statistics.last_advanced_tick);
        }
    }

    pub(super) fn record_item_produced(&mut self, item_id: ItemId, amount: u64) {
        self.record_item_stat(item_id, amount, ItemStatisticDirection::Produced);
    }

    pub(super) fn record_item_consumed(&mut self, item_id: ItemId, amount: u64) {
        self.record_item_stat(item_id, amount, ItemStatisticDirection::Consumed);
    }

    fn record_item_stat(
        &mut self,
        item_id: ItemId,
        amount: u64,
        direction: ItemStatisticDirection,
    ) {
        if amount == 0 {
            return;
        }
        self.advance_statistics_to_current_tick();
        self.ensure_current_item_statistics_bucket();

        let index = self.current_statistics_bucket_index();
        let bucket = &mut self.item_statistics.buckets[index];
        match direction {
            ItemStatisticDirection::Produced => {
                add_stat(&mut bucket.produced, item_id, amount);
                add_stat(&mut self.item_statistics.rolling_produced, item_id, amount);
                add_stat(&mut self.item_statistics.total_produced, item_id, amount);
            }
            ItemStatisticDirection::Consumed => {
                add_stat(&mut bucket.consumed, item_id, amount);
                add_stat(&mut self.item_statistics.rolling_consumed, item_id, amount);
                add_stat(&mut self.item_statistics.total_consumed, item_id, amount);
            }
        }
    }

    pub(super) fn record_fluid_produced(&mut self, fluid_id: FluidId, amount: u64) {
        self.record_fluid_stat(fluid_id, amount, StatisticDirection::Produced);
    }

    pub(super) fn record_fluid_consumed(&mut self, fluid_id: FluidId, amount: u64) {
        self.record_fluid_stat(fluid_id, amount, StatisticDirection::Consumed);
    }

    fn record_fluid_stat(&mut self, fluid_id: FluidId, amount: u64, direction: StatisticDirection) {
        if amount == 0 {
            return;
        }
        self.advance_statistics_to_current_tick();
        self.ensure_current_fluid_statistics_bucket();

        let index = self.current_statistics_bucket_index();
        let bucket = &mut self.fluid_statistics.buckets[index];
        match direction {
            StatisticDirection::Produced => {
                add_stat(&mut bucket.produced, fluid_id, amount);
                add_stat(
                    &mut self.fluid_statistics.rolling_produced,
                    fluid_id,
                    amount,
                );
                add_stat(&mut self.fluid_statistics.total_produced, fluid_id, amount);
            }
            StatisticDirection::Consumed => {
                add_stat(&mut bucket.consumed, fluid_id, amount);
                add_stat(
                    &mut self.fluid_statistics.rolling_consumed,
                    fluid_id,
                    amount,
                );
                add_stat(&mut self.fluid_statistics.total_consumed, fluid_id, amount);
            }
        }
    }

    pub(super) fn record_power_sample(&mut self) {
        self.advance_statistics_to_current_tick();
        let index = self.current_statistics_bucket_index();
        self.power_statistics.samples[index] = PowerStatisticsSample {
            tick: self.tick,
            production_watts: self.power_summary.production_watts,
            available_production_watts: self.power_summary.available_production_watts,
            consumption_watts: self.power_summary.consumption_watts,
            satisfaction_permyriad: self.power_summary.satisfaction_permyriad,
        };
    }

    fn ensure_current_item_statistics_bucket(&mut self) {
        let index = self.current_statistics_bucket_index();
        if self.item_statistics.buckets[index].tick != self.tick {
            self.clear_item_statistics_bucket(self.tick);
        }
    }

    fn ensure_current_fluid_statistics_bucket(&mut self) {
        let index = self.current_statistics_bucket_index();
        if self.fluid_statistics.buckets[index].tick != self.tick {
            self.clear_fluid_statistics_bucket(self.tick);
        }
    }

    fn clear_item_statistics_bucket(&mut self, tick: u64) {
        let index = (tick % ITEM_STATISTICS_WINDOW_TICKS) as usize;
        let bucket = &mut self.item_statistics.buckets[index];
        for (item_id, amount) in std::mem::take(&mut bucket.produced) {
            subtract_stat(&mut self.item_statistics.rolling_produced, item_id, amount);
        }
        for (item_id, amount) in std::mem::take(&mut bucket.consumed) {
            subtract_stat(&mut self.item_statistics.rolling_consumed, item_id, amount);
        }
        bucket.tick = tick;
    }

    fn clear_fluid_statistics_bucket(&mut self, tick: u64) {
        let index = (tick % ITEM_STATISTICS_WINDOW_TICKS) as usize;
        let bucket = &mut self.fluid_statistics.buckets[index];
        for (fluid_id, amount) in std::mem::take(&mut bucket.produced) {
            subtract_stat(
                &mut self.fluid_statistics.rolling_produced,
                fluid_id,
                amount,
            );
        }
        for (fluid_id, amount) in std::mem::take(&mut bucket.consumed) {
            subtract_stat(
                &mut self.fluid_statistics.rolling_consumed,
                fluid_id,
                amount,
            );
        }
        bucket.tick = tick;
    }

    fn clear_power_statistics_sample(&mut self, tick: u64) {
        let index = (tick % ITEM_STATISTICS_WINDOW_TICKS) as usize;
        self.power_statistics.samples[index] = PowerStatisticsSample {
            tick,
            ..PowerStatisticsSample::default()
        };
    }

    fn current_statistics_bucket_index(&self) -> usize {
        (self.tick % ITEM_STATISTICS_WINDOW_TICKS) as usize
    }
}

#[derive(Clone, Copy)]
enum ItemStatisticDirection {
    Produced,
    Consumed,
}

#[derive(Clone, Copy)]
enum StatisticDirection {
    Produced,
    Consumed,
}

pub(super) fn power_sample_is_recorded(sample: PowerStatisticsSample) -> bool {
    sample.tick != 0
        || sample.production_watts != 0
        || sample.available_production_watts != 0
        || sample.consumption_watts != 0
}

fn add_stat<K: Ord>(stats: &mut BTreeMap<K, u64>, key: K, amount: u64) {
    let current = stats.entry(key).or_default();
    *current = current.saturating_add(amount);
}

fn subtract_stat<K: Ord>(stats: &mut BTreeMap<K, u64>, key: K, amount: u64) {
    let Some(current) = stats.get_mut(&key) else {
        return;
    };
    *current = current.saturating_sub(amount);
    if *current == 0 {
        stats.remove(&key);
    }
}
