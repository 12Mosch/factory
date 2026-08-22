use bevy::prelude::*;

#[derive(Component, Default, Clone)]
pub(crate) struct BuildBarRoot;

#[derive(Component, Default, Clone)]
pub(crate) struct BuildSlotButton {
    pub(crate) slot_index: usize,
}

#[derive(Component, Default, Clone)]
pub(crate) struct BuildSlotCountText {
    pub(crate) slot_index: usize,
}

#[derive(Component, Default, Clone)]
pub(crate) struct BuildSlotLabelText {
    pub(crate) slot_index: usize,
}

/// Button on the build bar that toggles the buildings menu.
#[derive(Component, Default, Clone)]
pub(crate) struct BuildMenuButton;

#[derive(Component, Default, Clone)]
pub(crate) struct BuildRotateButton;

#[derive(Component, Default, Clone)]
pub(crate) struct BuildRotateButtonText;

#[derive(Component, Default, Clone)]
pub(crate) struct BuildCancelButton;

#[derive(Component, Default, Clone)]
pub(crate) struct BuildStatusText;
