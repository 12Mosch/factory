use super::*;

impl Inventory {
    pub fn with_slot_count(slot_count: usize) -> Self {
        Self {
            slots: vec![None; slot_count],
        }
    }

    pub fn player() -> Self {
        Self::with_slot_count(PLAYER_INVENTORY_SLOT_COUNT)
    }

    pub fn can_insert(&self, catalog: &PrototypeCatalog, item_id: ItemId, count: u16) -> bool {
        if count == 0 {
            return true;
        }

        let Some(stack_size) = item_stack_size(catalog, item_id) else {
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

        let stack_size = item_stack_size(catalog, item_id).ok_or(InventoryError::UnknownItem)?;
        if self.insert_capacity(item_id, stack_size) < u32::from(count) {
            return Err(InventoryError::InsufficientSpace);
        }

        let mut remaining = u32::from(count);

        for stack in self.slots.iter_mut().flatten() {
            if stack.item_id != item_id || stack.count >= stack_size {
                continue;
            }

            let available = u32::from(stack_size - stack.count);
            let inserted = remaining.min(available) as u16;
            stack.count += inserted;
            remaining -= u32::from(inserted);

            if remaining == 0 {
                return Ok(());
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
                return Ok(());
            }
        }

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
            .filter_map(|slot| slot.as_ref())
            .filter(|stack| stack.item_id == item_id)
            .map(|stack| u32::from(stack.count))
            .sum()
    }

    pub(super) fn insert_capacity(&self, item_id: ItemId, stack_size: u16) -> u32 {
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
}

impl From<InventoryError> for ContainerError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem => Self::UnknownItem,
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("container transfers remove a known slot stack")
            }
        }
    }
}

impl From<InventoryError> for BurnerDrillError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem => Self::UnknownItem,
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("burner drill transfers remove a known slot stack")
            }
        }
    }
}

impl From<InventoryError> for FurnaceError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem => Self::UnknownItem,
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("furnace transfers remove a known slot stack")
            }
        }
    }
}

impl From<InventoryError> for BoilerError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem => Self::UnknownItem,
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("boiler transfers remove a known slot stack")
            }
        }
    }
}

impl From<InventoryError> for AssemblerError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem => Self::UnknownItem,
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("assembler transfers remove a known slot stack")
            }
        }
    }
}

pub(super) fn stack_in_slot(
    inventory: &Inventory,
    slot_index: usize,
) -> Result<ItemStack, ContainerError> {
    inventory
        .slots
        .get(slot_index)
        .ok_or(ContainerError::InvalidSlot { slot_index })?
        .ok_or(ContainerError::EmptySlot { slot_index })
}

pub(super) fn ensure_inventory_can_accept(
    catalog: &PrototypeCatalog,
    inventory: &Inventory,
    stack: ItemStack,
) -> Result<(), ContainerError> {
    if inventory.can_insert(catalog, stack.item_id, stack.count) {
        Ok(())
    } else if item_stack_size(catalog, stack.item_id).is_none() {
        Err(ContainerError::UnknownItem)
    } else {
        Err(ContainerError::InsufficientSpace)
    }
}
