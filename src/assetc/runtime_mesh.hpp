#pragma once

#include <array>
#include <cstdint>
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

#pragma pack(push, 1)

struct MeshHeader
{
    uint32_t magic;   // "HMSH"
    uint32_t version;
    // 8 bytes
    uint32_t vertexCount;
    uint32_t vertexStride;
    // 16 bytes
    uint32_t indexCount;
    uint32_t indexSize;
    // 24 bytes
    uint32_t meshletCount;
    uint32_t _pad1;
    // 32 bytes
    uint64_t vertexOffset;
    uint64_t indexOffset;
    // 48 bytes
    uint64_t meshletOffset;
    uint64_t meshletVerticesOffset;
    uint64_t meshletTrianglesOffset;
    uint64_t meshletBoundsOffset;
    // 80 bytes
    std::array<uint8_t, 16> reserved;
    // 96 bytes
};
static_assert(sizeof(MeshHeader) == 96, "MeshHeader must be 96 bytes");

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
    int16_t normal[4];
    int16_t tangent[4];
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

constexpr uint32_t MeshMagic = 0x4853'4D48; // 'HMSH' little-endian

}
