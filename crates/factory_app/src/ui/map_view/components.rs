use bevy::prelude::*;

use crate::map::resources::MapOverlay;

#[derive(Component)]
pub(crate) struct MinimapRoot;

#[derive(Component)]
pub(crate) struct MinimapImage;

#[derive(Component)]
pub(crate) struct MinimapResourceImage;

#[derive(Component)]
pub(crate) struct MinimapOverlayRoot;

#[derive(Component)]
pub(crate) struct FullMapRoot;

#[derive(Component)]
pub(crate) struct FullMapImage;

#[derive(Component)]
pub(crate) struct FullMapResourceImage;

#[derive(Component)]
pub(crate) struct FullMapOverlayRoot;

#[derive(Component)]
pub(crate) struct FullMapOverlayButton {
    pub(crate) overlay: MapOverlay,
}

#[derive(Component)]
pub(crate) struct FullMapRecenterButton;
