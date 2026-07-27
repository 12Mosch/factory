use super::*;

/// Moves a player stack into whichever roboport inventory will take it.
///
/// A single click has to land somewhere sensible, and the two inventories are
/// disjoint by policy — robots go in the robot slots, repair material in the
/// material slots — so trying the robot slots first and falling back to
/// material never puts an item in the wrong half.
pub fn player_slot_to_roboport(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, RoboportError> {
    let stack = sim
        .player_inventory
        .item_slot(player_slot_index)
        .ok_or(RoboportError::InvalidSlot {
            slot_index: player_slot_index,
        })?
        .stack()
        .ok_or(RoboportError::EmptySlot {
            slot_index: player_slot_index,
        })?;
    let accepts_robot = item_slot_policy_accepts(
        &sim.world.prototypes,
        &sim.research,
        &sim.entities,
        ItemSlotPolicy::Robot,
        ItemSlotOperation::PlayerInsert,
        stack.item_id(),
    );

    if accepts_robot {
        player_slot_to_roboport_inventory(
            sim,
            entity_id,
            player_slot_index,
            RoboportInventory::Robots,
        )
    } else {
        player_slot_to_roboport_inventory(
            sim,
            entity_id,
            player_slot_index,
            RoboportInventory::Materials,
        )
    }
}

/// Which of a roboport's two inventories a transfer targets, together with the
/// acceptance policy that inventory enforces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) enum RoboportInventory {
    Robots,
    Materials,
}

impl RoboportInventory {
    fn policy(self) -> ItemSlotPolicy {
        match self {
            Self::Robots => ItemSlotPolicy::Robot,
            Self::Materials => ItemSlotPolicy::ConstructionMaterial,
        }
    }

    fn rejection(self, item_id: ItemId) -> RoboportError {
        match self {
            Self::Robots => RoboportError::InvalidRobot(item_id),
            Self::Materials => RoboportError::InvalidMaterial(item_id),
        }
    }

    fn of(self, state: &RoboportState) -> &Inventory {
        match self {
            Self::Robots => &state.robots,
            Self::Materials => &state.materials,
        }
    }

    fn of_mut(self, state: &mut RoboportState) -> &mut Inventory {
        match self {
            Self::Robots => &mut state.robots,
            Self::Materials => &mut state.materials,
        }
    }
}

pub(in crate::simulation) fn player_slot_to_roboport_inventory(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
    target: RoboportInventory,
) -> Result<TransferOutcome, RoboportError> {
    let destination = target.of(sim.entities.roboport_state(entity_id)?);
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: sim.player_inventory.item_slot(player_slot_index),
            slot_index: player_slot_index,
        },
        TransferDestination::Inventory(destination),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                target.policy(),
                ItemSlotOperation::PlayerInsert,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, |item_id| target.rejection(item_id)))?;

    let destination = target.of_mut(sim.entities.roboport_state_mut(entity_id)?);
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(
            sim.player_inventory
                .item_slot_mut(player_slot_index)
                .expect("a planned player source slot remains in bounds"),
        ),
        TransferDestinationMut::Inventory(destination),
    ))
}

pub(in crate::simulation) fn roboport_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
    target: RoboportInventory,
) -> Result<TransferOutcome, RoboportError> {
    let source = target.of(sim.entities.roboport_state(entity_id)?);
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: source.item_slot(slot_index),
            slot_index,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                target.policy(),
                ItemSlotOperation::PlayerExtract,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, |item_id| target.rejection(item_id)))?;

    let source = target.of_mut(sim.entities.roboport_state_mut(entity_id)?);
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(
            source
                .item_slot_mut(slot_index)
                .expect("a planned roboport source slot remains in bounds"),
        ),
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    ))
}
