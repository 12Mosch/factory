use super::super::*;
use super::ids::*;

pub(super) fn validate_chart_state(sim: &Simulation) -> Result<(), SimValidationError> {
    for coord in &sim.chart.revealed_chunks {
        if !sim.world.chunks.contains_key(coord) {
            return Err(SimValidationError::InvalidChartChunk(*coord));
        }
    }

    Ok(())
}

pub(super) fn validate_item_statistics(sim: &Simulation) -> Result<(), SimValidationError> {
    validate_rolling_statistics(
        &sim.world.prototypes,
        sim.tick,
        &sim.statistics.items.buckets,
        sim.statistics.items.last_advanced_tick,
        &sim.statistics.items.rolling_produced,
        &sim.statistics.items.rolling_consumed,
        &sim.statistics.items.total_produced,
        &sim.statistics.items.total_consumed,
        |bucket| bucket.tick,
        |bucket| &bucket.produced,
        |bucket| &bucket.consumed,
        item_exists,
        SimValidationError::InvalidItemStatistics,
        ItemId::new(0),
    )
}

pub(super) fn validate_fluid_statistics(sim: &Simulation) -> Result<(), SimValidationError> {
    validate_rolling_statistics(
        &sim.world.prototypes,
        sim.tick,
        &sim.statistics.fluids.buckets,
        sim.statistics.fluids.last_advanced_tick,
        &sim.statistics.fluids.rolling_produced,
        &sim.statistics.fluids.rolling_consumed,
        &sim.statistics.fluids.total_produced,
        &sim.statistics.fluids.total_consumed,
        |bucket| bucket.tick,
        |bucket| &bucket.produced,
        |bucket| &bucket.consumed,
        fluid_exists,
        SimValidationError::InvalidFluidStatistics,
        FluidId::new(0),
    )
}

