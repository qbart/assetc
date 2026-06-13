#include "viewer/mesh_view.hpp"

#include "assetc/runtime_mesh.hpp"

#include <algorithm>
#include <cmath>
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
    std::vector<std::vector<uint32_t>> prevSlices(d.submeshCount); // per-submesh, previous level

    for (uint32_t s = 0; s < d.submeshCount; ++s)
    {
        const SubMesh &sm = submeshes[s];
        std::vector<uint32_t> slice;
        if (static_cast<size_t>(sm.firstIndex) + sm.indexCount <= idx0.size())
            slice.assign(idx0.begin() + sm.firstIndex, idx0.begin() + sm.firstIndex + sm.indexCount);
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
            out.lodIndices[l].insert(out.lodIndices[l].end(), slice.begin(), slice.end());
            prevSlices[s] = std::move(slice);
        }
    }

    out.valid = true;
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

// Past these counts the painter's-sort / per-edge cost stops being interactive,
// so we degrade rather than stall the UI. LODs exist precisely to stay under them.
constexpr size_t kMaxSolidTris = 150000;
constexpr size_t kMaxWireTris  = 300000;

} // namespace

void DrawMeshPreview(const MeshCpu &m, MeshCamera &cam)
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
    ImGui::Text("%u verts  %u submeshes  %u materials", m.vertexCount, m.submeshCount,
                m.materialCount);
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

    ImGui::Checkbox("solid", &cam.solid);
    ImGui::SameLine();
    ImGui::Checkbox("wireframe", &cam.wire);
    ImGui::SameLine();
    if (ImGui::SmallButton("reset view"))
    {
        cam.yaw       = 0.7f;
        cam.pitch     = 0.5f;
        cam.distScale = 1.0f;
    }
    ImGui::SameLine();
    ImGui::TextDisabled("(drag: orbit, wheel: zoom)");

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

    const float fov   = 0.78f; // ~45 deg vertical
    const float focal = (0.5f * size.y) / std::tan(fov * 0.5f);
    const ImVec2 mid(p0.x + size.x * 0.5f, p0.y + size.y * 0.5f);
    const float near = std::max(m.radius * 1e-3f, 1e-4f);

    // Project every vertex once into screen space; mark those behind the near plane.
    static std::vector<float> sx, sy, vz;
    static std::vector<uint8_t> ok;
    sx.resize(m.vertexCount);
    sy.resize(m.vertexCount);
    vz.resize(m.vertexCount);
    ok.resize(m.vertexCount);
    for (uint32_t i = 0; i < m.vertexCount; ++i)
    {
        const V3 p{m.positions[i * 3 + 0], m.positions[i * 3 + 1], m.positions[i * 3 + 2]};
        const V3 rel = Sub(p, eye);
        const float z = Dot(rel, fwd);
        vz[i]         = z;
        ok[i]         = z > near;
        if (ok[i])
        {
            sx[i] = mid.x + (Dot(rel, right) / z) * focal;
            sy[i] = mid.y - (Dot(rel, up) / z) * focal;
        }
    }

    const std::vector<uint32_t> &idx = m.lodIndices[cam.lod];
    const size_t                 triCount = idx.size() / 3;

    const bool drawSolid = cam.solid && triCount > 0 && triCount <= kMaxSolidTris;
    const bool drawWire  = cam.wire && triCount > 0 && triCount <= kMaxWireTris;

    if (drawSolid)
    {
        // Painter's algorithm: gather front-facing, fully-visible triangles, sort
        // far-to-near by mean view depth, then fill with flat lambert shading.
        static std::vector<std::pair<float, uint32_t>> order; // (depth, tri)
        order.clear();
        order.reserve(triCount);
        for (uint32_t t = 0; t < triCount; ++t)
        {
            const uint32_t a = idx[t * 3 + 0], b = idx[t * 3 + 1], c = idx[t * 3 + 2];
            if (!ok[a] || !ok[b] || !ok[c])
                continue;
            order.emplace_back((vz[a] + vz[b] + vz[c]) * (1.0f / 3.0f), t);
        }
        std::sort(order.begin(), order.end(),
                  [](const auto &l, const auto &r) { return l.first > r.first; });

        const V3 lightDir{-fwd.x, -fwd.y, -fwd.z}; // headlight: toward the camera
        for (const auto &[depth, t] : order)
        {
            const uint32_t a = idx[t * 3 + 0], b = idx[t * 3 + 1], c = idx[t * 3 + 2];
            const V3       pa{m.positions[a * 3], m.positions[a * 3 + 1], m.positions[a * 3 + 2]};
            const V3       pb{m.positions[b * 3], m.positions[b * 3 + 1], m.positions[b * 3 + 2]};
            const V3       pc{m.positions[c * 3], m.positions[c * 3 + 1], m.positions[c * 3 + 2]};
            const V3       n     = Normalize(Cross(Sub(pb, pa), Sub(pc, pa)));
            const float    facing = Dot(n, lightDir);
            if (facing <= 0.0f)
                continue; // back-facing
            const float shade = 0.18f + 0.82f * std::clamp(facing, 0.0f, 1.0f);
            const int   g     = static_cast<int>(shade * 235.0f);
            const ImU32 col   = IM_COL32(g, g, static_cast<int>(g * 0.96f) + 6, 255);
            dl->AddTriangleFilled(ImVec2(sx[a], sy[a]), ImVec2(sx[b], sy[b]),
                                  ImVec2(sx[c], sy[c]), col);
        }
    }
    else if (cam.solid && triCount > kMaxSolidTris)
    {
        const ImVec2 tp(p0.x + 8, p0.y + 8);
        dl->AddText(tp, IM_COL32(255, 210, 120, 255),
                    "LOD too dense for solid fill — showing wireframe");
    }

    if (drawWire || (cam.solid && triCount > kMaxSolidTris && triCount <= kMaxWireTris))
    {
        const ImU32 wcol = drawSolid ? IM_COL32(150, 170, 210, 90) : IM_COL32(150, 200, 255, 200);
        for (uint32_t t = 0; t < triCount; ++t)
        {
            const uint32_t a = idx[t * 3 + 0], b = idx[t * 3 + 1], c = idx[t * 3 + 2];
            if (!ok[a] || !ok[b] || !ok[c])
                continue;
            dl->AddLine(ImVec2(sx[a], sy[a]), ImVec2(sx[b], sy[b]), wcol);
            dl->AddLine(ImVec2(sx[b], sy[b]), ImVec2(sx[c], sy[c]), wcol);
            dl->AddLine(ImVec2(sx[c], sy[c]), ImVec2(sx[a], sy[a]), wcol);
        }
    }
    else if (triCount > kMaxWireTris)
    {
        dl->AddText(ImVec2(p0.x + 8, p0.y + 8), IM_COL32(255, 150, 150, 255),
                    "mesh too dense to preview at this LOD");
    }

    dl->PopClipRect();
}

} // namespace viewer
