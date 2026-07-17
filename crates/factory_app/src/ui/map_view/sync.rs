use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::map::resources::{
    MapDetailCache, MapDetailCacheKey, MapDisplaySettings, MapOverlay, MapOverlayMarkers,
    MapTextureCache, MapTextureLayer, MapViewState,
};
use crate::resources::SimResource;

use super::components::{
    FullMapImage, FullMapOverlayButton, FullMapOverlayRoot, FullMapResourceImage, FullMapRoot,
    MinimapImage, MinimapOverlayRoot, MinimapResourceImage, MinimapRoot,
};
use super::drawing::{
    MINIMAP_CONTENT_SIZE, MapOverlayContext, layer_button_border_color, layer_button_color,
    reconcile_map_overlay, set_full_map_image_node_size, spawn_full_map, spawn_minimap,
};
use super::layout::{
    FULL_MAP_MAX_ZOOM, FULL_MAP_MIN_ZOOM, camera_tile_rect, fullscreen_crop_bounds,
    fullscreen_cursor_chunk, fullscreen_map_display_size, fullscreen_map_image_size,
    minimap_crop_bounds, texture_rect_for_world_bounds,
};

type MinimapResourceImages<'w, 's> = Query<
    'w,
    's,
    (&'static mut ImageNode, &'static mut Node),
    (With<MinimapResourceImage>, Without<MinimapImage>),
>;
type FullMapResourceImages<'w, 's> = Query<
    'w,
    's,
    (&'static mut ImageNode, &'static mut Node),
    (With<FullMapResourceImage>, Without<FullMapImage>),
>;

#[derive(SystemParam)]
pub(crate) struct MinimapSyncParams<'w, 's> {
    cache: Res<'w, MapTextureCache>,
    sim: Res<'w, SimResource>,
    settings: Res<'w, MapDisplaySettings>,
    markers: Res<'w, MapOverlayMarkers>,
    details: ResMut<'w, MapDetailCache>,
    roots: Query<'w, 's, Entity, With<MinimapRoot>>,
    images:
        Query<'w, 's, &'static mut ImageNode, (With<MinimapImage>, Without<MinimapResourceImage>)>,
    resource_images: MinimapResourceImages<'w, 's>,
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
    let player = params.sim.read().player();
    let (player_x, player_y) = player.position_tiles();
    let crop_bounds = minimap_crop_bounds(map_bounds, Vec2::new(player_x, player_y));
    let texture_rect = texture_rect_for_world_bounds(map_bounds, crop_bounds);

    let mut roots_iter = params.roots.iter();
    let Some(_root) = roots_iter.next() else {
        let resources = params
            .cache
            .layer(MapTextureLayer::Resources)
            .and_then(|cache| cache.handle.clone());
        spawn_minimap(&mut commands, handle.clone(), resources, texture_rect);
        return;
    };
    for duplicate in roots_iter {
        commands.entity(duplicate).despawn();
    }

    for mut image in &mut params.images {
        image.image = handle.clone();
        image.rect = Some(texture_rect);
    }
    for (mut image, mut node) in &mut params.resource_images {
        if let Some(resource) = params.cache.layer(MapTextureLayer::Resources)
            && let Some(handle) = &resource.handle
        {
            image.image = handle.clone();
        }
        image.rect = Some(texture_rect);
        node.display = if params.settings.overlays.is_enabled(MapOverlay::Resources) {
            Display::Flex
        } else {
            Display::None
        };
    }

