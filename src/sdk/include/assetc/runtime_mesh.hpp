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

constexpr uint32_t MeshMagic   = MakeFourCC('H', 'M', 'S', 'H'); // 0x4853'4D48
constexpr uint32_t MeshVersion = 2;                             // v2: submesh table + 28B vertex

constexpr uint32_t kNoMaterial = 0xFFFFFFFFu; // SubMesh::materialSlot sentinel

enum class ChunkId : uint32_t
{
    Desc             = MakeFourCC('D', 'E', 'S', 'C'),
    Bounds           = MakeFourCC('B', 'N', 'D', 'S'),
    Vertices         = MakeFourCC('V', 'T', 'X', 'S'),
    Indices          = MakeFourCC('I', 'D', 'X', 'S'),
    Meshlets         = MakeFourCC('M', 'L', 'E', 'T'),
    MeshletVertices  = MakeFourCC('M', 'L', 'V', 'R'),
    MeshletTriangles = MakeFourCC('M', 'L', 'T', 'R'),
    MeshletBounds    = MakeFourCC('M', 'L', 'B', 'N'),
    Materials        = MakeFourCC('M', 'T', 'R', 'L'),
    SubMeshes        = MakeFourCC('S', 'U', 'B', 'M'),
    Skin             = MakeFourCC('S', 'K', 'I', 'N'), // optional: per-vertex joints/weights
    Skeleton         = MakeFourCC('S', 'K', 'E', 'L'), // optional: joint hierarchy + bind pose
    LodIndices       = MakeFourCC('L', 'O', 'D', 'I'), // optional: u32 index data for reduced LODs
    LodTable         = MakeFourCC('L', 'O', 'D', 'T'), // optional: per-submesh LOD ranges into LODI
};

#pragma pack(push, 1)

struct FileHeader
{
    uint32_t magic;      // == MeshMagic
    uint32_t version;    // == MeshVersion
    uint32_t chunkCount; // entries in ChunkTable
    uint32_t flags;      // file-level flags, reserved
    uint64_t _reserved1; // (was contentHash; held for future use)
    uint64_t _reserved2;
};
static_assert(sizeof(FileHeader) == 32, "FileHeader must be 32 bytes");

// DESC chunk: the single source of truth for the mesh's shape. Every data chunk
// (VTXS/IDXS/MLET/...) is a *pure array* with no inline prelude, so the loader
// reads counts/strides from here and mmap-casts each chunk directly.
struct MeshDesc
{
    uint32_t vertexCount;
    uint32_t indexCount;     // total indices across all submeshes (global into VTXS)
    uint32_t meshletCount;   // MLET / MLBN element count
    uint32_t submeshCount;   // SUBM element count (>= 1)
    uint32_t materialCount;  // MTRL element count (compact: only referenced materials)
    uint16_t vertexStride;   // == sizeof(GpuVertex)
    uint8_t  indexWidth;     // 2 or 4 bytes per IDXS element
    uint8_t  flags;          // bit0 = tangents present/valid
    uint32_t meshletMaxVerts; // build param (verts per meshlet)
    uint32_t meshletMaxTris;  // build param (tris per meshlet)
};
static_assert(sizeof(MeshDesc) == 32, "MeshDesc must be 32 bytes");

enum MeshFlags : uint8_t
{
    MeshFlag_HasTangents = 1u << 0,
};

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
    Vec3     pos;
    Vec3     normal;
    Vec4     tangent;
    Vec2     uv;
    uint16_t joints[4] = {0, 0, 0, 0}; // skinning: index into skeleton; 0 when static
    float    weights[4] = {0, 0, 0, 0};
};

struct GpuVertex
{
    float   position[3]; // object-space XYZ, raw float32
    int16_t normal[2];   // octahedral normal (R16G16_SNORM)
    int16_t tangent[2];  // octahedral tangent + handedness in bit 0 of x (R16G16_SINT)
    float   uv[2];
};
static_assert(sizeof(GpuVertex) == 28, "GpuVertex must be 28 bytes (v2)");

// Optional per-vertex skinning, parallel to the VTXS array (SKIN chunk, one per
// vertex). Present only for skinned meshes; `joints` index into the SKEL array,
// `weights` are normalized to sum 1. Suggested formats: joints ->
// VK_FORMAT_R16G16B16A16_UINT, weights -> VK_FORMAT_R32G32B32A32_SFLOAT.
struct GpuSkinVertex
{
    uint16_t joints[4];
    float    weights[4];
};
static_assert(sizeof(GpuSkinVertex) == 24, "GpuSkinVertex must be 24 bytes");

// One skeleton joint (SKEL chunk, pure array). `parent` indexes into this array
// (-1 = root). `inverseBind` maps mesh space -> this joint's bind space (column
// major). The bind TRS is the joint's local bind-pose transform that animation
// channels override per node; the engine composes pose * inverseBind for skinning.
struct GpuJoint
{
    float    inverseBind[16];
    float    bindT[3];
    float    bindR[4]; // unit quaternion x,y,z,w
    float    bindS[3];
    int32_t  parent;
    uint32_t _pad;
};
static_assert(sizeof(GpuJoint) == 112, "GpuJoint must be 112 bytes");

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

