//! Deterministic construction-job reconciliation, dispatch, and arrival work.

use crate::construction::ConstructionJob;
use crate::robots::{Robot, RobotId};
use crate::simulation::combat_ops::repair_restore_health;
use crate::simulation::*;
use factory_data::RobotKind;

use super::dispatch::{commit_robot_dispatch, dispatching_roboport};

pub(super) const CONSTRUCTION_JOB_EXAMINATION_BUDGET: usize = 32;

impl Simulation {
    /// Reconciles durable reservations with live targets and robots, then adds
    /// one repair job for each damaged friendly entity that has no unfinished
    /// repair work.
    pub(in crate::simulation) fn reconcile_construction_jobs(&mut self) {
        let reservations = self
            .construction
            .reservations
            .iter()
            .map(|(job, robot_id)| (*job, *robot_id))
            .collect::<Vec<_>>();
        for (job, robot_id) in reservations {
            let robot_matches = self
                .robot_flights
                .robots
                .get(&robot_id)
                .is_some_and(|robot| robot.construction_job == Some(job));
            let valid = self.construction_job_is_valid(job);
            let covered_by_home = robot_matches
                && self
                    .robot_flights
                    .robots
                    .get(&robot_id)
                    .is_some_and(|robot| self.job_is_covered_by_robot_home(job, robot));
            if robot_matches && valid && covered_by_home {
                continue;
            }

            self.construction.reservations.remove(&job);
            if !valid {
                self.untrack_construction_job(job);
            }
            if let Some(robot) = self.robot_flights.robots.get_mut(&robot_id)
                && robot.construction_job == Some(job)
            {
                abort_robot_job(robot);
                if robot
                    .home_roboport
                    .is_some_and(|home| !self.entities.roboports.contains_key(&home))
                {
                    robot.home_roboport = None;
                }
            }
            if valid {
                self.enqueue_job_once(job);
            }
        }

        let repair_targets = self
            .entities
            .entity_health
            .iter()
            .filter_map(|(entity_id, health)| {
                (health.faction == Faction::Player
                    && health.current < health.maximum
                    && !self.construction.deconstruction_marks.contains(entity_id))
                .then_some(*entity_id)
            })
            .collect::<Vec<_>>();
        for entity_id in repair_targets {
            self.enqueue_job_once(ConstructionJob::Repair(entity_id));
        }
    }

    /// Examines a bounded prefix of the queue. A temporarily blocked job moves
    /// to the tail; stale jobs are discarded; successful jobs become reserved.
    pub(in crate::simulation) fn dispatch_construction_jobs(&mut self) {
        let examinations = self
            .construction
            .queue
            .len()
            .min(CONSTRUCTION_JOB_EXAMINATION_BUDGET);
        for _ in 0..examinations {
            let Some(job) = self.construction.pop_job() else {
                break;
            };
            if !self.construction_job_is_valid(job) {
                self.untrack_construction_job(job);
                continue;
            }
            if !self.try_dispatch_construction_job(job) {
                self.construction.enqueue_job(job);
            }
        }
    }

    pub(in crate::simulation) fn resolve_construction_arrival(&mut self, robot_id: RobotId) {
        let Some(mut robot) = self.robot_flights.robots.remove(&robot_id) else {
            return;
        };
        let Some(job) = robot.construction_job else {
            self.robot_flights.robots.insert(robot_id, robot);
            return;
        };
        if self.construction.reservations.get(&job) != Some(&robot_id) {
            abort_robot_job(&mut robot);
            self.robot_flights.robots.insert(robot_id, robot);
            return;
        }

        match job {
            ConstructionJob::BuildGhost(ghost_id) => {
                self.complete_robot_build(&mut robot, ghost_id);
            }
            ConstructionJob::Deconstruct(entity_id) => {
                self.complete_robot_deconstruction(&mut robot, entity_id);
            }
            ConstructionJob::Repair(entity_id) => {
                self.complete_robot_repair(&mut robot, entity_id);
            }
        }
        self.construction.reservations.remove(&job);
        robot.construction_job = None;
        robot.errand = None;
        if self.construction_job_is_valid(job) {
            self.enqueue_job_once(job);
        } else {
            self.untrack_construction_job(job);
        }
        if robot
            .home_roboport
            .is_some_and(|home| !self.entities.roboports.contains_key(&home))
        {
            robot.home_roboport = None;
        }
        self.robot_flights.robots.insert(robot_id, robot);
    }

