#pragma once

#include "assetc/runtime_material.hpp"

#include "../deps/ktx.hpp"

#include <cstdint>
#include <span>
#include <string>
#include <string_view>
#include <vector>

namespace gltf
{
struct GLTF;
}

namespace assetc
{

// A glTF image that must be transcoded to its own `.ktx2`, deduped by source
// image index. `mode` carries the color space + encoder settings for the slot
// the image is first used in (sRGB color, linear color/ORM, normal, ...).
struct TextureExport
{
    uint32_t       imageIndex; // index into tg3_model.images
    ktx::UASTCMode mode;
    uint64_t       refHash;    // content hash (image bytes + mode); names tex/<hash>.ktx2
};

struct CompiledMaterials
{
    std::vector<GpuMaterial>   rows;     // dense slot order: rows[i] == SubMesh::materialSlot i
    std::vector<TextureExport> textures; // unique images to encode as .ktx2 companions
};

// Build the per-source material table + texture export list from a glTF.
//
// `denseSourceIndices` is CompiledMesh::materialSourceIndex: dense material slot
// -> source material index. The returned rows are in that same slot order, so a
// submesh's materialSlot indexes straight into the `.hmat`.
//
// Texture refs are content hashes (image bytes + encoder mode); the caller writes
// each image to the flat content store as `tex/<refHash>.ktx2`, so identical
// textures across different glTF sources resolve to one file (see main.cpp wiring).
CompiledMaterials BuildMaterialsFromGltf(const gltf::GLTF &src,
                                         std::span<const uint32_t> denseSourceIndices,
                                         std::string_view sourceRef);

// Transcode one glTF image to a `.ktx2` at `destPath`. Handles images the loader
// already decoded (raw pixels) as well as still-encoded bytes in a buffer view or
// the image span. `compress` false writes a raw R8G8B8A8 KTX2 instead of UASTC.
// Returns 0 on success, non-zero with a logged error.
int EncodeGltfImageToKtx2(const gltf::GLTF &src, uint32_t imageIndex, ktx::UASTCMode mode,
                          const std::string &destPath, unsigned threadCount, bool compress = true);

} // namespace assetc
