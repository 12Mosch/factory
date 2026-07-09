use bevy::asset::{AssetId, RenderAssetUsages};
use bevy::image::ImageSampler;
use bevy::prelude::{Assets, Image, IntoScheduleConfigs, Resource, UVec2};
use bevy::render::render_asset::{RenderAssets, prepare_assets};
use bevy::render::render_resource::{
    Extent3d, Origin3d, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureDimension,
    TextureFormat,
};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy::render::{ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems};

use crate::map::resources::{MapLayerTextureCache, MapTextureUploadRect};

use super::pixels::UNREVEALED_PIXEL;

#[derive(Clone, Debug)]
pub(crate) struct MapTextureUploadCommand {
    pub image_id: AssetId<Image>,
    pub texture_size: UVec2,
    pub rect: MapTextureUploadRect,
    pub data: Vec<u8>,
}

#[derive(Resource, Default)]
pub(crate) struct MapTextureUploadQueue {
    pub commands: Vec<MapTextureUploadCommand>,
}

#[derive(Resource, Default)]
pub(crate) struct ExtractedMapTextureUploads {
    commands: Vec<MapTextureUploadCommand>,
}

pub(crate) fn register_map_texture_upload_systems(app: &mut bevy::prelude::App) {
    app.init_resource::<MapTextureUploadQueue>();

    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };

    render_app
        .init_resource::<ExtractedMapTextureUploads>()
        .add_systems(ExtractSchedule, extract_map_texture_uploads)
        .add_systems(
            Render,
            apply_map_texture_uploads
                .in_set(RenderSystems::PrepareResources)
                .after(prepare_assets::<GpuImage>),
        );
}

pub(super) fn upload_layer_texture(
    cache: &mut MapLayerTextureCache,
    images: &mut Assets<Image>,
    uploads: &mut MapTextureUploadQueue,
) {
    let bounds = cache.bounds.unwrap_or_default();
    let texture_size = UVec2::new(bounds.width, bounds.height);
    let Some(pixels) = cache.pixels.as_ref() else {
        return;
    };

    let full_upload = cache.dirty_regions.is_full()
        || cache
            .handle
            .as_ref()
            .and_then(|handle| images.get(handle.id()))
            .is_none_or(|image| image.width() != bounds.width || image.height() != bounds.height);

    if full_upload {
        let pixels = pixels.clone();
        replace_layer_image(cache, images, &pixels, bounds.width, bounds.height);
        cache.dirty_regions.clear();
        return;
    }

    if cache.dirty_regions.is_empty() {
        return;
    }

    let Some(handle) = cache.handle.as_ref() else {
        let pixels = pixels.clone();
        replace_layer_image(cache, images, &pixels, bounds.width, bounds.height);
        cache.dirty_regions.clear();
        return;
    };
    let image_id = handle.id();

    for rect in cache.dirty_regions.take_rects() {
        uploads.commands.push(MapTextureUploadCommand {
            image_id,
            texture_size,
            rect,
            data: pack_rect_pixels(pixels, bounds.width, rect),
        });
    }
}

fn replace_layer_image(
    cache: &mut MapLayerTextureCache,
    images: &mut Assets<Image>,
    pixels: &[u8],
    width: u32,
    height: u32,
) {
    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &UNREVEALED_PIXEL,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(pixels.to_vec());
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

fn pack_rect_pixels(pixels: &[u8], texture_width: u32, rect: MapTextureUploadRect) -> Vec<u8> {
    let row_bytes = rect.width as usize * 4;
    let mut data = Vec::with_capacity(row_bytes * rect.height as usize);
    for row in rect.y..rect.y + rect.height {
        let offset = ((row * texture_width + rect.x) * 4) as usize;
        data.extend_from_slice(&pixels[offset..offset + row_bytes]);
    }
    data
}

fn extract_map_texture_uploads(
    mut uploads: bevy::prelude::ResMut<ExtractedMapTextureUploads>,
    mut main_world: bevy::prelude::ResMut<MainWorld>,
) {
    let mut queue = main_world.resource_mut::<MapTextureUploadQueue>();
    uploads.commands.append(&mut queue.commands);
}

fn apply_map_texture_uploads(
    mut uploads: bevy::prelude::ResMut<ExtractedMapTextureUploads>,
    gpu_images: bevy::prelude::Res<RenderAssets<GpuImage>>,
    render_queue: bevy::prelude::Res<RenderQueue>,
) {
    uploads.commands = filter_pending_map_texture_uploads(
        std::mem::take(&mut uploads.commands),
        |image_id| gpu_images.get(image_id),
        |gpu_image, command| {
            render_queue.write_texture(
                image_copy_with_origin(gpu_image, command.rect),
                &command.data,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(command.rect.width * 4),
                    rows_per_image: Some(command.rect.height),
                },
                Extent3d {
                    width: command.rect.width,
                    height: command.rect.height,
                    depth_or_array_layers: 1,
                },
            );
        },
    );
}

fn filter_pending_map_texture_uploads<'a, F, W>(
    commands: Vec<MapTextureUploadCommand>,
    mut gpu_image: F,
    mut write: W,
) -> Vec<MapTextureUploadCommand>
where
    F: FnMut(AssetId<Image>) -> Option<&'a GpuImage>,
    W: FnMut(&GpuImage, &MapTextureUploadCommand),
{
    let mut retained = Vec::new();
    for command in commands {
        let Some(gpu_image) = gpu_image(command.image_id) else {
            retained.push(command);
            continue;
        };

        if is_stale_for_texture_size(&command, gpu_image_size(gpu_image)) {
            continue;
        }

        write(gpu_image, &command);
    }
    retained
}

