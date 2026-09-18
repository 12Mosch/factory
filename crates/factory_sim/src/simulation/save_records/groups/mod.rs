//! Record groups: what each record carries, one module per subsystem.
//!
//! [`BorrowedRecordFields`] is the single borrowed view of one generation's
//! durable state. Construct it from the live simulation for pre-capture size
//! checks or from a captured snapshot for encoding; both roots expose the
//! same fields, so the group list and tuple order exist exactly once.
//!
//! Dispatch is centralized here: [`encode_record`], [`measure_record`], and
//! [`decode_into_slot`] cover exactly the keys declared in
//! [`super::registry::RECORD_REGISTRY`] (enforced by
//! `registry_and_dispatch_agree`). Payload logic itself lives in the
//! subsystem modules below — open `player.rs`, not this file, to change what
//! the player record carries.
//!
//! Groups encode as bincode tuples of borrowed fields in the documented
//! order, so encoding never clones snapshot subsystems; the decoder
//! destructures the same tuples. Field order in each subsystem module is the
//! wire order.

mod chunks;
mod core;
mod environment;
mod networks;
mod player;
mod robots;
mod singles;
mod statistics;
mod trains;

use super::super::robot_ops::RobotLogisticWorkState;
use super::super::save::SimulationSnapshotOwned;
use super::super::*;
#[cfg(test)]
use super::codec::decode_group;
use super::codec::{ManifestEntry, RecordHeader};
use super::registry::*;
use bincode::Options;

/// Borrowed view of one generation's durable state.
pub(super) struct BorrowedRecordFields<'a> {
    pub(super) tick: u64,
    pub(super) world_seed: u64,
    pub(super) day_night_cycle: &'a Option<DayNightCycleState>,
    pub(super) config: &'a SimulationConfig,
    pub(super) entity_topology_revision: u64,
    pub(super) world_chunk_revision: u64,
    pub(super) world_walkability_revision: u64,
    pub(super) prototypes: &'a PrototypeCatalog,
    pub(super) chart: &'a ChartState,
    pub(super) chunk_generation_queue: &'a ChunkGenerationQueue,
    pub(super) item_statistics: &'a ItemStatistics,
    pub(super) fluid_statistics: &'a FluidStatistics,
    pub(super) power_statistics: &'a PowerStatistics,
    pub(super) rockets_launched: u64,
    pub(super) player_deaths: u64,
    pub(super) entities: &'a EntityStore,
    pub(super) construction: &'a ConstructionState,
    pub(super) player: &'a PlayerState,
    pub(super) player_equipment: &'a PlayerEquipmentState,
    pub(super) player_weapon: &'a PlayerWeaponState,
    pub(super) delayed_combat: &'a DelayedCombatState,
    pub(super) player_inventory: &'a Inventory,
    pub(super) corpses: &'a BTreeMap<u64, PlayerCorpse>,
    pub(super) manual_mining_progress: &'a Option<ManualMiningProgress>,
    pub(super) crafting_queue: &'a CraftingQueue,
    pub(super) onboarding_progress: &'a OnboardingProgress,
    pub(super) research: &'a ResearchState,
    pub(super) power_summary: &'a PowerSummary,
    pub(super) power_networks: &'a Vec<PowerNetworkSnapshot>,
    pub(super) entity_power_statuses: &'a DenseEntityMap<EntityPowerStatus>,
    pub(super) fluid_networks: &'a Vec<FluidNetworkSnapshot>,
    pub(super) fluid_topology_dirty: bool,
    pub(super) heat_networks: &'a Vec<HeatNetworkSnapshot>,
    pub(super) heat_topology_dirty: bool,
    pub(super) robot_networks: &'a Vec<RobotNetworkSnapshot>,
    pub(super) robot_logistic_work: &'a RobotLogisticWorkState,
    pub(super) robot_flights: &'a RobotFlightSubsystem,
    pub(super) rolling_stock: &'a RollingStockSubsystem,
    pub(super) pending_train_route_searches:
        &'a BTreeMap<TrainId, rolling_stock_ops::PendingTrainRouteSearch>,
    pub(super) pollution: &'a PollutionState,
    pub(super) enemies: &'a EnemySubsystem,
    pub(super) transport: &'a TransportLaneCache,
    pub(super) enemy_navigation: &'a enemy::EnemyNavigation,
    pub(super) attack_targets: &'a enemy::AttackTargetCache,
    pub(super) chunks: &'a BTreeMap<ChunkCoord, Chunk>,
}