pub(super) fn validate_power_statistics(sim: &Simulation) -> Result<(), SimValidationError> {
    if sim.statistics.power.samples.len() != ITEM_STATISTICS_WINDOW_TICKS as usize
        || sim.statistics.power.last_advanced_tick != sim.tick
    {
        return Err(SimValidationError::InvalidPowerStatistics);
    }

    for sample in &sim.statistics.power.samples {
        let in_window = sample.tick <= sim.tick
            && sample.tick.saturating_add(ITEM_STATISTICS_WINDOW_TICKS) > sim.tick;
        if power_sample_is_recorded(*sample) && !in_window {
            return Err(SimValidationError::InvalidPowerStatistics);
        }
        if sample.satisfaction_permyriad > POWER_SATISFACTION_FULL_PERMYRIAD {
            return Err(SimValidationError::InvalidPowerStatistics);
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_rolling_statistics<K, B>(
    catalog: &PrototypeCatalog,
    tick: u64,
    buckets: &[B],
    last_advanced_tick: u64,
    rolling_produced: &BTreeMap<K, u64>,
    rolling_consumed: &BTreeMap<K, u64>,
    total_produced: &BTreeMap<K, u64>,
    total_consumed: &BTreeMap<K, u64>,
    bucket_tick: impl Fn(&B) -> u64,
    bucket_produced: impl Fn(&B) -> &BTreeMap<K, u64>,
    bucket_consumed: impl Fn(&B) -> &BTreeMap<K, u64>,
    exists: impl Fn(&PrototypeCatalog, K) -> bool,
    error: impl Fn(K) -> SimValidationError,
    fallback_key: K,
) -> Result<(), SimValidationError>
where
    K: Copy + Ord,
{
    if buckets.len() != ITEM_STATISTICS_WINDOW_TICKS as usize || last_advanced_tick != tick {
        return Err(error(fallback_key));
    }

    let mut computed_rolling_produced = BTreeMap::<K, u64>::new();
    let mut computed_rolling_consumed = BTreeMap::<K, u64>::new();

    for bucket in buckets {
        let bucket_tick = bucket_tick(bucket);
        let produced = bucket_produced(bucket);
        let consumed = bucket_consumed(bucket);
        let in_window =
            bucket_tick <= tick && bucket_tick.saturating_add(ITEM_STATISTICS_WINDOW_TICKS) > tick;

        if (!produced.is_empty() || !consumed.is_empty()) && !in_window {
            let key = produced
                .keys()
                .chain(consumed.keys())
                .copied()
                .next()
                .unwrap_or(fallback_key);
            return Err(error(key));
        }
        for (key, amount) in produced.iter().chain(consumed.iter()) {
            if *amount == 0 || !exists(catalog, *key) {
                return Err(error(*key));
            }
        }
        if in_window {
            for (key, amount) in produced {
                add_checked_stat(&mut computed_rolling_produced, *key, *amount, error(*key))?;
            }
            for (key, amount) in consumed {
                add_checked_stat(&mut computed_rolling_consumed, *key, *amount, error(*key))?;
            }
        }
    }

    if &computed_rolling_produced != rolling_produced
        || &computed_rolling_consumed != rolling_consumed
    {
        return Err(error(fallback_key));
    }

    for (key, amount) in rolling_produced
        .iter()
        .chain(rolling_consumed.iter())
        .chain(total_produced.iter())
        .chain(total_consumed.iter())
    {
        if *amount == 0 || !exists(catalog, *key) {
            return Err(error(*key));
        }
    }

    Ok(())
}

fn add_checked_stat<K: Ord>(
    stats: &mut BTreeMap<K, u64>,
    key: K,
    amount: u64,
    error: SimValidationError,
) -> Result<(), SimValidationError> {
    let current = stats.entry(key).or_default();
    *current = current.checked_add(amount).ok_or(error)?;
    Ok(())
}

pub(super) fn validate_world_resources(world: &WorldSim) -> Result<(), SimValidationError> {
    for chunk in world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            if world.prototypes.tile(tile.tile_id).is_none() {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                return Err(SimValidationError::MissingTile {
                    x: chunk.coord.tile_at(local_x, local_y).0,
                    y: chunk.coord.tile_at(local_x, local_y).1,
                });
            }

            if let Some(resource) = tile.resource
                && !item_exists(&world.prototypes, resource.resource_item)
            {
                return Err(SimValidationError::UnknownItem(resource.resource_item));
            }
        }
    }

    Ok(())
}

pub(super) fn validate_placed_entities(sim: &Simulation) -> Result<(), SimValidationError> {
    for placed in sim.entities.placed_entities.values() {
        let prototype = sim.world.prototypes.entity(placed.prototype_id).ok_or(
            SimValidationError::InvalidEntityPrototype {
                entity_id: placed.id,
                prototype_id: placed.prototype_id,
            },
        )?;
        let expected_footprint = EntityFootprint::from_size(
            placed.x,
            placed.y,
            prototype.size.x,
            prototype.size.y,
            placed.direction,
        );

        if placed.footprint != expected_footprint || placed.footprint.validate().is_err() {
            return Err(SimValidationError::InvalidEntityFootprint {
                entity_id: placed.id,
            });
        }

        for (x, y) in placed.footprint.tiles() {
            let tile = sim
                .world
                .tile_at(x, y)
                .ok_or(SimValidationError::InvalidEntityTile {
                    entity_id: placed.id,
                    x,
                    y,
                })?;
            if !entity_can_occupy_tile(prototype, tile) {
                return Err(SimValidationError::InvalidEntityTile {
                    entity_id: placed.id,
                    x,
                    y,
                });
            }
        }

        if prototype.entity_kind == EntityKind::OffshorePump
            && !offshore_pump_water_tiles(&placed.footprint, placed.direction)
                .into_iter()
                .any(|(x, y)| sim.world.tile_at(x, y).is_some_and(is_water_like_tile))
        {
            return Err(SimValidationError::InvalidEntityTile {
                entity_id: placed.id,
                x: placed.footprint.x,
                y: placed.footprint.y,
            });
        }
    }

    Ok(())
}

fn entity_can_occupy_tile(prototype: &factory_data::EntityPrototype, tile: &TileCell) -> bool {
    if matches!(
        prototype.entity_kind,
        EntityKind::MiningDrill | EntityKind::Pumpjack
    ) {
        tile.collision.walkable
    } else {
        tile.collision.buildable
    }
}
