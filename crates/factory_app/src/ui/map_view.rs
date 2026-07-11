#![allow(unused_imports)]

mod components;
mod drawing;
mod input;
mod layout;
mod sync;

pub(crate) use components::{
    FullMapImage, FullMapLayerButton, FullMapOverlayRoot, FullMapRecenterButton, FullMapRoot,
    MinimapImage, MinimapOverlayRoot, MinimapRoot,
};
pub(crate) use drawing::{MINIMAP_FRAME_SIZE, MINIMAP_RIGHT_OFFSET, MINIMAP_TOP_OFFSET};
pub(crate) use input::handle_full_map_buttons;
pub use layout::{
    FULL_MAP_BASELINE_VIEW_HEIGHT, FULL_MAP_MAX_ZOOM, FULL_MAP_MIN_ZOOM, clamp_map_center,
    fullscreen_crop_bounds, texture_rect_for_world_bounds,
};
pub(crate) use layout::{fullscreen_map_display_size, fullscreen_map_image_size};
pub(crate) use sync::{sync_full_map_view, sync_minimap};

#[cfg(test)]
mod tests;
