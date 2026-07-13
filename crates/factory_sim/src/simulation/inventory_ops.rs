use super::*;

macro_rules! impl_inventory_error_conversion {
    ($target:ty, $transfer_context:literal) => {
        impl From<InventoryError> for $target {
            fn from(error: InventoryError) -> Self {
                match error {
                    InventoryError::UnknownItem(_) => Self::UnknownItem,
                    InventoryError::InvalidSlot { slot_index } => Self::InvalidSlot { slot_index },
                    InventoryError::EmptySlot { slot_index } => Self::EmptySlot { slot_index },
                    InventoryError::InsufficientSpace => Self::InsufficientSpace,
                    InventoryError::InsufficientItems => unreachable!(concat!(
                        $transfer_context,
                        " transfers remove a known slot stack"
                    )),
                    InventoryError::EmptyItemStack(_)
                    | InventoryError::StackExceedsLimit { .. } => {
                        unreachable!("inventory operations only create validated stacks")
                    }
                }
            }
        }
    };
}

impl_inventory_error_conversion!(ContainerError, "container");
impl_inventory_error_conversion!(BurnerDrillError, "burner drill");
impl_inventory_error_conversion!(FurnaceError, "furnace");
impl_inventory_error_conversion!(BoilerError, "boiler");
impl_inventory_error_conversion!(AssemblerError, "assembler");
