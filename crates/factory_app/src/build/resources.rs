use bevy::prelude::Resource;
use factory_data::{EntityPrototypeId, ItemId};
use factory_sim::Direction;

#[derive(Resource, Default)]
pub struct BuildPlacementState {
    pub selected: Option<BuildSelection>,
    pub direction: Direction,
    pub last_status: BuildPlacementStatus,
}

pub const HOTBAR_SLOT_COUNT: usize = 10;

#[derive(Resource, Default)]
pub struct HotbarState {
    pub slots: [Option<BuildSelection>; HOTBAR_SLOT_COUNT],
}

impl HotbarState {
    pub fn slot(&self, slot_index: usize) -> Option<BuildSelection> {
        self.slots.get(slot_index).copied().flatten()
    }

    pub fn slot_of(&self, selection: BuildSelection) -> Option<usize> {
        self.slots.iter().position(|slot| *slot == Some(selection))
    }

    /// Assigns the selection to the first empty slot and returns that slot
    /// index, or `None` when the hotbar is full. Selections already on the
    /// hotbar keep their existing slot.
    pub fn assign_to_first_empty(&mut self, selection: BuildSelection) -> Option<usize> {
        if let Some(existing) = self.slot_of(selection) {
            return Some(existing);
        }
        let slot_index = self.slots.iter().position(Option::is_none)?;
        self.slots[slot_index] = Some(selection);
        Some(slot_index)
    }

    pub fn remove(&mut self, selection: BuildSelection) -> bool {
        match self.slot_of(selection) {
            Some(slot_index) => {
                self.slots[slot_index] = None;
                true
            }
            None => false,
        }
    }
}

#[derive(Resource, Default)]
pub struct BuildMenuState {
    pub open: bool,
    pub message: Option<String>,
}

#[derive(Resource, Default)]
pub struct BuildPlacementPreviewState {
    pub cursor_tile: Option<(i32, i32)>,
    pub preview: Option<factory_sim::BuildPlacementPreview>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildSelection {
    pub prototype_id: EntityPrototypeId,
    pub item_id: ItemId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum BuildPlacementStatus {
    #[default]
    Ready,
    Placed(String),
    CannotPlace(String),
    MissingInventory(String),
    Locked(String),
}
