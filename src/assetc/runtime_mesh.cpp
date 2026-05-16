#include "runtime_mesh.hpp"

#include "../deps/fmt.hpp"

#include <algorithm>
#include <bit>
#include <cmath>
#include <fstream>
#include <ios>

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
// Shader-side decoder (matches OctEncode exactly):
//
//     vec3 octDecode(vec2 e) {
//         vec3 n = vec3(e, 1.0 - abs(e.x) - abs(e.y));
//         if (n.z < 0.0) n.xy = (1.0 - abs(n.yx)) * sign(n.xy);
//         return normalize(n);
//     }
//
// Normal slot (recommended Vulkan format = VK_FORMAT_R16G16_SNORM — hardware
// divides by 32767 on fetch, shader receives a vec2 in [-1, 1] directly):
//
//     vec3 normal = octDecode(inNormalPacked);
//
// Tangent slot (must be VK_FORMAT_R16G16_SINT — shader needs integer access to
// recover the handedness bit, then divides by 32767 manually):
//
//     float handedness = ((inTangentPacked.x & 1) == 0) ? 1.0 : -1.0;
//     vec2  enc        = vec2(inTangentPacked) / 32767.0;
//     vec3  tangent    = octDecode(enc);
//     vec3  bitangent  = cross(normal, tangent) * handedness;

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
