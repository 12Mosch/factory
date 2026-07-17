use bevy::prelude::{Color, Handle, Image, Resource, Vec2};
use factory_sim::{CHUNK_SIZE, ChunkCoord, WorldTileCoord};
use std::collections::{BTreeMap, BTreeSet};

const MAX_DIRTY_REGION_RECTS: usize = 512;
pub const MAX_MAP_TEXTURE_SIDE_TILES: u32 = 2048;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MapTextureLayer {
    #[default]
    Surface,
    Resources,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MapOverlay {
    Pollution,
    Resources,
    PowerNetworks,
    ProductionProblems,
    Enemies,
    ConstructionPlans,
}

impl MapOverlay {
    pub const ALL: [Self; 6] = [
        Self::Pollution,
        Self::Resources,
        Self::PowerNetworks,
        Self::ProductionProblems,
        Self::Enemies,
        Self::ConstructionPlans,
    ];
}

const MAP_OVERLAY_COUNT: usize = MapOverlay::ALL.len();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapOverlaySettings {
    enabled: [bool; MAP_OVERLAY_COUNT],
}

impl Default for MapOverlaySettings {
    fn default() -> Self {
        let mut settings = Self {
            enabled: [false; MAP_OVERLAY_COUNT],
        };
        settings.set_enabled(MapOverlay::Resources, true);
        settings.set_enabled(MapOverlay::Enemies, true);
        settings.set_enabled(MapOverlay::ConstructionPlans, true);
        settings
    }
}

impl MapOverlaySettings {
    pub fn is_enabled(self, overlay: MapOverlay) -> bool {
        self.enabled[overlay as usize]
    }

    pub fn set_enabled(&mut self, overlay: MapOverlay, enabled: bool) {
        self.enabled[overlay as usize] = enabled;
    }

    pub fn toggle(&mut self, overlay: MapOverlay) -> bool {
        let enabled = !self.is_enabled(overlay);
        self.set_enabled(overlay, enabled);
        enabled
    }

    pub(crate) fn enabled_bits(self) -> u64 {
        self.enabled
            .into_iter()
            .enumerate()
            .fold(0, |bits, (index, enabled)| {
                bits | (u64::from(enabled) << index)
            })
    }
}

#[derive(Resource)]
pub struct MapViewState {
    pub open: bool,
    pub center_tile: Vec2,
    pub zoom: f32,
    pub follow_player: bool,
}

impl Default for MapViewState {
    fn default() -> Self {
        Self {
            open: false,
            center_tile: Vec2::ZERO,
            zoom: 1.0,
            follow_player: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapPointMarker {
    pub position: Vec2,
    pub color: Color,
}

#[derive(Resource, Default)]
pub struct MapOverlayMarkers {
    pub pings: Vec<MapPointMarker>,
    pub waypoints: Vec<MapPointMarker>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MapDetailCacheKey {
    pub crop_bounds: MapTextureBounds,
    pub image_size_bits: (u32, u32),
    pub player_bits: (u32, u32),
    pub camera_bits: Option<(u32, u32, u32, u32)>,
    pub chunk_cursor: Option<ChunkCoord>,
    pub overlay_bits: u64,
    pub debug_reveal_all: bool,
    pub reveal_revision: u64,
    pub topology_revision: u64,
    pub pollution_revision: u64,
    pub enemy_revision: u64,
    pub power_revision: u64,
    pub production_revision: u64,
    pub marker_signature: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MapOverlayLayer {
    Navigation,
    Pollution,
    Entities,
    Power,
    ProductionProblems,
    Enemies,
    Construction,
    Player,
    Markers,
}

impl MapOverlayLayer {
    pub(crate) const ALL: [Self; 9] = [
        Self::Navigation,
        Self::Pollution,
        Self::Entities,
        Self::Power,
        Self::ProductionProblems,
        Self::Enemies,
        Self::Construction,
        Self::Player,
        Self::Markers,
    ];

    const COUNT: usize = Self::ALL.len();

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Default)]
struct MapDetailCacheEntry {
    key: Option<MapDetailCacheKey>,
    layer_entities: [Vec<bevy::prelude::Entity>; MapOverlayLayer::COUNT],
}

#[derive(Resource, Default)]
pub struct MapDetailCache {
    entries: BTreeMap<bevy::prelude::Entity, MapDetailCacheEntry>,
}

impl MapDetailCache {
    pub(crate) fn changed_layers(
        &mut self,
        root: bevy::prelude::Entity,
        key: MapDetailCacheKey,
    ) -> [bool; MapOverlayLayer::COUNT] {
        let entry = self.entries.entry(root).or_default();
        let Some(previous) = entry.key.replace(key) else {
            return [true; MapOverlayLayer::COUNT];
        };

        let geometry_changed = previous.crop_bounds != key.crop_bounds
            || previous.image_size_bits != key.image_size_bits;
        let visibility_changed = previous.debug_reveal_all != key.debug_reveal_all
            || previous.reveal_revision != key.reveal_revision;
        let topology_changed = previous.topology_revision != key.topology_revision;
        let overlay_changed = |overlay: MapOverlay| {
            let bit = 1_u64 << overlay as usize;
            (previous.overlay_bits ^ key.overlay_bits) & bit != 0
        };
        let overlay_enabled = |overlay: MapOverlay| {
            let bit = 1_u64 << overlay as usize;
            key.overlay_bits & bit != 0
        };

        [
            geometry_changed
                || previous.camera_bits != key.camera_bits
                || previous.chunk_cursor != key.chunk_cursor,
            geometry_changed
                || visibility_changed
                || (overlay_enabled(MapOverlay::Pollution)
                    && previous.pollution_revision != key.pollution_revision)
                || overlay_changed(MapOverlay::Pollution),
            geometry_changed || visibility_changed || topology_changed,
            geometry_changed
                || visibility_changed
                || topology_changed
                || (overlay_enabled(MapOverlay::PowerNetworks)
                    && previous.power_revision != key.power_revision)
                || overlay_changed(MapOverlay::PowerNetworks),
            geometry_changed
                || visibility_changed
                || topology_changed
                || (overlay_enabled(MapOverlay::ProductionProblems)
                    && previous.production_revision != key.production_revision)
                || overlay_changed(MapOverlay::ProductionProblems),
            geometry_changed
                || visibility_changed
                || topology_changed
                || (overlay_enabled(MapOverlay::Enemies)
                    && previous.enemy_revision != key.enemy_revision)
                || overlay_changed(MapOverlay::Enemies),
            geometry_changed
                || visibility_changed
                || topology_changed
                || overlay_changed(MapOverlay::ConstructionPlans),
            geometry_changed || previous.player_bits != key.player_bits,
            geometry_changed || previous.marker_signature != key.marker_signature,
        ]
    }

    pub(crate) fn layer_entities_mut(
        &mut self,
        root: bevy::prelude::Entity,
        layer: MapOverlayLayer,
    ) -> &mut Vec<bevy::prelude::Entity> {
        &mut self.entries.entry(root).or_default().layer_entities[layer.index()]
    }

    pub(crate) fn remove_root(&mut self, root: bevy::prelude::Entity) -> bool {
        self.entries.remove(&root).is_some()
    }

    pub fn clear(&mut self) {
        for entry in self.entries.values_mut() {
            entry.key = None;
        }
    }
}

#[derive(Clone, Copy, Debug, Resource, Default)]
pub struct MapDisplaySettings {
    pub debug_reveal_all: bool,
    pub show_chunk_grid: bool,
    pub overlays: MapOverlaySettings,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapTextureBounds {
    pub min_x: WorldTileCoord,
    pub min_y: WorldTileCoord,
    pub width: u32,
    pub height: u32,
}

impl MapTextureBounds {
    pub fn contains_tile(self, tile: (WorldTileCoord, WorldTileCoord)) -> bool {
        self.contains_tile_wide(tile)
    }

    pub fn contains_chunk(self, coord: ChunkCoord) -> bool {
        let chunk_size = i64::from(CHUNK_SIZE);
        let (min_x, min_y) = coord.min_tile();
        let max_x = (i64::from(coord.x) + 1) * chunk_size - 1;
        let max_y = (i64::from(coord.y) + 1) * chunk_size - 1;

        self.contains_tile_wide((min_x, min_y)) && self.contains_tile_wide((max_x, max_y))
    }

    fn contains_tile_wide(self, tile: (i64, i64)) -> bool {
        let min_x = self.min_x;
        let min_y = self.min_y;
        let max_x = min_x + i64::from(self.width);
        let max_y = min_y + i64::from(self.height);

        tile.0 >= min_x && tile.0 < max_x && tile.1 >= min_y && tile.1 < max_y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail_key(overlays: MapOverlaySettings) -> MapDetailCacheKey {
        MapDetailCacheKey {
            crop_bounds: MapTextureBounds::default(),
            image_size_bits: (176.0_f32.to_bits(), 176.0_f32.to_bits()),
            player_bits: (0, 0),
            camera_bits: None,
            chunk_cursor: None,
            overlay_bits: overlays.enabled_bits(),
            debug_reveal_all: false,
            reveal_revision: 0,
            topology_revision: 0,
            pollution_revision: 0,
            enemy_revision: 0,
            power_revision: 0,
            production_revision: 0,
            marker_signature: 0,
        }
    }

    #[test]
    fn overlay_defaults_enable_resources_enemies_and_plans_only() {
        let settings = MapOverlaySettings::default();
        for overlay in MapOverlay::ALL {
            assert_eq!(
                settings.is_enabled(overlay),
                matches!(
                    overlay,
                    MapOverlay::Resources | MapOverlay::Enemies | MapOverlay::ConstructionPlans
                )
            );
        }
    }

    #[test]
    fn overlay_toggles_are_independent() {
        let mut settings = MapOverlaySettings::default();
        settings.toggle(MapOverlay::Pollution);
        assert!(settings.is_enabled(MapOverlay::Pollution));
        assert!(settings.is_enabled(MapOverlay::Resources));
        settings.set_enabled(MapOverlay::Enemies, false);
        assert!(!settings.is_enabled(MapOverlay::Enemies));
        assert!(settings.is_enabled(MapOverlay::ConstructionPlans));
    }

    #[test]
    fn detail_cache_invalidates_only_the_enabled_changed_subsystem() {
        let mut cache = MapDetailCache::default();
        let root = bevy::prelude::Entity::PLACEHOLDER;
        let key = detail_key(MapOverlaySettings::default());
        assert!(
            cache
                .changed_layers(root, key)
                .into_iter()
                .all(|changed| changed)
        );
        assert!(
            cache
                .changed_layers(root, key)
                .into_iter()
                .all(|changed| !changed)
        );

        let mut disabled_production_changed = key;
        disabled_production_changed.production_revision = 1;
        assert!(
            cache
                .changed_layers(root, disabled_production_changed)
                .into_iter()
                .all(|changed| !changed),
            "disabled overlays must not wake up for simulation revisions"
        );

        let mut enabled_enemy_changed = disabled_production_changed;
        enabled_enemy_changed.enemy_revision = 1;
        let changed = cache.changed_layers(root, enabled_enemy_changed);
        for (index, layer) in MapOverlayLayer::ALL.into_iter().enumerate() {
            assert_eq!(changed[index], layer == MapOverlayLayer::Enemies);
        }
    }

    #[test]
    fn detail_cache_removes_all_state_for_despawned_root() {
        let mut cache = MapDetailCache::default();
        let root = bevy::prelude::Entity::PLACEHOLDER;
        cache.changed_layers(root, detail_key(MapOverlaySettings::default()));
        cache
            .layer_entities_mut(root, MapOverlayLayer::Player)
            .push(root);

        assert!(cache.remove_root(root));
        assert!(!cache.remove_root(root));
        assert!(
            cache
                .changed_layers(root, detail_key(MapOverlaySettings::default()))
                .into_iter()
                .all(|changed| changed)
        );
        assert!(
            cache
                .layer_entities_mut(root, MapOverlayLayer::Player)
                .is_empty()
        );
    }

    #[test]
    fn map_texture_bounds_contains_tile_handles_extreme_edges() {
        let bounds = MapTextureBounds {
            min_x: i64::from(i32::MAX),
            min_y: i64::from(i32::MIN),
            width: 1,
            height: 1,
        };

        assert!(bounds.contains_tile((i64::from(i32::MAX), i64::from(i32::MIN))));
        assert!(!bounds.contains_tile((i64::from(i32::MAX - 1), i64::from(i32::MIN))));
        assert!(!bounds.contains_tile((i64::from(i32::MAX), i64::from(i32::MIN + 1))));
    }

    #[test]
    fn map_texture_bounds_contains_chunk_handles_extreme_coords() {
        let bounds = MapTextureBounds {
            min_x: i64::from(i32::MIN),
            min_y: i64::from(i32::MIN),
            width: u32::MAX,
            height: u32::MAX,
        };

        assert!(!bounds.contains_chunk(ChunkCoord {
            x: i32::MAX,
            y: i32::MAX,
        }));
        assert!(!bounds.contains_chunk(ChunkCoord {
            x: i32::MIN,
            y: i32::MIN,
        }));
    }

    #[test]
    fn dirty_tile_rect_uses_bevy_image_coordinates() {
        let bounds = MapTextureBounds {
            min_x: -3,
            min_y: -2,
            width: 8,
            height: 6,
        };
        let mut regions = MapTextureDirtyRegions::default();

        regions.mark_world_tile(bounds, 2, 1);

        assert_eq!(
            regions.rects(),
            &[MapTextureUploadRect {
                x: 5,
                y: 2,
                width: 1,
                height: 1,
            }]
        );
    }

    #[test]
    fn dirty_chunk_rect_covers_only_changed_chunk() {
        let bounds = MapTextureBounds {
            min_x: -20,
            min_y: -8,
            width: 30,
            height: 20,
        };
        let mut regions = MapTextureDirtyRegions::default();

        regions.mark_world_chunk(bounds, ChunkCoord { x: -1, y: 0 });

        assert_eq!(
            regions.rects(),
            &[MapTextureUploadRect {
                x: 0,
                y: 0,
                width: 20,
                height: 12,
            }]
        );
    }
}

#[derive(Resource, Default)]
pub struct MapTextureCache {
    pub layers: BTreeMap<MapTextureLayer, MapLayerTextureCache>,
}

impl MapTextureCache {
    pub fn layer(&self, layer: MapTextureLayer) -> Option<&MapLayerTextureCache> {
        self.layers.get(&layer)
    }

    pub fn surface(&self) -> Option<&MapLayerTextureCache> {
        self.layer(MapTextureLayer::Surface)
    }

    pub fn layer_mut(&mut self, layer: MapTextureLayer) -> &mut MapLayerTextureCache {
        self.layers.entry(layer).or_default()
    }
}

#[derive(Default)]
pub struct MapLayerTextureCache {
    pub handle: Option<Handle<Image>>,
    pub bounds: Option<MapTextureBounds>,
    pub pixels: Option<Vec<u8>>,
    pub dirty_regions: MapTextureDirtyRegions,
    pub painted_chunks: BTreeMap<ChunkCoord, MapChunkPaintState>,
    pub last_chunk_revision: u64,
    pub last_resource_revision: u64,
    pub last_revealed_revision: u64,
    pub last_debug_flags: (bool, bool),
    pub last_texture_update_tick: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MapTextureUploadRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapTextureDirtyRegions {
    full: bool,
    rects: Vec<MapTextureUploadRect>,
}

impl MapTextureDirtyRegions {
    pub(crate) fn mark_full(&mut self) {
        self.full = true;
        self.rects.clear();
    }

    pub(crate) fn mark_world_tile(
        &mut self,
        bounds: MapTextureBounds,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) {
        let Some((image_x, image_y)) = world_tile_to_image(bounds, x, y) else {
            return;
        };
        self.push_rect(MapTextureUploadRect {
            x: image_x,
            y: image_y,
            width: 1,
            height: 1,
        });
    }

    pub(crate) fn mark_world_chunk(&mut self, bounds: MapTextureBounds, coord: ChunkCoord) {
        let chunk_size = i64::from(CHUNK_SIZE);
        let (chunk_min_x, chunk_min_y) = coord.min_tile();
        let chunk_max_x = chunk_min_x + chunk_size - 1;
        let chunk_max_y = chunk_min_y + chunk_size - 1;

        let bounds_min_x = bounds.min_x;
        let bounds_min_y = bounds.min_y;
        let bounds_max_x = bounds_min_x + i64::from(bounds.width) - 1;
        let bounds_max_y = bounds_min_y + i64::from(bounds.height) - 1;

        let min_x = chunk_min_x.max(bounds_min_x);
        let max_x = chunk_max_x.min(bounds_max_x);
        let min_y = chunk_min_y.max(bounds_min_y);
        let max_y = chunk_max_y.min(bounds_max_y);
        if min_x > max_x || min_y > max_y {
            return;
        }

        let image_x = (min_x - bounds_min_x) as u32;
        let image_top_y = (bounds_max_y - max_y) as u32;
        self.push_rect(MapTextureUploadRect {
            x: image_x,
            y: image_top_y,
            width: (max_x - min_x + 1) as u32,
            height: (max_y - min_y + 1) as u32,
        });
    }

    pub(crate) fn clear(&mut self) {
        self.full = false;
        self.rects.clear();
    }

    pub(crate) fn is_empty(&self) -> bool {
        !self.full && self.rects.is_empty()
    }

    pub(crate) fn is_full(&self) -> bool {
        self.full
    }

    #[cfg(test)]
    pub(crate) fn rects(&self) -> &[MapTextureUploadRect] {
        &self.rects
    }

    pub(crate) fn take_rects(&mut self) -> Vec<MapTextureUploadRect> {
        self.full = false;
        std::mem::take(&mut self.rects)
    }

    fn push_rect(&mut self, rect: MapTextureUploadRect) {
        if self.full {
            return;
        }

        if rect.width == 0 || rect.height == 0 {
            return;
        }

        if let Some(existing) = self.rects.iter_mut().find(|existing| {
            existing.y == rect.y
                && existing.height == rect.height
                && rect.x <= existing.x.saturating_add(existing.width)
                && existing.x <= rect.x.saturating_add(rect.width)
        }) {
            let min_x = existing.x.min(rect.x);
            let max_x = existing
                .x
                .saturating_add(existing.width)
                .max(rect.x.saturating_add(rect.width));
            existing.x = min_x;
            existing.width = max_x - min_x;
            return;
        }

        self.rects.push(rect);
        if self.rects.len() > MAX_DIRTY_REGION_RECTS {
            self.mark_full();
        }
    }
}

fn world_tile_to_image(
    bounds: MapTextureBounds,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> Option<(u32, u32)> {
    if !bounds.contains_tile((x, y)) {
        return None;
    }
    let image_x = (x - bounds.min_x) as u32;
    let image_y = bounds.height - 1 - (y - bounds.min_y) as u32;
    Some((image_x, image_y))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapChunkPaintState {
    pub revealed: bool,
}

#[derive(Resource, Default)]
pub struct VisibleChunks {
    pub chunks: BTreeSet<ChunkCoord>,
    pub tile_bounds: Option<MapTextureBounds>,
    pub revision: u64,
}
