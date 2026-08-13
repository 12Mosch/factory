//! Configuring a logistic chest: the rows a requester or buffer asks for, and
//! the filter a storage chest accepts.

use crate::simulation::*;
use factory_data::LogisticChestPrototype;

impl Simulation {
    pub fn logistic_chest_state(&self, entity_id: EntityId) -> Option<&LogisticChestState> {
        self.entities.logistic_chests.get(&entity_id)
    }

    pub fn logistic_chest_prototype(&self, entity_id: EntityId) -> Option<LogisticChestPrototype> {
        self.entities
            .placed_entity(entity_id)
            .and_then(|placed| self.world.prototypes.entity(placed.prototype_id))
            .and_then(|prototype| prototype.logistic_chest)
    }

    /// Largest amount of `item_id` a chest of this prototype could hold.
    ///
    /// Requests are capped by it because a request bigger than the chest is one
    /// the network can never finish: robots would keep delivering into a full
    /// inventory forever.
    pub(in crate::simulation) fn logistic_request_capacity(
        &self,
        prototype: &factory_data::EntityPrototype,
        item_id: ItemId,
    ) -> Option<u32> {
        let stack_size = u32::from(self.world.prototypes.item(item_id)?.stack_size);
        let slots = u32::try_from(prototype.inventory_slot_count?).ok()?;
        Some(slots.saturating_mul(stack_size))
    }

    /// Rewrites one configured row.
    ///
    /// The chest is marked as changed even when only the amount moved, because
    /// the row is half of what the logistic index counts: a request the index
    /// has not seen is demand the network would never act on.
    pub fn set_logistic_request(
        &mut self,
        entity_id: EntityId,
        slot_index: usize,
        request: LogisticRequest,
    ) -> Result<(), LogisticChestError> {
        let prototype = self
            .entities
            .placed_entity(entity_id)
            .and_then(|placed| self.world.prototypes.entity(placed.prototype_id))
            .ok_or(LogisticChestError::MissingEntity(entity_id))?;
        let logistic_chest = prototype
            .logistic_chest
            .ok_or(LogisticChestError::NotLogisticChest(entity_id))?;

        let request = self.validated_logistic_request(prototype, logistic_chest, request)?;
        let state = self
            .entities
            .logistic_chests
            .get_mut(&entity_id)
            .ok_or(LogisticChestError::NotLogisticChest(entity_id))?;
        let slot = state
            .requests
            .get_mut(slot_index)
            .ok_or(LogisticChestError::InvalidSlot { slot_index })?;
        *slot = request;
        self.entities.note_logistic_endpoint_changed(entity_id);
        Ok(())
    }

    /// Normalizes and checks a row against the chest's mode.
    ///
    /// Clearing the item clears the amount with it, so an unset row can never
    /// carry a leftover number that the index would have no item to attach to.
    /// An amount larger than the chest could hold is clamped rather than
    /// refused: a request the chest has no room for is one the network could
    /// never finish, and clamping is what the player meant by holding the
    /// stepper down.
    fn validated_logistic_request(
        &self,
        prototype: &factory_data::EntityPrototype,
        logistic_chest: LogisticChestPrototype,
        request: LogisticRequest,
    ) -> Result<LogisticRequest, LogisticChestError> {
        let Some(item_id) = request.item else {
            return Ok(LogisticRequest::default());
        };
        let capacity = self
            .logistic_request_capacity(prototype, item_id)
            .ok_or(LogisticChestError::UnknownItem(item_id))?;
        if !logistic_chest.mode.requests_items() {
            if request.count != 0 {
                return Err(LogisticChestError::ModeTakesNoAmount);
            }
            return Ok(request);
        }
        Ok(LogisticRequest {
            count: request.count.min(capacity),
            ..request
        })
    }
}
