#include "encode_mesh.hpp"

#include "../deps/fmt.hpp"
#include "../deps/gltf.hpp"
#include "../deps/obj.hpp"
#include "../deps/tiny/mikktspace.h"

#include <meshoptimizer.h>

#include <algorithm>
#include <cmath>
#include <cstring>
#include <limits>
#include <map>
#include <string>
#include <unordered_map>
#include <utility>

namespace assetc
{

namespace
{

constexpr uint64_t FnvOffset = 0xcbf29ce484222325ULL;
constexpr uint64_t FnvPrime  = 0x00000100000001b3ULL;

// Flat per-corner vertex: one entry per triangle vertex, pre-dedupe.
struct FlatVertex
{
    Vec3     pos;
    Vec3     normal;
    Vec2     uv;
    Vec4     tangent;        // MikkTSpace fills xyz + handedness sign in w
    uint16_t joints[4] = {0, 0, 0, 0}; // skinning (zero for static meshes)
    float    weights[4] = {0, 0, 0, 0};
};
static_assert(sizeof(FlatVertex) == 72, "FlatVertex must have no padding");

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

void FillFaceNormals(std::vector<FlatVertex> &flat) noexcept
{
    for (size_t i = 0; i + 3 <= flat.size(); i += 3)
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

bool RunMikkTSpace(std::vector<FlatVertex> &flat)
{
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
    return genTangSpaceDefault(&mkc) != 0;
}

// --- glTF accessor helpers ---------------------------------------------------

const uint8_t *AccessorPtr(const tg3_model &m, const tg3_accessor &a) noexcept
{
    if (a.buffer_view < 0 || static_cast<uint32_t>(a.buffer_view) >= m.buffer_views_count)
        return nullptr;
    const auto &bv = m.buffer_views[a.buffer_view];
    if (bv.buffer < 0 || static_cast<uint32_t>(bv.buffer) >= m.buffers_count)
        return nullptr;
    const auto &buf = m.buffers[bv.buffer];
    return buf.data.data + bv.byte_offset + a.byte_offset;
}

size_t AccessorStride(const tg3_model &m, const tg3_accessor &a) noexcept
{
    if (a.buffer_view >= 0 && static_cast<uint32_t>(a.buffer_view) < m.buffer_views_count)
    {
        const auto &bv = m.buffer_views[a.buffer_view];
        if (bv.byte_stride > 0)
            return bv.byte_stride;
    }
    const int cs = tg3_component_size(a.component_type);
    const int nc = tg3_num_components(a.type);
    return static_cast<size_t>(cs) * static_cast<size_t>(nc);
}

int FindAttr(const tg3_primitive &p, const char *name) noexcept
{
    for (uint32_t i = 0; i < p.attributes_count; ++i)
    {
        if (tg3_str_equals_cstr(p.attributes[i].key, name))
            return p.attributes[i].value;
    }
    return -1;
}

void DumpGltfStructure(const tg3_model &m)
{
    fmtx::Info(fmt::format("gltf: meshes={} materials={} accessors={} buffer_views={} buffers={}",
                           m.meshes_count, m.materials_count, m.accessors_count,
                           m.buffer_views_count, m.buffers_count));
    for (uint32_t mi = 0; mi < m.meshes_count; ++mi)
    {
        const auto       &mesh = m.meshes[mi];
        const std::string name(mesh.name.data ? mesh.name.data : "", mesh.name.len);
        fmtx::Info(fmt::format("  mesh[{}] \"{}\" primitives={}", mi, name,
                               mesh.primitives_count));
        for (uint32_t pi = 0; pi < mesh.primitives_count; ++pi)
        {
            const auto &prim   = mesh.primitives[pi];
            const int   posIdx = FindAttr(prim, "POSITION");
            uint64_t    vc     = 0;
            if (posIdx >= 0 && static_cast<uint32_t>(posIdx) < m.accessors_count)
                vc = m.accessors[posIdx].count;
            uint64_t ic = 0;
            if (prim.indices >= 0 && static_cast<uint32_t>(prim.indices) < m.accessors_count)
                ic = m.accessors[prim.indices].count;
            const int mode = prim.mode < 0 ? TG3_MODE_TRIANGLES : prim.mode;

            // Build a compact attribute list "POSITION,NORMAL,TEXCOORD_0,TANGENT".
            std::string attrs;
            for (uint32_t i = 0; i < prim.attributes_count; ++i)
            {
                if (i > 0)
                    attrs.push_back(',');
                attrs.append(prim.attributes[i].key.data,
                             prim.attributes[i].key.len);
            }
            fmtx::Info(fmt::format("    prim[{}] verts={} idx={} mode={} material={} attrs=[{}]",
                                   pi, vc, ic, mode, prim.material, attrs));
        }
    }
    // Spot-check: any bufferview marked as draco-decoded?
    for (uint32_t bi = 0; bi < m.buffer_views_count; ++bi)
    {
        if (m.buffer_views[bi].draco_decoded)
        {
            fmtx::Info(fmt::format("  bufferview[{}] draco_decoded=1", bi));
        }
    }
}

// One source primitive to be finalized into a submesh.
struct PrimitiveInput
{
    std::vector<FlatVertex> flat;           // per-corner triangle list (size % 3 == 0)
    int                     sourceMaterial; // index into the source material set, or -1
    bool                    skinned = false; // has JOINTS_0/WEIGHTS_0
};

// --- Shared finalize: per-primitive dedupe/optimize/meshletize -> submesh table ---
//
// Each primitive becomes one SubMesh: deduped into its own vertex range, appended
// to the shared buffers, vertex-cache optimized, and meshletized independently so
// no meshlet ever crosses a material boundary. Indices are global (offset by the
// primitive's baseVertex). Materials are compacted: only referenced ones are
// emitted, in first-use order, with SubMesh::materialSlot indexing into that list.
CompiledMesh FinalizeMesh(std::vector<PrimitiveInput>     &&prims,
                          std::span<const std::string_view> allMaterialNames,
                          std::string_view                  sourceRef)
{
    CompiledMesh cm{};

    constexpr size_t MaxVerts   = 64;
    constexpr size_t MaxTris    = 124;
    constexpr float  ConeWeight = 0.25f;
    constexpr float  Inf        = std::numeric_limits<float>::infinity();

    // If any primitive is skinned, the output carries a SKIN stream parallel to the
    // vertex buffer (zero-weight entries for any non-skinned primitives mixed in).
    bool anySkinned = false;
    for (const auto &p : prims)
        anySkinned |= p.skinned;

    // Compact material slots: source index -> dense slot, in first-use order.
    std::vector<int>                  denseToSource;
    std::unordered_map<int, uint32_t> sourceToDense;
    auto materialSlotFor = [&](int src) -> uint32_t {
        if (src < 0)
            return kNoMaterial;
        auto [it, inserted] =
            sourceToDense.emplace(src, static_cast<uint32_t>(denseToSource.size()));
        if (inserted)
            denseToSource.push_back(src);
        return it->second;
    };

    for (auto &prim : prims)
    {
        if (prim.flat.empty())
            continue;

        // Dedupe within this primitive.
        std::vector<CpuVertex> localVerts;
        std::vector<uint32_t>  localIdx;
        localVerts.reserve(prim.flat.size() / 2);
        localIdx.reserve(prim.flat.size());
        std::unordered_map<VertexKey, uint32_t, VertexKeyHash> seen;
        seen.reserve(prim.flat.size());
        for (const auto &fv : prim.flat)
        {
            VertexKey key{fv};
            auto [it, inserted] = seen.emplace(key, static_cast<uint32_t>(localVerts.size()));
            if (inserted)
            {
                CpuVertex cv{fv.pos, fv.normal, fv.tangent, fv.uv, {}, {}};
                for (int j = 0; j < 4; ++j)
                {
                    cv.joints[j]  = fv.joints[j];
                    cv.weights[j] = fv.weights[j];
                }
                localVerts.push_back(cv);
            }
            localIdx.push_back(it->second);
        }

        meshopt_optimizeVertexCache(localIdx.data(), localIdx.data(), localIdx.size(),
                                    localVerts.size());

        // Per-submesh bounds (AABB + sphere) from local positions.
        Vec3 mn{Inf, Inf, Inf};
        Vec3 mx{-Inf, -Inf, -Inf};
        for (const auto &v : localVerts)
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
        for (const auto &v : localVerts)
        {
            const float dx = v.pos.x - center.x, dy = v.pos.y - center.y, dz = v.pos.z - center.z;
            r2             = std::max(r2, dx * dx + dy * dy + dz * dz);
        }

        const uint32_t baseVertex  = static_cast<uint32_t>(cm.mesh.vertices.size());
        const uint32_t firstIndex  = static_cast<uint32_t>(cm.mesh.indices.size());

        // Append vertices (oct pack) to the shared buffer.
        cm.mesh.vertices.reserve(cm.mesh.vertices.size() + localVerts.size());
        for (const auto &cv : localVerts)
        {
            GpuVertex gv{};
            gv.position[0] = cv.pos.x;
            gv.position[1] = cv.pos.y;
            gv.position[2] = cv.pos.z;
            const auto n   = OctEncode(cv.normal);
            gv.normal[0]   = n[0];
            gv.normal[1]   = n[1];
            const auto t   = OctEncodeTangent(Vec3{cv.tangent.x, cv.tangent.y, cv.tangent.z},
                                              cv.tangent.w);
            gv.tangent[0]  = t[0];
            gv.tangent[1]  = t[1];
            gv.uv[0]       = cv.uv.x;
            gv.uv[1]       = cv.uv.y;
            cm.mesh.vertices.push_back(gv);

            if (anySkinned)
            {
                // Normalize weights to sum 1; fall back to a rigid bind to joint 0
                // when a vertex has no weights (non-skinned prim in a skinned mesh).
                GpuSkinVertex sv{};
                float         sum = cv.weights[0] + cv.weights[1] + cv.weights[2] + cv.weights[3];
                if (sum <= 0.0f)
                {
                    sv.joints[0]  = 0;
                    sv.weights[0] = 1.0f;
                }
                else
                {
                    for (int j = 0; j < 4; ++j)
                    {
                        sv.joints[j]  = cv.joints[j];
                        sv.weights[j] = cv.weights[j] / sum;
                    }
                }
                cm.mesh.skinVertices.push_back(sv);
            }
        }

        // Append global indices (offset into the shared vertex buffer).
        cm.mesh.indices.reserve(cm.mesh.indices.size() + localIdx.size());
        for (uint32_t li : localIdx)
            cm.mesh.indices.push_back(li + baseVertex);

        // Meshlets over THIS submesh only, built against local positions.
        const size_t maxMeshlets = meshopt_buildMeshletsBound(localIdx.size(), MaxVerts, MaxTris);
        std::vector<meshopt_Meshlet> mo(maxMeshlets);
        std::vector<unsigned int>    mvi(maxMeshlets * MaxVerts);
        std::vector<unsigned char>   mti(maxMeshlets * MaxTris * 3);
        const size_t                 meshletCount = meshopt_buildMeshlets(
            mo.data(), mvi.data(), mti.data(), localIdx.data(), localIdx.size(),
            &localVerts[0].pos.x, localVerts.size(), sizeof(CpuVertex), MaxVerts, MaxTris,
            ConeWeight);
        if (meshletCount == 0)
        {
            fmtx::Error("FinalizeMesh: meshopt_buildMeshlets produced 0 meshlets");
            return CompiledMesh{};
        }

        const uint32_t firstMeshlet = static_cast<uint32_t>(cm.mesh.meshlets.size());
        for (size_t i = 0; i < meshletCount; ++i)
        {
            const auto    &srcM     = mo[i];
            const uint32_t mlvrBase = static_cast<uint32_t>(cm.mesh.meshletVertices.size());
            for (uint32_t v = 0; v < srcM.vertex_count; ++v)
                cm.mesh.meshletVertices.push_back(mvi[srcM.vertex_offset + v] + baseVertex);

            const uint32_t       triBase  = static_cast<uint32_t>(cm.mesh.meshletTriangles.size());
            const unsigned char *triBytes = &mti[srcM.triangle_offset];
            for (uint32_t t = 0; t < srcM.triangle_count; ++t)
                cm.mesh.meshletTriangles.push_back(MeshletTriangle{
                    triBytes[t * 3 + 0], triBytes[t * 3 + 1], triBytes[t * 3 + 2]});

            cm.mesh.meshlets.push_back(
                Meshlet{mlvrBase, srcM.vertex_count, triBase, srcM.triangle_count});

            const meshopt_Bounds b = meshopt_computeMeshletBounds(
                &mvi[srcM.vertex_offset], &mti[srcM.triangle_offset], srcM.triangle_count,
                &localVerts[0].pos.x, localVerts.size(), sizeof(CpuVertex));
            cm.mesh.meshletBounds.push_back(
                MeshletBounds{Vec3{b.center[0], b.center[1], b.center[2]}, b.radius,
                              Vec3{b.cone_axis[0], b.cone_axis[1], b.cone_axis[2]}, b.cone_cutoff});
        }

        // Reduced LODs: simplify this submesh's index set toward fractions of its
        // triangle count. Each level is appended to the shared LOD index buffer as
        // global indices; an empty range (count 0) means simplification stalled and
        // the engine should fall back to the previous level.
        constexpr uint32_t kExtraLods           = 2;
        constexpr float    kLodRatios[kExtraLods] = {0.5f, 0.25f};
        cm.mesh.lodCount                        = kExtraLods;
        for (uint32_t l = 0; l < kExtraLods; ++l)
        {
            const size_t target =
                std::max<size_t>(3, (static_cast<size_t>(localIdx.size() * kLodRatios[l]) / 3) * 3);
            MeshLod lod{};
            if (target < localIdx.size())
            {
                std::vector<unsigned int> simplified(localIdx.size());
                float                     err = 0.0f;
                const size_t              got = meshopt_simplify(
                    simplified.data(), localIdx.data(), localIdx.size(), &localVerts[0].pos.x,
                    localVerts.size(), sizeof(CpuVertex), target, /*target_error=*/0.05f,
                    /*options=*/0, &err);
                if (got >= 3 && got < localIdx.size())
                {
                    lod.firstIndex = static_cast<uint32_t>(cm.mesh.lodIndices.size());
                    lod.indexCount = static_cast<uint32_t>(got);
                    cm.mesh.lodIndices.reserve(cm.mesh.lodIndices.size() + got);
                    for (size_t i = 0; i < got; ++i)
                        cm.mesh.lodIndices.push_back(simplified[i] + baseVertex);
                }
            }
            cm.mesh.lodTable.push_back(lod);
        }

        SubMesh sm{};
        sm.firstIndex   = firstIndex;
        sm.indexCount   = static_cast<uint32_t>(localIdx.size());
        sm.firstMeshlet = firstMeshlet;
        sm.meshletCount = static_cast<uint32_t>(cm.mesh.meshlets.size()) - firstMeshlet;
        sm.materialSlot = materialSlotFor(prim.sourceMaterial);
        sm.baseVertex   = 0;
        sm.bounds       = MeshBounds{mn, mx, center, std::sqrt(r2)};
        cm.submeshes.push_back(sm);
    }

    if (cm.submeshes.empty())
    {
        fmtx::Error("FinalizeMesh: no usable primitives");
        return CompiledMesh{};
    }

    // Mesh-wide bounds = union over all vertices.
    Vec3 mn{Inf, Inf, Inf};
    Vec3 mx{-Inf, -Inf, -Inf};
    for (const auto &v : cm.mesh.vertices)
    {
        mn.x = std::min(mn.x, v.position[0]);
        mn.y = std::min(mn.y, v.position[1]);
        mn.z = std::min(mn.z, v.position[2]);
        mx.x = std::max(mx.x, v.position[0]);
        mx.y = std::max(mx.y, v.position[1]);
        mx.z = std::max(mx.z, v.position[2]);
    }
    const Vec3 center{0.5f * (mn.x + mx.x), 0.5f * (mn.y + mx.y), 0.5f * (mn.z + mx.z)};
    float      r2 = 0.0f;
    for (const auto &v : cm.mesh.vertices)
    {
        const float dx = v.position[0] - center.x, dy = v.position[1] - center.y,
                    dz = v.position[2] - center.z;
        r2             = std::max(r2, dx * dx + dy * dy + dz * dz);
    }
    cm.bounds = MeshBounds{mn, mx, center, std::sqrt(r2)};

    // Compact material refs, one per dense slot in first-use order. materialSourceIndex
    // records the source material index for each dense slot, so the material-table
    // encoder can build .hmat rows in the same slot order the submeshes index into.
    cm.materialRefs.reserve(denseToSource.size());
    cm.materialSourceIndex.reserve(denseToSource.size());
    for (int src : denseToSource)
    {
        std::string ref(sourceRef);
        ref.push_back('/');
        ref.append(MaterialLeaf(allMaterialNames[src], static_cast<size_t>(src), allMaterialNames));
        cm.materialRefs.push_back(HashAssetRef(ref));
        cm.materialSourceIndex.push_back(static_cast<uint32_t>(src));
    }

    return cm;
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

std::string MaterialLeaf(std::string_view name, size_t index,
                         std::span<const std::string_view> allNames)
{
    if (name.empty())
        return fmt::format("material_{}", index);

    std::string lower;
    lower.reserve(name.size());
    for (char c : name)
        lower.push_back(AsciiLower(c));

    for (size_t j = 0; j < allNames.size(); ++j)
    {
        if (j == index)
            continue;
        const auto &other = allNames[j];
        if (other.size() != name.size())
            continue;
        bool same = true;
        for (size_t k = 0; k < name.size(); ++k)
        {
            if (AsciiLower(other[k]) != lower[k])
            {
                same = false;
                break;
            }
        }
        if (same)
            return fmt::format("material_{}", index);
    }
    return lower;
}

CompiledMesh BuildFromObj(const obj::OBJ &src, std::string_view sourceRef)
{
    CompiledMesh cm{};

    const auto &attrib = src.attrib;
    if (attrib.vertices.empty())
    {
        fmtx::Error("BuildFromObj: source has no positions");
        return cm;
    }

    // 1) Expand shapes into per-material buckets of FlatVertex. One bucket becomes
    //    one submesh. std::map keeps a deterministic (material-id ordered) output.
    //    tinyobj's ObjReader triangulates by default, so every face is 3 verts.
    struct ObjBucket
    {
        std::vector<FlatVertex> flat;
        bool                    missingNormal = false;
    };
    std::map<int, ObjBucket> buckets;

    bool      warnedMissingUV     = false;
    bool      warnedMissingNormal = false;
    const int materialCount       = static_cast<int>(src.materials.size());

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
                continue;
            }

            int matId = f < sh.mesh.material_ids.size() ? sh.mesh.material_ids[f] : -1;
            if (matId < 0 || matId >= materialCount)
                matId = -1;
            auto &bk = buckets[matId];

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
                    v.normal         = Vec3{0.0f, 0.0f, 0.0f};
                    bk.missingNormal = true;
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

                bk.flat.push_back(v);
            }
            cursor += 3;
        }
    }

