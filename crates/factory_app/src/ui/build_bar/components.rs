use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct BuildBarRoot;

#[derive(Component)]
pub(crate) struct BuildSlotButton {
    pub(crate) slot_index: usize,
}

#[derive(Component)]
pub(crate) struct BuildSlotCountText {
    pub(crate) slot_index: usize,
}

#[derive(Component)]
pub(crate) struct BuildSlotLabelText {
    pub(crate) slot_index: usize,
}

/// Button on the build bar that toggles the buildings menu.
#[derive(Component)]
pub(crate) struct BuildMenuButton;

#[derive(Component)]
pub(crate) struct BuildRotateButton;

#[derive(Component)]
pub(crate) struct BuildRotateButtonText;

#[derive(Component)]
pub(crate) struct BuildCancelButton;

#[derive(Component)]
pub(crate) struct BuildStatusText;
