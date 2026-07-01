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
    if sim.item_statistics.buckets.len() != ITEM_STATISTICS_WINDOW_TICKS as usize
        || sim.item_statistics.last_advanced_tick != sim.tick
    {
        return Err(SimValidationError::InvalidItemStatistics(ItemId::new(0)));
    }

    let mut rolling_produced = BTreeMap::<ItemId, u64>::new();
    let mut rolling_consumed = BTreeMap::<ItemId, u64>::new();

    for bucket in &sim.item_statistics.buckets {
        let in_window = bucket.tick <= sim.tick
            && bucket.tick.saturating_add(ITEM_STATISTICS_WINDOW_TICKS) > sim.tick;
        if (!bucket.produced.is_empty() || !bucket.consumed.is_empty()) && !in_window {
            return Err(SimValidationError::InvalidItemStatistics(
                bucket
                    .produced
                    .keys()
                    .chain(bucket.consumed.keys())
                    .copied()
                    .next()
                    .unwrap_or_else(|| ItemId::new(0)),
            ));
        }
        for (item_id, amount) in bucket.produced.iter().chain(bucket.consumed.iter()) {
            if *amount == 0 || !item_exists(&sim.world.prototypes, *item_id) {
                return Err(SimValidationError::InvalidItemStatistics(*item_id));
            }
        }
        if in_window {
            for (item_id, amount) in &bucket.produced {
                add_checked_stat(
                    &mut rolling_produced,
                    *item_id,
                    *amount,
                    SimValidationError::InvalidItemStatistics(*item_id),
                )?;
            }
            for (item_id, amount) in &bucket.consumed {
                add_checked_stat(
                    &mut rolling_consumed,
                    *item_id,
                    *amount,
                    SimValidationError::InvalidItemStatistics(*item_id),
                )?;
            }
        }
    }

    if rolling_produced != sim.item_statistics.rolling_produced
        || rolling_consumed != sim.item_statistics.rolling_consumed
    {
        return Err(SimValidationError::InvalidItemStatistics(ItemId::new(0)));
    }

    for (item_id, amount) in sim
        .item_statistics
        .rolling_produced
        .iter()
        .chain(sim.item_statistics.rolling_consumed.iter())
        .chain(sim.item_statistics.total_produced.iter())
        .chain(sim.item_statistics.total_consumed.iter())
    {
        if *amount == 0 || !item_exists(&sim.world.prototypes, *item_id) {
            return Err(SimValidationError::InvalidItemStatistics(*item_id));
        }
    }

    Ok(())
}

pub(super) fn validate_fluid_statistics(sim: &Simulation) -> Result<(), SimValidationError> {
    if sim.fluid_statistics.buckets.len() != ITEM_STATISTICS_WINDOW_TICKS as usize
        || sim.fluid_statistics.last_advanced_tick != sim.tick
    {
        return Err(SimValidationError::InvalidFluidStatistics(FluidId::new(0)));
    }

    let mut rolling_produced = BTreeMap::<FluidId, u64>::new();
    let mut rolling_consumed = BTreeMap::<FluidId, u64>::new();

    for bucket in &sim.fluid_statistics.buckets {
        let in_window = bucket.tick <= sim.tick
            && bucket.tick.saturating_add(ITEM_STATISTICS_WINDOW_TICKS) > sim.tick;
        if (!bucket.produced.is_empty() || !bucket.consumed.is_empty()) && !in_window {
            return Err(SimValidationError::InvalidFluidStatistics(
                bucket
                    .produced
                    .keys()
                    .chain(bucket.consumed.keys())
                    .copied()
                    .next()
                    .unwrap_or_else(|| FluidId::new(0)),
            ));
        }
        for (fluid_id, amount) in bucket.produced.iter().chain(bucket.consumed.iter()) {
            if *amount == 0 || !fluid_exists(&sim.world.prototypes, *fluid_id) {
                return Err(SimValidationError::InvalidFluidStatistics(*fluid_id));
            }
        }
        if in_window {
            for (fluid_id, amount) in &bucket.produced {
                add_checked_stat(
                    &mut rolling_produced,
                    *fluid_id,
                    *amount,
                    SimValidationError::InvalidFluidStatistics(*fluid_id),
                )?;
            }
            for (fluid_id, amount) in &bucket.consumed {
                add_checked_stat(
                    &mut rolling_consumed,
                    *fluid_id,
                    *amount,
                    SimValidationError::InvalidFluidStatistics(*fluid_id),
                )?;
            }
        }
    }

    if rolling_produced != sim.fluid_statistics.rolling_produced
        || rolling_consumed != sim.fluid_statistics.rolling_consumed
    {
        return Err(SimValidationError::InvalidFluidStatistics(FluidId::new(0)));
    }

    for (fluid_id, amount) in sim
        .fluid_statistics
        .rolling_produced
        .iter()
        .chain(sim.fluid_statistics.rolling_consumed.iter())
        .chain(sim.fluid_statistics.total_produced.iter())
        .chain(sim.fluid_statistics.total_consumed.iter())
    {
        if *amount == 0 || !fluid_exists(&sim.world.prototypes, *fluid_id) {
            return Err(SimValidationError::InvalidFluidStatistics(*fluid_id));
        }
    }

    Ok(())
}

pub(super) fn validate_power_statistics(sim: &Simulation) -> Result<(), SimValidationError> {
    if sim.power_statistics.samples.len() != ITEM_STATISTICS_WINDOW_TICKS as usize
        || sim.power_statistics.last_advanced_tick != sim.tick
    {
        return Err(SimValidationError::InvalidPowerStatistics);
    }

    for sample in &sim.power_statistics.samples {
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
            if world
                .prototypes
                .tiles
                .get(tile.tile_id.index())
                .is_none_or(|prototype| prototype.id != tile.tile_id)
            {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                return Err(SimValidationError::MissingTile {
                    x: chunk.coord.x * CHUNK_SIZE + local_x,
                    y: chunk.coord.y * CHUNK_SIZE + local_y,
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
        let prototype = entity_prototype_by_id(&sim.world.prototypes, placed.prototype_id).ok_or(
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
    if prototype.entity_kind == EntityKind::MiningDrill {
        tile.collision.walkable
    } else {
        tile.collision.buildable
    }
}
