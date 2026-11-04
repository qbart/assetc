//! SDK values to Bevy resources.

use assetc_sdk as sdk;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDataOrder, TextureDimension, TextureFormat, TextureViewDescriptor,
    TextureViewDimension,
};

/// A texture with all its mip levels: 2D, cubemap (six layers) or volume.
pub fn image(t: &sdk::Texture) -> Image {
    let format = match t.format {
        sdk::texture::Format::Rgba8Srgb => TextureFormat::Rgba8UnormSrgb,
        sdk::texture::Format::Rgba8Unorm => TextureFormat::Rgba8Unorm,
        sdk::texture::Format::Rgba16Float => TextureFormat::Rgba16Float,
    };
    let (dimension, layers) = if t.is_volume() {
        (TextureDimension::D3, t.depth)
    } else {
        (TextureDimension::D2, t.faces)
    };
    let mut image = Image::new_uninit(
        Extent3d {
            width: t.width,
            height: t.height,
            depth_or_array_layers: layers,
        },
        dimension,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    // KTX2 keeps each level's faces together: mip-major.
    image.data = Some(
        t.levels
            .iter()
            .flat_map(|l| l.data.iter().copied())
            .collect(),
    );
    image.data_order = TextureDataOrder::MipMajor;
    image.texture_descriptor.mip_level_count = t.levels.len() as u32;
    if t.is_cubemap() {
        image.texture_view_descriptor = Some(TextureViewDescriptor {
            dimension: Some(TextureViewDimension::Cube),
            ..default()
        });
    }
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// Level 0 of a height texture as Bevy's depth map: `1 - height`, linear.
pub fn depth(height: &sdk::Texture) -> Option<Image> {
    let level = height.levels.first()?;
    let pixels = level
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let d = 255 - p[0];
            [d, d, d, 255]
        })
        .collect();
    let mut image = Image::new(
        Extent3d {
            width: level.width,
            height: level.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    Some(image)
}

/// An SDK material as Bevy's, its textures from `texture`.
pub fn material(
    m: &sdk::Material,
    mut texture: impl FnMut(u64) -> Option<Handle<Image>>,
    depth: Option<Handle<Image>>,
) -> StandardMaterial {
    let [r, g, b, a] = m.base_color;
    let [er, eg, eb] = m.emissive.map(|c| c * m.emissive_strength);
    let orm = texture(m.textures.orm);
    StandardMaterial {
        base_color: Color::linear_rgba(r, g, b, a),
        base_color_texture: texture(m.textures.base_color),
        metallic: m.metallic,
        perceptual_roughness: m.roughness,
        metallic_roughness_texture: orm.clone(),
        occlusion_texture: orm,
        normal_map_texture: texture(m.textures.normal),
        emissive: LinearRgba::rgb(er, eg, eb),
        emissive_texture: texture(m.textures.emissive),
        parallax_depth_scale: m.height_scale,
        depth_map: depth,
        alpha_mode: match m.alpha_mode {
            sdk::material::AlphaMode::Opaque => AlphaMode::Opaque,
            sdk::material::AlphaMode::Mask => AlphaMode::Mask(m.alpha_cutoff),
            sdk::material::AlphaMode::Blend => AlphaMode::Blend,
        },
        double_sided: m.double_sided,
        cull_mode: (!m.double_sided).then_some(bevy::render::render_resource::Face::Back),
        ..default()
    }
}
