use super::TransportLaneCache;
use crate::logistics::BeltItemId;
use crate::simulation::{EntityId, EntityStore, SimValidationError};

impl TransportLaneCache {
    pub(in crate::simulation) fn validate_item_tracking(
        &self,
        entities: &EntityStore,
    ) -> Result<(), SimValidationError> {
        let mut seen = std::collections::BTreeSet::new();
        for item in entities
            .transport_belts
            .values()
            .flat_map(|segment| segment.lanes.iter())
            .flat_map(|lane| lane.items.iter())
            .chain(
                entities
                    .splitters
                    .values()
                    .flat_map(|state| state.input_lanes.iter())
                    .flat_map(|lanes| lanes.iter())
                    .flat_map(|lane| lane.items.iter()),
            )
        {
            if item.id.raw() == 0
                || !seen.insert(item.id)
                || (self.next_item_id != 0 && item.id.raw() >= self.next_item_id)
            {
                return Err(SimValidationError::InvalidBeltItemIdentity);
            }
        }
        Ok(())
    }

    pub(in crate::simulation) fn allocate_item_id(&mut self) -> BeltItemId {
        assert_ne!(self.next_item_id, 0, "belt item identity space exhausted");
        let id = BeltItemId::new(self.next_item_id);
        self.next_item_id = self.next_item_id.checked_add(1).unwrap_or(0);
        id
    }

    pub(in crate::simulation) fn mark_items_changed(&mut self, entity_id: EntityId) {
        mark_item_revision(
            &mut self.item_revision,
            &mut self.item_revisions_by_entity,
            entity_id,
        );
    }

    pub(in crate::simulation) fn item_revision(&self) -> u64 {
        self.item_revision
    }

    pub(in crate::simulation) fn entity_item_revision(&self, entity_id: EntityId) -> u64 {
        usize::try_from(entity_id.raw())
            .ok()
            .and_then(|index| self.item_revisions_by_entity.get(index))
            .copied()
            .unwrap_or(0)
    }
}

pub(in crate::simulation::belt_ops) fn mark_item_revision(
    item_revision: &mut u64,
    item_revisions_by_entity: &mut Vec<u64>,
    entity_id: EntityId,
) {
    *item_revision = item_revision.wrapping_add(1);
    if *item_revision == 0 {
        *item_revision = 1;
    }
    let Ok(index) = usize::try_from(entity_id.raw()) else {
        return;
    };
    if item_revisions_by_entity.len() <= index {
        item_revisions_by_entity.resize(index + 1, 0);
    }
    item_revisions_by_entity[index] = *item_revision;
}