impl<'a> BorrowedRecordFields<'a> {
    pub(super) fn from_simulation(sim: &'a Simulation) -> Self {
        Self {
            tick: sim.tick,
            world_seed: sim.world.seed,
            day_night_cycle: &sim.day_night_cycle,
            config: &sim.config,
            entity_topology_revision: sim.entity_topology_revision,
            world_chunk_revision: sim.world.chunk_revision,
            world_walkability_revision: sim.world.walkability_revision,
            prototypes: &sim.world.prototypes,
            chart: &sim.chart,
            chunk_generation_queue: &sim.chunk_generation_queue,
            item_statistics: &sim.statistics.items,
            fluid_statistics: &sim.statistics.fluids,
            power_statistics: &sim.statistics.power,
            rockets_launched: sim.statistics.rockets_launched,
            player_deaths: sim.statistics.player_deaths,
            entities: &sim.entities,
            construction: &sim.construction,
            player: &sim.player,
            player_equipment: &sim.player_equipment,
            player_weapon: &sim.player_weapon,
            delayed_combat: &sim.delayed_combat,
            player_inventory: &sim.player_inventory,
            corpses: &sim.corpses,
            manual_mining_progress: &sim.manual_mining_progress,
            crafting_queue: &sim.crafting_queue,
            onboarding_progress: &sim.onboarding_progress,
            research: &sim.research,
            power_summary: &sim.power.summary,
            power_networks: &sim.power.networks,
            entity_power_statuses: &sim.power.entity_statuses,
            fluid_networks: &sim.fluids.networks,
            fluid_topology_dirty: sim.fluids.topology_dirty,
            heat_networks: &sim.heat.networks,
            heat_topology_dirty: sim.heat.topology_dirty,
            robot_networks: &sim.robots.networks,
            robot_logistic_work: &sim.robots.logistic_work,
            robot_flights: &sim.robot_flights,
            rolling_stock: &sim.rolling_stock,
            pending_train_route_searches: &sim.train_routing.pending,
            pollution: &sim.pollution,
            enemies: &sim.enemies,
            transport: &sim.transport,
            enemy_navigation: &sim.enemy_navigation,
            attack_targets: &sim.attack_targets,
            chunks: &sim.world.chunks,
        }
    }

    pub(super) fn from_snapshot(state: &'a SimulationSnapshotOwned) -> Self {
        Self {
            tick: state.tick,
            world_seed: state.world_seed,
            day_night_cycle: &state.day_night_cycle,
            config: &state.config,
            entity_topology_revision: state.entity_topology_revision,
            world_chunk_revision: state.world_chunk_revision,
            world_walkability_revision: state.world_walkability_revision,
            prototypes: &state.prototypes,
            chart: &state.chart,
            chunk_generation_queue: &state.chunk_generation_queue,
            item_statistics: &state.item_statistics,
            fluid_statistics: &state.fluid_statistics,
            power_statistics: &state.power_statistics,
            rockets_launched: state.rockets_launched,
            player_deaths: state.player_deaths,
            entities: &state.entities,
            construction: &state.construction,
            player: &state.player,
            player_equipment: &state.player_equipment,
            player_weapon: &state.player_weapon,
            delayed_combat: &state.delayed_combat,
            player_inventory: &state.player_inventory,
            corpses: &state.corpses,
            manual_mining_progress: &state.manual_mining_progress,
            crafting_queue: &state.crafting_queue,
            onboarding_progress: &state.onboarding_progress,
            research: &state.research,
            power_summary: &state.power_summary,
            power_networks: &state.power_networks,
            entity_power_statuses: &state.entity_power_statuses,
            fluid_networks: &state.fluid_networks,
            fluid_topology_dirty: state.fluid_topology_dirty,
            heat_networks: &state.heat_networks,
            heat_topology_dirty: state.heat_topology_dirty,
            robot_networks: &state.robot_networks,
            robot_logistic_work: &state.robot_logistic_work,
            robot_flights: &state.robot_flights,
            rolling_stock: &state.rolling_stock,
            pending_train_route_searches: &state.pending_train_route_searches,
            pollution: &state.pollution,
            enemies: &state.enemies,
            transport: &state.transport,
            enemy_navigation: &state.enemy_navigation,
            attack_targets: &state.attack_targets,
            chunks: &state.chunks,
        }
    }

    /// Lists every record key: the registry globals plus one per world chunk.
    pub(super) fn keys(&self) -> Vec<String> {
        let mut keys = Vec::with_capacity(REQUIRED_GLOBAL_KEYS.len() + self.chunks.len());
        keys.extend(REQUIRED_GLOBAL_KEYS.iter().map(|key| key.to_string()));
        keys.extend(self.chunks.keys().map(|coord| chunk_key(*coord)));
        keys
    }
}

