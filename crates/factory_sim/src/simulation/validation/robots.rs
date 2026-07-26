use super::super::*;
use crate::robots::TileBounds;
use crate::simulation::robot_ops::coverage_bounds;

/// Checks one roboport's durable state against its prototype: the two
/// inventories must have the declared slot counts and valid contents, and the
/// charging buffer must not exceed the capacity it fills toward.
///
/// Slot counts are part of the check because they are set once at placement
/// from the prototype; a mismatch means the state and the catalog disagree
/// about how large the roboport is, which no later code would notice.
pub(in crate::simulation) fn validate_roboport(
    sim: &Simulation,
    entity_id: EntityId,
    state: &RoboportState,
) -> Result<(), SimValidationError> {
    let roboport = sim
        .entities
        .placed_entity(entity_id)
        .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
        .filter(|prototype| prototype.entity_kind == EntityKind::Roboport)
        .and_then(|prototype| prototype.roboport)
        .ok_or(SimValidationError::InvalidEntityState { entity_id })?;

    if state.robots.slots().len() != roboport.robot_slot_count
        || state.materials.slots().len() != roboport.material_slot_count
        || state.charge_energy_joules > roboport.charging_energy_buffer_joules
    {
        return Err(SimValidationError::InvalidRoboportState { entity_id });
    }

    super::inventory::validate_inventory(&sim.world.prototypes, &state.robots)?;
    super::inventory::validate_inventory(&sim.world.prototypes, &state.materials)?;
    Ok(())
}

/// Checks the durable robot-network snapshots against the roboports they
/// summarize: dense ids, every roboport in exactly one network, and coverage
/// bounds and charge totals that follow from the members.
///
/// A dirty topology means the snapshots were discarded by an invalidation and
/// have not been rebuilt yet, so there is nothing to check against — the next
/// robot pass restores them.
pub(super) fn validate_robot_network_snapshots(sim: &Simulation) -> Result<(), SimValidationError> {
    if sim.robots.topology_dirty {
        return Ok(());
    }

    let expected_roboports = sim
        .entities
        .roboports
        .keys()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut networked_roboports = BTreeSet::new();

    for (expected_network_id, network) in sim.robots.networks.iter().enumerate() {
        let invalid = || SimValidationError::InvalidRobotNetwork {
            network_id: network.network_id,
        };
        if network.network_id != expected_network_id as u32
            || network.roboports.is_empty()
            || network.charge_energy_joules > network.charge_capacity_joules
        {
            return Err(invalid());
        }

        let mut construction_bounds: Option<TileBounds> = None;
        let mut logistic_bounds: Option<TileBounds> = None;
        let mut charge_energy_joules = 0_u64;
        let mut charge_capacity_joules = 0_u64;

        for member in &network.roboports {
            if !networked_roboports.insert(member.entity_id) {
                return Err(invalid());
            }

            let placed = sim
                .entities
                .placed_entity(member.entity_id)
                .ok_or_else(invalid)?;
            let roboport = sim
                .world
                .prototypes
                .entity(placed.prototype_id)
                .and_then(|prototype| prototype.roboport)
                .ok_or_else(invalid)?;
            let state = sim
                .entities
                .roboports
                .get(&member.entity_id)
                .ok_or_else(invalid)?;

            let construction =
                coverage_bounds(placed.footprint, roboport.construction_radius_tiles);
            let logistic = coverage_bounds(placed.footprint, roboport.logistic_radius_tiles);
            if member.construction_bounds != construction
                || member.logistic_bounds != logistic
                || member.charge_energy_joules != state.charge_energy_joules
                || member.charge_capacity_joules != roboport.charging_energy_buffer_joules
            {
                return Err(invalid());
            }

            construction_bounds = Some(match construction_bounds {
                Some(bounds) => bounds.union(construction),
                None => construction,
            });
            logistic_bounds = Some(match logistic_bounds {
                Some(bounds) => bounds.union(logistic),
                None => logistic,
            });
            charge_energy_joules = charge_energy_joules.saturating_add(state.charge_energy_joules);
            charge_capacity_joules =
                charge_capacity_joules.saturating_add(roboport.charging_energy_buffer_joules);
        }

        if construction_bounds != Some(network.construction_bounds)
            || logistic_bounds != Some(network.logistic_bounds)
            || charge_energy_joules != network.charge_energy_joules
            || charge_capacity_joules != network.charge_capacity_joules
        {
            return Err(invalid());
        }
    }

    // A roboport no network claims (or one claimed twice over) is a property of
    // that roboport, not of any one network, so report the roboport itself.
    if let Some(entity_id) = expected_roboports
        .symmetric_difference(&networked_roboports)
        .next()
    {
        return Err(SimValidationError::InvalidRoboportState {
            entity_id: *entity_id,
        });
    }

    Ok(())
}
