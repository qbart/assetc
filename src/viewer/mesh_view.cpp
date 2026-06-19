#include "viewer/mesh_view.hpp"

#include "assetc/runtime_material.hpp"
#include "assetc/runtime_mesh.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <utility>

#include <imgui.h>

namespace viewer
{

using assetc::ChunkEntry;
using assetc::ChunkId;
using assetc::FileHeader;
using assetc::GpuVertex;
using assetc::LodTableHeader;
using assetc::MeshBounds;
using assetc::MeshDesc;
using assetc::MeshLod;
using assetc::SubMesh;

namespace
{

// Locate a chunk by id in an already-bounds-checked table. Returns nullptr if absent.
const ChunkEntry *FindChunk(const std::vector<ChunkEntry> &table, ChunkId id)
{
    const auto v = static_cast<uint32_t>(id);
    for (const auto &e : table)
        if (e.fourcc == v)
            return &e;
    return nullptr;
}

} // namespace

MeshCpu ParseHMesh(const uint8_t *data, size_t size)
{
    MeshCpu out;
    auto    fail = [&](std::string msg) -> MeshCpu {
        out.valid = false;
        out.error = std::move(msg);
        return out;
    };

    if (size < sizeof(FileHeader))
        return fail("file too small for header");

    FileHeader hdr{};
    std::memcpy(&hdr, data, sizeof(hdr));
    if (hdr.magic != assetc::MeshMagic)
        return fail("bad magic (not a .hmesh)");
    if (hdr.version != assetc::MeshVersion)
        return fail("unsupported .hmesh version");

    const size_t tableOff   = sizeof(FileHeader);
    const size_t tableBytes = static_cast<size_t>(hdr.chunkCount) * sizeof(ChunkEntry);
    if (tableOff + tableBytes > size)
        return fail("chunk table truncated");

    std::vector<ChunkEntry> table(hdr.chunkCount);
    std::memcpy(table.data(), data + tableOff, tableBytes);
    for (const auto &e : table)
        if (e.offset > size || e.size > size - e.offset)
            return fail("chunk extends past EOF");

    const ChunkEntry *desc = FindChunk(table, ChunkId::Desc);
    const ChunkEntry *bnds = FindChunk(table, ChunkId::Bounds);
    const ChunkEntry *vtxs = FindChunk(table, ChunkId::Vertices);
    const ChunkEntry *idxs = FindChunk(table, ChunkId::Indices);
    const ChunkEntry *subm = FindChunk(table, ChunkId::SubMeshes);
    if (!desc || !bnds || !vtxs || !idxs || !subm)
        return fail("missing required chunk (DESC/BNDS/VTXS/IDXS/SUBM)");
    if (desc->size != sizeof(MeshDesc) || bnds->size != sizeof(MeshBounds))
        return fail("malformed DESC/BNDS");

    MeshDesc d{};
    std::memcpy(&d, data + desc->offset, sizeof(d));
    MeshBounds b{};
    std::memcpy(&b, data + bnds->offset, sizeof(b));

    if (d.vertexStride < sizeof(float) * 3)
        return fail("vertex stride too small");
    if (vtxs->size != static_cast<size_t>(d.vertexCount) * d.vertexStride)
        return fail("VTXS size mismatch");
    if (d.indexWidth != 2 && d.indexWidth != 4)
        return fail("bad index width");
    if (idxs->size != static_cast<size_t>(d.indexCount) * d.indexWidth)
        return fail("IDXS size mismatch");
    if (subm->size != static_cast<size_t>(d.submeshCount) * sizeof(SubMesh))
        return fail("SUBM size mismatch");

    out.vertexCount  = d.vertexCount;
    out.submeshCount = d.submeshCount;
    out.materialCount = d.materialCount;
    std::memcpy(out.aabbMin, &b.aabbMin, sizeof(out.aabbMin));
    std::memcpy(out.aabbMax, &b.aabbMax, sizeof(out.aabbMax));
    std::memcpy(out.center, &b.sphereCenter, sizeof(out.center));
    out.radius = b.sphereRadius > 0.0f ? b.sphereRadius : 1.0f;

    // Positions: first 3 floats of each GpuVertex (offset 0), honoring the stride.
    out.positions.resize(static_cast<size_t>(d.vertexCount) * 3);
    const uint8_t *vp = data + vtxs->offset;
    for (uint32_t i = 0; i < d.vertexCount; ++i)
        std::memcpy(&out.positions[static_cast<size_t>(i) * 3],
                    vp + static_cast<size_t>(i) * d.vertexStride, sizeof(float) * 3);

    // Read the global LOD0 index buffer (IDXS), widening u16 -> u32.
    std::vector<uint32_t> idx0(d.indexCount);
    const uint8_t        *ip = data + idxs->offset;
    if (d.indexWidth == 2)
        for (uint32_t i = 0; i < d.indexCount; ++i)
            idx0[i] = *reinterpret_cast<const uint16_t *>(ip + static_cast<size_t>(i) * 2);
    else
        std::memcpy(idx0.data(), ip, static_cast<size_t>(d.indexCount) * 4);

    std::vector<SubMesh> submeshes(d.submeshCount);
    std::memcpy(submeshes.data(), data + subm->offset, subm->size);

    // Optional reduced LODs: LODT header + MeshLod[submesh*lod] row-major, into LODI.
    const ChunkEntry *lodt = FindChunk(table, ChunkId::LodTable);
    const ChunkEntry *lodi = FindChunk(table, ChunkId::LodIndices);
    uint32_t          lodCount = 0;
    std::vector<MeshLod>  lodTable;
    std::vector<uint32_t> lodData;
    if (lodt && lodt->size >= sizeof(LodTableHeader))
    {
        LodTableHeader lh{};
        std::memcpy(&lh, data + lodt->offset, sizeof(lh));
        const size_t want = sizeof(LodTableHeader) +
                            static_cast<size_t>(lh.lodCount) * lh.submeshCount * sizeof(MeshLod);
        if (lh.submeshCount == d.submeshCount && lodt->size == want)
        {
            lodCount = lh.lodCount;
            lodTable.resize(static_cast<size_t>(lh.lodCount) * lh.submeshCount);
            std::memcpy(lodTable.data(), data + lodt->offset + sizeof(LodTableHeader),
                        lodTable.size() * sizeof(MeshLod));
            if (lodi)
            {
                lodData.resize(lodi->size / sizeof(uint32_t));
                std::memcpy(lodData.data(), data + lodi->offset, lodData.size() * sizeof(uint32_t));
            }
        }
    }

    // Expand into one directly-drawable index list per level. Per-submesh slices
    // are kept so an empty reduced range ("indexCount == 0") falls back to the
    // previous level for that submesh, matching the format's semantics.
    const uint32_t levels = lodCount + 1;
    out.lodIndices.assign(levels, {});
    out.lodSubmeshCount.assign(levels, std::vector<uint32_t>(d.submeshCount, 0));
    out.submeshMaterial.resize(d.submeshCount);
    for (uint32_t s = 0; s < d.submeshCount; ++s)
        out.submeshMaterial[s] = submeshes[s].materialSlot;
    std::vector<std::vector<uint32_t>> prevSlices(d.submeshCount); // per-submesh, previous level

    for (uint32_t s = 0; s < d.submeshCount; ++s)
    {
        const SubMesh &sm = submeshes[s];
        std::vector<uint32_t> slice;
        if (static_cast<size_t>(sm.firstIndex) + sm.indexCount <= idx0.size())
            slice.assign(idx0.begin() + sm.firstIndex, idx0.begin() + sm.firstIndex + sm.indexCount);
        out.lodSubmeshCount[0][s] = static_cast<uint32_t>(slice.size());
        out.lodIndices[0].insert(out.lodIndices[0].end(), slice.begin(), slice.end());
        prevSlices[s] = std::move(slice);
    }

    for (uint32_t l = 1; l < levels; ++l)
    {
        for (uint32_t s = 0; s < d.submeshCount; ++s)
        {
            const MeshLod &ml = lodTable[static_cast<size_t>(s) * lodCount + (l - 1)];
            std::vector<uint32_t> slice;
            if (ml.indexCount > 0 &&
                static_cast<size_t>(ml.firstIndex) + ml.indexCount <= lodData.size())
                slice.assign(lodData.begin() + ml.firstIndex,
                             lodData.begin() + ml.firstIndex + ml.indexCount);
            else
                slice = prevSlices[s]; // stalled simplification: reuse the previous level
            out.lodSubmeshCount[l][s] = static_cast<uint32_t>(slice.size());
            out.lodIndices[l].insert(out.lodIndices[l].end(), slice.begin(), slice.end());
            prevSlices[s] = std::move(slice);
        }
    }

    // Meshlet partition (MLET + MLVR + MLTR) for the meshlet-colored preview. Each
    // triangle's 3 local indices (MLTR) index into the meshlet's slice of MLVR, whose
    // entries are GLOBAL vertex indices into VTXS. Bounds-checked throughout.
    const ChunkEntry *mletC = FindChunk(table, ChunkId::Meshlets);
    const ChunkEntry *mlvrC = FindChunk(table, ChunkId::MeshletVertices);
    const ChunkEntry *mltrC = FindChunk(table, ChunkId::MeshletTriangles);
    if (mletC && mlvrC && mltrC)
    {
        const uint32_t mlCount   = static_cast<uint32_t>(mletC->size / sizeof(assetc::Meshlet));
        const uint32_t mlvrCount = static_cast<uint32_t>(mlvrC->size / sizeof(uint32_t));
        const uint32_t mltrCount =
            static_cast<uint32_t>(mltrC->size / sizeof(assetc::MeshletTriangle));
        out.meshletCount = mlCount;
        for (uint32_t mi = 0; mi < mlCount; ++mi)
        {
            assetc::Meshlet ml{};
            std::memcpy(&ml, data + mletC->offset + static_cast<size_t>(mi) * sizeof(ml), sizeof(ml));
            for (uint32_t t = 0; t < ml.triangleCount; ++t)
            {
                const uint32_t ti = ml.triangleOffset + t;
                if (ti >= mltrCount)
                    break;
                assetc::MeshletTriangle tri{};
                std::memcpy(&tri, data + mltrC->offset + static_cast<size_t>(ti) * sizeof(tri),
                            sizeof(tri));
                const uint32_t local[3] = {tri.i0, tri.i1, tri.i2};
                uint32_t       g[3];
                bool           okTri = true;
                for (int k = 0; k < 3; ++k)
                {
                    const uint32_t vi = ml.vertexOffset + local[k];
                    if (vi >= mlvrCount)
                    {
                        okTri = false;
                        break;
                    }
                    std::memcpy(&g[k], data + mlvrC->offset + static_cast<size_t>(vi) * 4, 4);
                    if (g[k] >= out.vertexCount)
                    {
                        okTri = false;
                        break;
                    }
                }
                if (!okTri)
                    continue;
                out.meshletTris.push_back(g[0]);
                out.meshletTris.push_back(g[1]);
                out.meshletTris.push_back(g[2]);
                out.meshletTriOwner.push_back(mi);
            }
        }
    }

    // Per-meshlet bounds (MLBN), parallel to MLET. Size must match the meshlet count
    // we just read; otherwise leave it empty and the hover overlay stays disabled.
    const ChunkEntry *mlbnC = FindChunk(table, ChunkId::MeshletBounds);
    if (mlbnC && out.meshletCount > 0 &&
        mlbnC->size == static_cast<size_t>(out.meshletCount) * sizeof(assetc::MeshletBounds))
    {
        out.meshletBounds.resize(out.meshletCount);
        std::memcpy(out.meshletBounds.data(), data + mlbnC->offset, mlbnC->size);
    }

    out.valid = true;
    return out;
}

std::vector<std::array<float, 4>> ParseHMatColors(const uint8_t *data, size_t size)
{
    using assetc::GpuMaterial;
    using assetc::MatFileHeader;
    std::vector<std::array<float, 4>> out;
    if (size < sizeof(MatFileHeader))
        return out;
    MatFileHeader h{};
    std::memcpy(&h, data, sizeof(h));
    if (h.magic != assetc::MatMagic || h.version != assetc::MatVersion)
        return out;
    if (sizeof(MatFileHeader) + static_cast<size_t>(h.count) * sizeof(GpuMaterial) != size)
        return out;
    out.resize(h.count);
    for (uint32_t i = 0; i < h.count; ++i)
    {
        GpuMaterial mat{};
        std::memcpy(&mat, data + sizeof(MatFileHeader) + static_cast<size_t>(i) * sizeof(GpuMaterial),
                    sizeof(GpuMaterial));
        out[i] = {mat.baseColorFactor[0], mat.baseColorFactor[1], mat.baseColorFactor[2],
                  mat.baseColorFactor[3]};
    }
    return out;
}

// --- software preview ---------------------------------------------------------

namespace
{

struct V3
{
    float x, y, z;
};
V3   Sub(const V3 &a, const V3 &b) { return {a.x - b.x, a.y - b.y, a.z - b.z}; }
float Dot(const V3 &a, const V3 &b) { return a.x * b.x + a.y * b.y + a.z * b.z; }
V3   Cross(const V3 &a, const V3 &b)
{
    return {a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x};
}
V3 Normalize(const V3 &v)
{
    const float l = std::sqrt(Dot(v, v));
    return l > 0.0f ? V3{v.x / l, v.y / l, v.z / l} : V3{0, 0, 0};
}

// Past these counts the rasterizer / per-edge cost stops being interactive, so we
// degrade rather than stall the UI. LODs exist precisely to stay under them.
constexpr size_t kMaxSolidTris = 400000;
constexpr size_t kMaxWireTris  = 300000;
constexpr float  kFov          = 0.78f; // ~45 deg vertical field of view
constexpr int    kMaxDim       = 2048;  // cap the rasterized framebuffer dimension

// Material factors are stored linear; the rest of the inspector shows sRGB-ish
// pixels, so encode for display before drawing.
float LinToSrgb(float c)
{
    c = std::clamp(c, 0.0f, 1.0f);
    return c <= 0.0031308f ? c * 12.92f : 1.055f * std::pow(c, 1.0f / 2.4f) - 0.055f;
}

V3 Hsv(float h, float s, float v)
{
    h          = h - std::floor(h); // wrap to [0,1)
    const float hh = h * 6.0f;
    const int   i  = static_cast<int>(hh) % 6;
    const float f  = hh - std::floor(hh);
    const float p = v * (1.0f - s), q = v * (1.0f - s * f), t = v * (1.0f - s * (1.0f - f));
    switch (i)
    {
    case 0:  return {v, t, p};
    case 1:  return {q, v, p};
    case 2:  return {p, v, t};
    case 3:  return {p, q, v};
    case 4:  return {t, p, v};
    default: return {v, p, q};
    }
}

// An unbounded, well-separated palette: golden-ratio hue stepping keeps adjacent
// indices far apart in hue, while small saturation/value cycling separates colors
// that wrap back to a similar hue — so even hundreds of meshlets stay distinct.
V3 PaletteColor(uint32_t i)
{
    const float h = std::fmod(static_cast<float>(i) * 0.61803398875f, 1.0f);
    const float s = 0.50f + 0.18f * static_cast<float>(i % 3);       // 0.50 / 0.68 / 0.86
    const float v = 0.96f - 0.16f * static_cast<float>((i / 3) % 2); // 0.96 / 0.80
    return Hsv(h, s, v);
}

// Distinct base hue for a key (material slot, submesh, or meshlet index). Used for
// the debug-colored views and as the fallback when a submesh has no real material.
V3 MaterialColor(uint32_t key) { return PaletteColor(key); }

// Z-buffered flat-shaded software rasterizer. Renders the triangle set into a
// packed RGBA8 framebuffer with correct per-pixel occlusion: depth is 1/z (linear
// in screen space, so perspective-correct) and nearest wins. Two-sided lighting so
// open-mesh back faces aren't black. This replaces the painter's-algorithm sort,
// which mis-ordered interpenetrating triangles/submeshes.
void Rasterize(const MeshCpu &m, const std::vector<uint32_t> &idx, size_t triCount,
               const std::vector<float> &vx, const std::vector<float> &vy,
               const std::vector<float> &vz, const std::vector<uint8_t> &ok, bool colorFill,
               const std::vector<V3> &triColor, const V3 &fwd, int W, int H,
               std::vector<uint32_t> &color, std::vector<float> &depth)
{
    color.assign(static_cast<size_t>(W) * H, IM_COL32(26, 26, 32, 255));
    depth.assign(static_cast<size_t>(W) * H, 0.0f); // 1/z; 0 == infinitely far
    const float focal = (0.5f * H) / std::tan(kFov * 0.5f);
    const float cx = W * 0.5f, cy = H * 0.5f;

    auto edge = [](float x0, float y0, float x1, float y1, float px, float py) {
        return (x1 - x0) * (py - y0) - (y1 - y0) * (px - x0);
    };

    for (size_t t = 0; t < triCount; ++t)
    {
        const uint32_t a = idx[t * 3 + 0], b = idx[t * 3 + 1], c = idx[t * 3 + 2];
        if (!ok[a] || !ok[b] || !ok[c])
            continue;
        const float ax = cx + (vx[a] / vz[a]) * focal, ay = cy - (vy[a] / vz[a]) * focal;
        const float bx = cx + (vx[b] / vz[b]) * focal, by = cy - (vy[b] / vz[b]) * focal;
        const float dx = cx + (vx[c] / vz[c]) * focal, dy = cy - (vy[c] / vz[c]) * focal;

        const float area = edge(ax, ay, bx, by, dx, dy);
        if (std::fabs(area) < 1e-6f)
            continue;
        const float invArea = 1.0f / area;
        const float iza = 1.0f / vz[a], izb = 1.0f / vz[b], izc = 1.0f / vz[c];

        int minx = std::max(0, (int)std::floor(std::min({ax, bx, dx})));
        int maxx = std::min(W - 1, (int)std::ceil(std::max({ax, bx, dx})));
        int miny = std::max(0, (int)std::floor(std::min({ay, by, dy})));
        int maxy = std::min(H - 1, (int)std::ceil(std::max({ay, by, dy})));
        if (minx > maxx || miny > maxy)
            continue;

        // Flat shade from the object-space face normal, lit along the view axis and
        // two-sided so a back face seen through an opening still reads.
        const V3 pa{m.positions[a * 3], m.positions[a * 3 + 1], m.positions[a * 3 + 2]};
        const V3 pb{m.positions[b * 3], m.positions[b * 3 + 1], m.positions[b * 3 + 2]};
        const V3 pc{m.positions[c * 3], m.positions[c * 3 + 1], m.positions[c * 3 + 2]};
        const V3    n      = Normalize(Cross(Sub(pb, pa), Sub(pc, pa)));
        const float facing = std::clamp(std::fabs(Dot(n, fwd)), 0.0f, 1.0f);
        const float shade  = 0.18f + 0.82f * facing;
        const V3    base   = colorFill ? triColor[t] : V3{0.92f, 0.92f, 0.94f};
        const int   rr     = (int)(std::clamp(base.x * shade, 0.0f, 1.0f) * 255.0f);
        const int   gg     = (int)(std::clamp(base.y * shade, 0.0f, 1.0f) * 255.0f);
        const int   bb     = (int)(std::clamp(base.z * shade, 0.0f, 1.0f) * 255.0f);
        const uint32_t col = IM_COL32(rr, gg, bb, 255);

        for (int py = miny; py <= maxy; ++py)
        {
            for (int px = minx; px <= maxx; ++px)
            {
                const float fx = px + 0.5f, fy = py + 0.5f;
                const float w0 = edge(bx, by, dx, dy, fx, fy) * invArea; // weight for A
                const float w1 = edge(dx, dy, ax, ay, fx, fy) * invArea; // weight for B
                const float w2 = edge(ax, ay, bx, by, fx, fy) * invArea; // weight for C
                if (w0 < -1e-5f || w1 < -1e-5f || w2 < -1e-5f)
                    continue;
                const float iz = w0 * iza + w1 * izb + w2 * izc; // perspective-correct 1/z
                const size_t p = static_cast<size_t>(py) * W + px;
                if (iz > depth[p])
                {
                    depth[p] = iz;
                    color[p] = col;
                }
            }
        }
    }
}

} // namespace

void DrawMeshPreview(GpuContext &gpu, const MeshCpu &m, MeshCamera &cam, MeshRender &render)
{
    const int levels = static_cast<int>(m.lodIndices.size());
    if (levels == 0)
    {
        ImGui::TextDisabled("no geometry");
        return;
    }
    cam.lod = std::clamp(cam.lod, 0, levels - 1);

    // --- header / controls ----------------------------------------------------
    const float dx = m.aabbMax[0] - m.aabbMin[0];
    const float dy = m.aabbMax[1] - m.aabbMin[1];
    const float dz = m.aabbMax[2] - m.aabbMin[2];
    ImGui::Text("%u verts  %u submeshes  %u materials  %u meshlets", m.vertexCount, m.submeshCount,
                m.materialCount, m.meshletCount);
    ImGui::Text("bounds: %.3f x %.3f x %.3f", dx, dy, dz);

    const std::string lodLabel =
        cam.lod == 0 ? "LOD 0 (full)" : ("LOD " + std::to_string(cam.lod));
    if (ImGui::BeginCombo("detail", lodLabel.c_str()))
    {
        for (int l = 0; l < levels; ++l)
        {
            const size_t tris = m.lodIndices[l].size() / 3;
            const std::string item =
                (l == 0 ? "LOD 0 (full)  " : "LOD " + std::to_string(l) + "  ") +
                std::to_string(tris) + " tris";
            if (ImGui::Selectable(item.c_str(), cam.lod == l))
                cam.lod = l;
        }
        ImGui::EndCombo();
    }

    // Per-level triangle counts, with reduction vs. LOD0, so the LOD payoff is legible.
    const size_t baseTris = m.lodIndices[0].size() / 3;
    const size_t curTris  = m.lodIndices[cam.lod].size() / 3;
    if (cam.lod == 0 || baseTris == 0)
        ImGui::Text("%zu triangles", curTris);
    else
        ImGui::Text("%zu triangles  (%.0f%% of LOD0)", curTris,
                    100.0 * static_cast<double>(curTris) / static_cast<double>(baseTris));

    static const char *kModeNames[MeshMode_Count] = {"wireframe", "solid",     "debug material",
                                                      "material",  "submeshes", "meshlets"};
    cam.mode = std::clamp(cam.mode, 0, (int)MeshMode_Count - 1);
    ImGui::SetNextItemWidth(160.0f);
    ImGui::Combo("mode", &cam.mode, kModeNames, MeshMode_Count);
    if (cam.mode == MeshMode_Meshlet)
    {
        ImGui::SameLine();
        ImGui::TextDisabled("(LOD0)");
    }
    ImGui::SameLine();
    if (ImGui::SmallButton("reset view"))
    {
        cam.yaw       = 0.7f;
        cam.pitch     = 0.5f;
        cam.distScale = 1.0f;
    }
    ImGui::SameLine();
    ImGui::TextDisabled("(drag: orbit, wheel: zoom)");

    // Bounds overlays (BNDS chunk): the AABB box and the bounding sphere, drawn as
    // wireframe through ImGui's draw list over whatever render mode is active.
    ImGui::Checkbox("AABB", &cam.showAabb);
    ImGui::SameLine();
    ImGui::Checkbox("sphere", &cam.showSphere);
    if (!m.meshletBounds.empty())
    {
        ImGui::SameLine();
        ImGui::Checkbox("meshlet hover", &cam.showMeshletHover);
    }

    // --- canvas ---------------------------------------------------------------
    ImVec2 size = ImGui::GetContentRegionAvail();
    size.x      = std::max(size.x, 64.0f);
    size.y      = std::max(size.y, 64.0f);
    const ImVec2 p0 = ImGui::GetCursorScreenPos();
    const ImVec2 p1(p0.x + size.x, p0.y + size.y);

    ImGui::InvisibleButton("##meshcanvas", size,
                           ImGuiButtonFlags_MouseButtonLeft | ImGuiButtonFlags_MouseButtonRight);
    const bool    active  = ImGui::IsItemActive();
    const bool    hovered = ImGui::IsItemHovered();
    const ImGuiIO &io     = ImGui::GetIO();
    if (active)
    {
        cam.yaw -= io.MouseDelta.x * 0.01f;
        cam.pitch += io.MouseDelta.y * 0.01f;
        cam.pitch = std::clamp(cam.pitch, -1.55f, 1.55f);
    }
    if (hovered && io.MouseWheel != 0.0f)
        cam.distScale = std::clamp(cam.distScale * std::pow(0.88f, io.MouseWheel), 0.1f, 12.0f);

    ImDrawList *dl = ImGui::GetWindowDrawList();
    dl->PushClipRect(p0, p1, true);
    dl->AddRectFilled(p0, p1, IM_COL32(26, 26, 32, 255));

    // Camera basis from yaw/pitch, auto-fit to the bounding sphere.
    const V3    center{m.center[0], m.center[1], m.center[2]};
    const float fitDist = m.radius * 2.6f * cam.distScale;
    const V3    dir{std::cos(cam.pitch) * std::sin(cam.yaw), std::sin(cam.pitch),
                 std::cos(cam.pitch) * std::cos(cam.yaw)};
    const V3    eye{center.x + dir.x * fitDist, center.y + dir.y * fitDist,
                 center.z + dir.z * fitDist};
    const V3    fwd = Normalize(Sub(center, eye));      // look direction
    V3          right = Normalize(Cross(fwd, V3{0, 1, 0}));
    if (Dot(right, right) < 1e-6f)                       // looking near-vertical
        right = V3{1, 0, 0};
    const V3 up = Cross(right, fwd);

    const float zNear = std::max(m.radius * 1e-3f, 1e-4f);

    // Project every vertex once into camera space (vx,vy on the view plane, vz along
    // the look axis); mark those behind the near plane. Both the rasterizer and the
    // wireframe path derive their pixel coords from these.
    static std::vector<float> vx, vy, vz;
    static std::vector<uint8_t> ok;
    vx.resize(m.vertexCount);
    vy.resize(m.vertexCount);
    vz.resize(m.vertexCount);
    ok.resize(m.vertexCount);
    for (uint32_t i = 0; i < m.vertexCount; ++i)
    {
        const V3 p{m.positions[i * 3 + 0], m.positions[i * 3 + 1], m.positions[i * 3 + 2]};
        const V3 rel = Sub(p, eye);
        const float z = Dot(rel, fwd);
        vz[i]         = z;
        ok[i]         = z > zNear;
        vx[i]         = Dot(rel, right);
        vy[i]         = Dot(rel, up);
    }

    // Hover pick: which meshlet is under the cursor. We splat the LOD0 meshlet
    // triangle list to the canvas (same projection the overlays use) and run the
    // rasterizer's point-in-triangle + perspective-correct depth test at the single
    // mouse pixel, keeping the nearest hit's owning meshlet. O(meshlet tris) per
    // frame, only while hovering with the toggle on, so it stays interactive.
    int hoveredMeshlet = -1;
    if (cam.showMeshletHover && hovered && !m.meshletBounds.empty() && !m.meshletTris.empty())
    {
        const float  pkFocal = (0.5f * size.y) / std::tan(kFov * 0.5f);
        const ImVec2 mid(p0.x + size.x * 0.5f, p0.y + size.y * 0.5f);
        const float  mx = io.MousePos.x, my = io.MousePos.y;
        auto         edge = [](float x0, float y0, float x1, float y1, float px, float py) {
            return (x1 - x0) * (py - y0) - (y1 - y0) * (px - x0);
        };
        float        bestIz   = 0.0f;
        const size_t mtCount  = m.meshletTris.size() / 3;
        for (size_t t = 0; t < mtCount; ++t)
        {
            const uint32_t a = m.meshletTris[t * 3 + 0], b = m.meshletTris[t * 3 + 1],
                           c = m.meshletTris[t * 3 + 2];
            if (!ok[a] || !ok[b] || !ok[c])
                continue;
            const float ax = mid.x + (vx[a] / vz[a]) * pkFocal, ay = mid.y - (vy[a] / vz[a]) * pkFocal;
            const float bx = mid.x + (vx[b] / vz[b]) * pkFocal, by = mid.y - (vy[b] / vz[b]) * pkFocal;
            const float cx = mid.x + (vx[c] / vz[c]) * pkFocal, cy = mid.y - (vy[c] / vz[c]) * pkFocal;
            const float area = edge(ax, ay, bx, by, cx, cy);
            if (std::fabs(area) < 1e-6f)
                continue;
            const float inv = 1.0f / area;
            const float w0  = edge(bx, by, cx, cy, mx, my) * inv;
            const float w1  = edge(cx, cy, ax, ay, mx, my) * inv;
            const float w2  = edge(ax, ay, bx, by, mx, my) * inv;
            if (w0 < -1e-4f || w1 < -1e-4f || w2 < -1e-4f)
                continue;
            const float iz = w0 / vz[a] + w1 / vz[b] + w2 / vz[c]; // perspective-correct 1/z
            if (iz > bestIz)
            {
                bestIz         = iz;
                hoveredMeshlet = static_cast<int>(m.meshletTriOwner[t]);
            }
        }
    }

    // Meshlet mode draws the meshlet-partitioned LOD0 triangle list (its own index
    // set); every other mode draws the selected LOD's index list.
    const bool                   meshletMode = cam.mode == MeshMode_Meshlet;
    const std::vector<uint32_t> &idx      = meshletMode ? m.meshletTris : m.lodIndices[cam.lod];
    const size_t                 triCount = idx.size() / 3;

    const bool wantFill   = cam.mode != MeshMode_Wire;
    const bool colorFill  = cam.mode == MeshMode_DebugMaterial || cam.mode == MeshMode_Material ||
                          cam.mode == MeshMode_Submesh || meshletMode;
    const bool realColors = cam.mode == MeshMode_Material; // else debug palette
    const bool drawFill   = wantFill && triCount > 0 && triCount <= kMaxSolidTris;
    const bool drawWire   = cam.mode == MeshMode_Wire && triCount > 0 && triCount <= kMaxWireTris;

    if (drawFill)
    {
        // Resolve each triangle's base color. Meshlet mode colors per meshlet; the
        // rest color per submesh range (submeshes are contiguous in `idx`): "submeshes"
        // tints by submesh index, "material" by the real .hmat baseColorFactor, "debug
        // material" by a distinct palette color per material slot.
        static std::vector<V3> triColor;
        if (meshletMode)
        {
            triColor.resize(triCount);
            for (size_t t = 0; t < triCount; ++t)
                triColor[t] = PaletteColor(m.meshletTriOwner[t]);
        }
        else if (colorFill && cam.lod < (int)m.lodSubmeshCount.size())
        {
            triColor.assign(triCount, V3{0.8f, 0.8f, 0.8f});
            const auto &counts = m.lodSubmeshCount[cam.lod];
            size_t      tri    = 0;
            for (uint32_t s = 0; s < counts.size(); ++s)
            {
                const uint32_t slot = m.submeshMaterial[s];
                V3             base;
                if (cam.mode == MeshMode_Submesh)
                    base = PaletteColor(s);
                else if (realColors && slot != assetc::kNoMaterial && slot < m.materialColors.size())
                {
                    const auto &c = m.materialColors[slot];
                    base = V3{LinToSrgb(c[0]), LinToSrgb(c[1]), LinToSrgb(c[2])};
                }
                else
                {
                    const uint32_t key =
                        (slot != assetc::kNoMaterial) ? slot : (m.materialCount + s);
                    base = MaterialColor(key);
                }
                for (uint32_t k = 0; k < counts[s] / 3 && tri < triCount; ++k, ++tri)
                    triColor[tri] = base;
            }
        }

        // Render target tracks the canvas size (in framebuffer pixels for crispness),
        // capped. Re-raster + re-upload only when the view signature changes, so an
        // idle preview costs nothing beyond drawing the cached image.
        const float scale = std::max(1.0f, io.DisplayFramebufferScale.x);
        const int   W = std::clamp((int)(size.x * scale), 16, kMaxDim);
        const int   H = std::clamp((int)(size.y * scale), 16, kMaxDim);
        const bool  dirty = !render.has || render.lod != cam.lod || render.mode != cam.mode ||
                           render.w != W || render.h != H || render.yaw != cam.yaw ||
                           render.pitch != cam.pitch || render.dist != cam.distScale;
        if (dirty)
        {
            Rasterize(m, idx, triCount, vx, vy, vz, ok, colorFill, triColor, fwd, W, H,
                      render.color, render.depth);

            std::vector<MipPixels> mip(1);
            mip[0].width  = (uint32_t)W;
            mip[0].height = (uint32_t)H;
            mip[0].rgba.resize(static_cast<size_t>(W) * H * 4);
            std::memcpy(mip[0].rgba.data(), render.color.data(),
                        static_cast<size_t>(W) * H * 4);
            if (render.tex.valid)
                gpu.DestroyTexture(render.tex);
            render.tex   = gpu.CreateTexture(mip);
            render.has   = render.tex.valid;
            render.lod   = cam.lod;
            render.mode  = cam.mode;
            render.w     = W;
            render.h     = H;
            render.yaw   = cam.yaw;
            render.pitch = cam.pitch;
            render.dist  = cam.distScale;
        }
        if (render.tex.valid && !render.tex.mipIds.empty())
            dl->AddImage((ImTextureID)(uintptr_t)render.tex.mipIds[0], p0, p1);
    }
    else if (meshletMode && m.meshletTris.empty())
    {
        dl->AddText(ImVec2(p0.x + 8, p0.y + 8), IM_COL32(255, 210, 120, 255),
                    "this mesh has no meshlet chunks");
    }
    else if (wantFill && triCount > kMaxSolidTris)
    {
        dl->AddText(ImVec2(p0.x + 8, p0.y + 8), IM_COL32(255, 210, 120, 255),
                    "too dense to rasterize — pick a lower LOD");
    }

    if (drawWire)
    {
        // Wireframe needs no depth buffer; draw edges straight through ImGui. Derive
        // screen coords from camera space using the canvas (point) dimensions.
        const float  focal = (0.5f * size.y) / std::tan(kFov * 0.5f);
        const ImVec2 mid(p0.x + size.x * 0.5f, p0.y + size.y * 0.5f);
        const ImU32  wcol = IM_COL32(150, 200, 255, 200);
        auto         project = [&](uint32_t i) {
            return ImVec2(mid.x + (vx[i] / vz[i]) * focal, mid.y - (vy[i] / vz[i]) * focal);
        };
        for (uint32_t t = 0; t < triCount; ++t)
        {
            const uint32_t a = idx[t * 3 + 0], b = idx[t * 3 + 1], c = idx[t * 3 + 2];
            if (!ok[a] || !ok[b] || !ok[c])
                continue;
            const ImVec2 pa = project(a), pb = project(b), pc = project(c);
            dl->AddLine(pa, pb, wcol);
            dl->AddLine(pb, pc, wcol);
            dl->AddLine(pc, pa, wcol);
        }
    }
    else if (cam.mode == MeshMode_Wire && triCount > kMaxWireTris)
    {
        dl->AddText(ImVec2(p0.x + 8, p0.y + 8), IM_COL32(255, 150, 150, 255),
                    "mesh too dense to preview at this LOD");
    }

    // --- overlays (mesh bounds + hovered meshlet) -----------------------------
    // Project an arbitrary world point with the same camera basis the mesh uses;
    // returns false when it falls behind the near plane (so the edge is dropped).
    // Uses the canvas (point) projection, matching both the displayed image and the
    // wireframe path so the overlay registers with the geometry in every mode.
    if (cam.showAabb || cam.showSphere || hoveredMeshlet >= 0)
    {
        const float  ovFocal = (0.5f * size.y) / std::tan(kFov * 0.5f);
        const ImVec2 mid(p0.x + size.x * 0.5f, p0.y + size.y * 0.5f);
        auto         project = [&](const V3 &p, ImVec2 &out) -> bool {
            const V3    rel = Sub(p, eye);
            const float z   = Dot(rel, fwd);
            if (z <= zNear)
                return false;
            out = ImVec2(mid.x + (Dot(rel, right) / z) * ovFocal,
                         mid.y - (Dot(rel, up) / z) * ovFocal);
            return true;
        };
        auto edgeLine = [&](const V3 &a, const V3 &b, ImU32 col) {
            ImVec2 sa, sb;
            if (project(a, sa) && project(b, sb))
                dl->AddLine(sa, sb, col, 1.5f);
        };
        // Three great circles (one per coordinate plane) approximating a sphere.
        auto sphereWire = [&](const V3 &sc, float r, ImU32 col) {
            constexpr int kSeg = 48;
            for (int axis = 0; axis < 3; ++axis)
            {
                V3   prev{};
                bool havePrev = false;
                for (int s = 0; s <= kSeg; ++s)
                {
                    const float a  = (6.2831853f * static_cast<float>(s)) / kSeg;
                    const float ca = std::cos(a) * r, sa = std::sin(a) * r;
                    V3          p  = sc;
                    if (axis == 0) { p.y += ca; p.z += sa; }      // YZ plane
                    else if (axis == 1) { p.x += ca; p.z += sa; } // XZ plane
                    else { p.x += ca; p.y += sa; }                // XY plane
                    if (havePrev)
                        edgeLine(prev, p, col);
                    prev     = p;
                    havePrev = true;
                }
            }
        };

        if (cam.showAabb)
        {
            const ImU32 col   = IM_COL32(255, 224, 96, 220); // amber
            const float xs[2] = {m.aabbMin[0], m.aabbMax[0]};
            const float ys[2] = {m.aabbMin[1], m.aabbMax[1]};
            const float zs[2] = {m.aabbMin[2], m.aabbMax[2]};
            V3          c[8];
            for (int i = 0; i < 8; ++i)
                c[i] = V3{xs[(i >> 0) & 1], ys[(i >> 1) & 1], zs[(i >> 2) & 1]};
            // The 12 box edges: every pair of corners that differs in one axis bit.
            for (int i = 0; i < 8; ++i)
                for (int bit = 0; bit < 3; ++bit)
                {
                    const int j = i | (1 << bit);
                    if (j != i)
                        edgeLine(c[i], c[j], col);
                }
        }

        if (cam.showSphere)
            sphereWire(V3{m.center[0], m.center[1], m.center[2]}, m.radius,
                       IM_COL32(120, 220, 255, 200)); // cyan

        // Hovered meshlet: its bounding sphere (green) + normal cone (magenta). The
        // normal cone is the spread of triangle normals meshopt records for backface
        // cluster culling; cone_cutoff == sin(half-angle), so the half-angle is
        // asin(cutoff). cutoff >= 1 means "normals span > 90deg, never cullable" — we
        // draw just the axis in that case. Apex is the sphere center (we don't store
        // meshopt's cone_apex); good enough to read off the direction and spread.
        if (hoveredMeshlet >= 0 && hoveredMeshlet < static_cast<int>(m.meshletBounds.size()))
        {
            const auto &mb = m.meshletBounds[hoveredMeshlet];
            const V3    mc{mb.center.x, mb.center.y, mb.center.z};
            const float len = mb.radius > 0.0f ? mb.radius : m.radius * 0.1f;
            sphereWire(mc, mb.radius, IM_COL32(120, 255, 140, 220)); // green

            const ImU32 ccol = IM_COL32(255, 140, 220, 230); // magenta
            V3          axis = Normalize(V3{mb.coneAxis.x, mb.coneAxis.y, mb.coneAxis.z});
            if (Dot(axis, axis) > 1e-6f)
            {
                const float cutoff = std::clamp(mb.coneCutoff, 0.0f, 1.0f);
                if (cutoff >= 0.999f)
                {
                    edgeLine(mc, V3{mc.x + axis.x * len, mc.y + axis.y * len, mc.z + axis.z * len},
                             ccol);
                }
                else
                {
                    const float alpha = std::asin(cutoff);     // normal-cone half-angle
                    const float fr    = len * std::cos(alpha); // along axis to the ring plane
                    const float rr    = len * std::sin(alpha); // ring radius
                    // An orthonormal basis spanning the plane perpendicular to the axis.
                    V3 t0 = std::fabs(axis.y) < 0.99f ? Normalize(Cross(axis, V3{0, 1, 0}))
                                                      : V3{1, 0, 0};
                    V3 t1 = Cross(axis, t0);
                    const V3      fc{mc.x + axis.x * fr, mc.y + axis.y * fr, mc.z + axis.z * fr};
                    constexpr int kSeg = 32;
                    V3            prev{};
                    bool          havePrev = false;
                    for (int s = 0; s <= kSeg; ++s)
                    {
                        const float a  = (6.2831853f * static_cast<float>(s)) / kSeg;
                        const float cs = std::cos(a) * rr, sn = std::sin(a) * rr;
                        const V3    p{fc.x + t0.x * cs + t1.x * sn, fc.y + t0.y * cs + t1.y * sn,
                                   fc.z + t0.z * cs + t1.z * sn};
                        if (havePrev)
                            edgeLine(prev, p, ccol); // ring
                        if (s % (kSeg / 8) == 0)
                            edgeLine(mc, p, ccol); // spoke from apex
                        prev     = p;
                        havePrev = true;
                    }
                }
            }

            char buf[80];
            std::snprintf(buf, sizeof(buf), "meshlet #%d   r=%.3f   cutoff=%.2f", hoveredMeshlet,
                          mb.radius, mb.coneCutoff);
            dl->AddText(ImVec2(p0.x + 8, p1.y - 22), IM_COL32(160, 255, 180, 255), buf);
        }
    }

    dl->PopClipRect();
}

} // namespace viewer
