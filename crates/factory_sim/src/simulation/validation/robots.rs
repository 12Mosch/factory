use super::super::*;
use crate::construction::ConstructionJob;
use crate::robots::{RobotActivity, TileBounds};
use crate::simulation::robot_ops::coverage_bounds;
use factory_data::RobotKind;

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
    validate_inventory_policy(sim, entity_id, &state.robots, ItemSlotPolicy::Robot)?;
    validate_inventory_policy(
        sim,
        entity_id,
        &state.materials,
        ItemSlotPolicy::ConstructionMaterial,
    )?;
    Ok(())
}

/// Checks one chest's logistic rows against the role its prototype declares.
///
/// The row count is fixed at placement from the prototype, so a mismatch means
/// the save and the catalog disagree about the chest's shape. The per-row rules
/// mirror what [`crate::Simulation::set_logistic_request`] enforces, which is
/// what stops a hand-edited save from parking an amount on a filter row or
/// asking for more than the chest could ever hold.
pub(in crate::simulation) fn validate_logistic_chest(
    sim: &Simulation,
    entity_id: EntityId,
    state: &LogisticChestState,
) -> Result<(), SimValidationError> {
    let invalid = SimValidationError::InvalidLogisticChestState { entity_id };
    let prototype = sim
        .entities
        .placed_entity(entity_id)
        .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
        .filter(|prototype| prototype.entity_kind == EntityKind::Chest)
        .ok_or(invalid)?;
    let logistic_chest = prototype.logistic_chest.ok_or(invalid)?;

    if state.requests.len() != usize::from(logistic_chest.request_slot_count) {
        return Err(invalid);
    }
    for request in &state.requests {
        let Some(item_id) = request.item else {
            // An unset row asks for nothing, so an amount on it would be a
            // request no code could ever act on.
            if request.count != 0 {
                return Err(invalid);
            }
            continue;
        };
        let capacity = sim
            .logistic_request_capacity(prototype, item_id)
            .ok_or(SimValidationError::InvalidMachineItem { entity_id, item_id })?;
        let allowed = if logistic_chest.mode.requests_items() {
            capacity
        } else {
            // A storage chest's row is a filter; an amount on it would mean
            // nothing.
            0
        };
        if request.count > allowed {
            return Err(invalid);
        }
    }
    Ok(())
}