    if (buckets.empty())
    {
        fmtx::Error("BuildFromObj: produced 0 triangles");
        return cm;
    }

    std::vector<PrimitiveInput> prims;
    prims.reserve(buckets.size());
    for (auto &[matId, bk] : buckets)
    {
        if (bk.flat.empty())
            continue;
        if (bk.missingNormal)
            FillFaceNormals(bk.flat);
        if (!RunMikkTSpace(bk.flat))
        {
            fmtx::Error("BuildFromObj: MikkTSpace failed");
            return cm;
        }
        prims.push_back(PrimitiveInput{std::move(bk.flat), matId});
    }

    std::vector<std::string_view> matNames;
    matNames.reserve(src.materials.size());
    for (const auto &mat : src.materials)
        matNames.emplace_back(mat.name);

    return FinalizeMesh(std::move(prims), matNames, sourceRef);
}

// --- BuildFromGltf -----------------------------------------------------------

namespace
{

// --- Node transforms: bake glTF world matrices into vertices (combine mode) ----
//
// Column-major 4x4, matching glTF's node.matrix layout. m[col*4 + row].
struct Mat4
{
    double m[16];
};

Mat4 Mat4Identity() noexcept
{
    Mat4 r{};
    r.m[0] = r.m[5] = r.m[10] = r.m[15] = 1.0;
    return r;
}

Mat4 Mat4Mul(const Mat4 &a, const Mat4 &b) noexcept // a * b
{
    Mat4 r{};
    for (int c = 0; c < 4; ++c)
        for (int row = 0; row < 4; ++row)
        {
            double s = 0.0;
            for (int k = 0; k < 4; ++k)
                s += a.m[k * 4 + row] * b.m[c * 4 + k];
            r.m[c * 4 + row] = s;
        }
    return r;
}

Mat4 Mat4FromColMajor(const double *d) noexcept
{
    Mat4 r{};
    for (int i = 0; i < 16; ++i)
        r.m[i] = d[i];
    return r;
}

// Compose T * R * S from glTF node TRS (rotation is a unit quaternion x,y,z,w).
Mat4 Mat4FromTRS(const double t[3], const double q[4], const double s[3]) noexcept
{
    const double x = q[0], y = q[1], z = q[2], w = q[3];
    const double xx = x * x, yy = y * y, zz = z * z;
    const double xy = x * y, xz = x * z, yz = y * z, wx = w * x, wy = w * y, wz = w * z;
    Mat4         r = Mat4Identity();
    r.m[0]  = (1.0 - 2.0 * (yy + zz)) * s[0];
    r.m[1]  = (2.0 * (xy + wz)) * s[0];
    r.m[2]  = (2.0 * (xz - wy)) * s[0];
    r.m[4]  = (2.0 * (xy - wz)) * s[1];
    r.m[5]  = (1.0 - 2.0 * (xx + zz)) * s[1];
    r.m[6]  = (2.0 * (yz + wx)) * s[1];
    r.m[8]  = (2.0 * (xz + wy)) * s[2];
    r.m[9]  = (2.0 * (yz - wx)) * s[2];
    r.m[10] = (1.0 - 2.0 * (xx + yy)) * s[2];
    r.m[12] = t[0];
    r.m[13] = t[1];
    r.m[14] = t[2];
    return r;
}

Vec3 XformPoint(const Mat4 &M, const Vec3 &p) noexcept
{
    return Vec3{static_cast<float>(M.m[0] * p.x + M.m[4] * p.y + M.m[8] * p.z + M.m[12]),
                static_cast<float>(M.m[1] * p.x + M.m[5] * p.y + M.m[9] * p.z + M.m[13]),
                static_cast<float>(M.m[2] * p.x + M.m[6] * p.y + M.m[10] * p.z + M.m[14])};
}

Vec3 XformDir(const Mat4 &M, const Vec3 &v) noexcept // 3x3 linear part (tangents)
{
    return Vec3{static_cast<float>(M.m[0] * v.x + M.m[4] * v.y + M.m[8] * v.z),
                static_cast<float>(M.m[1] * v.x + M.m[5] * v.y + M.m[9] * v.z),
                static_cast<float>(M.m[2] * v.x + M.m[6] * v.y + M.m[10] * v.z)};
}

// Apply a world matrix to a flat triangle list, in place. Normals use the cofactor
// matrix (det * inverse-transpose) so non-uniform scale stays correct; tangent
// handedness flips when the transform mirrors (negative determinant).
void TransformFlat(std::vector<FlatVertex> &flat, const Mat4 &world) noexcept
{
    const double a0[3] = {world.m[0], world.m[1], world.m[2]};
    const double a1[3] = {world.m[4], world.m[5], world.m[6]};
    const double a2[3] = {world.m[8], world.m[9], world.m[10]};
    // Cofactor columns = cross products of the basis columns.
    const Vec3 c0{static_cast<float>(a1[1] * a2[2] - a1[2] * a2[1]),
                  static_cast<float>(a1[2] * a2[0] - a1[0] * a2[2]),
                  static_cast<float>(a1[0] * a2[1] - a1[1] * a2[0])};
    const Vec3 c1{static_cast<float>(a2[1] * a0[2] - a2[2] * a0[1]),
                  static_cast<float>(a2[2] * a0[0] - a2[0] * a0[2]),
                  static_cast<float>(a2[0] * a0[1] - a2[1] * a0[0])};
    const Vec3 c2{static_cast<float>(a0[1] * a1[2] - a0[2] * a1[1]),
                  static_cast<float>(a0[2] * a1[0] - a0[0] * a1[2]),
                  static_cast<float>(a0[0] * a1[1] - a0[1] * a1[0])};
    const double det   = a0[0] * c0.x + a0[1] * c0.y + a0[2] * c0.z;
    const float  hflip = det < 0.0 ? -1.0f : 1.0f;

    for (auto &v : flat)
    {
        v.pos = XformPoint(world, v.pos);

        Vec3        n{c0.x * v.normal.x + c1.x * v.normal.y + c2.x * v.normal.z,
                      c0.y * v.normal.x + c1.y * v.normal.y + c2.y * v.normal.z,
                      c0.z * v.normal.x + c1.z * v.normal.y + c2.z * v.normal.z};
        const float nl = std::sqrt(n.x * n.x + n.y * n.y + n.z * n.z);
        if (nl > 0.0f)
        {
            n.x /= nl;
            n.y /= nl;
            n.z /= nl;
        }
        v.normal = n;

        Vec3        t  = XformDir(world, Vec3{v.tangent.x, v.tangent.y, v.tangent.z});
        const float tl = std::sqrt(t.x * t.x + t.y * t.y + t.z * t.z);
        if (tl > 0.0f)
        {
            t.x /= tl;
            t.y /= tl;
            t.z /= tl;
        }
        v.tangent = Vec4{t.x, t.y, t.z, v.tangent.w * hflip};
    }
}

// Extract one glTF primitive into a flat per-corner triangle list. Returns false
// (with a logged warning) if the primitive can't be used, so the caller can skip
// it without failing the whole mesh. Mode must be checked by the caller.
bool ExtractGltfPrimitive(const tg3_model &m, const tg3_primitive &prim, uint32_t primIndex,
                          std::vector<FlatVertex> &flat, bool &skinned)
{
    skinned          = false;
    const int posIdx = FindAttr(prim, "POSITION");
    if (posIdx < 0 || static_cast<uint32_t>(posIdx) >= m.accessors_count)
    {
        fmtx::Warn(fmt::format("BuildFromGltf: prim[{}] missing POSITION; skipped", primIndex));
        return false;
    }
    const int normIdx   = FindAttr(prim, "NORMAL");
    const int uvIdx     = FindAttr(prim, "TEXCOORD_0");
    const int tanIdx    = FindAttr(prim, "TANGENT");
    const int jointsIdx = FindAttr(prim, "JOINTS_0");
    const int weightsIdx = FindAttr(prim, "WEIGHTS_0");

    const auto &posAcc = m.accessors[posIdx];
    if (posAcc.component_type != TG3_COMPONENT_TYPE_FLOAT || posAcc.type != TG3_TYPE_VEC3)
    {
        fmtx::Warn(fmt::format("BuildFromGltf: prim[{}] POSITION not FLOAT VEC3; skipped",
                               primIndex));
        return false;
    }
    const uint64_t vc      = posAcc.count;
    const uint8_t *posData = AccessorPtr(m, posAcc);
    const size_t   posStr  = AccessorStride(m, posAcc);
    if (!posData)
    {
        fmtx::Warn(fmt::format("BuildFromGltf: prim[{}] POSITION has no buffer view; skipped",
                               primIndex));
        return false;
    }

    std::vector<Vec3> positions(vc);
    for (uint64_t i = 0; i < vc; ++i)
    {
        const float *p = reinterpret_cast<const float *>(posData + i * posStr);
        positions[i]   = Vec3{p[0], p[1], p[2]};
    }

    std::vector<Vec3> normals;
    bool              hasNormals = false;
    if (normIdx >= 0 && static_cast<uint32_t>(normIdx) < m.accessors_count)
    {
        const auto &a = m.accessors[normIdx];
        if (a.component_type == TG3_COMPONENT_TYPE_FLOAT && a.type == TG3_TYPE_VEC3
            && a.count == vc)
        {
            const uint8_t *d = AccessorPtr(m, a);
            const size_t   s = AccessorStride(m, a);
            if (d)
            {
                normals.resize(vc);
                for (uint64_t i = 0; i < vc; ++i)
                {
                    const float *p = reinterpret_cast<const float *>(d + i * s);
                    normals[i]     = Vec3{p[0], p[1], p[2]};
                }
                hasNormals = true;
            }
        }
    }

    std::vector<Vec2> uvs;
    bool              hasUVs = false;
    if (uvIdx >= 0 && static_cast<uint32_t>(uvIdx) < m.accessors_count)
    {
        const auto &a = m.accessors[uvIdx];
        if (a.component_type == TG3_COMPONENT_TYPE_FLOAT && a.type == TG3_TYPE_VEC2
            && a.count == vc)
        {
            const uint8_t *d = AccessorPtr(m, a);
            const size_t   s = AccessorStride(m, a);
            if (d)
            {
                uvs.resize(vc);
                for (uint64_t i = 0; i < vc; ++i)
                {
                    const float *p = reinterpret_cast<const float *>(d + i * s);
                    uvs[i]         = Vec2{p[0], p[1]}; // glTF UV is top-left, matches Vulkan
                }
                hasUVs = true;
            }
        }
    }

    std::vector<Vec4> tangents;
    bool              hasTangents = false;
    if (tanIdx >= 0 && static_cast<uint32_t>(tanIdx) < m.accessors_count)
    {
        const auto &a = m.accessors[tanIdx];
        if (a.component_type == TG3_COMPONENT_TYPE_FLOAT && a.type == TG3_TYPE_VEC4
            && a.count == vc)
        {
            const uint8_t *d = AccessorPtr(m, a);
            const size_t   s = AccessorStride(m, a);
            if (d)
            {
                tangents.resize(vc);
                for (uint64_t i = 0; i < vc; ++i)
                {
                    const float *p = reinterpret_cast<const float *>(d + i * s);
                    tangents[i]    = Vec4{p[0], p[1], p[2], p[3]};
                }
                hasTangents = true;
            }
        }
    }

    // Skinning: JOINTS_0 (VEC4 of u8/u16) + WEIGHTS_0 (VEC4 of float or normalized
    // u8/u16). Both must be present and well-formed for the primitive to be skinned.
    std::vector<std::array<uint16_t, 4>> jnts;
    std::vector<std::array<float, 4>>    wgts;
    bool                                 hasJoints = false, hasWeights = false;
    if (jointsIdx >= 0 && static_cast<uint32_t>(jointsIdx) < m.accessors_count)
    {
        const auto &a = m.accessors[jointsIdx];
        const auto *d = AccessorPtr(m, a);
        const size_t s = AccessorStride(m, a);
        if (d && a.type == TG3_TYPE_VEC4 && a.count == vc)
        {
            jnts.resize(vc);
            for (uint64_t i = 0; i < vc; ++i)
            {
                const uint8_t *e = d + i * s;
                for (int k = 0; k < 4; ++k)
                {
                    if (a.component_type == TG3_COMPONENT_TYPE_UNSIGNED_BYTE)
                        jnts[i][k] = e[k];
                    else // UNSIGNED_SHORT
                        jnts[i][k] = reinterpret_cast<const uint16_t *>(e)[k];
                }
            }
            hasJoints = true;
        }
    }
    if (weightsIdx >= 0 && static_cast<uint32_t>(weightsIdx) < m.accessors_count)
    {
        const auto &a = m.accessors[weightsIdx];
        const auto *d = AccessorPtr(m, a);
        const size_t s = AccessorStride(m, a);
        if (d && a.type == TG3_TYPE_VEC4 && a.count == vc)
        {
            wgts.resize(vc);
            for (uint64_t i = 0; i < vc; ++i)
            {
                const uint8_t *e = d + i * s;
                for (int k = 0; k < 4; ++k)
                {
                    switch (a.component_type)
                    {
                    case TG3_COMPONENT_TYPE_FLOAT:
                        wgts[i][k] = reinterpret_cast<const float *>(e)[k];
                        break;
                    case TG3_COMPONENT_TYPE_UNSIGNED_BYTE:
                        wgts[i][k] = e[k] / 255.0f;
                        break;
                    case TG3_COMPONENT_TYPE_UNSIGNED_SHORT:
                        wgts[i][k] = reinterpret_cast<const uint16_t *>(e)[k] / 65535.0f;
                        break;
                    default:
                        wgts[i][k] = 0.0f;
                        break;
                    }
                }
            }
            hasWeights = true;
        }
    }
    skinned = hasJoints && hasWeights;
    if ((hasJoints != hasWeights))
        fmtx::Warn(fmt::format(
            "BuildFromGltf: prim[{}] has only one of JOINTS_0/WEIGHTS_0; treated as static",
            primIndex));

    std::vector<uint32_t> indices;
    if (prim.indices >= 0 && static_cast<uint32_t>(prim.indices) < m.accessors_count)
    {
        const auto &a = m.accessors[prim.indices];
        if (a.type != TG3_TYPE_SCALAR)
        {
            fmtx::Warn(fmt::format("BuildFromGltf: prim[{}] indices not SCALAR; skipped",
                                   primIndex));
            return false;
        }
        const uint8_t *d = AccessorPtr(m, a);
        const size_t   s = AccessorStride(m, a);
        if (!d)
        {
            fmtx::Warn(fmt::format("BuildFromGltf: prim[{}] indices have no buffer view; skipped",
                                   primIndex));
            return false;
        }
        indices.resize(a.count);
        switch (a.component_type)
        {
        case TG3_COMPONENT_TYPE_UNSIGNED_INT:
            for (uint64_t i = 0; i < a.count; ++i)
                indices[i] = *reinterpret_cast<const uint32_t *>(d + i * s);
            break;
        case TG3_COMPONENT_TYPE_UNSIGNED_SHORT:
            for (uint64_t i = 0; i < a.count; ++i)
                indices[i] = *reinterpret_cast<const uint16_t *>(d + i * s);
            break;
        case TG3_COMPONENT_TYPE_UNSIGNED_BYTE:
            for (uint64_t i = 0; i < a.count; ++i)
                indices[i] = *reinterpret_cast<const uint8_t *>(d + i * s);
            break;
        default:
            fmtx::Warn(fmt::format("BuildFromGltf: prim[{}] indices component_type {} unsupported; "
                                   "skipped",
                                   primIndex, a.component_type));
            return false;
        }
    }
    else
    {
        indices.resize(vc);
        for (uint64_t i = 0; i < vc; ++i)
            indices[i] = static_cast<uint32_t>(i);
    }

    if (indices.size() % 3 != 0)
    {
        fmtx::Warn(fmt::format("BuildFromGltf: prim[{}] index count not divisible by 3; skipped",
                               primIndex));
        return false;
    }

    flat.clear();
    flat.reserve(indices.size());
    for (uint32_t idx : indices)
    {
        if (idx >= vc)
        {
            fmtx::Warn(fmt::format("BuildFromGltf: prim[{}] index {} >= vertex count {}; skipped",
                                   primIndex, idx, vc));
            return false;
        }
        FlatVertex v{};
        v.pos     = positions[idx];
        v.normal  = hasNormals ? normals[idx] : Vec3{0, 0, 0};
        v.uv      = hasUVs ? uvs[idx] : Vec2{0, 0};
        v.tangent = hasTangents ? tangents[idx] : Vec4{0, 0, 0, 0};
        if (skinned)
        {
            for (int k = 0; k < 4; ++k)
            {
                v.joints[k]  = jnts[idx][k];
                v.weights[k] = wgts[idx][k];
            }
        }
        flat.push_back(v);
    }

    if (!hasNormals)
    {
        fmtx::Warn(fmt::format("BuildFromGltf: prim[{}] missing NORMAL; using face normals",
                               primIndex));
        FillFaceNormals(flat);
    }

    if (!hasTangents)
    {
        if (!hasUVs)
        {
            fmtx::Warn(fmt::format(
                "BuildFromGltf: prim[{}] missing TANGENT and TEXCOORD_0; tangents will be zero",
                primIndex));
        }
        else if (!RunMikkTSpace(flat))
        {
            fmtx::Warn(fmt::format("BuildFromGltf: prim[{}] MikkTSpace failed; skipped", primIndex));
            return false;
        }
    }

    return true;
}

// --- Skeleton + animation extraction -----------------------------------------

// Decompose a column-major 4x4 into translation/rotation(quat xyzw)/scale. Used
// only for skeleton joints authored with a baked node.matrix instead of TRS.
void DecomposeTRS(const double m[16], float T[3], float R[4], float S[3]) noexcept
{
    T[0] = static_cast<float>(m[12]);
    T[1] = static_cast<float>(m[13]);
    T[2] = static_cast<float>(m[14]);

    double cx[3] = {m[0], m[1], m[2]};
    double cy[3] = {m[4], m[5], m[6]};
    double cz[3] = {m[8], m[9], m[10]};
    double sx = std::sqrt(cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]);
    double sy = std::sqrt(cy[0] * cy[0] + cy[1] * cy[1] + cy[2] * cy[2]);
    double sz = std::sqrt(cz[0] * cz[0] + cz[1] * cz[1] + cz[2] * cz[2]);
    S[0]      = static_cast<float>(sx);
    S[1]      = static_cast<float>(sy);
    S[2]      = static_cast<float>(sz);
    if (sx > 0) { cx[0] /= sx; cx[1] /= sx; cx[2] /= sx; }
    if (sy > 0) { cy[0] /= sy; cy[1] /= sy; cy[2] /= sy; }
    if (sz > 0) { cz[0] /= sz; cz[1] /= sz; cz[2] /= sz; }

