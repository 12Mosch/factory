use factory_data::{ItemId, PrototypeCatalog};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Inventory {
    slots: Vec<ItemSlot>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ItemStack {
    item_id: ItemId,
    count: u16,
}

/// One persistent item-storage slot.
///
/// The transparent representation intentionally matches the legacy
/// `Option<ItemStack>` encoding used by version 18 saves.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ItemSlot(Option<ItemStack>);

impl PartialEq<Option<ItemStack>> for ItemSlot {
    fn eq(&self, other: &Option<ItemStack>) -> bool {
        self.0 == *other
    }
}

impl PartialEq<ItemSlot> for Option<ItemStack> {
    fn eq(&self, other: &ItemSlot) -> bool {
        *self == other.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryError {
    UnknownItem(ItemId),
    EmptyItemStack(ItemId),
    StackExceedsLimit {
        item_id: ItemId,
        count: u16,
        stack_size: u16,
    },
    InvalidSlot {
        slot_index: usize,
    },
    EmptySlot {
        slot_index: usize,
    },
    InsufficientSpace,
    InsufficientItems,
}

impl ItemStack {
    pub fn new(
        catalog: &PrototypeCatalog,
        item_id: ItemId,
        count: u16,
    ) -> Result<Self, InventoryError> {
        let stack = Self { item_id, count };
        validate_stack(catalog, stack)?;
        Ok(stack)
    }

    pub fn item_id(self) -> ItemId {
        self.item_id
    }

    pub fn count(self) -> u16 {
        self.count
    }
}

impl ItemSlot {
    pub fn from_stack(
        catalog: &PrototypeCatalog,
        stack: ItemStack,
    ) -> Result<Self, InventoryError> {
        validate_stack(catalog, stack)?;
        Ok(Self(Some(stack)))
    }

    pub fn stack(self) -> Option<ItemStack> {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0.is_none()
    }

    pub fn take_stack(&mut self) -> Option<ItemStack> {
        self.0.take()
    }

    pub fn validate(self, catalog: &PrototypeCatalog) -> Result<(), InventoryError> {
        if let Some(stack) = self.0 {
            validate_stack(catalog, stack)?;
        }
        Ok(())
    }

    pub fn can_insert(self, catalog: &PrototypeCatalog, stack: ItemStack) -> bool {
        validate_stack(catalog, stack).is_ok()
            && self.capacity_for(
                stack.item_id,
                stack_size(catalog, stack.item_id)
                    .expect("a validated stack always has a catalog prototype"),
            ) >= stack.count
    }

    pub fn can_insert_item(self, catalog: &PrototypeCatalog, item_id: ItemId, count: u16) -> bool {
        ItemStack::new(catalog, item_id, count).is_ok_and(|stack| self.can_insert(catalog, stack))
    }

    pub fn insert_capacity(
        self,
        catalog: &PrototypeCatalog,
        item_id: ItemId,
    ) -> Result<u16, InventoryError> {
        let stack_size =
            stack_size(catalog, item_id).ok_or(InventoryError::UnknownItem(item_id))?;
        Ok(self.capacity_for(item_id, stack_size))
    }

    pub fn insert_stack(
        &mut self,
        catalog: &PrototypeCatalog,
        stack: ItemStack,
    ) -> Result<(), InventoryError> {
        validate_stack(catalog, stack)?;
        let stack_size = stack_size(catalog, stack.item_id)
            .expect("a validated stack always has a catalog prototype");
        if self.capacity_for(stack.item_id, stack_size) < stack.count {
            return Err(InventoryError::InsufficientSpace);
        }
        self.commit_prevalidated_insert(stack.item_id, stack.count, stack_size);
        Ok(())
    }

    pub fn insert(
        &mut self,
        catalog: &PrototypeCatalog,
        item_id: ItemId,
        count: u16,
    ) -> Result<(), InventoryError> {
        self.insert_stack(catalog, ItemStack::new(catalog, item_id, count)?)
    }

    pub fn remove(&mut self, item_id: ItemId, count: u16) -> Result<(), InventoryError> {
        if count == 0 {
            return Ok(());
        }
        let Some(stack) = self.0 else {
            return Err(InventoryError::InsufficientItems);
        };
        if stack.item_id != item_id || stack.count < count {
            return Err(InventoryError::InsufficientItems);
        }
        self.commit_prevalidated_removal(item_id, count);
        Ok(())
    }

    pub(crate) fn capacity_for(self, item_id: ItemId, stack_size: u16) -> u16 {
        match self.0 {
            None => stack_size,
            Some(stack) if stack.item_id == item_id => stack_size.saturating_sub(stack.count),
            Some(_) => 0,
        }
    }

    pub(crate) fn commit_prevalidated_insert(
        &mut self,
        item_id: ItemId,
        count: u16,
        stack_size: u16,
    ) {
        debug_assert!(count > 0);
        assert!(self.capacity_for(item_id, stack_size) >= count);
        match &mut self.0 {
            Some(existing) => {
                assert_eq!(existing.item_id, item_id);
                existing.count += count;
            }
            None => self.0 = Some(ItemStack { item_id, count }),
        }
    }

    pub(crate) fn commit_prevalidated_removal(&mut self, item_id: ItemId, count: u16) {
        debug_assert!(count > 0);
        let stack = self
            .0
            .as_mut()
            .expect("a planned item slot remains occupied during commit");
        assert_eq!(stack.item_id, item_id);
        assert!(stack.count >= count);
        stack.count -= count;
        if stack.count == 0 {
            self.0 = None;
        }
    }
}

impl Inventory {
    pub fn with_slot_count(slot_count: usize) -> Self {
        Self {
            slots: vec![ItemSlot::default(); slot_count],
        }
    }

    pub fn player() -> Self {
        Self::with_slot_count(crate::PLAYER_INVENTORY_SLOT_COUNT)
    }

    pub fn from_slots(
        catalog: &PrototypeCatalog,
        slots: Vec<ItemSlot>,
    ) -> Result<Self, InventoryError> {
        for slot in &slots {
            slot.validate(catalog)?;
        }
        Ok(Self { slots })
    }

    pub fn slot(&self, slot_index: usize) -> Option<ItemStack> {
        self.slots.get(slot_index).and_then(|slot| slot.stack())
    }

    pub fn slots(&self) -> &[ItemSlot] {
        &self.slots
    }

    pub(crate) fn item_slot(&self, slot_index: usize) -> Option<&ItemSlot> {
        self.slots.get(slot_index)
    }

    pub(crate) fn item_slot_mut(&mut self, slot_index: usize) -> Option<&mut ItemSlot> {
        self.slots.get_mut(slot_index)
    }

    pub fn take_slot(&mut self, slot_index: usize) -> Result<ItemStack, InventoryError> {
        let slot = self
            .slots
            .get_mut(slot_index)
            .ok_or(InventoryError::InvalidSlot { slot_index })?;
        slot.take_stack()
            .ok_or(InventoryError::EmptySlot { slot_index })
    }

    pub fn can_insert(&self, catalog: &PrototypeCatalog, item_id: ItemId, count: u16) -> bool {
        if count == 0 {
            return true;
        }

        let Some(stack_size) = stack_size(catalog, item_id) else {
            return false;
        };

        self.insert_capacity(item_id, stack_size) >= u32::from(count)
    }

    pub fn insert(
        &mut self,
        catalog: &PrototypeCatalog,
        item_id: ItemId,
        count: u16,
    ) -> Result<(), InventoryError> {
        if count == 0 {
            return Ok(());
        }

        let stack_size =
            stack_size(catalog, item_id).ok_or(InventoryError::UnknownItem(item_id))?;
        if self.insert_capacity(item_id, stack_size) < u32::from(count) {
            return Err(InventoryError::InsufficientSpace);
        }

        self.insert_validated(item_id, count, stack_size);
        Ok(())
    }

    pub fn insert_stack(
        &mut self,
        catalog: &PrototypeCatalog,
        stack: ItemStack,
    ) -> Result<(), InventoryError> {
        validate_stack(catalog, stack)?;
        let stack_size = stack_size(catalog, stack.item_id)
            .expect("a validated item stack always has a catalog prototype");
        if self.insert_capacity(stack.item_id, stack_size) < u32::from(stack.count) {
            return Err(InventoryError::InsufficientSpace);
        }

        self.insert_validated(stack.item_id, stack.count, stack_size);
        Ok(())
    }

    pub fn can_remove(&self, item_id: ItemId, count: u16) -> bool {
        count == 0 || self.count(item_id) >= u32::from(count)
    }

    pub fn remove(&mut self, item_id: ItemId, count: u16) -> Result<(), InventoryError> {
        if count == 0 {
            return Ok(());
        }

        if !self.can_remove(item_id, count) {
            return Err(InventoryError::InsufficientItems);
        }

        let mut remaining = count;
        for slot in &mut self.slots {
            let Some(stack) = slot.stack() else {
                continue;
            };

            if stack.item_id != item_id {
                continue;
            }

            let removed = remaining.min(stack.count);
            remaining -= removed;
            slot.commit_prevalidated_removal(item_id, removed);

            if remaining == 0 {
                return Ok(());
            }
        }

        Ok(())
    }

    pub fn count(&self, item_id: ItemId) -> u32 {
        self.slots
            .iter()
            .filter_map(|slot| slot.stack())
            .filter(|stack| stack.item_id == item_id)
            .map(|stack| u32::from(stack.count))
            .sum()
    }

    pub(crate) fn insert_capacity(&self, item_id: ItemId, stack_size: u16) -> u32 {
        self.slots
            .iter()
            .map(|slot| u32::from(slot.capacity_for(item_id, stack_size)))
            .sum()
    }

    /// Inserts a quantity whose item validity and destination capacity were
    /// checked before the transfer commit began.
    pub(crate) fn commit_prevalidated_insert(
        &mut self,
        item_id: ItemId,
        count: u16,
        stack_size: u16,
    ) {
        debug_assert!(count > 0);
        debug_assert!(self.insert_capacity(item_id, stack_size) >= u32::from(count));
        self.insert_validated(item_id, count, stack_size);
    }

    fn insert_validated(&mut self, item_id: ItemId, count: u16, stack_size: u16) {
        let mut remaining = u32::from(count);

        for slot in &mut self.slots {
            let capacity = slot.capacity_for(item_id, stack_size);
            if slot.is_empty() || capacity == 0 {
                continue;
            }
            let inserted = remaining.min(u32::from(capacity)) as u16;
            slot.commit_prevalidated_insert(item_id, inserted, stack_size);
            remaining -= u32::from(inserted);
            if remaining == 0 {
                return;
            }
        }

        for slot in &mut self.slots {
            if !slot.is_empty() {
                continue;
            }

            let inserted = remaining.min(u32::from(stack_size)) as u16;
            slot.commit_prevalidated_insert(item_id, inserted, stack_size);
            remaining -= u32::from(inserted);
            if remaining == 0 {
                return;
            }
        }

        debug_assert_eq!(remaining, 0);
    }
}

pub(crate) fn validate_stack(
    catalog: &PrototypeCatalog,
    stack: ItemStack,
) -> Result<(), InventoryError> {
    let stack_size =
        stack_size(catalog, stack.item_id).ok_or(InventoryError::UnknownItem(stack.item_id))?;
    if stack.count == 0 {
        return Err(InventoryError::EmptyItemStack(stack.item_id));
    }
    if stack.count > stack_size {
        return Err(InventoryError::StackExceedsLimit {
            item_id: stack.item_id,
            count: stack.count,
            stack_size,
        });
    }
    Ok(())
}

fn stack_size(catalog: &PrototypeCatalog, item_id: ItemId) -> Option<u16> {
    catalog.item(item_id).map(|item| item.stack_size)
}

#[cfg(test)]
pub(crate) fn test_stack(item_id: ItemId, count: u16) -> ItemStack {
    use std::sync::OnceLock;

    static CATALOG: OnceLock<PrototypeCatalog> = OnceLock::new();
    let catalog = CATALOG
        .get_or_init(|| PrototypeCatalog::load_base().expect("base prototype catalog should load"));
    ItemStack::new(catalog, item_id, count).expect("test stack should satisfy catalog invariants")
}

#[cfg(test)]
pub(crate) fn test_slot(stack: ItemStack) -> ItemSlot {
    ItemSlot(Some(stack))
}

#[cfg(test)]
pub(crate) fn test_inventory(slots: Vec<Option<ItemStack>>) -> Inventory {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let slots = slots
        .into_iter()
        .map(|stack| stack.map_or_else(ItemSlot::default, test_slot))
        .collect();
    Inventory::from_slots(&catalog, slots).expect("test inventory layout should be valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_catalog() -> PrototypeCatalog {
        PrototypeCatalog::load_base().expect("base prototype catalog should load")
    }

    fn item_id(catalog: &PrototypeCatalog, name: &str) -> ItemId {
        factory_data::item_id_by_name(catalog, name)
    }

    #[test]
    fn item_stack_constructor_validates_all_invariants() {
        let catalog = base_catalog();
        let iron_plate = item_id(&catalog, "iron_plate");
        let stack_size = catalog.item(iron_plate).unwrap().stack_size;
        let unknown_item = ItemId::new(u16::MAX);

        let stack = ItemStack::new(&catalog, iron_plate, stack_size)
            .expect("a full known stack should be valid");
        assert_eq!(stack.item_id(), iron_plate);
        assert_eq!(stack.count(), stack_size);
        assert_eq!(
            ItemStack::new(&catalog, iron_plate, 0),
            Err(InventoryError::EmptyItemStack(iron_plate))
        );
        assert_eq!(
            ItemStack::new(&catalog, unknown_item, 1),
            Err(InventoryError::UnknownItem(unknown_item))
        );
        assert_eq!(
            ItemStack::new(&catalog, iron_plate, stack_size + 1),
            Err(InventoryError::StackExceedsLimit {
                item_id: iron_plate,
                count: stack_size + 1,
                stack_size,
            })
        );
    }

    #[test]
    fn exact_layout_inventory_supports_slot_reads_and_checked_takes() {
        let catalog = base_catalog();
        let iron_plate = item_id(&catalog, "iron_plate");
        let stack = ItemStack::new(&catalog, iron_plate, 7).unwrap();
        let mut inventory = Inventory::from_slots(
            &catalog,
            vec![ItemSlot::default(), test_slot(stack), ItemSlot::default()],
        )
        .expect("exact inventory layout should be valid");

        assert_eq!(inventory.slot(0), None);
        assert_eq!(inventory.slot(1), Some(stack));
        assert_eq!(inventory.slot(3), None);
        assert_eq!(
            inventory.slots(),
            &[ItemSlot::default(), test_slot(stack), ItemSlot::default()]
        );
        assert_eq!(inventory.take_slot(1), Ok(stack));
        assert_eq!(inventory.slot(1), None);
        assert_eq!(
            inventory.take_slot(1),
            Err(InventoryError::EmptySlot { slot_index: 1 })
        );
        assert_eq!(
            inventory.take_slot(3),
            Err(InventoryError::InvalidSlot { slot_index: 3 })
        );
    }

    #[test]
    fn from_slots_revalidates_stacks_against_its_catalog() {
        let source_catalog = base_catalog();
        let item_id = source_catalog.items.last().unwrap().id;
        let stack = ItemStack::new(&source_catalog, item_id, 1).unwrap();
        let mut target_catalog = source_catalog.clone();
        target_catalog.items.pop();

        assert_eq!(
            Inventory::from_slots(&target_catalog, vec![test_slot(stack)]),
            Err(InventoryError::UnknownItem(item_id))
        );
    }

    #[test]
    fn insert_stack_merges_then_uses_empty_slots() {
        let catalog = base_catalog();
        let iron_plate = item_id(&catalog, "iron_plate");
        let existing = ItemStack::new(&catalog, iron_plate, 90).unwrap();
        let incoming = ItemStack::new(&catalog, iron_plate, 20).unwrap();
        let mut inventory =
            Inventory::from_slots(&catalog, vec![test_slot(existing), ItemSlot::default()])
                .unwrap();

        inventory
            .insert_stack(&catalog, incoming)
            .expect("incoming stack should merge and split across slots");

        assert_eq!(
            inventory.slots(),
            &[
                test_slot(ItemStack::new(&catalog, iron_plate, 100).unwrap()),
                test_slot(ItemStack::new(&catalog, iron_plate, 10).unwrap()),
            ]
        );
    }

    #[test]
    fn insert_stack_is_atomic_when_space_is_insufficient() {
        let catalog = base_catalog();
        let iron_plate = item_id(&catalog, "iron_plate");
        let existing = ItemStack::new(&catalog, iron_plate, 90).unwrap();
        let incoming = ItemStack::new(&catalog, iron_plate, 20).unwrap();
        let mut inventory = Inventory::from_slots(&catalog, vec![test_slot(existing)]).unwrap();
        let before = inventory.clone();

        assert_eq!(
            inventory.insert_stack(&catalog, incoming),
            Err(InventoryError::InsufficientSpace)
        );
        assert_eq!(inventory, before);
    }

    #[test]
    fn insert_stack_revalidates_against_the_destination_catalog() {
        let source_catalog = base_catalog();
        let item_id = source_catalog.items.last().unwrap().id;
        let stack = ItemStack::new(&source_catalog, item_id, 1).unwrap();
        let mut target_catalog = source_catalog.clone();
        target_catalog.items.pop();
        let mut inventory = Inventory::with_slot_count(1);

        assert_eq!(
            inventory.insert_stack(&target_catalog, stack),
            Err(InventoryError::UnknownItem(item_id))
        );
        assert_eq!(inventory.slots(), &[ItemSlot::default()]);
    }

    #[test]
    fn item_slot_checked_mutations_are_atomic() {
        let catalog = base_catalog();
        let iron = item_id(&catalog, "iron_plate");
        let copper = item_id(&catalog, "copper_plate");
        let mut slot =
            ItemSlot::from_stack(&catalog, ItemStack::new(&catalog, iron, 90).unwrap()).unwrap();
        let before = slot;

        assert_eq!(
            slot.insert_stack(&catalog, ItemStack::new(&catalog, copper, 1).unwrap()),
            Err(InventoryError::InsufficientSpace)
        );
        assert_eq!(slot, before);
        assert_eq!(
            slot.remove(iron, 91),
            Err(InventoryError::InsufficientItems)
        );
        assert_eq!(slot, before);

        slot.insert(&catalog, iron, 10).unwrap();
        assert!(!slot.is_empty());
        assert!(!slot.can_insert_item(&catalog, iron, 1));
        slot.remove(iron, 40).unwrap();
        assert_eq!(slot.stack().unwrap().count(), 60);
        assert_eq!(slot.take_stack().unwrap().count(), 60);
        assert!(slot.is_empty());
    }

    #[test]
    fn item_slot_rejects_unknown_zero_and_oversized_stacks() {
        let catalog = base_catalog();
        let iron = item_id(&catalog, "iron_plate");
        let unknown = ItemId::new(u16::MAX);
        let stack_size = catalog.item(iron).unwrap().stack_size;
        let mut slot = ItemSlot::default();

        assert_eq!(
            slot.insert(&catalog, unknown, 1),
            Err(InventoryError::UnknownItem(unknown))
        );
        assert_eq!(
            slot.insert(&catalog, iron, 0),
            Err(InventoryError::EmptyItemStack(iron))
        );
        assert_eq!(
            slot.insert(&catalog, iron, stack_size + 1),
            Err(InventoryError::StackExceedsLimit {
                item_id: iron,
                count: stack_size + 1,
                stack_size,
            })
        );
        assert!(slot.is_empty());
    }

    #[test]
    fn item_slot_serialization_matches_legacy_option_encoding() {
        let catalog = base_catalog();
        let iron = item_id(&catalog, "iron_plate");
        let stack = ItemStack::new(&catalog, iron, 7).unwrap();

        for legacy in [None, Some(stack)] {
            let slot = legacy.map_or_else(ItemSlot::default, test_slot);
            assert_eq!(
                bincode::serialize(&slot).unwrap(),
                bincode::serialize(&legacy).unwrap()
            );
        }

        let legacy = vec![None, Some(stack), None];
        let slots = legacy
            .iter()
            .copied()
            .map(|stack| stack.map_or_else(ItemSlot::default, test_slot))
            .collect::<Vec<_>>();
        assert_eq!(
            bincode::serialize(&slots).unwrap(),
            bincode::serialize(&legacy).unwrap()
        );
    }
}
