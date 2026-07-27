use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferOutcome {
    pub moved_quantity: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TransferPlan {
    pub(super) item_id: ItemId,
    pub(super) moved_quantity: u16,
    pub(super) stack_size: u16,
}

#[derive(Clone, Copy)]
pub(super) struct TransferSource<'a> {
    pub(super) slot: Option<&'a ItemSlot>,
    pub(super) slot_index: usize,
}

#[derive(Clone, Copy)]
pub(super) enum TransferDestination<'a> {
    Inventory(&'a Inventory),
    SingleSlot(&'a ItemSlot),
}

pub(super) enum TransferSourceMut<'a> {
    Slot(&'a mut ItemSlot),
}

pub(super) enum TransferDestinationMut<'a> {
    Inventory(&'a mut Inventory),
    SingleSlot(&'a mut ItemSlot),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TransferPlanError {
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    RejectedItem(ItemId),
    UnknownItem(ItemId),
    InsufficientSpace,
}

impl TransferSource<'_> {
    fn stack(self) -> Result<ItemStack, TransferPlanError> {
        self.slot
            .ok_or(TransferPlanError::InvalidSlot {
                slot_index: self.slot_index,
            })?
            .stack()
            .ok_or(TransferPlanError::EmptySlot {
                slot_index: self.slot_index,
            })
    }
}

impl TransferDestination<'_> {
    fn capacity(self, item_id: ItemId, stack_size: u16) -> u32 {
        match self {
            Self::Inventory(inventory) => inventory.insert_capacity(item_id, stack_size),
            Self::SingleSlot(slot) => u32::from(slot.capacity_for(item_id, stack_size)),
        }
    }
}

pub(super) fn plan_transfer(
    catalog: &PrototypeCatalog,
    source: TransferSource<'_>,
    destination: TransferDestination<'_>,
    accepts_item: impl FnOnce(ItemId) -> bool,
) -> Result<TransferPlan, TransferPlanError> {
    let stack = source.stack()?;
    crate::inventory::validate_stack(catalog, stack)
        .map_err(|_| TransferPlanError::UnknownItem(stack.item_id()))?;

    if !accepts_item(stack.item_id()) {
        return Err(TransferPlanError::RejectedItem(stack.item_id()));
    }

    let stack_size = item_stack_size(catalog, stack.item_id())
        .ok_or(TransferPlanError::UnknownItem(stack.item_id()))?;
    let capacity = destination.capacity(stack.item_id(), stack_size);
    let moved_quantity = u32::from(stack.count()).min(capacity) as u16;
    if moved_quantity == 0 {
        return Err(TransferPlanError::InsufficientSpace);
    }

    Ok(TransferPlan {
        item_id: stack.item_id(),
        moved_quantity,
        stack_size,
    })
}

pub(super) fn commit_transfer(
    plan: TransferPlan,
    source: TransferSourceMut<'_>,
    destination: TransferDestinationMut<'_>,
) -> TransferOutcome {
    match destination {
        TransferDestinationMut::Inventory(inventory) => {
            inventory.commit_prevalidated_insert(plan.item_id, plan.moved_quantity, plan.stack_size)
        }
        TransferDestinationMut::SingleSlot(slot) => {
            slot.commit_prevalidated_insert(plan.item_id, plan.moved_quantity, plan.stack_size);
        }
    }

    match source {
        TransferSourceMut::Slot(slot) => {
            slot.commit_prevalidated_removal(plan.item_id, plan.moved_quantity);
        }
    }

    TransferOutcome {
        moved_quantity: plan.moved_quantity,
    }
}