    fn complete_robot_build(&mut self, robot: &mut Robot, ghost_id: crate::construction::GhostId) {
        let Some(ghost) = self.construction.ghosts.get(&ghost_id).cloned() else {
            move_payload_to_cargo(robot);
            return;
        };
        let Some(payload) = robot.payload else {
            return;
        };
        let Ok(build_item) = entity_recovery_ops::build_item_for_entity(self, ghost.prototype_id)
        else {
            move_payload_to_cargo(robot);
            return;
        };
        if payload.item_id() != build_item || payload.count() != 1 {
            move_payload_to_cargo(robot);
            return;
        }

        let request = placement::EntityPlacementRequest {
            prototype_id: ghost.prototype_id,
            x: ghost.x,
            y: ghost.y,
            direction: ghost.direction,
        };
        let Ok(entity_id) = placement_mutation_ops::place_entity(self, request) else {
            move_payload_to_cargo(robot);
            return;
        };
        robot.payload = None;
        if let Some(recipe) = ghost.recipe {
            let _ = self.select_assembler_recipe(entity_id, recipe);
        }
    }

    fn complete_robot_deconstruction(&mut self, robot: &mut Robot, entity_id: EntityId) {
        if !self.construction.deconstruction_marks.contains(&entity_id) {
            return;
        }
        let Some(placed) = self.entities.placed_entity(entity_id).cloned() else {
            return;
        };
        let Ok(recovery) = entity_recovery_ops::entity_recovery(self, &placed) else {
            return;
        };
        if entity_mutation::remove(self, entity_id).is_none() {
            return;
        }
        robot.cargo.extend(recovery.stacks);
        robot.bulk_cargo.extend(recovery.bulk_items);
    }

    fn complete_robot_repair(&mut self, robot: &mut Robot, entity_id: EntityId) {
        let Some(payload) = robot.payload else {
            return;
        };
        let Some(restore_health) = repair_restore_health(&self.world.prototypes, payload.item_id())
        else {
            move_payload_to_cargo(robot);
            return;
        };
        if !self.repair_target_is_valid(entity_id) {
            move_payload_to_cargo(robot);
            return;
        }

        self.restore_entity_health(entity_id, restore_health);
        robot.payload = None;
    }

    fn try_dispatch_construction_job(&mut self, job: ConstructionJob) -> bool {
        let Some((target_x, target_y)) = self.construction_job_target_tile(job) else {
            return false;
        };
        // Personal coverage has deterministic precedence in overlaps. It owns
        // only player inventory and never borrows robots or materials from the
        // stationary network below, so either source commits the one shared
        // reservation and the other never sees the job again.
        if self.try_dispatch_personal_construction_job(job, target_x, target_y) {
            return true;
        }
        let Some(network_id) = self.construction_network_covering_tile(target_x, target_y) else {
            return false;
        };
        let mut member_ids = std::mem::take(&mut self.robots.charging_scratch);
        member_ids.clear();
        let Some(network) = self.robots.topology_networks.get(network_id as usize) else {
            self.robots.charging_scratch = member_ids;
            return false;
        };
        member_ids.extend(network.roboports.iter().map(|node| node.entity_id));
        let dispatched =
            self.try_dispatch_construction_job_in_network(job, target_x, target_y, &member_ids);
        self.robots.charging_scratch = member_ids;
        dispatched
    }

