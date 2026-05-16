#include "encode_mesh.hpp"

#include "../deps/fmt.hpp"
#include "../deps/obj.hpp"
#include "../deps/tiny/mikktspace.h"

#include <meshoptimizer.h>

#include <algorithm>
#include <cmath>
#include <cstring>
#include <limits>
#include <unordered_map>

namespace assetc
{

namespace
{

constexpr uint64_t FnvOffset = 0xcbf29ce484222325ULL;
constexpr uint64_t FnvPrime  = 0x00000100000001b3ULL;

// Flat per-corner vertex: one entry per triangle vertex, pre-dedupe.
struct FlatVertex
{
    Vec3 pos;
    Vec3 normal;
    Vec2 uv;
    Vec4 tangent; // MikkTSpace fills xyz + handedness sign in w
};
static_assert(sizeof(FlatVertex) == 48, "FlatVertex must have no padding");

struct MikkCtx
{
    std::vector<FlatVertex> *verts;
};

int MikkGetNumFaces(const SMikkTSpaceContext *ctx)
{
    auto *m = static_cast<MikkCtx *>(ctx->m_pUserData);
    return static_cast<int>(m->verts->size() / 3);
}

int MikkGetNumVerticesOfFace(const SMikkTSpaceContext *, const int)
{
    return 3;
}

void MikkGetPosition(const SMikkTSpaceContext *ctx, float out[], const int face, const int vert)
{
    auto       *m = static_cast<MikkCtx *>(ctx->m_pUserData);
    const auto &p = (*m->verts)[face * 3 + vert].pos;
    out[0]        = p.x;
    out[1]        = p.y;
    out[2]        = p.z;
}

void MikkGetNormal(const SMikkTSpaceContext *ctx, float out[], const int face, const int vert)
{
    auto       *m = static_cast<MikkCtx *>(ctx->m_pUserData);
    const auto &n = (*m->verts)[face * 3 + vert].normal;
    out[0]        = n.x;
    out[1]        = n.y;
    out[2]        = n.z;
}

void MikkGetTexCoord(const SMikkTSpaceContext *ctx, float out[], const int face, const int vert)
{
    auto       *m  = static_cast<MikkCtx *>(ctx->m_pUserData);
    const auto &uv = (*m->verts)[face * 3 + vert].uv;
    out[0]         = uv.x;
    out[1]         = uv.y;
}

void MikkSetTSpaceBasic(const SMikkTSpaceContext *ctx, const float t[], const float sign,
                        const int face, const int vert)
{
    auto *m  = static_cast<MikkCtx *>(ctx->m_pUserData);
    auto &tg = (*m->verts)[face * 3 + vert].tangent;
    tg       = Vec4{t[0], t[1], t[2], sign};
}

// Dedupe key: byte-identical FlatVertex (no padding -> safe to memcmp/hash).
struct VertexKey
{
    FlatVertex v;
    bool       operator==(const VertexKey &o) const noexcept
    {
        return std::memcmp(&v, &o.v, sizeof(FlatVertex)) == 0;
    }
};

struct VertexKeyHash
{
    size_t operator()(const VertexKey &k) const noexcept
    {
        const auto *p = reinterpret_cast<const uint8_t *>(&k.v);
        uint64_t    h = FnvOffset;
        for (size_t i = 0; i < sizeof(FlatVertex); ++i)
        {
            h ^= p[i];
            h *= FnvPrime;
        }
        return static_cast<size_t>(h);
    }
};

inline char AsciiLower(char c) noexcept
{
    return (c >= 'A' && c <= 'Z') ? static_cast<char>(c - 'A' + 'a') : c;
}

} // namespace

uint64_t HashAssetRef(std::string_view runtimeRefNoExt) noexcept
{
    uint64_t h = FnvOffset;
    for (char c : runtimeRefNoExt)
    {
        const auto b = static_cast<uint8_t>(AsciiLower(c));
        h ^= b;
        h *= FnvPrime;
    }
    return h;
}

CompiledMesh BuildFromObj(const obj::OBJ &src, std::string_view meshRef)
{
    CompiledMesh cm{};

    const auto &attrib = src.attrib;
    if (attrib.vertices.empty())
    {
        fmtx::Error("BuildFromObj: source has no positions");
        return cm;
    }

    // 1) Expand shapes into a flat triangle list of FlatVertex.
    //    tinyobj's ObjReader triangulates by default, so every face is 3 verts.
    size_t triangleCount = 0;
    for (const auto &sh : src.shapes)
        triangleCount += sh.mesh.num_face_vertices.size();

    std::vector<FlatVertex> flat;
    flat.reserve(triangleCount * 3);
    std::vector<int> faceMaterial;
    faceMaterial.reserve(triangleCount);

    bool warnedMissingUV     = false;
    bool warnedMissingNormal = false;

    for (const auto &sh : src.shapes)
    {
        size_t cursor = 0;
        for (size_t f = 0; f < sh.mesh.num_face_vertices.size(); ++f)
        {
            const int nv = sh.mesh.num_face_vertices[f];
            if (nv != 3)
            {
                fmtx::Warn(fmt::format(
                    "BuildFromObj: non-triangle face ({}); enable triangulate=true", nv));
                cursor += nv;
                faceMaterial.push_back(-1);
                continue;
            }

            for (int k = 0; k < 3; ++k)
            {
                const auto &idx = sh.mesh.indices[cursor + k];
                FlatVertex  v{};

                const int pi = idx.vertex_index;
                v.pos        = Vec3{attrib.vertices[3 * pi + 0], attrib.vertices[3 * pi + 1],
                                    attrib.vertices[3 * pi + 2]};

                if (idx.normal_index >= 0)
                {
                    const int ni = idx.normal_index;
                    v.normal     = Vec3{attrib.normals[3 * ni + 0], attrib.normals[3 * ni + 1],
                                        attrib.normals[3 * ni + 2]};
                }
                else
                {
                    if (!warnedMissingNormal)
                    {
                        fmtx::Warn("BuildFromObj: missing normals; using face normals");
                        warnedMissingNormal = true;
                    }
                    v.normal = Vec3{0.0f, 0.0f, 0.0f};
                }

                if (idx.texcoord_index >= 0)
                {
                    const int ti = idx.texcoord_index;
                    // Flip V for Vulkan (tinyobj returns GL-style UV).
                    v.uv = Vec2{attrib.texcoords[2 * ti + 0],
                                1.0f - attrib.texcoords[2 * ti + 1]};
                }
                else
                {
                    if (!warnedMissingUV)
                    {
                        fmtx::Warn("BuildFromObj: missing UVs; tangents will be degenerate");
                        warnedMissingUV = true;
                    }
                    v.uv = Vec2{0.0f, 0.0f};
                }

                flat.push_back(v);
            }
            faceMaterial.push_back(sh.mesh.material_ids[f]);
            cursor += 3;
        }
    }

    if (flat.empty())
    {
        fmtx::Error("BuildFromObj: produced 0 triangles");
        return cm;
    }

    // 2) Fill missing normals with per-face normals.
    if (warnedMissingNormal)
    {
        for (size_t i = 0; i < flat.size(); i += 3)
        {
            const auto &a = flat[i + 0].pos;
            const auto &b = flat[i + 1].pos;
            const auto &c = flat[i + 2].pos;
            const float ux = b.x - a.x, uy = b.y - a.y, uz = b.z - a.z;
            const float vx = c.x - a.x, vy = c.y - a.y, vz = c.z - a.z;
            Vec3        n{uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx};
            const float len = std::sqrt(n.x * n.x + n.y * n.y + n.z * n.z);
            if (len > 0.0f)
            {
                n.x /= len;
                n.y /= len;
                n.z /= len;
            }
            flat[i + 0].normal = n;
            flat[i + 1].normal = n;
            flat[i + 2].normal = n;
        }
    }

    // 3) MikkTSpace tangents — pre-dedupe (mikk needs face-vertex access).
    MikkCtx              mctx{&flat};
    SMikkTSpaceInterface mIface{};
    mIface.m_getNumFaces          = MikkGetNumFaces;
    mIface.m_getNumVerticesOfFace = MikkGetNumVerticesOfFace;
    mIface.m_getPosition          = MikkGetPosition;
    mIface.m_getNormal            = MikkGetNormal;
    mIface.m_getTexCoord          = MikkGetTexCoord;
    mIface.m_setTSpaceBasic       = MikkSetTSpaceBasic;

    SMikkTSpaceContext mkc{};
    mkc.m_pInterface = &mIface;
    mkc.m_pUserData  = &mctx;
    if (genTangSpaceDefault(&mkc) == 0)
    {
        fmtx::Error("BuildFromObj: MikkTSpace failed");
        return cm;
    }

    // 4) Dedupe.
    std::vector<CpuVertex> cpuVerts;
    cpuVerts.reserve(flat.size() / 2);
    std::vector<uint32_t> indices;
    indices.reserve(flat.size());

    std::unordered_map<VertexKey, uint32_t, VertexKeyHash> seen;
    seen.reserve(flat.size());

    for (const auto &fv : flat)
    {
        VertexKey key{fv};
        auto [it, inserted] = seen.emplace(key, static_cast<uint32_t>(cpuVerts.size()));
        if (inserted)
            cpuVerts.push_back(CpuVertex{fv.pos, fv.normal, fv.tangent, fv.uv});
        indices.push_back(it->second);
    }

    // 5) Bounds.
    Vec3 mn{std::numeric_limits<float>::infinity(), std::numeric_limits<float>::infinity(),
            std::numeric_limits<float>::infinity()};
    Vec3 mx{-std::numeric_limits<float>::infinity(), -std::numeric_limits<float>::infinity(),
            -std::numeric_limits<float>::infinity()};
    for (const auto &v : cpuVerts)
    {
        mn.x = std::min(mn.x, v.pos.x);
        mn.y = std::min(mn.y, v.pos.y);
        mn.z = std::min(mn.z, v.pos.z);
        mx.x = std::max(mx.x, v.pos.x);
        mx.y = std::max(mx.y, v.pos.y);
        mx.z = std::max(mx.z, v.pos.z);
    }
    const Vec3 center{0.5f * (mn.x + mx.x), 0.5f * (mn.y + mx.y), 0.5f * (mn.z + mx.z)};
    float      r2 = 0.0f;
    for (const auto &v : cpuVerts)
    {
        const float dx = v.pos.x - center.x;
        const float dy = v.pos.y - center.y;
        const float dz = v.pos.z - center.z;
        r2             = std::max(r2, dx * dx + dy * dy + dz * dz);
    }
    cm.bounds = MeshBounds{mn, mx, center, std::sqrt(r2)};

    // 6) CpuVertex -> GpuVertex with oct packing.
    cm.mesh.vertices.resize(cpuVerts.size());
    for (size_t i = 0; i < cpuVerts.size(); ++i)
    {
        const auto &cv = cpuVerts[i];
        auto       &gv = cm.mesh.vertices[i];
        gv.position[0] = cv.pos.x;
        gv.position[1] = cv.pos.y;
        gv.position[2] = cv.pos.z;

        const auto n = OctEncode(cv.normal);
        gv.normal[0] = n[0];
        gv.normal[1] = n[1];
        gv.normal[2] = 0;
        gv.normal[3] = 0;

        const auto t = OctEncodeTangent(Vec3{cv.tangent.x, cv.tangent.y, cv.tangent.z},
                                        cv.tangent.w);
        gv.tangent[0] = t[0];
        gv.tangent[1] = t[1];
        gv.tangent[2] = 0;
        gv.tangent[3] = 0;

        gv.uv[0] = cv.uv.x;
        gv.uv[1] = cv.uv.y;
    }
    cm.mesh.indices = std::move(indices);

    // 7) Vertex cache optimization.
    meshopt_optimizeVertexCache(cm.mesh.indices.data(), cm.mesh.indices.data(),
                                cm.mesh.indices.size(), cm.mesh.vertices.size());

    // 8) Meshlet build (Vulkan mesh-shader friendly limits: 64 verts, 124 tris).
    constexpr size_t MaxVerts    = 64;
    constexpr size_t MaxTris     = 124;
    constexpr float  ConeWeight  = 0.25f;
    const size_t     maxMeshlets = meshopt_buildMeshletsBound(cm.mesh.indices.size(),
                                                              MaxVerts, MaxTris);

    std::vector<meshopt_Meshlet> mo(maxMeshlets);
    std::vector<unsigned int>    mvi(maxMeshlets * MaxVerts);
    std::vector<unsigned char>   mti(maxMeshlets * MaxTris * 3);

    const size_t meshletCount = meshopt_buildMeshlets(
        mo.data(), mvi.data(), mti.data(), cm.mesh.indices.data(), cm.mesh.indices.size(),
        &cm.mesh.vertices[0].position[0], cm.mesh.vertices.size(), sizeof(GpuVertex), MaxVerts,
        MaxTris, ConeWeight);

    if (meshletCount == 0)
    {
        fmtx::Error("BuildFromObj: meshopt_buildMeshlets produced 0 meshlets");
        return cm;
    }

    const auto &last = mo[meshletCount - 1];
    mvi.resize(last.vertex_offset + last.vertex_count);
    mti.resize(last.triangle_offset + ((last.triangle_count * 3u + 3u) & ~3u));
    mo.resize(meshletCount);

    cm.mesh.meshlets.resize(meshletCount);
    for (size_t i = 0; i < meshletCount; ++i)
    {
        cm.mesh.meshlets[i] = Meshlet{mo[i].vertex_offset, mo[i].vertex_count,
                                      mo[i].triangle_offset, mo[i].triangle_count};
    }
    cm.mesh.meshletVertices.assign(mvi.begin(), mvi.end());
    cm.mesh.meshletTriangles.resize(mti.size() / 3);
    for (size_t i = 0; i < mti.size() / 3; ++i)
    {
        cm.mesh.meshletTriangles[i] =
            MeshletTriangle{mti[3 * i + 0], mti[3 * i + 1], mti[3 * i + 2]};
    }

    // 9) Per-meshlet bounds (frustum + cone culling).
    cm.mesh.meshletBounds.resize(meshletCount);
    for (size_t i = 0; i < meshletCount; ++i)
    {
        const meshopt_Bounds b = meshopt_computeMeshletBounds(
            &mvi[mo[i].vertex_offset], &mti[mo[i].triangle_offset], mo[i].triangle_count,
            &cm.mesh.vertices[0].position[0], cm.mesh.vertices.size(), sizeof(GpuVertex));
        cm.mesh.meshletBounds[i] =
            MeshletBounds{Vec3{b.center[0], b.center[1], b.center[2]}, b.radius,
                          Vec3{b.cone_axis[0], b.cone_axis[1], b.cone_axis[2]}, b.cone_cutoff};
    }

    // 10) Material refs.
    //     For each *used* material, hash "<meshRef>/<material_name_lowercased>".
    //     Mesh sub-grouping by material is deferred to v2 (SUBM chunk); v1 just
    //     records the set of materials this mesh touches so the engine can pre-resolve.
    std::vector<bool> used(src.materials.size(), false);
    for (int mid : faceMaterial)
    {
        if (mid >= 0 && static_cast<size_t>(mid) < src.materials.size())
            used[mid] = true;
    }
    for (size_t m = 0; m < src.materials.size(); ++m)
    {
        if (!used[m])
            continue;
        std::string ref(meshRef);
        ref.push_back('/');
        for (char c : src.materials[m].name)
            ref.push_back(AsciiLower(c));
        cm.materialRefs.push_back(HashAssetRef(ref));
    }

    return cm;
}

} // namespace assetc
