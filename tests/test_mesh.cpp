#include "check.hpp"

#include "assetc/runtime_mesh.hpp"

#include <cstring>
#include <filesystem>
#include <fstream>
#include <vector>

namespace fs = std::filesystem;
using namespace assetc;

// Build a minimal but structurally-valid single-triangle mesh.
static Mesh OneTriangle(MeshBounds &bounds)
{
    Mesh m;
    const float pos[3][3] = {{0, 0, 0}, {1, 0, 0}, {0, 1, 0}};
    for (auto &p : pos)
    {
        GpuVertex v{};
        v.position[0] = p[0];
        v.position[1] = p[1];
        v.position[2] = p[2];
        auto n        = OctEncode({0, 0, 1});
        v.normal[0]   = n[0];
        v.normal[1]   = n[1];
        m.vertices.push_back(v);
    }
    m.indices = {0, 1, 2};
    bounds    = MeshBounds{{0, 0, 0}, {1, 1, 0}, {0.5f, 0.5f, 0}, 0.71f};
    return m;
}

static SubMesh OneSubmesh(const MeshBounds &b)
{
    SubMesh sm{};
    sm.firstIndex   = 0;
    sm.indexCount   = 3;
    sm.firstMeshlet = 0;
    sm.meshletCount = 0;
    sm.materialSlot = kNoMaterial;
    sm.baseVertex   = 0;
    sm.bounds       = b;
    return sm;
}

// Find a chunk's (offset,size) in a written .hmesh by fourcc.
static bool FindChunk(const std::string &path, ChunkId id, uint64_t &offset, uint64_t &size)
{
    std::ifstream in(path, std::ios::binary);
    FileHeader    hdr{};
    if (!in.read(reinterpret_cast<char *>(&hdr), sizeof(hdr)))
        return false;
    for (uint32_t i = 0; i < hdr.chunkCount; ++i)
    {
        ChunkEntry e{};
        in.seekg(sizeof(FileHeader) + i * sizeof(ChunkEntry));
        if (!in.read(reinterpret_cast<char *>(&e), sizeof(e)))
            return false;
        if (e.fourcc == static_cast<uint32_t>(id))
        {
            offset = e.offset;
            size   = e.size;
            return true;
        }
    }
    return false;
}

int main()
{
    const auto dir = fs::temp_directory_path() / "assetc_test_mesh";
    fs::create_directories(dir);

    MeshBounds bounds;
    Mesh       m = OneTriangle(bounds);
    SubMesh    sm = OneSubmesh(bounds);

    // --- baseline: a static mesh writes and validates, with no LOD chunks --------
    const auto plain = (dir / "plain.hmesh").string();
    CHECK_EQ(WriteHMesh(plain, m, bounds, std::span(&sm, 1), {}), 0);
    CHECK_EQ(ValidateHMesh(plain), 0);
    uint64_t off = 0, sz = 0;
    CHECK(!FindChunk(plain, ChunkId::LodTable, off, sz)); // absent when no LODs

    // --- with one reduced LOD: chunks present, sizes correct, validates ----------
    m.lodCount   = 1;
    m.lodIndices = {0, 1, 2}; // a (degenerate) reduced level reusing all 3 verts
    m.lodTable   = {MeshLod{0, 3}};
    const auto lod = (dir / "lod.hmesh").string();
    CHECK_EQ(WriteHMesh(lod, m, bounds, std::span(&sm, 1), {}), 0);
    CHECK_EQ(ValidateHMesh(lod), 0);

    CHECK(FindChunk(lod, ChunkId::LodIndices, off, sz));
    CHECK_EQ(sz, 3u * sizeof(uint32_t));
    CHECK(FindChunk(lod, ChunkId::LodTable, off, sz));
    CHECK_EQ(sz, sizeof(LodTableHeader) + 1u * sizeof(MeshLod));

    // --- a LOD range past the LODI buffer must fail validation -------------------
    m.lodTable = {MeshLod{0, 6}}; // 6 > 3 available indices
    const auto bad = (dir / "bad.hmesh").string();
    CHECK_EQ(WriteHMesh(bad, m, bounds, std::span(&sm, 1), {}), 0);
    CHECK(ValidateHMesh(bad) != 0);

    fs::remove_all(dir);
    return test::summary();
}
