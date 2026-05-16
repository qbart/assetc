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
#include <string>
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

// --- Shared finalize: dedupe -> bounds -> oct pack -> meshlets -> materials --

CompiledMesh FinalizeMesh(std::vector<FlatVertex> &&flat,
                          std::span<const std::string_view> materialNames,
                          std::string_view                  sourceRef)
{
    CompiledMesh cm{};

    // Dedupe.
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

    // Bounds (AABB + sphere).
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

    // CpuVertex -> GpuVertex (oct pack).
    cm.mesh.vertices.resize(cpuVerts.size());
    for (size_t i = 0; i < cpuVerts.size(); ++i)
    {
        const auto &cv = cpuVerts[i];
        auto       &gv = cm.mesh.vertices[i];
        gv.position[0] = cv.pos.x;
        gv.position[1] = cv.pos.y;
        gv.position[2] = cv.pos.z;
        const auto n   = OctEncode(cv.normal);
        gv.normal[0]   = n[0];
        gv.normal[1]   = n[1];
        gv.normal[2]   = 0;
        gv.normal[3]   = 0;
        const auto t   = OctEncodeTangent(Vec3{cv.tangent.x, cv.tangent.y, cv.tangent.z},
                                          cv.tangent.w);
        gv.tangent[0]  = t[0];
        gv.tangent[1]  = t[1];
        gv.tangent[2]  = 0;
        gv.tangent[3]  = 0;
        gv.uv[0]       = cv.uv.x;
        gv.uv[1]       = cv.uv.y;
    }
    cm.mesh.indices = std::move(indices);

    meshopt_optimizeVertexCache(cm.mesh.indices.data(), cm.mesh.indices.data(),
                                cm.mesh.indices.size(), cm.mesh.vertices.size());

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
        fmtx::Error("FinalizeMesh: meshopt_buildMeshlets produced 0 meshlets");
        return CompiledMesh{};
    }

    const auto  &last    = mo[meshletCount - 1];
    const size_t vtxUsed = last.vertex_offset + last.vertex_count;
    size_t       totalTris = 0;
    for (size_t i = 0; i < meshletCount; ++i)
        totalTris += mo[i].triangle_count;

    cm.mesh.meshletVertices.assign(mvi.begin(), mvi.begin() + vtxUsed);
    cm.mesh.meshlets.resize(meshletCount);
    cm.mesh.meshletTriangles.resize(totalTris);
    cm.mesh.meshletBounds.resize(meshletCount);

    size_t triCursor = 0;
    for (size_t i = 0; i < meshletCount; ++i)
    {
        const auto          &src      = mo[i];
        const unsigned char *triBytes = &mti[src.triangle_offset];
        for (uint32_t t = 0; t < src.triangle_count; ++t)
        {
            cm.mesh.meshletTriangles[triCursor + t] = MeshletTriangle{
                triBytes[t * 3 + 0], triBytes[t * 3 + 1], triBytes[t * 3 + 2]};
        }
        cm.mesh.meshlets[i] = Meshlet{src.vertex_offset, src.vertex_count,
                                      static_cast<uint32_t>(triCursor), src.triangle_count};
        const meshopt_Bounds b = meshopt_computeMeshletBounds(
            &mvi[src.vertex_offset], &mti[src.triangle_offset], src.triangle_count,
            &cm.mesh.vertices[0].position[0], cm.mesh.vertices.size(), sizeof(GpuVertex));
        cm.mesh.meshletBounds[i] =
            MeshletBounds{Vec3{b.center[0], b.center[1], b.center[2]}, b.radius,
                          Vec3{b.cone_axis[0], b.cone_axis[1], b.cone_axis[2]}, b.cone_cutoff};
        triCursor += src.triangle_count;
    }

    cm.materialRefs.reserve(materialNames.size());
    for (size_t i = 0; i < materialNames.size(); ++i)
    {
        std::string ref(sourceRef);
        ref.push_back('/');
        ref.append(MaterialLeaf(materialNames[i], i, materialNames));
        cm.materialRefs.push_back(HashAssetRef(ref));
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

    // 1) Expand shapes into a flat triangle list of FlatVertex.
    //    tinyobj's ObjReader triangulates by default, so every face is 3 verts.
    size_t triangleCount = 0;
    for (const auto &sh : src.shapes)
        triangleCount += sh.mesh.num_face_vertices.size();

    std::vector<FlatVertex> flat;
    flat.reserve(triangleCount * 3);

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
            cursor += 3;
        }
    }

    if (flat.empty())
    {
        fmtx::Error("BuildFromObj: produced 0 triangles");
        return cm;
    }

    if (warnedMissingNormal)
        FillFaceNormals(flat);

    if (!RunMikkTSpace(flat))
    {
        fmtx::Error("BuildFromObj: MikkTSpace failed");
        return cm;
    }

    std::vector<std::string_view> matNames;
    matNames.reserve(src.materials.size());
    for (const auto &mat : src.materials)
        matNames.emplace_back(mat.name);

    return FinalizeMesh(std::move(flat), matNames, sourceRef);
}

