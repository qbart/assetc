//! SDK values to Bevy resources.

use assetc as sdk;
use bevy::asset::RenderAssetUsages;
use bevy::image::CompressedImageFormats;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AstcBlock, AstcChannel, Extent3d, TextureDataOrder, TextureDimension, TextureFormat,
    TextureViewDescriptor, TextureViewDimension,
};

/// A texture with all its mip levels: 2D, cubemap (six layers) or volume. Block formats go
/// to the GPU as they are when `support` has them; three-channel ones are widened to RGBA. See
/// docs/features/demo.md#layout.
pub fn image(t: &sdk::Texture, support: CompressedImageFormats) -> Result<Image, String> {
    use sdk::texture::Format as F;
    let need = |flag: CompressedImageFormats, name: &str| {
        if support.contains(flag) {
            Ok(())
        } else {
            Err(format!("{name}: not supported by this GPU"))
        }
    };
    let astc = |channel| TextureFormat::Astc {
        block: AstcBlock::B4x4,
        channel,
    };
    // The bytes of alpha 1 after each texel of a three-channel format.
    let (format, alpha): (_, &[u8]) = match t.format {
        F::Rgba8Srgb => (TextureFormat::Rgba8UnormSrgb, &[]),
        F::Rgba8Unorm => (TextureFormat::Rgba8Unorm, &[]),
        F::Rgb8Srgb => (TextureFormat::Rgba8UnormSrgb, &[255]),
        F::Rgb8Unorm => (TextureFormat::Rgba8Unorm, &[255]),
        F::Rgba16Float => (TextureFormat::Rgba16Float, &[]),
        // Half-float 1.0, little-endian.
        F::Rgb16Float => (TextureFormat::Rgba16Float, &[0x00, 0x3C]),
        F::R8Unorm => (TextureFormat::R8Unorm, &[]),
        F::Rg8Unorm => (TextureFormat::Rg8Unorm, &[]),
        F::Bc7Srgb | F::Bc7Unorm | F::Bc4Unorm | F::Bc5Unorm | F::Bc6hUfloat => {
            need(CompressedImageFormats::BC, "BC")?;
            let format = match t.format {
                F::Bc7Srgb => TextureFormat::Bc7RgbaUnormSrgb,
                F::Bc7Unorm => TextureFormat::Bc7RgbaUnorm,
                F::Bc4Unorm => TextureFormat::Bc4RUnorm,
                F::Bc5Unorm => TextureFormat::Bc5RgUnorm,
                _ => TextureFormat::Bc6hRgbUfloat,
            };
            (format, &[])
        }
        F::Astc4x4Srgb => {
            need(CompressedImageFormats::ASTC_LDR, "ASTC")?;
            (astc(AstcChannel::UnormSrgb), &[])
        }
        F::Astc4x4Unorm => {
            need(CompressedImageFormats::ASTC_LDR, "ASTC")?;
            (astc(AstcChannel::Unorm), &[])
        }
        F::Astc4x4Float => {
            need(CompressedImageFormats::ASTC_HDR, "ASTC HDR")?;
            (astc(AstcChannel::Hdr), &[])
        }
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
    // Each level keeps its faces together: mip-major.
    let texel = t.format.bytes_per_texel();
    image.data = Some(
        t.levels
            .iter()
            .flat_map(|l| l.data.chunks(texel))
            .flat_map(|texel| texel.iter().chain(alpha).copied())
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
    Ok(image)
}

/// Level 0 of a height texture as Bevy's depth map: `1 - height`, linear.
pub fn depth(height: &sdk::Texture) -> Option<Image> {
    let level = height.levels.first()?;
    let pixels = red(height)?
        .into_iter()
        .flat_map(|h| {
            let d = 255 - h;
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

/// Level 0's red channel, a byte per texel, block formats decoded with imgio; `None` for half
/// floats.
fn red(t: &sdk::Texture) -> Option<Vec<u8>> {
    use imgio::{AstcBlock, AstcKind};
    use sdk::texture::Format as F;
    let level = t.levels.first()?;
    let format = match t.format {
        F::Bc4Unorm => imgio::Format::Bc4Unorm,
        F::Bc5Unorm => imgio::Format::Bc5Unorm,
        F::Bc7Unorm => imgio::Format::Bc7Unorm,
        F::Bc7Srgb => imgio::Format::Bc7Srgb,
        F::Astc4x4Unorm => imgio::Format::Astc(AstcBlock::B4x4, AstcKind::Unorm),
        F::Astc4x4Srgb => imgio::Format::Astc(AstcBlock::B4x4, AstcKind::Srgb),
        F::Bc6hUfloat | F::Astc4x4Float | F::Rgba16Float | F::Rgb16Float => return None,
        _ => {
            let n = t.format.bytes_per_texel();
            return Some(level.data.iter().step_by(n).copied().collect());
        }
    };
    let decoded = imgio::decode_to_buffer(format, &level.data, level.width, level.height)
        .inspect_err(|e| warn!("{e}"))
        .ok()?;
    let n = format.decoded().block_bytes();
    Some(decoded.iter().step_by(n).copied().collect())
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