pub(super) fn map_plan_error<E: From<InventoryError>>(
    error: TransferPlanError,
    rejected_item: impl FnOnce(ItemId) -> E,
) -> E {
    match error {
        TransferPlanError::InvalidSlot { slot_index } => {
            InventoryError::InvalidSlot { slot_index }.into()
        }
        TransferPlanError::EmptySlot { slot_index } => {
            InventoryError::EmptySlot { slot_index }.into()
        }
        TransferPlanError::RejectedItem(item_id) => rejected_item(item_id),
        TransferPlanError::UnknownItem(item_id) => InventoryError::UnknownItem(item_id).into(),
        TransferPlanError::InsufficientSpace => InventoryError::InsufficientSpace.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_partially_moves_between_exact_inventory_slots() {
        let catalog = PrototypeCatalog::load_base().expect("base prototypes should load");
        let item_id = factory_data::item_id_by_name(&catalog, "iron_plate");
        let mut source = Inventory::from_slots(
            &catalog,
            vec![
                test_slot(ItemStack::new(&catalog, item_id, 40).unwrap()),
                test_slot(ItemStack::new(&catalog, item_id, 60).unwrap()),
            ],
        )
        .unwrap();
        let mut destination = Inventory::from_slots(
            &catalog,
            vec![test_slot(ItemStack::new(&catalog, item_id, 75).unwrap())],
        )
        .unwrap();

        let plan = plan_transfer(
            &catalog,
            TransferSource {
                slot: source.item_slot(0),
                slot_index: 0,
            },
            TransferDestination::Inventory(&destination),
            |_| true,
        )
        .unwrap();
        let outcome = commit_transfer(
            plan,
            TransferSourceMut::Slot(source.item_slot_mut(0).unwrap()),
            TransferDestinationMut::Inventory(&mut destination),
        );

        assert_eq!(outcome, TransferOutcome { moved_quantity: 25 });
        assert_eq!(source.slot(0).unwrap().count(), 15);
        assert_eq!(source.slot(1).unwrap().count(), 60);
        assert_eq!(destination.slot(0).unwrap().count(), 100);
    }

    #[test]
    fn primitive_planning_errors_do_not_mutate_either_endpoint() {
        let catalog = PrototypeCatalog::load_base().expect("base prototypes should load");
        let iron_plate = factory_data::item_id_by_name(&catalog, "iron_plate");
        let coal = factory_data::item_id_by_name(&catalog, "coal");
        let source = Inventory::from_slots(
            &catalog,
            vec![test_slot(ItemStack::new(&catalog, iron_plate, 10).unwrap())],
        )
        .unwrap();
        let destination = Inventory::from_slots(
            &catalog,
            vec![test_slot(ItemStack::new(&catalog, coal, 100).unwrap())],
        )
        .unwrap();
        let source_before = source.clone();
        let destination_before = destination.clone();

        assert_eq!(
            plan_transfer(
                &catalog,
                TransferSource {
                    slot: source.item_slot(0),
                    slot_index: 0,
                },
                TransferDestination::Inventory(&destination),
                |_| true,
            ),
            Err(TransferPlanError::InsufficientSpace)
        );
        assert_eq!(source, source_before);
        assert_eq!(destination, destination_before);
    }

    #[test]
    fn unknown_source_items_fail_planning_without_mutation() {
        let source_catalog = PrototypeCatalog::load_base().expect("base prototypes should load");
        let item_id = source_catalog
            .items
            .last()
            .expect("base prototypes should contain items")
            .id;
        let source = Inventory::from_slots(
            &source_catalog,
            vec![test_slot(
                ItemStack::new(&source_catalog, item_id, 1).unwrap(),
            )],
        )
        .unwrap();
        let destination = Inventory::with_slot_count(1);
        let source_before = source.clone();
        let destination_before = destination.clone();
        let mut destination_catalog = source_catalog;
        destination_catalog.items.pop();

        assert_eq!(
            plan_transfer(
                &destination_catalog,
                TransferSource {
                    slot: source.item_slot(0),
                    slot_index: 0,
                },
                TransferDestination::Inventory(&destination),
                |_| true,
            ),
            Err(TransferPlanError::UnknownItem(item_id))
        );
        assert_eq!(source, source_before);
        assert_eq!(destination, destination_before);
    }
}