fn gpu_image_size(gpu_image: &GpuImage) -> UVec2 {
    UVec2::new(
        gpu_image.texture_descriptor.size.width,
        gpu_image.texture_descriptor.size.height,
    )
}

fn image_copy_with_origin(
    gpu_image: &GpuImage,
    rect: MapTextureUploadRect,
) -> TexelCopyTextureInfo<'_> {
    TexelCopyTextureInfo {
        origin: Origin3d {
            x: rect.x,
            y: rect.y,
            z: 0,
        },
        ..gpu_image.texture.as_image_copy()
    }
}

fn is_stale_for_texture_size(command: &MapTextureUploadCommand, texture_size: UVec2) -> bool {
    command.texture_size != texture_size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::resources::{MapTextureBounds, MapTextureDirtyRegions};

    fn image_asset(width: u32, height: u32, data: Option<Vec<u8>>) -> Image {
        let mut image = Image::new_fill(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &UNREVEALED_PIXEL,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        image.data = data;
        image.sampler = ImageSampler::nearest();
        image
    }

    #[test]
    fn stale_upload_commands_are_filtered_by_texture_size() {
        let image_id = AssetId::<Image>::default();
        let stale = MapTextureUploadCommand {
            image_id,
            texture_size: UVec2::new(2, 2),
            rect: MapTextureUploadRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            data: vec![1, 2, 3, 4],
        };

        let retained = filter_pending_map_texture_uploads(
            vec![stale],
            |_| None,
            |_, _| panic!("missing GpuImage must not be written"),
        );

        assert_eq!(retained.len(), 1);
    }

    #[test]
    fn upload_command_size_mismatch_is_stale() {
        let command = MapTextureUploadCommand {
            image_id: AssetId::<Image>::default(),
            texture_size: UVec2::new(4, 8),
            rect: MapTextureUploadRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            data: vec![1, 2, 3, 4],
        };

        assert!(is_stale_for_texture_size(&command, UVec2::new(8, 4)));
        assert!(!is_stale_for_texture_size(&command, UVec2::new(4, 8)));
    }

    #[test]
    fn resource_dirty_tile_queues_single_pixel_upload() {
        let bounds = MapTextureBounds {
            min_x: -2,
            min_y: -3,
            width: 5,
            height: 7,
        };
        let mut pixels = vec![0; bounds.width as usize * bounds.height as usize * 4];
        let offset = ((2 * bounds.width + 2) * 4) as usize;
        pixels[offset..offset + 4].copy_from_slice(&[9, 8, 7, 6]);

        let mut images = Assets::<Image>::default();
        let handle = images.add(image_asset(bounds.width, bounds.height, None));
        let mut dirty_regions = MapTextureDirtyRegions::default();
        dirty_regions.mark_world_tile(bounds, 0, 1);
        let mut cache = MapLayerTextureCache {
            handle: Some(handle.clone()),
            bounds: Some(bounds),
            pixels: Some(pixels),
            dirty_regions,
            ..Default::default()
        };
        let mut uploads = MapTextureUploadQueue::default();

        upload_layer_texture(&mut cache, &mut images, &mut uploads);

        assert_eq!(uploads.commands.len(), 1);
        let command = &uploads.commands[0];
        assert_eq!(
            command.rect,
            MapTextureUploadRect {
                x: 2,
                y: 2,
                width: 1,
                height: 1,
            }
        );
        assert_eq!(command.data, vec![9, 8, 7, 6]);
        assert_eq!(command.data.len(), 4);
        assert!(
            images
                .get(handle.id())
                .is_some_and(|image| image.data.is_none())
        );
        assert!(cache.dirty_regions.is_empty());
    }

    #[test]
    fn resize_or_bounds_growth_uses_full_upload() {
        let old_bounds = MapTextureBounds {
            min_x: 0,
            min_y: 0,
            width: 2,
            height: 2,
        };
        let new_bounds = MapTextureBounds {
            min_x: 0,
            min_y: 0,
            width: 3,
            height: 2,
        };
        let mut images = Assets::<Image>::default();
        let handle = images.add(image_asset(old_bounds.width, old_bounds.height, None));
        let pixels = vec![42; new_bounds.width as usize * new_bounds.height as usize * 4];
        let mut dirty_regions = MapTextureDirtyRegions::default();
        dirty_regions.mark_full();
        let mut cache = MapLayerTextureCache {
            handle: Some(handle.clone()),
            bounds: Some(new_bounds),
            pixels: Some(pixels.clone()),
            dirty_regions,
            ..Default::default()
        };
        let mut uploads = MapTextureUploadQueue::default();

        upload_layer_texture(&mut cache, &mut images, &mut uploads);

        let image = images.get(handle.id()).expect("image should be replaced");
        assert_eq!(image.width(), new_bounds.width);
        assert_eq!(image.height(), new_bounds.height);
        assert_eq!(image.data.as_deref(), Some(pixels.as_slice()));
        assert!(uploads.commands.is_empty());
        assert!(cache.dirty_regions.is_empty());
    }
}