    fn try_dispatch_personal_construction_job(
        &mut self,
        job: ConstructionJob,
        target_x: WorldTileCoord,
        target_y: WorldTileCoord,
    ) -> bool {
        if self
            .personal_roboport_coverage()
            .is_none_or(|bounds| !bounds.contains(target_x, target_y))
        {
            return false;
        }
        if let ConstructionJob::BuildGhost(ghost_id) = job {
            let Some(ghost) = self.construction.ghosts.get(&ghost_id) else {
                return false;
            };
            if placement_validation_ops::validate_entity_placement(
                self,
                placement::EntityPlacementRequest {
                    prototype_id: ghost.prototype_id,
                    x: ghost.x,
                    y: ghost.y,
                    direction: ghost.direction,
                },
            )
            .is_err()
            {
                return false;
            }
        }

        let payload_item = match job {
            ConstructionJob::BuildGhost(ghost_id) => {
                self.construction.ghosts.get(&ghost_id).and_then(|ghost| {
                    entity_recovery_ops::build_item_for_entity(self, ghost.prototype_id).ok()
                })
            }
            ConstructionJob::Repair(_) => first_player_repair_item(self),
            ConstructionJob::Deconstruct(_) => None,
        };
        if !matches!(job, ConstructionJob::Deconstruct(_))
            && payload_item.is_none_or(|item_id| self.player_inventory.count(item_id) == 0)
        {
            return false;
        }

        let Some((robot_item, profile)) = first_player_construction_robot(self) else {
            return false;
        };
        if self.player_equipment.personal_roboport_energy_joules < profile.energy_capacity_joules {
            return false;
        }

        if let Some(item_id) = payload_item {
            self.player_inventory
                .remove(item_id, 1)
                .expect("the personal construction payload was just counted");
        }
        self.player_inventory
            .remove(robot_item, 1)
            .expect("the personal construction robot was just counted");
        self.player_equipment.personal_roboport_energy_joules -= profile.energy_capacity_joules;

        let id = self.robot_flights.allocate_id();
        let target = (tile_center_fixed(target_x), tile_center_fixed(target_y));
        let payload = payload_item.map(|item_id| {
            ItemStack::new(&self.world.prototypes, item_id, 1)
                .expect("catalog material forms a one-item personal payload")
        });
        self.robot_flights.robots.insert(
            id,
            Robot {
                id,
                item_id: robot_item,
                x: self.player.x,
                y: self.player.y,
                energy_joules: profile.energy_capacity_joules,
                personal: true,
                home_roboport: None,
                errand: Some(target),
                activity: RobotActivity::Flying,
                construction_job: Some(job),
                delivery: None,
                payload,
                cargo: Vec::new(),
                bulk_cargo: Vec::new(),
            },
        );
        self.construction.reservations.insert(job, id);
        true
    }