/// Measures one record's encoded size without allocating its bytes.
pub(super) fn measure_tuple(
    value: &impl serde::Serialize,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    let bound = limits.max_record_bytes.min(limits.max_decoded_bytes);
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(bound)
        .serialized_size(value)
        .map_err(SaveLoadError::from)
}

/// Encodes the borrowed group for `key`. Every registry key must dispatch
/// here; chunk records resolve through the world map.
pub(super) fn encode_record(
    fields: &BorrowedRecordFields<'_>,
    key: &str,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    match key {
        KEY_CORE => core::encode(fields, limits),
        KEY_PROTOTYPES => singles::encode_prototypes(fields, limits),
        KEY_CHART => singles::encode_chart(fields, limits),
        KEY_CHUNK_QUEUE => singles::encode_chunk_queue(fields, limits),
        KEY_STATISTICS => statistics::encode(fields, limits),
        KEY_ENTITIES => singles::encode_entities(fields, limits),
        KEY_CONSTRUCTION => singles::encode_construction(fields, limits),
        KEY_PLAYER => player::encode(fields, limits),
        KEY_POWER => networks::encode_power(fields, limits),
        KEY_FLUIDS => networks::encode_fluids(fields, limits),
        KEY_HEAT => networks::encode_heat(fields, limits),
        KEY_ROBOTS => robots::encode(fields, limits),
        KEY_TRAINS => trains::encode(fields, limits),
        KEY_ENVIRONMENT => environment::encode(fields, limits),
        _ => match parse_chunk_key(key).and_then(|coord| fields.chunks.get(&coord)) {
            Some(chunk) => chunks::encode(chunk, limits),
            None => Err(record_error(format!(
                "record codec has no payload for key {key:?}"
            ))),
        },
    }
}

/// Measures the borrowed group for `key`, sharing the dispatch above so
/// sizing and encoding can never diverge.
pub(super) fn measure_record(
    fields: &BorrowedRecordFields<'_>,
    key: &str,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    match key {
        KEY_CORE => core::measure(fields, limits),
        KEY_PROTOTYPES => singles::measure_prototypes(fields, limits),
        KEY_CHART => singles::measure_chart(fields, limits),
        KEY_CHUNK_QUEUE => singles::measure_chunk_queue(fields, limits),
        KEY_STATISTICS => statistics::measure(fields, limits),
        KEY_ENTITIES => singles::measure_entities(fields, limits),
        KEY_CONSTRUCTION => singles::measure_construction(fields, limits),
        KEY_PLAYER => player::measure(fields, limits),
        KEY_POWER => networks::measure_power(fields, limits),
        KEY_FLUIDS => networks::measure_fluids(fields, limits),
        KEY_HEAT => networks::measure_heat(fields, limits),
        KEY_ROBOTS => robots::measure(fields, limits),
        KEY_TRAINS => trains::measure(fields, limits),
        KEY_ENVIRONMENT => environment::measure(fields, limits),
        _ => match parse_chunk_key(key).and_then(|coord| fields.chunks.get(&coord)) {
            Some(chunk) => chunks::measure(chunk, limits),
            None => Err(record_error(format!(
                "record codec has no payload for key {key:?}"
            ))),
        },
    }
}

/// Bounds every planned record's encoded size from borrowed state, before
/// any cloning: per-record, aggregate decoded, and framed artifact totals.
/// An oversized world fails here, while the simulation read lock can be
/// released without duplicating the world first.
pub(super) fn preflight_record_sizes(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    let keys = fields.keys();
    if keys.len() > MAX_RECORD_COUNT as usize
        || u64::try_from(keys.len()).unwrap_or(u64::MAX) > limits.max_collection_entries
    {
        return Err(SaveLoadError::TooLarge);
    }
    let mut decoded_total = 0u64;
    for key in &keys {
        let size = measure_record(fields, key, limits)?;
        decoded_total = decoded_total
            .checked_add(size)
            .ok_or(SaveLoadError::TooLarge)?;
    }
    if decoded_total > limits.max_decoded_bytes {
        return Err(SaveLoadError::TooLarge);
    }
    // Entry framing is 72 bytes plus the key, so the complete artifact total
    // is known without encoding anything.
    let manifest_len: u64 = keys.iter().map(|key| 72 + key.len() as u64).sum();
    let total = (RECORD_HEADER_SIZE as u64)
        .checked_add(manifest_len)
        .and_then(|framing| framing.checked_add(decoded_total))
        .ok_or(SaveLoadError::TooLarge)?;
    if total > limits.max_encoded_bytes {
        return Err(SaveLoadError::TooLarge);
    }
    Ok(())
}

