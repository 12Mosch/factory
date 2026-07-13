use super::super::*;

pub(super) fn validate_entity_occupancy(entities: &EntityStore) -> Result<(), SimValidationError> {
    let mut expected = BTreeMap::new();

    for placed in entities.placed_entities.values() {
        for (x, y) in placed.footprint.tiles() {
            if let Some(first) = expected.insert((x, y), placed.id) {
                return Err(SimValidationError::EntityOverlap {
                    x,
                    y,
                    first,
                    second: placed.id,
                });
            }
        }
    }

    if expected != entities.occupancy.occupied_tiles {
        return Err(SimValidationError::OccupancyMismatch);
    }

    Ok(())
}

macro_rules! ownership_check {
    // Auxiliary state maps (`_` entries) are shared across kinds, so they
    // only get a generic orphan check here; maps with kind-specific owner
    // rules additionally need a dedicated check below.
    ($sim:ident, $field:ident, _) => {
        for entity_id in $sim.entities.$field.keys() {
            if !$sim.entities.placed_entities.contains_key(entity_id) {
                return Err(SimValidationError::OrphanEntityState(*entity_id));
            }
        }
    };
    ($sim:ident, $field:ident, $kind:ident) => {
        for entity_id in $sim.entities.$field.keys() {
            validate_entity_state_kind($sim, *entity_id, EntityKind::$kind)?;
        }
    };
}

macro_rules! define_validate_entity_state_ownership {
    ($($field:ident : $ty:ty => $kind:tt),* $(,)?) => {
        pub(super) fn validate_entity_state_ownership_and_kind(
            sim: &Simulation,
        ) -> Result<(), SimValidationError> {
            $(ownership_check!(sim, $field, $kind);)*
            for entity_id in sim.entities.electric_consumers.keys() {
                validate_electric_consumer_owner(sim, *entity_id)?;
            }
            for entity_id in sim.entities.fluid_boxes.keys() {
                validate_fluid_box_owner(sim, *entity_id)?;
            }
            for entity_id in sim.entities.entity_health.keys() {
                validate_health_owner(sim, *entity_id)?;
            }

            Ok(())
        }
    };
}
for_each_entity_state_map!(define_validate_entity_state_ownership);