    fn try_dispatch_construction_job_in_network(
        &mut self,
        job: ConstructionJob,
        target_x: WorldTileCoord,
        target_y: WorldTileCoord,
        member_ids: &[EntityId],
    ) -> bool {
        if let ConstructionJob::BuildGhost(ghost_id) = job {
            let Some(ghost) = self.construction.ghosts.get(&ghost_id) else {
                return false;
            };
            if placement_validation_ops::validate_entity_placement(
                self,
                placement::EntityPlacementRequest {
                    prototype_id: ghost.prototype_id,
                    x: ghost.x,
                    y: ghost.y,
                    direction: ghost.direction,
                },
            )
            .is_err()
            {
                return false;
            }
        }

        let payload_item = match job {
            ConstructionJob::BuildGhost(ghost_id) => {
                self.construction.ghosts.get(&ghost_id).and_then(|ghost| {
                    entity_recovery_ops::build_item_for_entity(self, ghost.prototype_id).ok()
                })
            }
            ConstructionJob::Repair(_) => first_network_repair_item(self, member_ids),
            ConstructionJob::Deconstruct(_) => None,
        };
        if !matches!(job, ConstructionJob::Deconstruct(_)) && payload_item.is_none() {
            return false;
        }
        if payload_item
            .is_some_and(|item_id| network_material_count(self, member_ids, item_id) == 0)
        {
            return false;
        }

        let target_fixed = (tile_center_fixed(target_x), tile_center_fixed(target_y));
        let Some(dispatch) =
            dispatching_roboport(self, member_ids, target_fixed, RobotKind::Construction)
        else {
            return false;
        };

        let payload = match payload_item {
            Some(item_id) => {
                if !withdraw_network_material(self, member_ids, item_id) {
                    return false;
                }
                Some(
                    ItemStack::new(&self.world.prototypes, item_id, 1)
                        .expect("catalog material forms a one-item payload"),
                )
            }
            None => None,
        };

        // All fallible checks are complete. The commit below removes exactly
        // one robot before creating the matching reservation.
        let id = commit_robot_dispatch(self, dispatch, target_fixed, |robot| {
            robot.construction_job = Some(job);
            robot.payload = payload;
        });
        self.construction.reservations.insert(job, id);
        true
    }

    pub(in crate::simulation) fn construction_job_is_valid(&self, job: ConstructionJob) -> bool {
        match job {
            ConstructionJob::BuildGhost(ghost_id) => {
                self.construction.ghosts.contains_key(&ghost_id)
            }
            ConstructionJob::Deconstruct(entity_id) => {
                self.construction.deconstruction_marks.contains(&entity_id)
                    && self.entities.placed_entity(entity_id).is_some()
            }
            ConstructionJob::Repair(entity_id) => self.repair_target_is_valid(entity_id),
        }
    }

    fn repair_target_is_valid(&self, entity_id: EntityId) -> bool {
        !self.construction.deconstruction_marks.contains(&entity_id)
            && self
                .entities
                .entity_health
                .get(&entity_id)
                .is_some_and(|health| {
                    health.faction == Faction::Player && health.current < health.maximum
                })
            && self.entities.placed_entity(entity_id).is_some()
    }

    pub(in crate::simulation) fn construction_job_target_tile(
        &self,
        job: ConstructionJob,
    ) -> Option<(WorldTileCoord, WorldTileCoord)> {
        let footprint = match job {
            ConstructionJob::BuildGhost(ghost_id) => {
                self.construction.ghosts.get(&ghost_id)?.footprint
            }
            ConstructionJob::Deconstruct(entity_id) | ConstructionJob::Repair(entity_id) => {
                self.entities.placed_entity(entity_id)?.footprint
            }
        };
        Some((
            footprint.x + i64::from(footprint.width.saturating_sub(1)) / 2,
            footprint.y + i64::from(footprint.height.saturating_sub(1)) / 2,
        ))
    }

    fn job_is_covered_by_robot_home(&self, job: ConstructionJob, robot: &Robot) -> bool {
        if robot.personal {
            let Some((x, y)) = self.construction_job_target_tile(job) else {
                return false;
            };
            return self
                .personal_roboport_coverage()
                .is_some_and(|bounds| bounds.contains(x, y));
        }
        let Some(home) = robot.home_roboport else {
            return false;
        };
        let Some(home_network) = self.robots.network_ids_by_entity.get(&home).copied() else {
            return false;
        };
        let Some((x, y)) = self.construction_job_target_tile(job) else {
            return false;
        };
        self.construction_network_covering_tile(x, y) == Some(home_network)
    }

    fn enqueue_job_once(&mut self, job: ConstructionJob) {
        if !self.construction.reservations.contains_key(&job) && self.construction.enqueue_job(job)
        {
            self.track_construction_job(job);
        }
    }