/// Encodes one record payload from a captured snapshot.
pub(super) fn encode_group_by_key(
    state: &SimulationSnapshotOwned,
    key: &str,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    encode_record(&BorrowedRecordFields::from_snapshot(state), key, limits)
}

/// Decoded record slots: one generation under assembly.
///
/// Each payload is decoded into its slot immediately after its checksum
/// verifies, and the payload bytes are dropped before the next record reads.
/// Retained decode memory is the manifest plus decoded state, never decoded
/// state plus every encoded payload at once.
#[derive(Default)]
pub(super) struct PartialSnapshot {
    pub(super) core: Option<core::CoreTuple>,
    pub(super) prototypes: Option<PrototypeCatalog>,
    pub(super) chart: Option<ChartState>,
    pub(super) chunk_queue: Option<ChunkGenerationQueue>,
    pub(super) statistics: Option<statistics::StatisticsTuple>,
    pub(super) entities: Option<EntityStore>,
    pub(super) construction: Option<ConstructionState>,
    pub(super) player: Option<player::PlayerTuple>,
    pub(super) power: Option<networks::PowerTuple>,
    pub(super) fluids: Option<networks::FluidsTuple>,
    pub(super) heat: Option<networks::HeatTuple>,
    pub(super) robots: Option<robots::RobotsTuple>,
    pub(super) trains: Option<trains::TrainsTuple>,
    pub(super) environment: Option<environment::EnvironmentTuple>,
    pub(super) chunks: BTreeMap<ChunkCoord, Chunk>,
}

/// Enforces the identity codec for records this build must decode. Unknown
/// optional records skip this check: their bounds and checksums were already
/// verified, and their payloads are never decoded.
pub(super) fn check_identity_codec(entry: &ManifestEntry) -> Result<(), SaveLoadError> {
    if entry.codec_id != RECORD_CODEC_IDENTITY {
        return Err(record_error(format!(
            "record {:?} uses unsupported codec {} (supported: {RECORD_CODEC_IDENTITY})",
            entry.key, entry.codec_id
        )));
    }
    if entry.decoded_len != entry.encoded_len {
        return Err(record_error(format!(
            "record {:?} declares mismatched encoded and decoded lengths for the identity codec",
            entry.key
        )));
    }
    Ok(())
}

pub(super) fn check_record_schema(entry: &ManifestEntry) -> Result<(), SaveLoadError> {
    if entry.schema_version != RECORD_SCHEMA_VERSION {
        return Err(record_error(format!(
            "record {:?} uses unsupported schema version {} (supported: {RECORD_SCHEMA_VERSION})",
            entry.key, entry.schema_version
        )));
    }
    Ok(())
}

/// Decodes one verified payload into its assembly slot.
///
/// Unknown records annotated optional are skipped after their bounds and
/// checksums were verified; unknown records annotated required abort the
/// load.
pub(super) fn decode_into_slot(
    partial: &mut PartialSnapshot,
    entry: &ManifestEntry,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    let known = is_global_key(&entry.key) || parse_chunk_key(&entry.key).is_some();
    if known {
        check_identity_codec(entry)?;
        check_record_schema(entry)?;
    }
    match entry.key.as_str() {
        KEY_CORE => core::decode(partial, &entry.key, payload, limits),
        KEY_PROTOTYPES => singles::decode_prototypes(partial, &entry.key, payload, limits),
        KEY_CHART => singles::decode_chart(partial, &entry.key, payload, limits),
        KEY_CHUNK_QUEUE => singles::decode_chunk_queue(partial, &entry.key, payload, limits),
        KEY_STATISTICS => statistics::decode(partial, &entry.key, payload, limits),
        KEY_ENTITIES => singles::decode_entities(partial, &entry.key, payload, limits),
        KEY_CONSTRUCTION => singles::decode_construction(partial, &entry.key, payload, limits),
        KEY_PLAYER => player::decode(partial, &entry.key, payload, limits),
        KEY_POWER => networks::decode_power(partial, &entry.key, payload, limits),
        KEY_FLUIDS => networks::decode_fluids(partial, &entry.key, payload, limits),
        KEY_HEAT => networks::decode_heat(partial, &entry.key, payload, limits),
        KEY_ROBOTS => robots::decode(partial, &entry.key, payload, limits),
        KEY_TRAINS => trains::decode(partial, &entry.key, payload, limits),
        KEY_ENVIRONMENT => environment::decode(partial, &entry.key, payload, limits),
        _ => chunks::decode(partial, entry, payload, limits),
    }
}

