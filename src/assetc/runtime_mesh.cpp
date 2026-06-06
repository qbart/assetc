#include "runtime_mesh.hpp"

#include "../deps/fmt.hpp"

#include <algorithm>
#include <bit>
#include <cmath>
#include <cstddef>
#include <cstring>
#include <fstream>
#include <ios>
#include <type_traits>

static_assert(std::endian::native == std::endian::little,
              "assetc currently supports little-endian targets only");

namespace
{
constexpr size_t ChunkAlign = 16;

constexpr size_t AlignUp(size_t v, size_t a) noexcept
{
    return (v + a - 1) & ~(a - 1);
}
} // namespace

int assetc::WriteChunked(const std::string &path, std::span<const ChunkPayload> chunks)
{
    // Pass 1: lay out the file. Header, table, then payloads 16-byte aligned.
    std::vector<ChunkEntry> table(chunks.size());
    size_t cursor = sizeof(FileHeader) + chunks.size() * sizeof(ChunkEntry);
    for (size_t i = 0; i < chunks.size(); ++i)
    {
        cursor          = AlignUp(cursor, ChunkAlign);
        table[i].fourcc = std::to_underlying(chunks[i].id);
        table[i].flags  = 0;
        table[i].offset = cursor;
        table[i].size   = chunks[i].bytes.size();
        cursor += chunks[i].bytes.size();
    }

    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
    {
        fmtx::Error(fmt::format("open failed: {}", path));
        return 1;
    }

    FileHeader hdr{};
    hdr.magic      = MeshMagic;
    hdr.version    = MeshVersion;
    hdr.chunkCount = static_cast<uint32_t>(chunks.size());
    out.write(reinterpret_cast<const char *>(&hdr), sizeof(hdr));
    out.write(reinterpret_cast<const char *>(table.data()),
              static_cast<std::streamsize>(table.size() * sizeof(ChunkEntry)));

    static constexpr uint8_t padBytes[ChunkAlign]{};
    size_t                   written = sizeof(FileHeader) + table.size() * sizeof(ChunkEntry);
    for (size_t i = 0; i < chunks.size(); ++i)
    {
        size_t pad = table[i].offset - written;
        if (pad > 0)
            out.write(reinterpret_cast<const char *>(padBytes), static_cast<std::streamsize>(pad));
        out.write(reinterpret_cast<const char *>(chunks[i].bytes.data()),
                  static_cast<std::streamsize>(chunks[i].bytes.size()));
        written = table[i].offset + chunks[i].bytes.size();
    }

    if (!out.good())
    {
        fmtx::Error(fmt::format("write failed: {}", path));
        return 1;
    }
    return 0;
}

// --- Octahedral normal/tangent encoding ---------------------------------------
//
// Maps a unit vector to a 2D square via octahedral projection (Meyer et al. 2010),
// then quantizes each axis to int16 in SNORM range.
//
// Shader-side decoder in Slang (matches OctEncode exactly):
//
//     float3 octDecode(float2 e) {
//         float3 n = float3(e, 1.0 - abs(e.x) - abs(e.y));
//         if (n.z < 0.0) n.xy = (1.0 - abs(n.yx)) * sign(n.xy);
//         return normalize(n);
//     }
//
// Vertex input — GpuVertex is 28 bytes (v2), two packed slots, two Vulkan formats:
//
//   position -> VK_FORMAT_R32G32B32_SFLOAT  (offset 0)
//   normal   -> VK_FORMAT_R16G16_SNORM      (offset 12; /32767 on fetch -> [-1,1])
//   tangent  -> VK_FORMAT_R16G16_SINT       (offset 16; raw int to extract bit 0;
//                                            shader divides by 32767 manually)
//   uv       -> VK_FORMAT_R32G32_SFLOAT     (offset 20)
//
// Slang vertex input struct:
//
//     struct VSInput {
//         [[vk::location(0)]] float3 position       : POSITION;
//         [[vk::location(1)]] float2 normalPacked   : NORMAL;   // SNORM
//         [[vk::location(2)]] int2   tangentPacked  : TANGENT;  // SINT
//         [[vk::location(3)]] float2 uv             : TEXCOORD0;
//     };
//
// Decoding in the vertex shader:
//
//     float3 normal     = octDecode(input.normalPacked);
//     float  handedness = ((input.tangentPacked.x & 1) == 0) ? 1.0 : -1.0;
//     float3 tangent    = octDecode(float2(input.tangentPacked) / 32767.0);
//     float3 bitangent  = cross(normal, tangent) * handedness;

