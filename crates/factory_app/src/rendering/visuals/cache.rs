use bevy::prelude::*;
use std::collections::HashMap;

use super::layers::{color_to_unit_array, unit_to_u8};
use super::rasterizer::RasterizedVisual;
use super::templates::VisualTemplate;

pub(super) const MAX_VISUAL_CACHE_ENTRIES: usize = 512;

#[derive(Default, Resource)]
pub(crate) struct VisualAssetCache {
    entries: HashMap<VisualCacheKey, CachedVisual>,
    access_tick: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct VisualCacheKey {
    template: VisualTemplate,
    color: [u8; 4],
    size: [i32; 2],
}

impl VisualCacheKey {
    pub(super) fn new(template: VisualTemplate, color: Color, size: Vec2) -> Self {
        Self {
            template,
            color: color_key(color),
            size: size_key(size),
        }
    }
}

#[derive(Clone)]
pub(super) struct CachedVisual {
    pub(super) handle: Handle<Image>,
    pub(super) visual_size: Vec2,
    last_used: u64,
}

impl VisualAssetCache {
    pub(super) fn get_or_create(
        &mut self,
        key: VisualCacheKey,
        images: &mut Assets<Image>,
        create: impl FnOnce() -> RasterizedVisual,
    ) -> CachedVisual {
        self.access_tick = self.access_tick.wrapping_add(1);
        if self.access_tick == 0 {
            for entry in self.entries.values_mut() {
                entry.last_used = 0;
            }
            self.access_tick = 1;
        }

        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = self.access_tick;
            return entry.clone();
        }

        self.evict_lru_if_full();

        let visual = create();
        let entry = CachedVisual {
            handle: images.add(visual.image),
            visual_size: visual.visual_size,
            last_used: self.access_tick,
        };
        self.entries.insert(key, entry.clone());
        entry
    }

    fn evict_lru_if_full(&mut self) {
        if self.entries.len() < MAX_VISUAL_CACHE_ENTRIES {
            return;
        }

        let Some((&key, _entry)) = self.entries.iter().min_by_key(|(_, entry)| entry.last_used)
        else {
            return;
        };
        self.entries.remove(&key);
    }
}

fn color_key(color: Color) -> [u8; 4] {
    let color = color_to_unit_array(color);
    [
        unit_to_u8(color[0]),
        unit_to_u8(color[1]),
        unit_to_u8(color[2]),
        unit_to_u8(color[3]),
    ]
}

fn size_key(size: Vec2) -> [i32; 2] {
    [
        (size.x * 100.0).round() as i32,
        (size.y * 100.0).round() as i32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    #[test]
    fn size_key_preserves_centimeter_quantization() {
        assert_eq!(size_key(Vec2::new(1.234, 5.675)), [123, 568]);
        assert_eq!(size_key(Vec2::new(-1.235, 0.004)), [-124, 0]);
    }

    #[test]
    fn color_key_preserves_u8_quantization() {
        assert_eq!(
            color_key(Color::srgba(0.0, 0.5, 1.0, 0.25)),
            [0, 128, 255, 64]
        );
    }

    #[test]
    fn cache_returns_cached_handle_for_repeated_key() {
        let mut cache = VisualAssetCache::default();
        let mut images = Assets::<Image>::default();
        let key = cache_key(1);
        let mut create_count = 0;

        let first = cache.get_or_create(key, &mut images, || {
            create_count += 1;
            test_visual(Vec2::splat(1.0))
        });
        let second = cache.get_or_create(key, &mut images, || {
            create_count += 1;
            test_visual(Vec2::splat(2.0))
        });

        assert_eq!(create_count, 1);
        assert_eq!(first.handle, second.handle);
        assert_eq!(second.visual_size, Vec2::splat(1.0));
    }

    #[test]
    fn cache_evicts_least_recently_used_entry_when_full() {
        let mut cache = VisualAssetCache::default();
        let mut images = Assets::<Image>::default();
        let first_key = cache_key(0);
        let lru_key = cache_key(1);

        for index in 0..MAX_VISUAL_CACHE_ENTRIES {
            cache.get_or_create(cache_key(index as i32), &mut images, || {
                test_visual(Vec2::splat(1.0))
            });
        }
        cache.get_or_create(first_key, &mut images, || test_visual(Vec2::splat(2.0)));

        let new_key = cache_key(MAX_VISUAL_CACHE_ENTRIES as i32);
        cache.get_or_create(new_key, &mut images, || test_visual(Vec2::splat(3.0)));

        assert_eq!(cache.entries.len(), MAX_VISUAL_CACHE_ENTRIES);
        assert!(cache.entries.contains_key(&first_key));
        assert!(cache.entries.contains_key(&new_key));
        assert!(!cache.entries.contains_key(&lru_key));
    }

    fn cache_key(index: i32) -> VisualCacheKey {
        VisualCacheKey {
            template: VisualTemplate::BeltItem,
            color: [0, 0, 0, 255],
            size: [index, 0],
        }
    }

    fn test_visual(visual_size: Vec2) -> RasterizedVisual {
        let image = Image::new_fill(
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        RasterizedVisual { image, visual_size }
    }
}