/// Assembles one complete snapshot generation from decoded slots.
///
/// Missing required records and header/core identity mismatches abort before
/// any world is built, so every record resolves to one generation.
pub(super) fn assemble_snapshot(
    header: &RecordHeader,
    partial: PartialSnapshot,
) -> Result<SimulationSnapshotOwned, SaveLoadError> {
    let missing = |key: &str| {
        record_error(format!(
            "record container is missing required record {key:?}"
        ))
    };
    let core = partial.core.ok_or_else(|| missing(KEY_CORE))?;
    // Every record resolves to the single generation named by the header.
    if core.0 != header.tick || core.1 != header.world_seed {
        return Err(record_error(
            "record container mixes snapshot generations between its header and core record",
        ));
    }
    let statistics = partial.statistics.ok_or_else(|| missing(KEY_STATISTICS))?;
    let player = partial.player.ok_or_else(|| missing(KEY_PLAYER))?;
    let power = partial.power.ok_or_else(|| missing(KEY_POWER))?;
    let fluids = partial.fluids.ok_or_else(|| missing(KEY_FLUIDS))?;
    let heat = partial.heat.ok_or_else(|| missing(KEY_HEAT))?;
    let robots = partial.robots.ok_or_else(|| missing(KEY_ROBOTS))?;
    let trains = partial.trains.ok_or_else(|| missing(KEY_TRAINS))?;
    let environment = partial
        .environment
        .ok_or_else(|| missing(KEY_ENVIRONMENT))?;

    Ok(SimulationSnapshotOwned {
        tick: core.0,
        day_night_cycle: core.2,
        world_seed: core.1,
        prototypes: partial.prototypes.ok_or_else(|| missing(KEY_PROTOTYPES))?,
        chunks: partial.chunks,
        chunk_generation_queue: partial
            .chunk_queue
            .ok_or_else(|| missing(KEY_CHUNK_QUEUE))?,
        chart: partial.chart.ok_or_else(|| missing(KEY_CHART))?,
        item_statistics: statistics.0,
        fluid_statistics: statistics.1,
        power_statistics: statistics.2,
        rockets_launched: statistics.3,
        player_deaths: statistics.4,
        entities: partial.entities.ok_or_else(|| missing(KEY_ENTITIES))?,
        construction: partial
            .construction
            .ok_or_else(|| missing(KEY_CONSTRUCTION))?,
        player: player.0,
        player_equipment: player.1,
        player_weapon: player.2,
        delayed_combat: player.3,
        player_inventory: player.4,
        corpses: player.5,
        manual_mining_progress: player.6,
        crafting_queue: player.7,
        onboarding_progress: player.8,
        research: player.9,
        power_summary: power.0,
        power_networks: power.1,
        entity_power_statuses: power.2,
        fluid_networks: fluids.0,
        fluid_topology_dirty: fluids.1,
        heat_networks: heat.0,
        heat_topology_dirty: heat.1,
        robot_networks: robots.0,
        robot_logistic_work: robots.1,
        robot_flights: robots.2,
        rolling_stock: trains.0,
        pending_train_route_searches: trains.1,
        pollution: environment.0,
        enemies: environment.1,
        config: core.3,
        entity_topology_revision: core.4,
        world_chunk_revision: core.5,
        world_walkability_revision: core.6,
        transport: environment.2,
        enemy_navigation: environment.3,
        attack_targets: environment.4,
    })
}

/// Decodes only the tick from a core payload for tests, so test code does
/// not need to name the wire tuple type.
#[cfg(test)]
pub(super) fn decoded_core_tick(
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    let tuple: core::CoreTuple = decode_group("core", payload, limits)?;
    Ok(tuple.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_and_dispatch_agree() {
        // Every registry key must encode and measure from live state, and no
        // dispatch arm may exist outside the registry.
        let sim = Simulation::new_test_world(11);
        let fields = BorrowedRecordFields::from_simulation(&sim);
        let limits = crate::SaveLimits::default();
        for entry in RECORD_REGISTRY {
            let payload = encode_record(&fields, entry.key, limits)
                .unwrap_or_else(|_| panic!("registry key {:?} must encode", entry.key));
            let size = measure_record(&fields, entry.key, limits)
                .unwrap_or_else(|_| panic!("registry key {:?} must measure", entry.key));
            assert_eq!(
                size,
                payload.len() as u64,
                "measured and encoded sizes must agree for {:?}",
                entry.key
            );
        }
        assert!(encode_record(&fields, "no-such-record", limits).is_err());
        assert!(measure_record(&fields, "no-such-record", limits).is_err());
    }
}
