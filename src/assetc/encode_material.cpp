#include "encode_material.hpp"

#include "encode_mesh.hpp" // HashAssetRef

#include "../deps/fmt.hpp"
#include "../deps/gltf.hpp"
#include "../deps/stb.hpp"

#include <string>
#include <unordered_map>

namespace assetc
{

namespace
{

// Resolve a glTF texture index to its backing image index, or -1 if absent.
int ImageOfTexture(const tg3_model &m, int textureIndex) noexcept
{
    if (textureIndex < 0 || static_cast<uint32_t>(textureIndex) >= m.textures_count)
        return -1;
    const int srcImage = m.textures[textureIndex].source;
    if (srcImage < 0 || static_cast<uint32_t>(srcImage) >= m.images_count)
        return -1;
    return srcImage;
}

uint32_t AlphaModeBits(const tg3_str &mode) noexcept
{
    if (tg3_str_equals_cstr(mode, "MASK"))
        return MatFlag_AlphaMask;
    if (tg3_str_equals_cstr(mode, "BLEND"))
        return MatFlag_AlphaBlend;
    return MatFlag_AlphaOpaque;
}

// glTF defaults for a material slot whose source index is somehow out of range.
GpuMaterial DefaultMaterial() noexcept
{
    GpuMaterial r{};
    r.baseColorFactor[0] = r.baseColorFactor[1] = r.baseColorFactor[2] = r.baseColorFactor[3] =
        1.0f;
    r.metallicFactor    = 1.0f;
    r.roughnessFactor   = 1.0f;
    r.normalScale       = 1.0f;
    r.occlusionStrength = 1.0f;
    r.alphaCutoff       = 0.5f;
    r.flags             = MatFlag_AlphaOpaque;
    return r;
}

// Returns an owning RGBA8 image for a glTF image, or an empty Image (pixels ==
// nullptr) if no usable pixel data is reachable. The caller copy-initializes from
// this prvalue so guaranteed elision applies (stb::Image is not move-aware).
stb::Image LoadGltfImagePixels(const tg3_model &m, const tg3_image &img, uint32_t imageIndex)
{
    // Path 1: the loader already decoded the image to raw 8-bit pixels.
    const bool decoded = img.width > 0 && img.height > 0 && img.component > 0 && img.bits == 8 &&
                         img.image.data != nullptr &&
                         img.image.count >= static_cast<uint64_t>(img.width) *
                                                static_cast<uint64_t>(img.height) *
                                                static_cast<uint64_t>(img.component);
    if (decoded)
        return stb::MakeRGBA(img.image.data, img.width, img.height, img.component);

    // Path 2: still-encoded bytes live in a buffer view (typical for .glb).
    if (img.buffer_view >= 0 && static_cast<uint32_t>(img.buffer_view) < m.buffer_views_count)
    {
        const tg3_buffer_view &bv = m.buffer_views[img.buffer_view];
        if (bv.buffer >= 0 && static_cast<uint32_t>(bv.buffer) < m.buffers_count)
        {
            const tg3_buffer &buf = m.buffers[bv.buffer];
            if (buf.data.data && bv.byte_offset + bv.byte_length <= buf.data.count)
                return stb::LoadFromMemory(buf.data.data + bv.byte_offset,
                                           static_cast<int>(bv.byte_length));
        }
        fmtx::Error(fmt::format("EncodeGltfImageToKtx2: image {} has an invalid buffer view",
                                imageIndex));
        return stb::Image{};
    }

    // Path 3: still-encoded bytes kept as-is in the image span.
    if (img.image.data != nullptr && img.image.count > 0)
        return stb::LoadFromMemory(img.image.data, static_cast<int>(img.image.count));

    fmtx::Error(fmt::format(
        "EncodeGltfImageToKtx2: image {} has no embedded pixel data (unresolved external uri?)",
        imageIndex));
    return stb::Image{};
}

} // namespace

CompiledMaterials BuildMaterialsFromGltf(const gltf::GLTF &src,
                                         std::span<const uint32_t> denseSourceIndices,
                                         std::string_view          sourceRef)
{
    const tg3_model  &m = src.model;
    CompiledMaterials out;
    out.rows.reserve(denseSourceIndices.size());

    // imageIndex -> the color-space/mode it was first used with. One .ktx2 per
    // image, by index; warn if the same image is pulled into conflicting roles.
    std::unordered_map<uint32_t, ktx::UASTCMode> seenImages;

    auto useTexture = [&](int texIndex, int texCoord, ktx::UASTCMode mode) -> uint64_t {
        const int img = ImageOfTexture(m, texIndex);
        if (img < 0)
            return 0;
        if (texCoord != 0)
            fmtx::Warn(fmt::format(
                "BuildMaterialsFromGltf: texture uses TEXCOORD_{}, but only UV0 is exported",
                texCoord));
        const uint32_t ui = static_cast<uint32_t>(img);
        auto [it, inserted] = seenImages.emplace(ui, mode);
        if (inserted)
            out.textures.push_back(TextureExport{ui, mode});
        else if (it->second != mode)
            fmtx::Warn(fmt::format(
                "BuildMaterialsFromGltf: image {} used in conflicting color spaces; keeping first",
                ui));

        std::string ref(sourceRef);
        ref += "/tex_";
        ref += std::to_string(ui);
        return HashAssetRef(ref);
    };

    for (uint32_t slot = 0; slot < denseSourceIndices.size(); ++slot)
    {
        const uint32_t srcIdx = denseSourceIndices[slot];
        if (srcIdx >= m.materials_count)
        {
            out.rows.push_back(DefaultMaterial());
            continue;
        }
        const tg3_material &mat = m.materials[srcIdx];
        const auto         &pbr = mat.pbr_metallic_roughness;

        GpuMaterial row{};
        for (int i = 0; i < 4; ++i)
            row.baseColorFactor[i] = static_cast<float>(pbr.base_color_factor[i]);
        for (int i = 0; i < 3; ++i)
            row.emissiveFactor[i] = static_cast<float>(mat.emissive_factor[i]);
        row.metallicFactor    = static_cast<float>(pbr.metallic_factor);
        row.roughnessFactor   = static_cast<float>(pbr.roughness_factor);
        row.normalScale       = static_cast<float>(mat.normal_texture.scale);
        row.occlusionStrength = static_cast<float>(mat.occlusion_texture.strength);
        row.alphaCutoff       = static_cast<float>(mat.alpha_cutoff);
        row.flags = AlphaModeBits(mat.alpha_mode) | (mat.double_sided ? MatFlag_DoubleSided : 0u);

        // Color space / encoder mode per slot: sRGB for baseColor & emissive;
        // linear (no swizzle) for the metallicRoughness/occlusion ORM packing;
        // dedicated normal mode for normal maps.
        row.baseColorTex = useTexture(pbr.base_color_texture.index,
                                      pbr.base_color_texture.tex_coord, ktx::UASTCMode::Color);
        row.metallicRoughnessTex =
            useTexture(pbr.metallic_roughness_texture.index,
                       pbr.metallic_roughness_texture.tex_coord, ktx::UASTCMode::LinearColor);
        row.normalTex    = useTexture(mat.normal_texture.index, mat.normal_texture.tex_coord,
                                      ktx::UASTCMode::Normal);
        row.occlusionTex = useTexture(mat.occlusion_texture.index, mat.occlusion_texture.tex_coord,
                                      ktx::UASTCMode::LinearColor);
        row.emissiveTex  = useTexture(mat.emissive_texture.index, mat.emissive_texture.tex_coord,
                                      ktx::UASTCMode::Color);

        out.rows.push_back(row);
    }

    return out;
}

int EncodeGltfImageToKtx2(const gltf::GLTF &src, uint32_t imageIndex, ktx::UASTCMode mode,
                          const std::string &destPath, unsigned threadCount)
{
    const tg3_model &m = src.model;
    if (imageIndex >= m.images_count)
    {
        fmtx::Error(fmt::format("EncodeGltfImageToKtx2: image index {} out of range ({})",
                                imageIndex, m.images_count));
        return 1;
    }

    stb::Image pixels = LoadGltfImagePixels(m, m.images[imageIndex], imageIndex);
    if (!pixels.pixels)
    {
        fmtx::Error(fmt::format("EncodeGltfImageToKtx2: no pixels for image {} (-> {}): {}",
                                imageIndex, destPath, stb::ImageError()));
        return 1;
    }

    const KTX_error_code rc = ktx::FromImageToUASTC(pixels, destPath, mode, threadCount);
    if (rc != KTX_SUCCESS)
    {
        fmtx::Error(fmt::format("EncodeGltfImageToKtx2: ktx encode failed ({}) for {}",
                                static_cast<int>(rc), destPath));
        return 1;
    }
    return 0;
}

} // namespace assetc