// --- BuildFromGltf -----------------------------------------------------------

CompiledMesh BuildFromGltf(const gltf::GLTF &src, std::string_view sourceRef)
{
    const auto &m = src.model;
    if (m.meshes_count == 0 || m.meshes[0].primitives_count == 0)
    {
        fmtx::Error("BuildFromGltf: no meshes/primitives");
        return {};
    }
    if (m.meshes_count > 1)
        fmtx::Warn(fmt::format("BuildFromGltf: {} meshes; only mesh 0 is exported (v1)",
                               m.meshes_count));
    if (m.meshes[0].primitives_count > 1)
        fmtx::Warn(fmt::format("BuildFromGltf: {} primitives; only primitive 0 is exported (v1)",
                               m.meshes[0].primitives_count));

    const auto &prim = m.meshes[0].primitives[0];
    const int   mode = prim.mode < 0 ? TG3_MODE_TRIANGLES : prim.mode;
    if (mode != TG3_MODE_TRIANGLES)
    {
        fmtx::Error(fmt::format("BuildFromGltf: mode {} not supported (TRIANGLES only)", mode));
        return {};
    }

    const int posIdx = FindAttr(prim, "POSITION");
    if (posIdx < 0 || static_cast<uint32_t>(posIdx) >= m.accessors_count)
    {
        fmtx::Error("BuildFromGltf: missing POSITION");
        return {};
    }
    const int normIdx = FindAttr(prim, "NORMAL");
    const int uvIdx   = FindAttr(prim, "TEXCOORD_0");
    const int tanIdx  = FindAttr(prim, "TANGENT");

    const auto &posAcc = m.accessors[posIdx];
    if (posAcc.component_type != TG3_COMPONENT_TYPE_FLOAT || posAcc.type != TG3_TYPE_VEC3)
    {
        fmtx::Error("BuildFromGltf: POSITION must be FLOAT VEC3");
        return {};
    }
    const uint64_t   vc      = posAcc.count;
    const uint8_t   *posData = AccessorPtr(m, posAcc);
    const size_t     posStr  = AccessorStride(m, posAcc);
    if (!posData)
    {
        fmtx::Error("BuildFromGltf: POSITION accessor has no buffer view");
        return {};
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

    std::vector<uint32_t> indices;
    if (prim.indices >= 0 && static_cast<uint32_t>(prim.indices) < m.accessors_count)
    {
        const auto &a = m.accessors[prim.indices];
        if (a.type != TG3_TYPE_SCALAR)
        {
            fmtx::Error("BuildFromGltf: indices accessor must be SCALAR");
            return {};
        }
        const uint8_t *d = AccessorPtr(m, a);
        const size_t   s = AccessorStride(m, a);
        if (!d)
        {
            fmtx::Error("BuildFromGltf: indices accessor has no buffer view");
            return {};
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
            fmtx::Error(fmt::format("BuildFromGltf: indices component_type {} not supported",
                                    a.component_type));
            return {};
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
        fmtx::Error("BuildFromGltf: index count not divisible by 3");
        return {};
    }

    std::vector<FlatVertex> flat;
    flat.reserve(indices.size());
    for (uint32_t idx : indices)
    {
        if (idx >= vc)
        {
            fmtx::Error(fmt::format("BuildFromGltf: index {} >= vertex count {}", idx, vc));
            return {};
        }
        FlatVertex v{};
        v.pos     = positions[idx];
        v.normal  = hasNormals ? normals[idx] : Vec3{0, 0, 0};
        v.uv      = hasUVs ? uvs[idx] : Vec2{0, 0};
        v.tangent = hasTangents ? tangents[idx] : Vec4{0, 0, 0, 0};
        flat.push_back(v);
    }

    if (!hasNormals)
    {
        fmtx::Warn("BuildFromGltf: missing NORMAL; using face normals");
        FillFaceNormals(flat);
    }

    if (!hasTangents)
    {
        if (!hasUVs)
        {
            fmtx::Warn("BuildFromGltf: missing TANGENT and TEXCOORD_0; tangents will be zero");
        }
        else if (!RunMikkTSpace(flat))
        {
            fmtx::Error("BuildFromGltf: MikkTSpace failed");
            return {};
        }
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

    return FinalizeMesh(std::move(flat), names, sourceRef);
}

} // namespace assetc