    // Rotation matrix (columns cx,cy,cz) -> quaternion.
    const double t = cx[0] + cy[1] + cz[2];
    double       q[4];
    if (t > 0.0)
    {
        double s = std::sqrt(t + 1.0) * 2.0;
        q[3]     = 0.25 * s;
        q[0]     = (cy[2] - cz[1]) / s;
        q[1]     = (cz[0] - cx[2]) / s;
        q[2]     = (cx[1] - cy[0]) / s;
    }
    else if (cx[0] > cy[1] && cx[0] > cz[2])
    {
        double s = std::sqrt(1.0 + cx[0] - cy[1] - cz[2]) * 2.0;
        q[3]     = (cy[2] - cz[1]) / s;
        q[0]     = 0.25 * s;
        q[1]     = (cy[0] + cx[1]) / s;
        q[2]     = (cz[0] + cx[2]) / s;
    }
    else if (cy[1] > cz[2])
    {
        double s = std::sqrt(1.0 + cy[1] - cx[0] - cz[2]) * 2.0;
        q[3]     = (cz[0] - cx[2]) / s;
        q[0]     = (cy[0] + cx[1]) / s;
        q[1]     = 0.25 * s;
        q[2]     = (cz[1] + cy[2]) / s;
    }
    else
    {
        double s = std::sqrt(1.0 + cz[2] - cx[0] - cy[1]) * 2.0;
        q[3]     = (cx[1] - cy[0]) / s;
        q[0]     = (cz[0] + cx[2]) / s;
        q[1]     = (cz[1] + cy[2]) / s;
        q[2]     = 0.25 * s;
    }
    for (int i = 0; i < 4; ++i)
        R[i] = static_cast<float>(q[i]);
}

