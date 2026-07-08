use bevy::prelude::*;

use crate::map::resources::MapLayer;

#[derive(Component)]
pub(crate) struct MinimapRoot;

#[derive(Component)]
pub(crate) struct MinimapImage;

#[derive(Component)]
pub(crate) struct MinimapOverlayRoot;

#[derive(Component)]
pub(crate) struct FullMapRoot;

#[derive(Component)]
pub(crate) struct FullMapImage;

#[derive(Component)]
pub(crate) struct FullMapOverlayRoot;

#[derive(Component)]
pub(crate) struct FullMapLayerButton {
    pub(crate) layer: MapLayer,
}

#[derive(Component)]
pub(crate) struct FullMapRecenterButton;
