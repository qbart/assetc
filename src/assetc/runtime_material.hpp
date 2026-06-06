#pragma once

#include "runtime_mesh.hpp" // MakeFourCC

#include <cstdint>
#include <span>
#include <string>

namespace assetc
{

constexpr uint32_t MatMagic   = MakeFourCC('H', 'M', 'A', 'T');
constexpr uint32_t MatVersion = 1;

// A `.hmat` is the per-source material table: one row per material slot, in the
// SAME dense order as the companion `.hmesh` material slots, so `.hmat` row i ==
// SubMesh::materialSlot i. Textures are referenced by FNV1a64 runtime ref (the
// same hash space as mesh material/asset refs); 0 means "no texture, use factor
// only". The actual pixels live in separate `.ktx2` files.
enum MaterialFlags : uint32_t
{
    MatFlag_DoubleSided = 1u << 0,

    // glTF alpha mode in bits 1-2.
    MatFlag_AlphaOpaque  = 0u << 1,
    MatFlag_AlphaMask    = 1u << 1,
    MatFlag_AlphaBlend   = 2u << 1,
    MatFlag_AlphaModeBits = 0x3u << 1,
};

#pragma pack(push, 1)

struct MatFileHeader
{
    uint32_t magic;   // == MatMagic
    uint32_t version; // == MatVersion
    uint32_t count;   // material rows (matches the companion .hmesh materialCount)
    uint32_t flags;   // file-level flags, reserved
};
static_assert(sizeof(MatFileHeader) == 16, "MatFileHeader must be 16 bytes");

// Fixed-stride, mmappable PBR metallic-roughness material. Factors are stored in
// linear space (glTF baseColorFactor/emissiveFactor are already linear). The five
// texture refs cover the glTF metallic-roughness texture set.
struct GpuMaterial
{
    float baseColorFactor[4]; // linear RGBA multiplier
    float emissiveFactor[3];  // linear RGB
    float metallicFactor;
    float roughnessFactor;
    float normalScale;        // glTF normalTexture.scale
    float occlusionStrength;  // glTF occlusionTexture.strength
    float alphaCutoff;        // used when alpha mode == MASK
    uint32_t flags;           // MaterialFlags (double-sided + alpha mode)
    uint32_t _pad;            // align the u64 refs to 8 bytes

    uint64_t baseColorTex;         // FNV1a64 runtime ref of the .ktx2, 0 = none
    uint64_t metallicRoughnessTex; // ORM/MR: occlusion=R, roughness=G, metallic=B
    uint64_t normalTex;
    uint64_t occlusionTex;         // == metallicRoughnessTex when packed as ORM
    uint64_t emissiveTex;
};
static_assert(sizeof(GpuMaterial) == 96, "GpuMaterial must be 96 bytes");

#pragma pack(pop)

// Serialize a material table to a `.hmat`: MatFileHeader followed by `rows`
// GpuMaterial records. Returns 0 on success, non-zero with a logged error.
int WriteHMat(const std::string &path, std::span<const GpuMaterial> rows);

// Re-read a written `.hmat` and check magic/version and that the file size equals
// header + count * sizeof(GpuMaterial). Returns 0 on success.
int ValidateHMat(const std::string &path);

} // namespace assetc
