#include "runtime_mesh.hpp"

#include "../deps/fmt.hpp"

#include <algorithm>
#include <bit>
#include <cmath>
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
    hdr.version    = 1;
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
// Vertex input — two slots, two Vulkan formats:
//
//   Normal  -> VK_FORMAT_R16G16_SNORM   (hardware divides by 32767 on fetch;
//                                        shader receives float2 in [-1, 1])
//   Tangent -> VK_FORMAT_R16G16_SINT    (shader needs raw integer to extract
//                                        the handedness bit; divides manually)
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
                       std::span<const uint64_t> materialHashes)
{
    if (m.vertices.empty())
    {
        fmtx::Error(fmt::format("WriteHMesh: empty vertex buffer, refusing to write {}", path));
        return 1;
    }

    // VTXS: u32 count, u32 stride, then count*stride bytes.
    std::vector<std::byte> vtxBuf;
    {
        const uint32_t count  = static_cast<uint32_t>(m.vertices.size());
        const uint32_t stride = sizeof(GpuVertex);
        vtxBuf.reserve(8 + static_cast<size_t>(count) * stride);
        AppendPod(vtxBuf, count);
        AppendPod(vtxBuf, stride);
        const auto bytes = VecBytes(m.vertices);
        vtxBuf.insert(vtxBuf.end(), bytes.begin(), bytes.end());
    }

    // IDXS: u32 count, u32 size (2 or 4), then bytes. Downcast to u16 if vertexCount fits.
    std::vector<std::byte> idxBuf;
    {
        const uint32_t count  = static_cast<uint32_t>(m.indices.size());
        const bool     fits16 = m.vertices.size() <= 0xFFFFu;
        const uint32_t size   = fits16 ? 2u : 4u;
        idxBuf.reserve(8 + static_cast<size_t>(count) * size);
        AppendPod(idxBuf, count);
        AppendPod(idxBuf, size);
        if (fits16)
        {
            for (uint32_t v : m.indices)
            {
                const uint16_t v16 = static_cast<uint16_t>(v);
                AppendPod(idxBuf, v16);
            }
        }
        else
        {
            const auto bytes = VecBytes(m.indices);
            idxBuf.insert(idxBuf.end(), bytes.begin(), bytes.end());
        }
    }

    // MTRL: u32 count, then count*u64.
    std::vector<std::byte> mtrlBuf;
    {
        const uint32_t count = static_cast<uint32_t>(materialHashes.size());
        mtrlBuf.reserve(4 + static_cast<size_t>(count) * 8);
        AppendPod(mtrlBuf, count);
        const auto bytes = std::as_bytes(materialHashes);
        mtrlBuf.insert(mtrlBuf.end(), bytes.begin(), bytes.end());
    }

    const auto boundsBytes = std::as_bytes(std::span<const MeshBounds>(&bounds, 1));

    const ChunkPayload chunks[] = {
        {ChunkId::Bounds, boundsBytes},
        {ChunkId::Vertices, std::span<const std::byte>(vtxBuf.data(), vtxBuf.size())},
        {ChunkId::Indices, std::span<const std::byte>(idxBuf.data(), idxBuf.size())},
        {ChunkId::Meshlets, VecBytes(m.meshlets)},
        {ChunkId::MeshletVertices, VecBytes(m.meshletVertices)},
        {ChunkId::MeshletTriangles, VecBytes(m.meshletTriangles)},
        {ChunkId::MeshletBounds, VecBytes(m.meshletBounds)},
        {ChunkId::Materials, std::span<const std::byte>(mtrlBuf.data(), mtrlBuf.size())},
    };
    return WriteChunked(path, chunks);
}
