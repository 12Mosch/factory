//! Taking a stationed robot out of a roboport and putting it in the air.
//!
//! Both dispatchers — construction jobs and logistic deliveries — need the same
//! three things: a roboport in the network that stations the right kind of
//! robot, a charging buffer holding a full charge for it, and the commit that
//! turns the item into a flying unit. They differ only in what they hand the
//! robot afterwards, so that difference is a closure and everything else lives
//! here.

use crate::robots::{Robot, RobotActivity, RobotId};
use crate::simulation::*;
use factory_data::{RobotKind, RobotPrototype};

use super::flight::{footprint_center_fixed, squared_distance};

/// A roboport that can send one robot right now.
#[derive(Clone, Copy, Debug)]
pub(super) struct RobotDispatch {
    pub(super) roboport: EntityId,
    /// Item the robot is stationed as, which is also its flight profile.
    pub(super) item_id: ItemId,
    /// Energy the roboport pays out of its buffer to send it fully charged.
    pub(super) energy_capacity_joules: u64,
}

/// Roboport of `member_ids` closest to `target` that stations a `kind` robot and
/// can charge it.
///
/// Ties go to the lowest entity id so the answer never depends on iteration
/// order, and the distance is measured to wherever the robot is being sent
/// first — the job site for construction, the source chest for a delivery.
pub(super) fn dispatching_roboport(
    sim: &Simulation,
    member_ids: &[EntityId],
    target: (i64, i64),
    kind: RobotKind,
) -> Option<RobotDispatch> {
    let mut best: Option<(i128, RobotDispatch)> = None;
    for entity_id in member_ids {
        let Some(state) = sim.entities.roboports.get(entity_id) else {
            continue;
        };
        let Some((item_id, profile)) = stationed_robot_of_kind(sim, state, kind) else {
            continue;
        };
        if state.charge_energy_joules < profile.energy_capacity_joules {
            continue;
        }
        let Some(dock) = footprint_center_fixed(&sim.entities, *entity_id) else {
            continue;
        };
        let distance = squared_distance(dock.0 - target.0, dock.1 - target.1);
        let is_better = best.is_none_or(|(best_distance, best_dispatch)| {
            distance < best_distance
                || (distance == best_distance && *entity_id < best_dispatch.roboport)
        });
        if is_better {
            best = Some((
                distance,
                RobotDispatch {
                    roboport: *entity_id,
                    item_id,
                    energy_capacity_joules: profile.energy_capacity_joules,
                },
            ));
        }
    }
    best.map(|(_, dispatch)| dispatch)
}

/// Whether any member roboport could send a `kind` robot right now.
///
/// The cheap precondition a dispatcher checks before it starts matching: a
/// network with nothing to send should not pay for the search that finds work
/// for it.
pub(super) fn network_can_dispatch(
    sim: &Simulation,
    member_ids: &[EntityId],
    kind: RobotKind,
) -> bool {
    member_ids.iter().any(|entity_id| {
        let Some(state) = sim.entities.roboports.get(entity_id) else {
            return false;
        };
        stationed_robot_of_kind(sim, state, kind).is_some_and(|(_, profile)| {
            state.charge_energy_joules >= profile.energy_capacity_joules
        })
    })
}

/// First robot of `kind` in the roboport's slots, with the flight profile that
/// made it one.
///
/// The profile travels back with the item because finding the robot already
/// resolved it: looking it up a second time at the call site would be the same
/// work done twice, and would need an `expect` to restate what this function
/// just proved.
fn stationed_robot_of_kind(
    sim: &Simulation,
    state: &RoboportState,
    kind: RobotKind,
) -> Option<(ItemId, RobotPrototype)> {
    state
        .robots
        .slots()
        .iter()
        .filter_map(|slot| slot.stack())
        .find_map(|stack| {
            let profile = sim.world.prototypes.item(stack.item_id())?.robot?;
            (profile.kind == kind).then_some((stack.item_id(), profile))
        })
}

/// Turns the stationed robot `dispatch` names into a flying one bound for
/// `errand`, then hands it to `configure` for the work it was sent to do.
///
/// Callers must complete every fallible check before calling this: it removes a
/// robot and spends the charge unconditionally, which is what keeps the two from
/// ever happening without a robot in the air to show for them.
pub(super) fn commit_robot_dispatch(
    sim: &mut Simulation,
    dispatch: RobotDispatch,
    errand: (i64, i64),
    configure: impl FnOnce(&mut Robot),
) -> RobotId {
    let dock = footprint_center_fixed(&sim.entities, dispatch.roboport)
        .expect("the selected roboport is placed");
    let state = sim
        .entities
        .roboports
        .get_mut(&dispatch.roboport)
        .expect("the selected roboport still exists");
    state
        .robots
        .remove(dispatch.item_id, 1)
        .expect("the selected robot is still stationed");
    state.charge_energy_joules -= dispatch.energy_capacity_joules;
    sim.robots.mark_roboport_dirty(dispatch.roboport);

    let id = sim.robot_flights.allocate_id();
    let mut robot = Robot {
        id,
        item_id: dispatch.item_id,
        x: dock.0,
        y: dock.1,
        energy_joules: dispatch.energy_capacity_joules,
        home_roboport: Some(dispatch.roboport),
        errand: Some(errand),
        activity: RobotActivity::Flying,
        construction_job: None,
        delivery: None,
        payload: None,
        cargo: Vec::new(),
        bulk_cargo: Vec::new(),
    };
    configure(&mut robot);
    sim.robot_flights.robots.insert(id, robot);
    id
}
