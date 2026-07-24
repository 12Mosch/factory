use factory_data::ItemId;
use serde::{Deserialize, Serialize};

/// One module installed in the equipped armor's grid.
///
/// Entries are stored in canonical `(y, x, item_id)` order by the simulation.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct InstalledEquipment {
    pub item_id: ItemId,
    pub x: u8,
    pub y: u8,
}

/// Durable powered-equipment state kept separate from the copyable player
/// movement and health state.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PlayerEquipmentState {
    pub(crate) equipped_armor: Option<ItemId>,
    pub(crate) installed: Vec<InstalledEquipment>,
    pub(crate) battery_energy_joules: u64,
    pub(crate) shield_energy_joules: u64,
    pub(crate) generation_remainder_watt_ticks: u64,
    pub(crate) recharge_remainder_watt_ticks: u64,
}

impl PlayerEquipmentState {
    pub fn equipped_armor(&self) -> Option<ItemId> {
        self.equipped_armor
    }

    pub fn installed(&self) -> &[InstalledEquipment] {
        &self.installed
    }

    pub fn stored_energy_joules(&self) -> u64 {
        self.battery_energy_joules
    }

    pub fn shield_energy_joules(&self) -> u64 {
        self.shield_energy_joules
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEquipmentError {
    InvalidInventorySlot { slot_index: usize },
    EmptyInventorySlot { slot_index: usize },
    NotArmor(ItemId),
    NotEquipment(ItemId),
    NoArmorEquipped,
    ArmorGridNotEmpty,
    PlacementOutOfBounds,
    PlacementOverlaps,
    NoEquipmentAtCell { x: u8, y: u8 },
    InventoryFull,
}
