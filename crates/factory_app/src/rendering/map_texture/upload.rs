use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::{Assets, Image};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::map::resources::MapLayerTextureCache;

use super::pixels::UNREVEALED_PIXEL;

pub(super) fn upload_layer_texture(cache: &mut MapLayerTextureCache, images: &mut Assets<Image>) {
    let bounds = cache.bounds.unwrap_or_default();
    let Some(pixels) = cache.pixels.as_ref() else {
        return;
    };

    if let Some(handle) = cache.handle.as_ref()
        && let Some(mut image) = images.get_mut(handle.id())
        && image.width() == bounds.width
        && image.height() == bounds.height
    {
        match image.data.as_mut() {
            Some(data) => data.clone_from(pixels),
            None => image.data = Some(pixels.clone()),
        }
        return;
    }

    let mut image = Image::new_fill(
        Extent3d {
            width: bounds.width,
            height: bounds.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &UNREVEALED_PIXEL,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(pixels.clone());
    image.sampler = ImageSampler::nearest();

    let handle = match cache.handle.as_ref() {
        Some(handle) => {
            let _ = images.insert(handle.id(), image);
            handle.clone()
        }
        None => images.add(image),
    };
    cache.handle = Some(handle);
}
