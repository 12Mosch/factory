use super::*;

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

    pub(super) fn reveal_chunks_around_player(&mut self) {
        let (tile_x, tile_y) = self.player.tile_position();
        let player_chunk = ChunkCoord {
            x: tile_x.div_euclid(CHUNK_SIZE),
            y: tile_y.div_euclid(CHUNK_SIZE),
        };

        for y in player_chunk.y - 1..=player_chunk.y + 1 {
            for x in player_chunk.x - 1..=player_chunk.x + 1 {
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
            self.clear_statistics_bucket(self.item_statistics.last_advanced_tick);
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
        self.ensure_current_statistics_bucket();

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

    fn ensure_current_statistics_bucket(&mut self) {
        let index = self.current_statistics_bucket_index();
        if self.item_statistics.buckets[index].tick != self.tick {
            self.clear_statistics_bucket(self.tick);
        }
    }

    fn clear_statistics_bucket(&mut self, tick: u64) {
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

    fn current_statistics_bucket_index(&self) -> usize {
        (self.tick % ITEM_STATISTICS_WINDOW_TICKS) as usize
    }
}

#[derive(Clone, Copy)]
enum ItemStatisticDirection {
    Produced,
    Consumed,
}

fn add_stat(stats: &mut BTreeMap<ItemId, u64>, item_id: ItemId, amount: u64) {
    let current = stats.entry(item_id).or_default();
    *current = current.saturating_add(amount);
}

fn subtract_stat(stats: &mut BTreeMap<ItemId, u64>, item_id: ItemId, amount: u64) {
    let Some(current) = stats.get_mut(&item_id) else {
        return;
    };
    *current = current.saturating_sub(amount);
    if *current == 0 {
        stats.remove(&item_id);
    }
}
