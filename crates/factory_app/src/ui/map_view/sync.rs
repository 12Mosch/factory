use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::map::resources::{
    MapDisplaySettings, MapLayer, MapOverlayMarkers, MapTextureCache, MapViewState,
};
use crate::resources::SimResource;

use super::components::{
    FullMapImage, FullMapLayerButton, FullMapOverlayRoot, FullMapRoot, MinimapImage,
    MinimapOverlayRoot, MinimapRoot,
};
use super::drawing::{
    MINIMAP_CONTENT_SIZE, MapOverlayContext, layer_button_border_color, layer_button_color,
    rebuild_map_overlay, set_full_map_image_node_size, spawn_full_map, spawn_minimap,
};
use super::layout::{
    FULL_MAP_MAX_ZOOM, FULL_MAP_MIN_ZOOM, camera_tile_rect, fullscreen_crop_bounds,
    fullscreen_cursor_chunk, fullscreen_map_display_size, fullscreen_map_image_size,
    minimap_crop_bounds, texture_rect_for_world_bounds,
};

#[derive(SystemParam)]
pub(crate) struct MinimapSyncParams<'w, 's> {
    cache: Res<'w, MapTextureCache>,
    sim: Res<'w, SimResource>,
    settings: Res<'w, MapDisplaySettings>,
    markers: Res<'w, MapOverlayMarkers>,
    roots: Query<'w, 's, Entity, With<MinimapRoot>>,
    images: Query<'w, 's, &'static mut ImageNode, With<MinimapImage>>,
    overlay_roots: Query<'w, 's, Entity, With<MinimapOverlayRoot>>,
    cameras: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<Camera2d>>,
}

pub(crate) fn sync_minimap(mut commands: Commands, mut params: MinimapSyncParams) {
    let Some(surface_cache) = params.cache.surface() else {
        return;
    };
    let Some(handle) = surface_cache.handle.as_ref() else {
        return;
    };
    let Some(map_bounds) = surface_cache.bounds else {
        return;
    };
    let player = params.sim.sim.player();
    let (player_x, player_y) = player.position_tiles();
    let crop_bounds = minimap_crop_bounds(map_bounds, Vec2::new(player_x, player_y));
    let texture_rect = texture_rect_for_world_bounds(map_bounds, crop_bounds);

    let mut roots_iter = params.roots.iter();
    let Some(_root) = roots_iter.next() else {
        spawn_minimap(&mut commands, handle.clone(), texture_rect);
        return;
    };
    for duplicate in roots_iter {
        commands.entity(duplicate).despawn();
    }

    for mut image in &mut params.images {
        image.image = handle.clone();
        image.rect = Some(texture_rect);
    }

    let camera_rect = camera_tile_rect(&params.cameras);
    for overlay_root in &params.overlay_roots {
        rebuild_map_overlay(
            &mut commands,
            overlay_root,
            MapOverlayContext {
                crop_bounds,
                image_size: Vec2::splat(MINIMAP_CONTENT_SIZE),
                player_position: Vec2::new(player_x, player_y),
                sim: &params.sim.sim,
                settings: &params.settings,
                layer: MapLayer::Surface,
                camera_rect,
                chunk_cursor: None,
                markers: &params.markers,
            },
        );
    }
}

#[derive(SystemParam)]
pub(crate) struct FullMapSyncParams<'w, 's> {
    cache: Res<'w, MapTextureCache>,
    state: Res<'w, MapViewState>,
    sim: Res<'w, SimResource>,
    settings: Res<'w, MapDisplaySettings>,
    markers: Res<'w, MapOverlayMarkers>,
    roots: Query<'w, 's, Entity, With<FullMapRoot>>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    images: Query<'w, 's, &'static mut ImageNode, With<FullMapImage>>,
    image_nodes: Query<'w, 's, &'static mut Node, With<FullMapImage>>,
    image_layout:
        Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<FullMapImage>>,
    overlay_roots: Query<'w, 's, Entity, With<FullMapOverlayRoot>>,
    cameras: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<Camera2d>>,
    layer_buttons: Query<
        'w,
        's,
        (
            &'static FullMapLayerButton,
            &'static Interaction,
            &'static mut BackgroundColor,
            &'static mut BorderColor,
        ),
        With<Button>,
    >,
}

pub(crate) fn sync_full_map_view(mut commands: Commands, mut params: FullMapSyncParams) {
    if !params.state.open {
        for entity in &params.roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Some(layer_cache) = params.cache.layer(params.state.selected_layer) else {
        return;
    };
    let Some(handle) = layer_cache.handle.as_ref() else {
        return;
    };
    let Some(map_bounds) = layer_cache.bounds else {
        return;
    };
    let image_size = fullscreen_map_image_size(params.windows.iter().next());
    let crop_bounds = fullscreen_crop_bounds(
        map_bounds,
        params.state.center_tile,
        params
            .state
            .zoom
            .clamp(FULL_MAP_MIN_ZOOM, FULL_MAP_MAX_ZOOM),
        image_size,
    );
    let display_size = fullscreen_map_display_size(image_size, crop_bounds);
    let texture_rect = texture_rect_for_world_bounds(map_bounds, crop_bounds);

    let mut roots_iter = params.roots.iter();
    let Some(_root) = roots_iter.next() else {
        spawn_full_map(
            &mut commands,
            handle.clone(),
            texture_rect,
            display_size,
            params.state.selected_layer,
        );
        return;
    };
    for duplicate in roots_iter {
        commands.entity(duplicate).despawn();
    }

    for mut image in &mut params.images {
        image.image = handle.clone();
        image.rect = Some(texture_rect);
    }
    for mut node in &mut params.image_nodes {
        set_full_map_image_node_size(&mut node, display_size);
    }
    for (button, interaction, mut background, mut border) in &mut params.layer_buttons {
        let selected = button.layer == params.state.selected_layer;
        *background = BackgroundColor(layer_button_color(*interaction, selected));
        *border = BorderColor::all(layer_button_border_color(selected));
    }

    let (player_x, player_y) = params.sim.sim.player().position_tiles();
    let camera_rect = camera_tile_rect(&params.cameras);
    let chunk_cursor = fullscreen_cursor_chunk(crop_bounds, &params.windows, &params.image_layout);
    for overlay_root in &params.overlay_roots {
        rebuild_map_overlay(
            &mut commands,
            overlay_root,
            MapOverlayContext {
                crop_bounds,
                image_size: display_size,
                player_position: Vec2::new(player_x, player_y),
                sim: &params.sim.sim,
                settings: &params.settings,
                layer: params.state.selected_layer,
                camera_rect,
                chunk_cursor,
                markers: &params.markers,
            },
        );
    }
}
