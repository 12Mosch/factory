use factory_data::{ItemId, PrototypeCatalog};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Inventory {
    slots: Vec<Option<ItemStack>>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ItemStack {
    item_id: ItemId,
    count: u16,
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

impl Inventory {
    pub fn with_slot_count(slot_count: usize) -> Self {
        Self {
            slots: vec![None; slot_count],
        }
    }

    pub fn player() -> Self {
        Self::with_slot_count(crate::PLAYER_INVENTORY_SLOT_COUNT)
    }

    pub fn from_slots(
        catalog: &PrototypeCatalog,
        slots: Vec<Option<ItemStack>>,
    ) -> Result<Self, InventoryError> {
        for stack in slots.iter().flatten().copied() {
            validate_stack(catalog, stack)?;
        }
        Ok(Self { slots })
    }

    pub fn slot(&self, slot_index: usize) -> Option<ItemStack> {
        self.slots.get(slot_index).copied().flatten()
    }

    pub fn slots(&self) -> &[Option<ItemStack>] {
        &self.slots
    }

    pub fn take_slot(&mut self, slot_index: usize) -> Result<ItemStack, InventoryError> {
        let slot = self
            .slots
            .get_mut(slot_index)
            .ok_or(InventoryError::InvalidSlot { slot_index })?;
        slot.take().ok_or(InventoryError::EmptySlot { slot_index })
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
            let Some(stack) = slot else {
                continue;
            };

            if stack.item_id != item_id {
                continue;
            }

            let removed = remaining.min(stack.count);
            stack.count -= removed;
            remaining -= removed;

            if stack.count == 0 {
                *slot = None;
            }

            if remaining == 0 {
                return Ok(());
            }
        }

        Ok(())
    }

    pub fn count(&self, item_id: ItemId) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|stack| stack.item_id == item_id)
            .map(|stack| u32::from(stack.count))
            .sum()
    }

    pub(crate) fn insert_capacity(&self, item_id: ItemId, stack_size: u16) -> u32 {
        self.slots
            .iter()
            .map(|slot| match slot {
                Some(stack) if stack.item_id == item_id && stack.count < stack_size => {
                    u32::from(stack_size - stack.count)
                }
                Some(_) => 0,
                None => u32::from(stack_size),
            })
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

    /// Removes a quantity from one exact source slot after a transfer plan has
    /// captured and validated that slot.
    pub(crate) fn commit_prevalidated_slot_removal(
        &mut self,
        slot_index: usize,
        item_id: ItemId,
        count: u16,
    ) {
        debug_assert!(count > 0);
        let slot = self
            .slots
            .get_mut(slot_index)
            .expect("a planned inventory source slot remains in bounds during commit");
        let stack = slot
            .as_mut()
            .expect("a planned inventory source slot remains occupied during commit");
        assert_eq!(
            stack.item_id, item_id,
            "a planned inventory source slot retains its item kind during commit"
        );
        assert!(
            stack.count >= count,
            "a planned inventory source slot retains the committed quantity"
        );

        stack.count -= count;
        if stack.count == 0 {
            *slot = None;
        }
    }

    fn insert_validated(&mut self, item_id: ItemId, count: u16, stack_size: u16) {
        let mut remaining = u32::from(count);

        for stack in self.slots.iter_mut().flatten() {
            if stack.item_id != item_id || stack.count >= stack_size {
                continue;
            }

            let inserted = remaining.min(u32::from(stack_size - stack.count)) as u16;
            stack.count += inserted;
            remaining -= u32::from(inserted);
            if remaining == 0 {
                return;
            }
        }

        for slot in &mut self.slots {
            if slot.is_some() {
                continue;
            }

            let inserted = remaining.min(u32::from(stack_size)) as u16;
            *slot = Some(ItemStack {
                item_id,
                count: inserted,
            });
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

pub(crate) fn single_slot_can_accept(
    catalog: &PrototypeCatalog,
    slot: Option<ItemStack>,
    stack: ItemStack,
) -> bool {
    let Ok(()) = validate_stack(catalog, stack) else {
        return false;
    };
    let stack_size = catalog
        .item(stack.item_id)
        .expect("a validated stack has a catalog prototype")
        .stack_size;

    match slot {
        None => true,
        Some(existing) if existing.item_id == stack.item_id => {
            u32::from(existing.count) + u32::from(stack.count) <= u32::from(stack_size)
        }
        Some(_) => false,
    }
}

pub(crate) fn insert_into_single_slot(
    catalog: &PrototypeCatalog,
    slot: &mut Option<ItemStack>,
    stack: ItemStack,
) -> Result<(), InventoryError> {
    validate_stack(catalog, stack)?;
    if !single_slot_can_accept(catalog, *slot, stack) {
        return Err(InventoryError::InsufficientSpace);
    }

    match slot {
        Some(existing) => existing.count += stack.count,
        None => *slot = Some(stack),
    }
    Ok(())
}

pub(crate) fn commit_prevalidated_single_slot_insert(
    slot: &mut Option<ItemStack>,
    item_id: ItemId,
    count: u16,
    stack_size: u16,
) {
    debug_assert!(count > 0);
    match slot {
        Some(existing) => {
            assert_eq!(
                existing.item_id, item_id,
                "a planned single-slot destination retains its item kind during commit"
            );
            existing.count = existing
                .count
                .checked_add(count)
                .expect("a planned single-slot insertion cannot overflow");
            assert!(
                existing.count <= stack_size,
                "a planned single-slot insertion stays within the item stack size"
            );
        }
        None => {
            assert!(
                count <= stack_size,
                "a planned single-slot insertion stays within the item stack size"
            );
            *slot = Some(ItemStack { item_id, count });
        }
    }
}

pub(crate) fn commit_prevalidated_single_slot_removal(
    slot: &mut Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) {
    debug_assert!(count > 0);
    let stack = slot
        .as_mut()
        .expect("a planned single-slot source remains occupied during commit");
    assert_eq!(
        stack.item_id, item_id,
        "a planned single-slot source retains its item kind during commit"
    );
    assert!(
        stack.count >= count,
        "a planned single-slot source retains the committed quantity"
    );

    stack.count -= count;
    if stack.count == 0 {
        *slot = None;
    }
}

pub(crate) fn insert_item_into_single_slot(
    catalog: &PrototypeCatalog,
    slot: &mut Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) -> Result<(), InventoryError> {
    insert_into_single_slot(catalog, slot, ItemStack::new(catalog, item_id, count)?)
}

pub(crate) fn remove_from_single_slot(
    slot: &mut Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) -> Result<(), InventoryError> {
    if count == 0 {
        return Ok(());
    }

    let Some(stack) = slot else {
        return Err(InventoryError::InsufficientItems);
    };
    if stack.item_id != item_id || stack.count < count {
        return Err(InventoryError::InsufficientItems);
    }

    stack.count -= count;
    if stack.count == 0 {
        *slot = None;
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
pub(crate) fn test_inventory(slots: Vec<Option<ItemStack>>) -> Inventory {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
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
        let mut inventory = Inventory::from_slots(&catalog, vec![None, Some(stack), None])
            .expect("exact inventory layout should be valid");

        assert_eq!(inventory.slot(0), None);
        assert_eq!(inventory.slot(1), Some(stack));
        assert_eq!(inventory.slot(3), None);
        assert_eq!(inventory.slots(), &[None, Some(stack), None]);
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
            Inventory::from_slots(&target_catalog, vec![Some(stack)]),
            Err(InventoryError::UnknownItem(item_id))
        );
    }

    #[test]
    fn insert_stack_merges_then_uses_empty_slots() {
        let catalog = base_catalog();
        let iron_plate = item_id(&catalog, "iron_plate");
        let existing = ItemStack::new(&catalog, iron_plate, 90).unwrap();
        let incoming = ItemStack::new(&catalog, iron_plate, 20).unwrap();
        let mut inventory = Inventory::from_slots(&catalog, vec![Some(existing), None]).unwrap();

        inventory
            .insert_stack(&catalog, incoming)
            .expect("incoming stack should merge and split across slots");

        assert_eq!(
            inventory.slots(),
            &[
                Some(ItemStack::new(&catalog, iron_plate, 100).unwrap()),
                Some(ItemStack::new(&catalog, iron_plate, 10).unwrap()),
            ]
        );
    }

    #[test]
    fn insert_stack_is_atomic_when_space_is_insufficient() {
        let catalog = base_catalog();
        let iron_plate = item_id(&catalog, "iron_plate");
        let existing = ItemStack::new(&catalog, iron_plate, 90).unwrap();
        let incoming = ItemStack::new(&catalog, iron_plate, 20).unwrap();
        let mut inventory = Inventory::from_slots(&catalog, vec![Some(existing)]).unwrap();
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
        assert_eq!(inventory.slots(), &[None]);
    }
}
