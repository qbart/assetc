#pragma once

#include <array>
#include <cstdint>
#include <span>
#include <string>
#include <utility>
#include <vector>

namespace assetc
{

struct Vec2
{
    float x, y;
};

struct Vec3
{
    float x, y, z;
};

struct Vec4
{
    float x, y, z, w;
};

constexpr uint32_t MakeFourCC(char a, char b, char c, char d)
{
    return static_cast<uint32_t>(static_cast<uint8_t>(a))
         | (static_cast<uint32_t>(static_cast<uint8_t>(b)) << 8)
         | (static_cast<uint32_t>(static_cast<uint8_t>(c)) << 16)
         | (static_cast<uint32_t>(static_cast<uint8_t>(d)) << 24);
}

constexpr uint32_t MeshMagic = MakeFourCC('H', 'M', 'S', 'H'); // 0x4853'4D48

enum class ChunkId : uint32_t
{
    Bounds           = MakeFourCC('B', 'N', 'D', 'S'),
    Vertices         = MakeFourCC('V', 'T', 'X', 'S'),
    Indices          = MakeFourCC('I', 'D', 'X', 'S'),
    Meshlets         = MakeFourCC('M', 'L', 'E', 'T'),
    MeshletVertices  = MakeFourCC('M', 'L', 'V', 'R'),
    MeshletTriangles = MakeFourCC('M', 'L', 'T', 'R'),
    MeshletBounds    = MakeFourCC('M', 'L', 'B', 'N'),
    Materials        = MakeFourCC('M', 'T', 'R', 'L'),
};

#pragma pack(push, 1)

struct FileHeader
{
    uint32_t magic;      // == MeshMagic
    uint32_t version;    // 1
    uint32_t chunkCount; // entries in ChunkTable
    uint32_t flags;      // file-level flags, reserved
    uint64_t _reserved1; // (was contentHash; held for future use)
    uint64_t _reserved2;
};
static_assert(sizeof(FileHeader) == 32, "FileHeader must be 32 bytes");

struct ChunkEntry
{
    uint32_t fourcc; // ChunkId value
    uint32_t flags;  // per-chunk flags (compression, alignment hint), reserved
    uint64_t offset; // from file start
    uint64_t size;   // payload bytes
};
static_assert(sizeof(ChunkEntry) == 24, "ChunkEntry must be 24 bytes");

struct MeshBounds
{
    Vec3  aabbMin;
    Vec3  aabbMax;
    Vec3  sphereCenter;
    float sphereRadius;
};
static_assert(sizeof(MeshBounds) == 40, "MeshBounds must be 40 bytes");

struct CpuVertex
{
    Vec3 pos;
    Vec3 normal;
    Vec4 tangent;
    Vec2 uv;
};

struct GpuVertex
{
    float   position[3];
    int16_t normal[4];  // [0..1] = octahedral normal; [2..3] reserved (0 in v1)
    int16_t tangent[4]; // [0..1] = octahedral tangent + handedness bit; [2..3] reserved
    float   uv[2];
};

struct MeshletBounds
{
    Vec3  center;
    float radius;

    Vec3  coneAxis;
    float coneCutoff;
};

struct Meshlet
{
    uint32_t vertexOffset;
    uint32_t vertexCount;

    uint32_t triangleOffset;
    uint32_t triangleCount;
};

struct MeshletTriangle
{
    uint8_t i0;
    uint8_t i1;
    uint8_t i2;
};

#pragma pack(pop)

struct EncoderMesh
{
    std::vector<Vec3> positions;
    std::vector<Vec3> normals;
    std::vector<Vec4> tangents;
    std::vector<Vec2> uvs;

    std::vector<uint32_t> indices;
};

struct Mesh
{
    std::vector<GpuVertex> vertices;
    std::vector<uint32_t>  indices;

    std::vector<Meshlet>         meshlets;
    std::vector<uint32_t>        meshletVertices;
    std::vector<MeshletTriangle> meshletTriangles;
    std::vector<MeshletBounds>   meshletBounds;
};

struct ChunkPayload
{
    ChunkId                    id;
    std::span<const std::byte> bytes;
};

int WriteChunked(const std::string &path, std::span<const ChunkPayload> chunks);

} // namespace assetc
