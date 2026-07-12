use bevy::prelude::Resource;
use factory_data::BuildingCategory;
use factory_data::{EntityPrototypeId, ItemId};
use factory_sim::{Blueprint, Direction, WorldTileCoord};

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BuildingMenuView {
    #[default]
    All,
    Favorites,
    Category(BuildingCategory),
}

#[derive(Resource, Default)]
pub struct BuildMenuState {
    pub open: bool,
    pub message: Option<String>,
    pub selected_view: BuildingMenuView,
    pub search_query: String,
}

impl BuildMenuState {
    pub fn open_fresh(&mut self) {
        self.open = true;
        self.message = None;
        self.selected_view = BuildingMenuView::All;
        self.search_query.clear();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.message = None;
        self.selected_view = BuildingMenuView::All;
        self.search_query.clear();
    }
}

#[derive(Resource, Default)]
pub struct BuildPlacementPreviewState {
    pub cursor_tile: Option<(WorldTileCoord, WorldTileCoord)>,
    pub preview: Option<factory_sim::BuildPlacementPreview>,
    /// Whether the preview reflects ghost placement (shift held) rather than
    /// an immediate build.
    pub ghost: bool,
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

/// Active construction-planning tool. Tools are mutually exclusive with a
/// build selection: activating one clears the other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlannerTool {
    #[default]
    None,
    /// Drag-select an area to mark entities for deconstruction
    /// (shift-drag cancels marks instead).
    Deconstruct,
    /// Drag-select an area to copy into the paste clipboard.
    Copy,
    /// Drag-select an area to save into the blueprint library.
    CaptureBlueprint,
    /// Clipboard blueprint follows the cursor; click to paste ghosts.
    Paste,
}

/// Construction-planning input state: the active tool, an in-progress drag
/// selection, and the copy/paste clipboard.
#[derive(Resource, Default)]
pub struct PlannerState {
    pub tool: PlannerTool,
    pub drag_start: Option<(WorldTileCoord, WorldTileCoord)>,
    pub clipboard: Option<Blueprint>,
    /// Paste-time-only rotation applied to the clipboard blueprint before
    /// preview/paste, in 90-degree clockwise steps (0..=3). Never persisted
    /// to the saved blueprint.
    pub rotation_steps: u8,
}

impl PlannerState {
    pub fn set_tool(&mut self, tool: PlannerTool) {
        self.tool = tool;
        self.drag_start = None;
        self.rotation_steps = 0;
    }
}

#[derive(Resource, Default)]
pub struct BlueprintLibraryWindowState {
    pub open: bool,
    /// Index of the blueprint currently being renamed, if any.
    pub editing_index: Option<usize>,
    pub rename_buffer: String,
}

impl BlueprintLibraryWindowState {
    pub fn close(&mut self) {
        self.open = false;
        self.editing_index = None;
        self.rename_buffer.clear();
    }
}

/// Live per-entity placement issues for the blueprint currently being
/// previewed under the cursor while [`PlannerTool::Paste`] is active.
#[derive(Resource, Default)]
pub struct PastePlacementPreviewState {
    pub active: bool,
    pub issues: Vec<factory_sim::BuildPlacementIssue>,
}
