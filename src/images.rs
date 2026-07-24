use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};

use crate::config::CloudsConfig;

/// Every cloud storage texture uses `Rgba16Float`. Cloud colour, density and
/// transmittance never need the dynamic range of `f32`, so half floats halve
/// both VRAM and — more importantly — the per-frame read/write bandwidth of the
/// raymarch (the atlas and worley LUTs are sampled hundreds of times per pixel).
const CLOUD_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

/// One zeroed `Rgba16Float` texel (4 channels x 2 bytes) used as the fill
/// pattern when allocating a storage image.
const ZERO_TEXEL: [u8; 8] = [0; 8];

pub(crate) fn cloud_output_size(config: &CloudsConfig) -> UVec2 {
    UVec2::new(
        config.render_resolution.x.ceil().max(1.0) as u32,
        config.render_resolution.y.ceil().max(1.0) as u32,
    )
}

/// Builds a 2D `Rgba16Float` read-write storage image sized to `size`. Used for
/// the render accumulation buffer, the base-noise atlas and the sky buffer.
pub(crate) fn build_cloud_output_image(size: UVec2) -> Image {
    build_storage_image(
        Extent3d {
            width: size.x.max(1),
            height: size.y.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
    )
}

fn build_storage_image(extent: Extent3d, dimension: TextureDimension) -> Image {
    let mut image = Image::new_fill(
        extent,
        dimension,
        &ZERO_TEXEL,
        CLOUD_TEXTURE_FORMAT,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::COPY_DST | TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING;
    image
}

pub(crate) fn upsert_cloud_output_image(
    images: &mut Assets<Image>,
    handle: &mut Handle<Image>,
    size: UVec2,
) {
    let image = build_cloud_output_image(size);
    if let Some(mut existing) = images.get_mut(&*handle) {
        *existing = image;
    } else {
        *handle = images.add(image);
    }
}

pub fn build_images(
    mut images: ResMut<Assets<Image>>,
    output_size: UVec2,
) -> (Handle<Image>, Handle<Image>, Handle<Image>, Handle<Image>) {
    let cloud_render_image = build_cloud_output_image(output_size);

    // The base-noise atlas is only ever sampled `% render_resolution` by the
    // compute shader, so it only needs to be `render_resolution`-sized — not a
    // fixed 1920x1920. Sizing it to the output resolution (like the render/sky
    // buffers) is behaviourally identical but frees tens of MB of VRAM.
    let cloud_atlas_image = build_cloud_output_image(output_size);

    let cloud_worley_image = build_storage_image(
        Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 32,
        },
        TextureDimension::D3,
    );

    let sky_image = build_cloud_output_image(output_size);

    (
        images.add(cloud_render_image),
        images.add(cloud_atlas_image),
        images.add(cloud_worley_image),
        images.add(sky_image),
    )
}