namespace
{
inline float OctSign(float v) noexcept
{
    return v >= 0.0f ? 1.0f : -1.0f;
}

inline int16_t QuantizeSnorm16(float v) noexcept
{
    v = std::clamp(v, -1.0f, 1.0f);
    return static_cast<int16_t>(std::lround(v * 32767.0f));
}

inline std::array<float, 2> OctProject(assetc::Vec3 n) noexcept
{
    const float invL1 = 1.0f / (std::fabs(n.x) + std::fabs(n.y) + std::fabs(n.z));
    n.x *= invL1;
    n.y *= invL1;
    n.z *= invL1;

    if (n.z >= 0.0f)
        return {n.x, n.y};

    return {
        (1.0f - std::fabs(n.y)) * OctSign(n.x),
        (1.0f - std::fabs(n.x)) * OctSign(n.y),
    };
}
} // namespace

std::array<int16_t, 2> assetc::OctEncode(Vec3 n) noexcept
{
    const auto e = OctProject(n);
    return {QuantizeSnorm16(e[0]), QuantizeSnorm16(e[1])};
}

std::array<int16_t, 2> assetc::OctEncodeTangent(Vec3 t, float handedness) noexcept
{
    auto packed = OctEncode(t);
    packed[0]   = static_cast<int16_t>(packed[0] & ~int16_t{1});
    if (handedness < 0.0f)
        packed[0] = static_cast<int16_t>(packed[0] | int16_t{1});
    return packed;
}

// --- WriteHMesh ---------------------------------------------------------------

namespace
{
template <typename T>
void AppendPod(std::vector<std::byte> &dst, const T &v)
{
    static_assert(std::is_trivially_copyable_v<T>);
    const auto *p = reinterpret_cast<const std::byte *>(&v);
    dst.insert(dst.end(), p, p + sizeof(T));
}

template <typename T>
std::span<const std::byte> VecBytes(const std::vector<T> &v) noexcept
{
    return std::as_bytes(std::span<const T>(v.data(), v.size()));
}
} // namespace

int assetc::WriteHMesh(const std::string &path, const Mesh &m, const MeshBounds &bounds,
                       std::span<const SubMesh> submeshes,
                       std::span<const uint64_t> materialHashes,
                       std::span<const GpuJoint> skeleton)
{
    if (m.vertices.empty())
    {
        fmtx::Error(fmt::format("WriteHMesh: empty vertex buffer, refusing to write {}", path));
        return 1;
    }
    if (submeshes.empty())
    {
        fmtx::Error(fmt::format("WriteHMesh: no submeshes, refusing to write {}", path));
        return 1;
    }

    const bool fits16 = m.vertices.size() <= 0xFFFFu;

    // DESC: the single descriptor; every other chunk is a pure array.
    MeshDesc desc{};
    desc.vertexCount     = static_cast<uint32_t>(m.vertices.size());
    desc.indexCount      = static_cast<uint32_t>(m.indices.size());
    desc.meshletCount    = static_cast<uint32_t>(m.meshlets.size());
    desc.submeshCount    = static_cast<uint32_t>(submeshes.size());
    desc.materialCount   = static_cast<uint32_t>(materialHashes.size());
    desc.vertexStride    = static_cast<uint16_t>(sizeof(GpuVertex));
    desc.indexWidth      = fits16 ? 2u : 4u;
    desc.flags           = MeshFlag_HasTangents;
    desc.meshletMaxVerts = 64;  // mirror MaxVerts in encode_mesh.cpp
    desc.meshletMaxTris  = 124; // mirror MaxTris in encode_mesh.cpp

    // IDXS: pure index array, downcast to u16 when vertex count fits.
    std::vector<std::byte> idxBuf;
    if (fits16)
    {
        idxBuf.reserve(m.indices.size() * 2);
        for (uint32_t v : m.indices)
        {
            const uint16_t v16 = static_cast<uint16_t>(v);
            AppendPod(idxBuf, v16);
        }
    }
    else
    {
        const auto bytes = VecBytes(m.indices);
        idxBuf.assign(bytes.begin(), bytes.end());
    }

    const auto descBytes   = std::as_bytes(std::span<const MeshDesc>(&desc, 1));
    const auto boundsBytes = std::as_bytes(std::span<const MeshBounds>(&bounds, 1));

    std::vector<ChunkPayload> chunks = {
        {ChunkId::Desc, descBytes},
        {ChunkId::Bounds, boundsBytes},
        {ChunkId::Vertices, VecBytes(m.vertices)},
        {ChunkId::Indices, std::span<const std::byte>(idxBuf.data(), idxBuf.size())},
        {ChunkId::Meshlets, VecBytes(m.meshlets)},
        {ChunkId::MeshletVertices, VecBytes(m.meshletVertices)},
        {ChunkId::MeshletTriangles, VecBytes(m.meshletTriangles)},
        {ChunkId::MeshletBounds, VecBytes(m.meshletBounds)},
        {ChunkId::Materials, std::as_bytes(materialHashes)},
        {ChunkId::SubMeshes, std::as_bytes(submeshes)},
    };
    // Optional skinning chunks. SKIN is parallel to VTXS (one entry per vertex);
    // SKEL is the joint array. Both absent for static meshes.
    if (!m.skinVertices.empty())
        chunks.push_back({ChunkId::Skin, VecBytes(m.skinVertices)});
    if (!skeleton.empty())
        chunks.push_back({ChunkId::Skeleton, std::as_bytes(skeleton)});
    return WriteChunked(path, chunks);
}