    pub(in crate::simulation) fn rebuild_construction_job_routing(&mut self) {
        self.robots.job_counts_by_network.fill(Default::default());
        self.robots.job_networks.clear();
        let jobs = self
            .construction
            .queue
            .iter()
            .chain(self.construction.reservations.keys())
            .copied()
            .collect::<Vec<_>>();
        for job in jobs {
            self.track_construction_job(job);
        }
    }

    pub(in crate::simulation) fn track_construction_job(&mut self, job: ConstructionJob) {
        if self.robots.topology_dirty || self.robots.job_networks.contains_key(&job) {
            return;
        }
        let Some((x, y)) = self.construction_job_target_tile(job) else {
            return;
        };
        let Some(network_id) = self.construction_network_covering_tile(x, y) else {
            return;
        };
        let Some(counts) = self
            .robots
            .job_counts_by_network
            .get_mut(network_id as usize)
        else {
            return;
        };
        counts.add(job);
        self.robots.job_networks.insert(job, network_id);
    }

    pub(in crate::simulation) fn untrack_construction_job(&mut self, job: ConstructionJob) {
        let Some(network_id) = self.robots.job_networks.remove(&job) else {
            return;
        };
        if let Some(counts) = self
            .robots
            .job_counts_by_network
            .get_mut(network_id as usize)
        {
            counts.remove(job);
        }
    }
}

fn first_player_construction_robot(
    sim: &Simulation,
) -> Option<(ItemId, factory_data::RobotPrototype)> {
    sim.player_inventory
        .slots()
        .iter()
        .filter_map(|slot| slot.stack())
        .find_map(|stack| {
            let profile = sim.world.prototypes.item(stack.item_id())?.robot?;
            (profile.kind == RobotKind::Construction).then_some((stack.item_id(), profile))
        })
}

fn first_player_repair_item(sim: &Simulation) -> Option<ItemId> {
    sim.player_inventory
        .slots()
        .iter()
        .filter_map(|slot| slot.stack())
        .map(|stack| stack.item_id())
        .find(|item_id| repair_restore_health(&sim.world.prototypes, *item_id).is_some())
}

pub(in crate::simulation) fn cancel_construction_job(sim: &mut Simulation, job: ConstructionJob) {
    sim.untrack_construction_job(job);
    sim.construction.remove_queued_job(job);
    let Some(robot_id) = sim.construction.reservations.remove(&job) else {
        return;
    };
    if let Some(robot) = sim.robot_flights.robots.get_mut(&robot_id)
        && robot.construction_job == Some(job)
    {
        abort_robot_job(robot);
    }
}

fn abort_robot_job(robot: &mut Robot) {
    move_payload_to_cargo(robot);
    robot.construction_job = None;
    robot.errand = None;
}

fn move_payload_to_cargo(robot: &mut Robot) {
    if let Some(payload) = robot.payload.take() {
        robot.cargo.push(payload);
    }
}

fn first_network_repair_item(sim: &Simulation, member_ids: &[EntityId]) -> Option<ItemId> {
    member_ids.iter().find_map(|entity_id| {
        sim.entities
            .roboports
            .get(entity_id)?
            .materials
            .slots()
            .iter()
            .filter_map(|slot| slot.stack())
            .map(|stack| stack.item_id())
            .find(|item_id| repair_restore_health(&sim.world.prototypes, *item_id).is_some())
    })
}

fn network_material_count(sim: &Simulation, member_ids: &[EntityId], item_id: ItemId) -> u32 {
    member_ids
        .iter()
        .filter_map(|entity_id| sim.entities.roboports.get(entity_id))
        .map(|state| state.materials.count(item_id))
        .sum()
}

fn withdraw_network_material(
    sim: &mut Simulation,
    member_ids: &[EntityId],
    item_id: ItemId,
) -> bool {
    for entity_id in member_ids {
        let Some(state) = sim.entities.roboports.get_mut(entity_id) else {
            continue;
        };
        if state.materials.remove(item_id, 1).is_ok() {
            sim.robots.mark_roboport_dirty(*entity_id);
            return true;
        }
    }
    false
}