struct SkinBuild
{
    std::vector<GpuJoint>             skeleton;
    std::vector<AnimClip>             animations;
    std::unordered_map<int, uint32_t> nodeToJoint;
};

const float *AccessorFloats(const tg3_model &m, const tg3_accessor &a, size_t &strideOut) noexcept
{
    if (a.component_type != TG3_COMPONENT_TYPE_FLOAT)
        return nullptr;
    strideOut = AccessorStride(m, a);
    return reinterpret_cast<const float *>(AccessorPtr(m, a));
}

SkinBuild BuildSkinAndAnimations(const tg3_model &m)
{
    SkinBuild out;
    if (m.skins_count == 0)
        return out;
    if (m.skins_count > 1)
        fmtx::Warn(fmt::format("BuildFromGltf: {} skins present; only skin[0] is exported",
                               m.skins_count));
    const tg3_skin &skin = m.skins[0];

    // child node -> parent node, for joint hierarchy.
    std::vector<int> parentOf(m.nodes_count, -1);
    for (uint32_t n = 0; n < m.nodes_count; ++n)
        for (uint32_t c = 0; c < m.nodes[n].children_count; ++c)
        {
            const int ci = m.nodes[n].children[c];
            if (ci >= 0 && static_cast<uint32_t>(ci) < m.nodes_count)
                parentOf[ci] = static_cast<int>(n);
        }

    for (uint32_t j = 0; j < skin.joints_count; ++j)
        out.nodeToJoint[skin.joints[j]] = j;

    // Inverse bind matrices (one mat4 per joint), or identity if absent.
    const float *ibm       = nullptr;
    size_t       ibmStride = 0;
    if (skin.inverse_bind_matrices >= 0 &&
        static_cast<uint32_t>(skin.inverse_bind_matrices) < m.accessors_count)
    {
        const auto &a = m.accessors[skin.inverse_bind_matrices];
        if (a.type == TG3_TYPE_MAT4)
            ibm = AccessorFloats(m, a, ibmStride);
    }

    out.skeleton.resize(skin.joints_count);
    for (uint32_t j = 0; j < skin.joints_count; ++j)
    {
        GpuJoint  &gj   = out.skeleton[j];
        const int  node = skin.joints[j];
        gj._pad         = 0;

        if (ibm)
        {
            const float *p = reinterpret_cast<const float *>(
                reinterpret_cast<const uint8_t *>(ibm) + static_cast<size_t>(j) * ibmStride);
            for (int i = 0; i < 16; ++i)
                gj.inverseBind[i] = p[i];
        }
        else
        {
            for (int i = 0; i < 16; ++i)
                gj.inverseBind[i] = (i % 5 == 0) ? 1.0f : 0.0f;
        }

        const auto &nd = m.nodes[node];
        if (nd.has_matrix)
        {
            DecomposeTRS(nd.matrix, gj.bindT, gj.bindR, gj.bindS);
        }
        else
        {
            for (int i = 0; i < 3; ++i)
                gj.bindT[i] = static_cast<float>(nd.translation[i]);
            for (int i = 0; i < 4; ++i)
                gj.bindR[i] = static_cast<float>(nd.rotation[i]);
            for (int i = 0; i < 3; ++i)
                gj.bindS[i] = static_cast<float>(nd.scale[i]);
        }

        const int parentNode = parentOf[node];
        auto      it = parentNode >= 0 ? out.nodeToJoint.find(parentNode) : out.nodeToJoint.end();
        gj.parent = it != out.nodeToJoint.end() ? static_cast<int32_t>(it->second) : -1;
    }

    // Animation clips: one per glTF animation; channels targeting non-joint nodes
    // or morph weights are skipped.
    for (uint32_t ai = 0; ai < m.animations_count; ++ai)
    {
        const auto &anim = m.animations[ai];
        AnimClip    clip;
        clip.name = anim.name.data ? std::string(anim.name.data, anim.name.len)
                                   : fmt::format("anim_{}", ai);

        for (uint32_t ci = 0; ci < anim.channels_count; ++ci)
        {
            const auto &chan = anim.channels[ci];
            auto        jit  = out.nodeToJoint.find(chan.target.node);
            if (jit == out.nodeToJoint.end())
                continue; // not a skeleton joint

            AnimPath path;
            if (tg3_str_equals_cstr(chan.target.path, "translation"))
                path = AnimPath::Translation;
            else if (tg3_str_equals_cstr(chan.target.path, "rotation"))
                path = AnimPath::Rotation;
            else if (tg3_str_equals_cstr(chan.target.path, "scale"))
                path = AnimPath::Scale;
            else
                continue; // morph weights etc. unsupported

            if (chan.sampler < 0 || static_cast<uint32_t>(chan.sampler) >= anim.samplers_count)
                continue;
            const auto &samp = anim.samplers[chan.sampler];
            if (samp.input < 0 || static_cast<uint32_t>(samp.input) >= m.accessors_count ||
                samp.output < 0 || static_cast<uint32_t>(samp.output) >= m.accessors_count)
                continue;

            const auto  &inAcc  = m.accessors[samp.input];
            const auto  &outAcc = m.accessors[samp.output];
            size_t       inStride = 0, outStride = 0;
            const float *times  = AccessorFloats(m, inAcc, inStride);
            const float *values = AccessorFloats(m, outAcc, outStride);
            if (!times || !values)
                continue;

            const int comps      = path == AnimPath::Rotation ? 4 : 3;
            const bool cubic     = tg3_str_equals_cstr(samp.interpolation, "CUBICSPLINE");
            const bool step      = tg3_str_equals_cstr(samp.interpolation, "STEP");
            const uint64_t nKeys = inAcc.count;

            AnimChannel ach;
            ach.joint  = jit->second;
            ach.path   = path;
            ach.interp = step ? AnimInterp::Step : AnimInterp::Linear;
            ach.keys.resize(nKeys);
            for (uint64_t k = 0; k < nKeys; ++k)
            {
                const float *t =
                    reinterpret_cast<const float *>(reinterpret_cast<const uint8_t *>(times) +
                                                    k * inStride);
                ach.keys[k].time = t[0];
                // CUBICSPLINE output is (inTangent, value, outTangent) triples; take
                // the middle value and degrade to linear.
                const uint64_t vIndex = cubic ? (3 * k + 1) : k;
                const float   *v      = reinterpret_cast<const float *>(
                    reinterpret_cast<const uint8_t *>(values) + vIndex * outStride);
                for (int c = 0; c < 4; ++c)
                    ach.keys[k].value[c] = c < comps ? v[c] : 0.0f;
                clip.duration = std::max(clip.duration, ach.keys[k].time);
            }
            if (cubic)
                fmtx::Warn(fmt::format(
                    "BuildFromGltf: animation '{}' uses CUBICSPLINE; degraded to LINEAR",
                    clip.name));
            clip.channels.push_back(std::move(ach));
        }
        if (!clip.channels.empty())
            out.animations.push_back(std::move(clip));
    }

    return out;
}

} // namespace