// --- ValidateHMesh ------------------------------------------------------------

int assetc::ValidateHMesh(const std::string &path)
{
    std::ifstream in(path, std::ios::binary | std::ios::ate);
    if (!in)
    {
        fmtx::Error(fmt::format("validate: cannot open {}", path));
        return 1;
    }
    const auto fileSize = static_cast<size_t>(in.tellg());
    in.seekg(0, std::ios::beg);

    std::vector<std::byte> buf(fileSize);
    in.read(reinterpret_cast<char *>(buf.data()), static_cast<std::streamsize>(fileSize));
    if (!in)
    {
        fmtx::Error(fmt::format("validate: read failed {}", path));
        return 1;
    }

    if (fileSize < sizeof(FileHeader))
    {
        fmtx::Error(fmt::format("validate: {} too small ({} B)", path, fileSize));
        return 1;
    }

    FileHeader hdr{};
    std::memcpy(&hdr, buf.data(), sizeof(FileHeader));
    if (hdr.magic != MeshMagic)
    {
        fmtx::Error(fmt::format("validate: {} bad magic 0x{:08x}", path, hdr.magic));
        return 1;
    }
    if (hdr.version != MeshVersion)
    {
        fmtx::Error(fmt::format("validate: {} unsupported version {} (expected {})", path,
                                hdr.version, MeshVersion));
        return 1;
    }

    const size_t tableOffset = sizeof(FileHeader);
    const size_t tableBytes  = static_cast<size_t>(hdr.chunkCount) * sizeof(ChunkEntry);
    if (tableOffset + tableBytes > fileSize)
    {
        fmtx::Error(fmt::format("validate: {} chunk table truncated", path));
        return 1;
    }

    std::vector<ChunkEntry> table(hdr.chunkCount);
    std::memcpy(table.data(), buf.data() + tableOffset, tableBytes);

    for (size_t i = 0; i < table.size(); ++i)
    {
        const auto &e = table[i];
        if (e.offset > fileSize || e.size > fileSize - e.offset)
        {
            fmtx::Error(fmt::format("validate: {} chunk {} (fourcc 0x{:08x}) extends past EOF",
                                    path, i, e.fourcc));
            return 1;
        }
    }

    auto findChunk = [&](ChunkId id) -> const ChunkEntry * {
        const auto v = std::to_underlying(id);
        for (const auto &e : table)
            if (e.fourcc == v)
                return &e;
        return nullptr;
    };

    const auto *desc = findChunk(ChunkId::Desc);
    const auto *bnds = findChunk(ChunkId::Bounds);
    const auto *vtxs = findChunk(ChunkId::Vertices);
    const auto *idxs = findChunk(ChunkId::Indices);
    const auto *subm = findChunk(ChunkId::SubMeshes);
    if (!desc || !bnds || !vtxs || !idxs || !subm)
    {
        fmtx::Error(fmt::format("validate: {} missing required chunk (DESC/BNDS/VTXS/IDXS/SUBM)",
                                path));
        return 1;
    }

    if (desc->size != sizeof(MeshDesc))
    {
        fmtx::Error(fmt::format("validate: {} DESC size {} != {}", path, desc->size,
                                sizeof(MeshDesc)));
        return 1;
    }
    MeshDesc d{};
    std::memcpy(&d, buf.data() + desc->offset, sizeof(MeshDesc));

    if (bnds->size != sizeof(MeshBounds))
    {
        fmtx::Error(fmt::format("validate: {} BNDS size {} != {}", path, bnds->size,
                                sizeof(MeshBounds)));
        return 1;
    }
    MeshBounds bounds{};
    std::memcpy(&bounds, buf.data() + bnds->offset, sizeof(MeshBounds));

    if (d.vertexStride != sizeof(GpuVertex))
    {
        fmtx::Error(fmt::format("validate: {} DESC.vertexStride {} != sizeof(GpuVertex)={}", path,
                                d.vertexStride, sizeof(GpuVertex)));
        return 1;
    }
    if (vtxs->size != static_cast<size_t>(d.vertexCount) * d.vertexStride)
    {
        fmtx::Error(fmt::format("validate: {} VTXS size {} != vertexCount*stride {}", path,
                                vtxs->size, static_cast<size_t>(d.vertexCount) * d.vertexStride));
        return 1;
    }
    if (d.indexWidth != 2 && d.indexWidth != 4)
    {
        fmtx::Error(fmt::format("validate: {} DESC.indexWidth {} not 2 or 4", path, d.indexWidth));
        return 1;
    }
    if (idxs->size != static_cast<size_t>(d.indexCount) * d.indexWidth)
    {
        fmtx::Error(fmt::format("validate: {} IDXS size {} != indexCount*width {}", path,
                                idxs->size, static_cast<size_t>(d.indexCount) * d.indexWidth));
        return 1;
    }

    // Optional meshlet/material chunks must match DESC counts when present.
    auto checkArray = [&](ChunkId id, const char *tag, size_t count, size_t elem) -> bool {
        const auto *c = findChunk(id);
        if (!c)
            return count == 0; // absent is fine only if the count says so
        if (c->size != count * elem)
        {
            fmtx::Error(fmt::format("validate: {} {} size {} != count*elem {}", path, tag, c->size,
                                    count * elem));
            return false;
        }
        return true;
    };
    if (!checkArray(ChunkId::Meshlets, "MLET", d.meshletCount, sizeof(Meshlet))
        || !checkArray(ChunkId::MeshletBounds, "MLBN", d.meshletCount, sizeof(MeshletBounds))
        || !checkArray(ChunkId::Materials, "MTRL", d.materialCount, sizeof(uint64_t)))
        return 1;

    // Optional skinning chunks: SKIN must be one entry per vertex; SKEL must be a
    // whole number of joints.
    if (const auto *skin = findChunk(ChunkId::Skin))
    {
        if (skin->size != static_cast<size_t>(d.vertexCount) * sizeof(GpuSkinVertex))
        {
            fmtx::Error(fmt::format("validate: {} SKIN size {} != vertexCount*{}", path, skin->size,
                                    sizeof(GpuSkinVertex)));
            return 1;
        }
    }
    if (const auto *skel = findChunk(ChunkId::Skeleton))
    {
        if (skel->size == 0 || skel->size % sizeof(GpuJoint) != 0)
        {
            fmtx::Error(fmt::format("validate: {} SKEL size {} not a multiple of {}", path,
                                    skel->size, sizeof(GpuJoint)));
            return 1;
        }
    }

    if (d.submeshCount == 0 || subm->size != static_cast<size_t>(d.submeshCount) * sizeof(SubMesh))
    {
        fmtx::Error(fmt::format("validate: {} SUBM size {} != submeshCount {} * {}", path,
                                subm->size, d.submeshCount, sizeof(SubMesh)));
        return 1;
    }
    std::vector<SubMesh> submeshes(d.submeshCount);
    std::memcpy(submeshes.data(), buf.data() + subm->offset, subm->size);

    const float range = std::max(
        {bounds.aabbMax.x - bounds.aabbMin.x, bounds.aabbMax.y - bounds.aabbMin.y,
         bounds.aabbMax.z - bounds.aabbMin.z, 1.0f});
    const float eps = 1e-5f * range;

    // Submeshes must cover [0,indexCount) and [0,meshletCount) contiguously, in order.
    uint32_t idxCursor = 0, mletCursor = 0;
    for (uint32_t s = 0; s < d.submeshCount; ++s)
    {
        const auto &sm = submeshes[s];
        if (sm.firstIndex != idxCursor || sm.firstMeshlet != mletCursor)
        {
            fmtx::Error(fmt::format(
                "validate: {} submesh {} not contiguous (firstIndex {} expected {}, "
                "firstMeshlet {} expected {})",
                path, s, sm.firstIndex, idxCursor, sm.firstMeshlet, mletCursor));
            return 1;
        }
        if (sm.indexCount % 3 != 0)
        {
            fmtx::Error(fmt::format("validate: {} submesh {} indexCount {} not multiple of 3", path,
                                    s, sm.indexCount));
            return 1;
        }
        if (sm.materialSlot != kNoMaterial && sm.materialSlot >= d.materialCount)
        {
            fmtx::Error(fmt::format("validate: {} submesh {} materialSlot {} >= materialCount {}",
                                    path, s, sm.materialSlot, d.materialCount));
            return 1;
        }
        if (sm.bounds.aabbMin.x < bounds.aabbMin.x - eps
            || sm.bounds.aabbMin.y < bounds.aabbMin.y - eps
            || sm.bounds.aabbMin.z < bounds.aabbMin.z - eps
            || sm.bounds.aabbMax.x > bounds.aabbMax.x + eps
            || sm.bounds.aabbMax.y > bounds.aabbMax.y + eps
            || sm.bounds.aabbMax.z > bounds.aabbMax.z + eps)
        {
            fmtx::Error(fmt::format("validate: {} submesh {} AABB not contained in mesh AABB", path,
                                    s));
            return 1;
        }
        idxCursor += sm.indexCount;
        mletCursor += sm.meshletCount;
    }
    if (idxCursor != d.indexCount || mletCursor != d.meshletCount)
    {
        fmtx::Error(fmt::format(
            "validate: {} submeshes cover {} idx / {} meshlets, expected {} / {}", path, idxCursor,
            mletCursor, d.indexCount, d.meshletCount));
        return 1;
    }

    // AABB containment check on positions (VTXS is now a pure array, no prelude).
    const std::byte *vtxData = buf.data() + vtxs->offset;
    for (uint32_t i = 0; i < d.vertexCount; ++i)
    {
        float pos[3]{};
        std::memcpy(pos, vtxData + static_cast<size_t>(i) * d.vertexStride
                            + offsetof(GpuVertex, position),
                    sizeof(pos));
        if (pos[0] < bounds.aabbMin.x - eps || pos[0] > bounds.aabbMax.x + eps
            || pos[1] < bounds.aabbMin.y - eps || pos[1] > bounds.aabbMax.y + eps
            || pos[2] < bounds.aabbMin.z - eps || pos[2] > bounds.aabbMax.z + eps)
        {
            fmtx::Error(fmt::format(
                "validate: {} vertex {} ({},{},{}) outside AABB [({},{},{})-({},{},{})]", path, i,
                pos[0], pos[1], pos[2], bounds.aabbMin.x, bounds.aabbMin.y, bounds.aabbMin.z,
                bounds.aabbMax.x, bounds.aabbMax.y, bounds.aabbMax.z));
            return 1;
        }
    }

    fmtx::Success(fmt::format("validate: {} ok ({} verts, {} idx@{}B, {} submeshes, {} mats, {} "
                              "chunks)",
                              path, d.vertexCount, d.indexCount, d.indexWidth, d.submeshCount,
                              d.materialCount, hdr.chunkCount));
    return 0;
}