fn owner_prototype(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<&factory_data::EntityPrototype, SimValidationError> {
    let placed = sim
        .entities
        .placed_entities
        .get(&entity_id)
        .ok_or(SimValidationError::OrphanEntityState(entity_id))?;
    sim.world.prototypes.entity(placed.prototype_id).ok_or(
        SimValidationError::InvalidEntityPrototype {
            entity_id,
            prototype_id: placed.prototype_id,
        },
    )
}

fn validate_fluid_box_owner(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<(), SimValidationError> {
    let prototype = owner_prototype(sim, entity_id)?;

    if prototype.fluid_boxes.is_empty() {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }

    Ok(())
}

fn validate_electric_consumer_owner(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<(), SimValidationError> {
    let prototype = owner_prototype(sim, entity_id)?;

    if prototype.electric_energy_source.is_none() {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }

    Ok(())
}

fn validate_health_owner(sim: &Simulation, entity_id: EntityId) -> Result<(), SimValidationError> {
    let prototype = owner_prototype(sim, entity_id)?;

    if prototype.max_health.is_none() {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }

    Ok(())
}

/// Enemy units are self-contained; validate their copied combat stats stay
/// coherent.
pub(super) fn validate_enemies(sim: &Simulation) -> Result<(), SimValidationError> {
    if !sim.config.is_valid()
        || sim.enemies.evolution_points > 10_000
        || sim.enemies.threat_events.len() > 256
    {
        return Err(SimValidationError::InvalidEnemyState);
    }
    let mut last_sequence = 0;
    for event in &sim.enemies.threat_events {
        if event.sequence <= last_sequence || event.sequence > sim.enemies.threat_sequence {
            return Err(SimValidationError::InvalidEnemyState);
        }
        last_sequence = event.sequence;
    }
    if sim
        .enemies
        .bases
        .keys()
        .next_back()
        .is_some_and(|id| id.raw() > sim.enemies.next_base_id)
        || sim
            .enemies
            .raids
            .keys()
            .next_back()
            .is_some_and(|id| id.raw() > sim.enemies.next_raid_id)
        || sim
            .enemies
            .expansions
            .keys()
            .next_back()
            .is_some_and(|id| id.raw() > sim.enemies.next_expansion_id)
    {
        return Err(SimValidationError::InvalidEnemyState);
    }
    for (spawner, base_id) in &sim.enemies.spawner_bases {
        if !sim.entities.enemy_spawners.contains_key(spawner)
            || !sim
                .enemies
                .bases
                .get(base_id)
                .is_some_and(|base| base.spawners.contains(spawner))
        {
            return Err(SimValidationError::InvalidEnemyState);
        }
    }
    // The reverse direction: every spawner with runtime state must belong to
    // a base (the loop above then guarantees that base lists it back).
    for spawner in sim.entities.enemy_spawners.keys() {
        if !sim.enemies.spawner_bases.contains_key(spawner) {
            return Err(SimValidationError::InvalidEnemyState);
        }
    }
    // `grouped` spans staged, raid, and expansion membership: a unit may be
    // claimed by at most one group across all three.
    let mut grouped = BTreeSet::new();
    for base in sim.enemies.bases.values() {
        if base.spawners.is_empty()
            || base
                .spawners
                .iter()
                .any(|spawner| sim.enemies.spawner_bases.get(spawner) != Some(&base.id))
        {
            return Err(SimValidationError::InvalidEnemyState);
        }
        let Some(attack_budget_cap) = sim.attack_budget_cap(base.id) else {
            return Err(SimValidationError::InvalidEnemyState);
        };
        if base.attack_budget_micro > attack_budget_cap {
            return Err(SimValidationError::AttackBudgetCapacityExceeded { base_id: base.id });
        }
        for id in &base.staged_units {
            if !grouped.insert(*id)
                || !sim
                    .enemies
                    .enemies
                    .get(id)
                    .is_some_and(|unit| unit.mission == EnemyMission::Staging(base.id))
            {
                return Err(SimValidationError::InvalidEnemyState);
            }
        }
    }
    for raid in sim.enemies.raids.values() {
        for id in &raid.members {
            if !grouped.insert(*id)
                || !sim
                    .enemies
                    .enemies
                    .get(id)
                    .is_some_and(|unit| unit.mission == EnemyMission::Raid(raid.id))
            {
                return Err(SimValidationError::InvalidEnemyState);
            }
        }
    }
    for party in sim.enemies.expansions.values() {
        if ChunkCoord::from_tile(party.destination.0, party.destination.1).is_none() {
            return Err(SimValidationError::InvalidEnemyState);
        }
        for id in &party.members {
            if !grouped.insert(*id)
                || !sim
                    .enemies
                    .enemies
                    .get(id)
                    .is_some_and(|unit| unit.mission == EnemyMission::Expansion(party.id))
            {
                return Err(SimValidationError::InvalidEnemyState);
            }
        }
    }
    for (id, enemy) in &sim.enemies.enemies {
        if enemy.id != *id
            || enemy.health.current == 0
            || enemy.health.current > enemy.health.maximum
            || enemy.health.faction != Faction::Enemy
            || !enemy.health.resistances.is_valid()
            || !enemy.attack.is_valid()
            || enemy.speed_fixed_per_tick == 0
            || enemy.id.raw() > sim.enemies.next_enemy_id
        {
            return Err(SimValidationError::InvalidEnemy { enemy_id: *id });
        }
        let mission_valid = match enemy.mission {
            EnemyMission::Guard => true,
            EnemyMission::Staging(base) => sim
                .enemies
                .bases
                .get(&base)
                .is_some_and(|state| state.staged_units.contains(id)),
            EnemyMission::Raid(raid) => sim
                .enemies
                .raids
                .get(&raid)
                .is_some_and(|state| state.members.contains(id)),
            EnemyMission::Expansion(expansion) => sim
                .enemies
                .expansions
                .get(&expansion)
                .is_some_and(|state| state.members.contains(id)),
        };
        if !mission_valid {
            return Err(SimValidationError::InvalidEnemyState);
        }
    }

    Ok(())
}

fn validate_entity_state_kind(
    sim: &Simulation,
    entity_id: EntityId,
    expected_kind: EntityKind,
) -> Result<(), SimValidationError> {
    let prototype = owner_prototype(sim, entity_id)?;

    if prototype.entity_kind != expected_kind {
        return Err(SimValidationError::InvalidEntityState { entity_id });
    }

    Ok(())
}
