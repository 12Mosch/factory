use bevy::prelude::*;

use super::{AppSet, InGameSet};
use crate::input::panels::handle_fullscreen_map_input;
use crate::map::resources::{
    MapDetailCache, MapDisplaySettings, MapOverlayMarkers, MapTextureCache, MapViewState,
};
use crate::rendering::map_texture::{register_map_texture_upload_systems, update_map_texture};
use crate::ui::map_view::{handle_full_map_buttons, sync_full_map_view, sync_minimap};

/// Map texture generation, minimap, and fullscreen map view.
pub(super) struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapViewState>()
            .init_resource::<MapOverlayMarkers>()
            .init_resource::<MapDisplaySettings>()
            .init_resource::<MapDetailCache>()
            .init_resource::<MapTextureCache>()
            .add_systems(
                PreUpdate,
                handle_fullscreen_map_input
                    .after(AppSet::PanelInput)
                    .in_set(InGameSet),
            )
            .add_systems(Update, update_map_texture.in_set(AppSet::MapTexture))
            .add_systems(
                Update,
                (
                    handle_full_map_buttons,
                    sync_minimap.after(AppSet::MapTexture),
                    sync_full_map_view
                        .after(AppSet::MapTexture)
                        .after(handle_full_map_buttons),
                )
                    .in_set(InGameSet),
            );

        register_map_texture_upload_systems(app);
    }
}