CompiledMesh BuildFromGltf(const gltf::GLTF &src, std::string_view sourceRef)
{
    const auto &m = src.model;
    DumpGltfStructure(m);

    if (m.meshes_count == 0)
    {
        fmtx::Error("BuildFromGltf: no meshes");
        return {};
    }

    // Combine mode: every primitive of every mesh referenced by the node graph
    // becomes a submesh, with the node's world transform baked into the vertices.
    std::vector<PrimitiveInput> prims;

    auto emitMesh = [&](uint32_t meshIdx, const Mat4 &world) {
        if (meshIdx >= m.meshes_count)
            return;
        const auto &mesh = m.meshes[meshIdx];
        for (uint32_t pi = 0; pi < mesh.primitives_count; ++pi)
        {
            const auto &prim = mesh.primitives[pi];
            const int   mode = prim.mode < 0 ? TG3_MODE_TRIANGLES : prim.mode;
            if (mode != TG3_MODE_TRIANGLES)
            {
                fmtx::Warn(fmt::format("BuildFromGltf: mesh[{}].prim[{}] mode {} not TRIANGLES; "
                                       "skipped",
                                       meshIdx, pi, mode));
                continue;
            }
            std::vector<FlatVertex> flat;
            bool                    primSkinned = false;
            if (!ExtractGltfPrimitive(m, prim, pi, flat, primSkinned))
                continue;
            // Skinned vertices are posed by joints; per glTF the skinned mesh node's
            // transform is ignored, so don't bake the world matrix into them.
            if (!primSkinned)
                TransformFlat(flat, world);
            prims.push_back(PrimitiveInput{std::move(flat), prim.material, primSkinned});
        }
    };

    if (m.nodes_count == 0)
    {
        // No scene graph: bake every mesh at identity.
        for (uint32_t i = 0; i < m.meshes_count; ++i)
            emitMesh(i, Mat4Identity());
    }
    else
    {
        // Roots = nodes that are nobody's child. Walk the forest depth-first,
        // accumulating world = parent * local.
        std::vector<uint8_t> isChild(m.nodes_count, 0);
        for (uint32_t n = 0; n < m.nodes_count; ++n)
            for (uint32_t c = 0; c < m.nodes[n].children_count; ++c)
            {
                const int ci = m.nodes[n].children[c];
                if (ci >= 0 && static_cast<uint32_t>(ci) < m.nodes_count)
                    isChild[ci] = 1;
            }

        std::vector<uint8_t>                    visited(m.nodes_count, 0);
        std::vector<std::pair<uint32_t, Mat4>>  stack;
        for (uint32_t n = 0; n < m.nodes_count; ++n)
            if (!isChild[n])
                stack.emplace_back(n, Mat4Identity());

        while (!stack.empty())
        {
            const auto [ni, parentWorld] = stack.back();
            stack.pop_back();
            if (ni >= m.nodes_count || visited[ni])
                continue;
            visited[ni] = 1;

            const auto &node  = m.nodes[ni];
            const Mat4  local = node.has_matrix
                                    ? Mat4FromColMajor(node.matrix)
                                    : Mat4FromTRS(node.translation, node.rotation, node.scale);
            const Mat4  world = Mat4Mul(parentWorld, local);

            if (node.mesh >= 0)
                emitMesh(static_cast<uint32_t>(node.mesh), world);

            for (uint32_t c = 0; c < node.children_count; ++c)
            {
                const int ci = node.children[c];
                if (ci >= 0)
                    stack.emplace_back(static_cast<uint32_t>(ci), world);
            }
        }
    }

    if (prims.empty())
    {
        fmtx::Error("BuildFromGltf: no usable primitives");
        return {};
    }

    // Materials: need stable backing storage for the string_view spans we pass
    // into FinalizeMesh (tg3_str is not null-terminated; copy into std::strings).
    std::vector<std::string>      nameStorage;
    std::vector<std::string_view> names;
    nameStorage.reserve(m.materials_count);
    names.reserve(m.materials_count);
    for (uint32_t i = 0; i < m.materials_count; ++i)
    {
        nameStorage.emplace_back(m.materials[i].name.data ? m.materials[i].name.data : "",
                                 m.materials[i].name.len);
        names.emplace_back(nameStorage.back());
    }

    CompiledMesh cm = FinalizeMesh(std::move(prims), names, sourceRef);
    if (!cm.mesh.vertices.empty())
    {
        SkinBuild sb  = BuildSkinAndAnimations(m);
        cm.skeleton   = std::move(sb.skeleton);
        cm.animations = std::move(sb.animations);
        if (!cm.mesh.skinVertices.empty() && cm.skeleton.empty())
            fmtx::Warn("BuildFromGltf: skinned vertices present but no skin/skeleton found");
    }
    return cm;
}

} // namespace assetc