/// Rejects contents no insertion path could have produced.
///
/// The two roboport inventories accept disjoint item sets, and every way in
/// (player transfer, inserter drop) enforces that. Checking it again here is
/// what stops a corrupt or hand-edited save from parking repair packs in the
/// robot slots — catalog-valid stacks that the policies would never admit.
fn validate_inventory_policy(
    sim: &Simulation,
    entity_id: EntityId,
    inventory: &Inventory,
    policy: ItemSlotPolicy,
) -> Result<(), SimValidationError> {
    for slot in inventory.slots() {
        let Some(stack) = slot.stack() else {
            continue;
        };
        if !item_slot_policy_accepts(
            &sim.world.prototypes,
            &sim.research,
            &sim.entities,
            policy,
            ItemSlotOperation::PlayerInsert,
            stack.item_id(),
        ) {
            return Err(SimValidationError::InvalidRoboportState { entity_id });
        }
    }
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
    let work_counts = crate::simulation::robot_ops::robot_network_work_counts(sim);

    for (expected_network_id, network) in sim.robots.networks.iter().enumerate() {
        let invalid = || SimValidationError::InvalidRobotNetwork {
            network_id: network.network_id,
        };
        if network.network_id != expected_network_id as u32
            || network.roboports.is_empty()
            || network.charge_energy_joules > network.charge_capacity_joules
            || work_counts
                .get(expected_network_id)
                .is_none_or(|(available, total, jobs)| {
                    network.available_construction_robots != *available
                        || network.total_construction_robots != *total
                        || network.jobs != *jobs
                })
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

/// Checks the robots in flight and the charging state they are registered in.
///
/// The two halves reference each other — a robot names the roboport it charges
/// at, and that roboport's pads name the robot — so both directions are checked
/// here. A one-sided reference is exactly what a mishandled roboport
/// destruction would leave behind, and nothing later in the tick would notice.
pub(super) fn validate_robot_flights(sim: &Simulation) -> Result<(), SimValidationError> {
    for (robot_id, robot) in &sim.robot_flights.robots {
        let invalid = || SimValidationError::InvalidRobotState {
            robot_id: *robot_id,
        };
        if robot.id != *robot_id || robot_id.raw() > sim.robot_flights.next_robot_id {
            return Err(invalid());
        }
        let profile = sim
            .world
            .prototypes
            .item(robot.item_id)
            .and_then(|item| item.robot)
            .ok_or_else(invalid)?;
        if robot.energy_joules > profile.energy_capacity_joules {
            return Err(invalid());
        }
        for stack in robot.cargo.iter().chain(robot.payload.iter()) {
            if ItemStack::new(&sim.world.prototypes, stack.item_id(), stack.count()).is_err() {
                return Err(invalid());
            }
        }
        match robot.construction_job {
            None if robot.payload.is_some() => return Err(invalid()),
            None => {}
            Some(job) => {
                if profile.kind != RobotKind::Construction
                    || sim.construction.reservations.get(&job) != Some(robot_id)
                {
                    return Err(invalid());
                }
                let payload_valid = match job {
                    ConstructionJob::BuildGhost(ghost_id) => {
                        let expected = sim.construction.ghosts.get(&ghost_id).and_then(|ghost| {
                            crate::simulation::entity_recovery_ops::build_item_for_entity(
                                sim,
                                ghost.prototype_id,
                            )
                            .ok()
                        });
                        robot.payload.is_some_and(|payload| {
                            Some(payload.item_id()) == expected && payload.count() == 1
                        })
                    }
                    ConstructionJob::Deconstruct(_) => robot.payload.is_none(),
                    ConstructionJob::Repair(_) => robot.payload.is_some_and(|payload| {
                        payload.count() == 1
                            && sim
                                .world
                                .prototypes
                                .item(payload.item_id())
                                .is_some_and(|item| item.repair.is_some())
                    }),
                };
                if !payload_valid {
                    return Err(invalid());
                }
            }
        }
        if robot
            .home_roboport
            .is_some_and(|entity_id| !sim.entities.roboports.contains_key(&entity_id))
        {
            return Err(invalid());
        }

        let registered = match robot.activity {
            RobotActivity::Flying => true,
            RobotActivity::SeekingCharge(roboport) => {
                sim.entities.roboports.contains_key(&roboport)
            }
            RobotActivity::Queued(roboport) => sim
                .robot_flights
                .charging
                .get(&roboport)
                .is_some_and(|state| state.queue.contains(robot_id)),
            RobotActivity::Charging(roboport) => sim
                .robot_flights
                .charging
                .get(&roboport)
                .is_some_and(|state| state.charging.contains(robot_id)),
        };
        if !registered {
            return Err(invalid());
        }
    }

    for (entity_id, state) in &sim.robot_flights.charging {
        let invalid = || SimValidationError::InvalidRoboportChargingState {
            entity_id: *entity_id,
        };
        let roboport = sim
            .entities
            .placed_entity(*entity_id)
            .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
            .and_then(|prototype| prototype.roboport)
            .filter(|_| sim.entities.roboports.contains_key(entity_id))
            .ok_or_else(invalid)?;
        if state.charging.len() > usize::from(roboport.charging_pad_count) {
            return Err(invalid());
        }
        for robot_id in &state.charging {
            if sim
                .robot_flights
                .robots
                .get(robot_id)
                .is_none_or(|robot| robot.activity != RobotActivity::Charging(*entity_id))
            {
                return Err(invalid());
            }
        }
        for robot_id in &state.queue {
            if state.charging.contains(robot_id)
                || sim
                    .robot_flights
                    .robots
                    .get(robot_id)
                    .is_none_or(|robot| robot.activity != RobotActivity::Queued(*entity_id))
            {
                return Err(invalid());
            }
        }
        // A queue that lists the same robot twice would serve it twice and
        // leave a pad claimed by a robot that already left.
        let mut seen = BTreeSet::new();
        if state.queue.iter().any(|robot_id| !seen.insert(*robot_id)) {
            return Err(invalid());
        }
    }

    Ok(())
}