// One drawable section = a contiguous index range and meshlet range sharing one
// material. Submeshes are laid out contiguously and in order across IDXS and
// MLET, so submesh[i].firstIndex == submesh[i-1].firstIndex + indexCount, etc.
//   classic:     drawIndexed(indexCount, 1, firstIndex, /*vertexOffset=*/baseVertex)
//   mesh-shader: dispatch meshlets [firstMeshlet, firstMeshlet + meshletCount)
struct SubMesh
{
    uint32_t   firstIndex;   // offset into the global IDXS buffer
    uint32_t   indexCount;   // indices in this submesh (multiple of 3)
    uint32_t   firstMeshlet; // offset into MLET
    uint32_t   meshletCount; // meshlets in this submesh
    uint32_t   materialSlot; // index into MTRL, or kNoMaterial
    uint32_t   baseVertex;   // 0 in v2 (indices are global); reserved for per-submesh u16
    MeshBounds bounds;       // per-submesh AABB + sphere
};
static_assert(sizeof(SubMesh) == 64, "SubMesh must be 64 bytes");

// One reduced level-of-detail index range for a submesh, into the LODI buffer
// (u32, global vertex indices, same VTXS as LOD0). indexCount == 0 means "no
// simpler level available; reuse the previous LOD". LOD0 is the full-res mesh in
// IDXS/SUBM and is not duplicated here.
struct MeshLod
{
    uint32_t firstIndex; // into LODI
    uint32_t indexCount; // multiple of 3
};
static_assert(sizeof(MeshLod) == 8, "MeshLod must be 8 bytes");

// LODT chunk header, followed by MeshLod[submeshCount * lodCount], row-major
// [submesh][lod] (lod 0 == LOD1, the first reduced level).
struct LodTableHeader
{
    uint32_t lodCount;     // reduced levels per submesh (excludes full-res LOD0)
    uint32_t submeshCount; // matches DESC.submeshCount
};
static_assert(sizeof(LodTableHeader) == 8, "LodTableHeader must be 8 bytes");

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

    // Parallel to `vertices`, empty unless the mesh is skinned (SKIN chunk).
    std::vector<GpuSkinVertex> skinVertices;

    // Reduced LODs (LODI/LODT chunks). lodIndices holds the concatenated u32
    // index data; lodTable has submeshCount*lodCount entries [submesh][lod].
    std::vector<uint32_t> lodIndices;
    std::vector<MeshLod>  lodTable;
    uint32_t              lodCount = 0; // reduced levels per submesh (0 = none)
};

struct ChunkPayload
{
    ChunkId                    id;
    std::span<const std::byte> bytes;
};

int WriteChunked(const std::string &path, std::span<const ChunkPayload> chunks);

// Serialize a Mesh + bounds + submesh table + material hashes to a v2 .hmesh.
// Emits DESC plus pure-array chunks (BNDS, VTXS, IDXS, MLET, MLVR, MLTR, MLBN,
// MTRL, SUBM): every data chunk is a raw array with no inline prelude; counts and
// strides live in DESC. Returns 0 on success, non-zero on failure.
//
// Indices are written as uint16_t when vertexCount <= 65535, else uint32_t;
// DESC.indexWidth records which.
int WriteHMesh(const std::string &path, const Mesh &m, const MeshBounds &bounds,
               std::span<const SubMesh> submeshes, std::span<const uint64_t> materialHashes,
               std::span<const GpuJoint> skeleton = {});

// Read a written .hmesh and run structural + semantic checks:
//   - magic, version, chunk table fits in file
//   - every chunk's [offset, offset+size) fits in file
//   - required chunks present (DESC, BNDS, VTXS, IDXS, SUBM)
//   - DESC.vertexStride == sizeof(GpuVertex); indexWidth is 2 or 4
//   - VTXS/IDXS/MLET/MLBN/MTRL/SUBM chunk sizes match DESC counts
//   - submeshes cover [0, indexCount) and [0, meshletCount) contiguously, in order
//   - every materialSlot is kNoMaterial or < materialCount; every indexCount % 3 == 0
//   - every vertex position is inside the BNDS AABB; every submesh AABB ⊆ mesh AABB
// Returns 0 on success, non-zero with logged error on the first failure.
int ValidateHMesh(const std::string &path);

// Pack a unit-length vector into two int16_t using octahedral projection.
// Range: each component encodes [-1, 1] as [-32767, 32767] (Vulkan SNORM convention).
std::array<int16_t, 2> OctEncode(Vec3 n) noexcept;

// Same as OctEncode, but bit 0 of the X component carries handedness:
//   bit 0 == 0 -> handedness >= 0
//   bit 0 == 1 -> handedness <  0
// The shader recovers the sign from the LSB and treats the rest as the direction.
// Costs ~1/32767 of x-axis precision, which is well below visible threshold.
std::array<int16_t, 2> OctEncodeTangent(Vec3 t, float handedness) noexcept;

} // namespace assetc