    let camera_rect = camera_tile_rect(&params.cameras);
    for overlay_root in &params.overlay_roots {
        let sim = params.sim.read();
        let key = map_detail_cache_key(
            crop_bounds,
            Vec2::splat(MINIMAP_CONTENT_SIZE),
            (Vec2::new(player_x, player_y), camera_rect, None),
            &sim,
            &params.settings,
            &params.markers,
        );
        let changed_layers = params.details.changed_layers(overlay_root, key);
        if !changed_layers.iter().any(|changed| *changed) {
            continue;
        }
        reconcile_map_overlay(
            &mut commands,
            overlay_root,
            &mut params.details,
            changed_layers,
            MapOverlayContext {
                crop_bounds,
                image_size: Vec2::splat(MINIMAP_CONTENT_SIZE),
                player_position: Vec2::new(player_x, player_y),
                sim: &sim,
                settings: &params.settings,
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
    details: ResMut<'w, MapDetailCache>,
    roots: Query<'w, 's, Entity, With<FullMapRoot>>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    images:
        Query<'w, 's, &'static mut ImageNode, (With<FullMapImage>, Without<FullMapResourceImage>)>,
    resource_images: FullMapResourceImages<'w, 's>,
    image_nodes: Query<'w, 's, &'static mut Node, With<FullMapImage>>,
    image_layout:
        Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<FullMapImage>>,
    overlay_roots: Query<'w, 's, Entity, With<FullMapOverlayRoot>>,
    cameras: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<Camera2d>>,
    layer_buttons: Query<
        'w,
        's,
        (
            &'static FullMapOverlayButton,
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

    let Some(layer_cache) = params.cache.layer(MapTextureLayer::Surface) else {
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
            params
                .cache
                .layer(MapTextureLayer::Resources)
                .and_then(|cache| cache.handle.clone()),
            texture_rect,
            display_size,
            params.settings.overlays,
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
    for (mut image, mut node) in &mut params.resource_images {
        if let Some(resource) = params.cache.layer(MapTextureLayer::Resources)
            && let Some(handle) = &resource.handle
        {
            image.image = handle.clone();
        }
        image.rect = Some(texture_rect);
        node.display = if params.settings.overlays.is_enabled(MapOverlay::Resources) {
            Display::Flex
        } else {
            Display::None
        };
    }
    for mut node in &mut params.image_nodes {
        set_full_map_image_node_size(&mut node, display_size);
    }
    for (button, interaction, mut background, mut border) in &mut params.layer_buttons {
        let selected = params.settings.overlays.is_enabled(button.overlay);
        *background = BackgroundColor(layer_button_color(*interaction, selected));
        *border = BorderColor::all(layer_button_border_color(selected));
    }

    let (player_x, player_y) = params.sim.read().player().position_tiles();
    let camera_rect = camera_tile_rect(&params.cameras);
    let chunk_cursor = fullscreen_cursor_chunk(crop_bounds, &params.windows, &params.image_layout);
    for overlay_root in &params.overlay_roots {
        let sim = params.sim.read();
        let key = map_detail_cache_key(
            crop_bounds,
            display_size,
            (Vec2::new(player_x, player_y), camera_rect, chunk_cursor),
            &sim,
            &params.settings,
            &params.markers,
        );
        let changed_layers = params.details.changed_layers(overlay_root, key);
        if !changed_layers.iter().any(|changed| *changed) {
            continue;
        }
        reconcile_map_overlay(
            &mut commands,
            overlay_root,
            &mut params.details,
            changed_layers,
            MapOverlayContext {
                crop_bounds,
                image_size: display_size,
                player_position: Vec2::new(player_x, player_y),
                sim: &sim,
                settings: &params.settings,
                camera_rect,
                chunk_cursor,
                markers: &params.markers,
            },
        );
    }
}

pub(super) fn map_detail_cache_key(
    crop_bounds: crate::map::resources::MapTextureBounds,
    image_size: Vec2,
    navigation: (
        Vec2,
        Option<super::layout::MapTileRect>,
        Option<factory_sim::ChunkCoord>,
    ),
    sim: &factory_sim::Simulation,
    settings: &MapDisplaySettings,
    markers: &MapOverlayMarkers,
) -> MapDetailCacheKey {
    let (player, camera, chunk_cursor) = navigation;
    MapDetailCacheKey {
        crop_bounds,
        image_size_bits: (image_size.x.to_bits(), image_size.y.to_bits()),
        player_bits: (player.x.to_bits(), player.y.to_bits()),
        camera_bits: camera.map(|rect| {
            (
                rect.min.x.to_bits(),
                rect.min.y.to_bits(),
                rect.max.x.to_bits(),
                rect.max.y.to_bits(),
            )
        }),
        chunk_cursor,
        overlay_bits: settings.overlays.enabled_bits(),
        debug_reveal_all: settings.debug_reveal_all,
        reveal_revision: sim.revealed_revision(),
        topology_revision: sim.entity_topology_revision(),
        pollution_revision: sim.pollution_map_revision(),
        enemy_revision: sim.enemy_map_revision(),
        power_revision: sim.power_map_revision(),
        production_revision: sim.production_status_revision(),
        marker_signature: marker_signature(markers),
    }
}

fn marker_signature(markers: &MapOverlayMarkers) -> u64 {
    let mut hasher = DefaultHasher::new();
    markers.pings.len().hash(&mut hasher);
    for marker in &markers.pings {
        marker.position.x.to_bits().hash(&mut hasher);
        marker.position.y.to_bits().hash(&mut hasher);
        let color = marker.color.to_srgba();
        color.red.to_bits().hash(&mut hasher);
        color.green.to_bits().hash(&mut hasher);
        color.blue.to_bits().hash(&mut hasher);
        color.alpha.to_bits().hash(&mut hasher);
    }
    markers.waypoints.len().hash(&mut hasher);
    for marker in &markers.waypoints {
        marker.position.x.to_bits().hash(&mut hasher);
        marker.position.y.to_bits().hash(&mut hasher);
        let color = marker.color.to_srgba();
        color.red.to_bits().hash(&mut hasher);
        color.green.to_bits().hash(&mut hasher);
        color.blue.to_bits().hash(&mut hasher);
        color.alpha.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}
