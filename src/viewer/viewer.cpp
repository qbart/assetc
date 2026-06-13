#include "viewer/viewer.hpp"
#include "viewer/gpu.hpp"
#include "viewer/mesh_view.hpp"

#include "assetc/pack.hpp"
#include "assetc/runtime_manifest.hpp"
#include "deps/fmt.hpp"

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <string>
#include <vector>

#include <imgui.h>
#include <imgui_internal.h> // DockBuilder API for the default layout
#include <ktx.h>

namespace fs = std::filesystem;

namespace
{

// ---------------------------------------------------------------------------
// Asset source: a flat list of entries that can be browsed and whose bytes can
// be read on demand. Backed either by a `.hpack` (read at offset/size) or by a
// compiled output directory (read whole files).
// ---------------------------------------------------------------------------

struct Entry
{
    std::string      path; // runtime-relative, forward-slash
    assetc::PackKind kind;
    uint64_t         size;
};

struct Source
{
    bool                            isPack = false;
    std::string                     packPath;          // when isPack
    std::vector<assetc::PackEntry>  packToc;           // when isPack (parallel to entries)
    std::string                     rootDir;           // when !isPack
    std::vector<Entry>              entries;
};

std::vector<uint8_t> ReadAll(std::istream &in, uint64_t off, uint64_t size)
{
    std::vector<uint8_t> buf(size);
    in.seekg((std::streamoff)off);
    in.read(reinterpret_cast<char *>(buf.data()), (std::streamsize)size);
    if (!in)
        buf.clear();
    return buf;
}

std::vector<uint8_t> ReadEntryBytes(const Source &src, int idx)
{
    if (idx < 0 || idx >= (int)src.entries.size())
        return {};
    if (src.isPack)
    {
        std::ifstream f(src.packPath, std::ios::binary);
        if (!f)
            return {};
        const auto &pe = src.packToc[idx];
        return ReadAll(f, pe.offset, pe.size);
    }
    const fs::path p = fs::path(src.rootDir) / src.entries[idx].path;
    std::ifstream  f(p, std::ios::binary | std::ios::ate);
    if (!f)
        return {};
    const auto sz = (uint64_t)f.tellg();
    f.seekg(0);
    return ReadAll(f, 0, sz);
}

bool BuildSource(const std::string &path, Source &out)
{
    std::error_code ec;
    if (fs::is_directory(path, ec))
    {
        out.isPack  = false;
        out.rootDir = path;
        for (auto it = fs::recursive_directory_iterator(
                 path, fs::directory_options::skip_permission_denied, ec);
             it != fs::recursive_directory_iterator(); it.increment(ec))
        {
            if (ec || !it->is_regular_file())
                continue;
            const auto rel = fs::relative(it->path(), path, ec).generic_string();
            if (rel.empty() || rel.starts_with(".assetc-cache"))
                continue;
            out.entries.push_back(
                Entry{rel, assetc::PackKindOf(rel), (uint64_t)it->file_size(ec)});
        }
        std::sort(out.entries.begin(), out.entries.end(),
                  [](const Entry &a, const Entry &b) { return a.path < b.path; });
        return !out.entries.empty();
    }

    // Treat as a pack file (accept a missing ".hpack" suffix).
    std::string pp = path;
    if (!fs::exists(pp, ec) && fs::exists(pp + ".hpack", ec))
        pp += ".hpack";
    if (!fs::exists(pp, ec))
    {
        fmtx::Error(fmt::format("not a directory or pack file: {}", path));
        return false;
    }
    if (assetc::ReadPackToc(pp, out.packToc) != 0)
        return false;
    out.isPack   = true;
    out.packPath = pp;
    for (const auto &pe : out.packToc)
        out.entries.push_back(Entry{pe.path, pe.kind, pe.size});
    return true;
}

const char *KindTag(assetc::PackKind k)
{
    using K = assetc::PackKind;
    switch (k)
    {
    case K::Mesh:      return "mesh";
    case K::Material:  return "mat ";
    case K::Manifest:  return "man ";
    case K::Animation: return "anim";
    case K::Texture:   return "tex ";
    case K::Shader:    return "spv ";
    case K::Font:      return "font";
    case K::Other:     break;
    }
    return "??? ";
}

std::string HumanSize(uint64_t b)
{
    const char *u[] = {"B", "KB", "MB", "GB"};
    double       v  = (double)b;
    int          i  = 0;
    while (v >= 1024.0 && i < 3)
    {
        v /= 1024.0;
        ++i;
    }
    return i == 0 ? fmt::format("{} {}", b, u[0]) : fmt::format("{:.1f} {}", v, u[i]);
}

// ---------------------------------------------------------------------------
// KTX2 decode -> per-mip RGBA8. Transcodes UASTC/Basis to RGBA32 first; raw
// (Zstd-supercompressed) RGBA8/RGB8 is inflated by libktx on load.
// ---------------------------------------------------------------------------

struct KtxMeta
{
    bool        ok = false;
    std::string error;
    uint32_t    width = 0, height = 0, depth = 0;
    uint32_t    levels = 0, layers = 0, faces = 0;
    bool        isCubemap = false;
    uint32_t    vkFormat  = 0;
    uint32_t    supercompression = 0;
    bool        wasTranscoded     = false;
};

const char *VkFormatName(uint32_t f)
{
    switch (f)
    {
    case 0:  return "UNDEFINED (pre-transcode)";
    case 9:  return "R8_UNORM";
    case 23: return "R8G8B8_UNORM";
    case 29: return "R8G8B8_SRGB";
    case 37: return "R8G8B8A8_UNORM";
    case 43: return "R8G8B8A8_SRGB";
    default: return "other";
    }
}

const char *SupercompressionName(uint32_t s)
{
    switch (s)
    {
    case 0:  return "none";
    case 1:  return "BasisLZ";
    case 2:  return "Zstandard";
    case 3:  return "ZLIB";
    default: return "?";
    }
}

// Decode the chain for one (layer, face). Returns mips in `out` (base first).
bool DecodeKtx(const std::vector<uint8_t> &bytes, uint32_t layer, uint32_t face,
               std::vector<viewer::MipPixels> &out, KtxMeta &meta)
{
    out.clear();
    meta = KtxMeta{};

    ktxTexture2 *k = nullptr;
    KTX_error_code e = ktxTexture2_CreateFromMemory(
        bytes.data(), bytes.size(), KTX_TEXTURE_CREATE_LOAD_IMAGE_DATA_BIT, &k);
    if (e != KTX_SUCCESS || !k)
    {
        meta.error = fmt::format("ktx open failed ({})", (int)e);
        return false;
    }

    meta.width            = k->baseWidth;
    meta.height           = k->baseHeight;
    meta.depth            = k->baseDepth;
    meta.levels           = k->numLevels;
    meta.layers           = k->numLayers;
    meta.faces            = k->numFaces;
    meta.isCubemap        = k->isCubemap;
    meta.supercompression = (uint32_t)k->supercompressionScheme;

    if (ktxTexture2_NeedsTranscoding(k))
    {
        e = ktxTexture2_TranscodeBasis(k, KTX_TTF_RGBA32, 0);
        if (e != KTX_SUCCESS)
        {
            meta.vkFormat = k->vkFormat;
            meta.error    = fmt::format("transcode failed ({})", (int)e);
            ktxTexture2_Destroy(k);
            return false;
        }
        meta.wasTranscoded = true;
    }
    meta.vkFormat = k->vkFormat;

    ktxTexture *base = reinterpret_cast<ktxTexture *>(k);
    layer            = layer < k->numLayers ? layer : 0;
    face             = face < k->numFaces ? face : 0;

    for (uint32_t i = 0; i < k->numLevels; ++i)
    {
        const uint32_t lw = std::max(1u, meta.width >> i);
        const uint32_t lh = std::max(1u, meta.height >> i);

        ktx_size_t off = 0;
        if (ktxTexture_GetImageOffset(base, i, layer, face, &off) != KTX_SUCCESS)
            break;
        const ktx_size_t isz = ktxTexture_GetImageSize(base, i);
        const uint8_t   *src = k->pData + off;
        const uint32_t   px  = lw * lh;
        if (px == 0)
            continue;
        const uint32_t bpp = (uint32_t)(isz / px);

        viewer::MipPixels mp;
        mp.width  = lw;
        mp.height = lh;
        mp.rgba.resize((size_t)px * 4);
        if (bpp == 4)
        {
            std::memcpy(mp.rgba.data(), src, (size_t)px * 4);
        }
        else if (bpp == 3)
        {
            for (uint32_t p = 0; p < px; ++p)
            {
                mp.rgba[p * 4 + 0] = src[p * 3 + 0];
                mp.rgba[p * 4 + 1] = src[p * 3 + 1];
                mp.rgba[p * 4 + 2] = src[p * 3 + 2];
                mp.rgba[p * 4 + 3] = 255;
            }
        }
        else if (bpp == 1)
        {
            for (uint32_t p = 0; p < px; ++p)
            {
                mp.rgba[p * 4 + 0] = src[p];
                mp.rgba[p * 4 + 1] = src[p];
                mp.rgba[p * 4 + 2] = src[p];
                mp.rgba[p * 4 + 3] = 255;
            }
        }
        else
        {
            meta.error = fmt::format("unsupported {} bytes/pixel (vkFormat {})", bpp, k->vkFormat);
            ktxTexture2_Destroy(k);
            return false;
        }
        out.push_back(std::move(mp));
    }

    ktxTexture2_Destroy(k);
    meta.ok = !out.empty();
    if (!meta.ok && meta.error.empty())
        meta.error = "no decodable mip levels";
    return meta.ok;
}

// FourCC of the first 4 bytes, for the non-texture details pane.
std::string FourCC(const std::vector<uint8_t> &b)
{
    if (b.size() < 4)
        return "(short)";
    std::string s;
    for (int i = 0; i < 4; ++i)
        s += (b[i] >= 32 && b[i] < 127) ? (char)b[i] : '.';
    return s;
}

uint32_t LE32(const std::vector<uint8_t> &b, size_t off)
{
    if (b.size() < off + 4)
        return 0;
    return (uint32_t)b[off] | ((uint32_t)b[off + 1] << 8) | ((uint32_t)b[off + 2] << 16) |
           ((uint32_t)b[off + 3] << 24);
}

// ---------------------------------------------------------------------------
// Per-selection view state.
// ---------------------------------------------------------------------------

struct View
{
    int                selected = -1;
    bool               dirty    = false;
    viewer::GpuTexture tex;
    bool               isTexture = false;
    KtxMeta            meta;
    bool               isMesh = false;
    viewer::MeshCpu    mesh;
    viewer::MeshCamera meshCam;
    std::string        details; // non-texture pane text
    int                mip = 0, face = 0, layer = 0;
    float              zoom = 1.0f;
    char               filter[128] = {0};
};

void LoadSelection(viewer::GpuContext &gpu, const Source &src, View &v)
{
    if (v.tex.valid)
        gpu.DestroyTexture(v.tex);
    v.isTexture = false;
    v.isMesh    = false;
    v.mesh      = viewer::MeshCpu{};
    v.details.clear();
    v.meta = KtxMeta{};
    if (v.selected < 0)
        return;

    const Entry         &e     = src.entries[v.selected];
    std::vector<uint8_t> bytes = ReadEntryBytes(src, v.selected);
    if (bytes.empty())
    {
        v.details = "could not read entry bytes";
        return;
    }

    if (e.kind == assetc::PackKind::Texture)
    {
        std::vector<viewer::MipPixels> mips;
        if (DecodeKtx(bytes, (uint32_t)v.layer, (uint32_t)v.face, mips, v.meta))
        {
            v.tex       = gpu.CreateTexture(mips);
            v.isTexture = v.tex.valid;
            if (v.mip >= (int)v.tex.mips)
                v.mip = 0;
        }
        return;
    }

    if (e.kind == assetc::PackKind::Mesh)
    {
        v.mesh   = viewer::ParseHMesh(bytes.data(), bytes.size());
        v.isMesh = v.mesh.valid;
        if (!v.isMesh)
        {
            v.details = fmt::format("mesh parse failed: {}", v.mesh.error);
            return;
        }
        // Pull real material colors from the companion .hmat (same path, .hmat
        // extension), if one exists in this source. Slot order matches the mesh.
        if (e.path.size() > 6 && e.path.compare(e.path.size() - 6, 6, ".hmesh") == 0)
        {
            const std::string matPath = e.path.substr(0, e.path.size() - 6) + ".hmat";
            for (int i = 0; i < (int)src.entries.size(); ++i)
                if (src.entries[i].path == matPath)
                {
                    const std::vector<uint8_t> mb = ReadEntryBytes(src, i);
                    if (!mb.empty())
                        v.mesh.materialColors = viewer::ParseHMatColors(mb.data(), mb.size());
                    break;
                }
        }
        return;
    }

    // Non-texture: header summary. Full mesh/material stats live in `assetc info`.
    std::string d = fmt::format("path: {}\nkind: {}\nsize: {}\n", e.path, KindTag(e.kind),
                                HumanSize(e.size));
    d += fmt::format("magic: '{}'  version: {}\n", FourCC(bytes), LE32(bytes, 4));
    if (e.kind == assetc::PackKind::Manifest)
    {
        d += fmt::format("entries: {}\n", LE32(bytes, 8));
    }
    d += "\n(use `assetc info` for full geometry / material stats)";
    v.details = std::move(d);
}

// Full-window dockspace. Builds a default layout the first time it runs: the
// "Assets" tree pinned to the left, "Inspector" filling the rest.
void DrawDockspace()
{
    const ImGuiID dockspace_id = ImGui::DockSpaceOverViewport(0, ImGui::GetMainViewport());

    static bool built = false;
    if (built)
        return;
    built = true;

    ImGui::DockBuilderRemoveNode(dockspace_id);
    ImGui::DockBuilderAddNode(dockspace_id, ImGuiDockNodeFlags_DockSpace);
    ImGui::DockBuilderSetNodeSize(dockspace_id, ImGui::GetMainViewport()->Size);

    ImGuiID main = dockspace_id;
    ImGuiID left = ImGui::DockBuilderSplitNode(main, ImGuiDir_Left, 0.28f, nullptr, &main);

    ImGui::DockBuilderDockWindow("Assets", left);
    ImGui::DockBuilderDockWindow("Inspector", main);
    ImGui::DockBuilderFinish(dockspace_id);
}

void DrawBrowser(Source &src, View &v)
{
    ImGui::Begin("Assets");

    ImGui::TextUnformatted(src.isPack ? src.packPath.c_str() : src.rootDir.c_str());
    ImGui::Text("%zu entries", src.entries.size());
    ImGui::InputTextWithHint("##filter", "filter path...", v.filter, sizeof(v.filter));
    ImGui::Separator();

    ImGui::BeginChild("list");
    const std::string flt = v.filter;
    for (int i = 0; i < (int)src.entries.size(); ++i)
    {
        const Entry &e = src.entries[i];
        if (!flt.empty() && e.path.find(flt) == std::string::npos)
            continue;
        const std::string label =
            fmt::format("[{}] {}##{}", KindTag(e.kind), e.path, i);
        if (ImGui::Selectable(label.c_str(), v.selected == i))
        {
            if (v.selected != i)
            {
                v.selected = i;
                v.mip = v.face = v.layer = 0;
                v.zoom                   = 1.0f;
                v.dirty                  = true;
            }
        }
    }
    ImGui::EndChild();
    ImGui::End();
}

void DrawInspector(viewer::GpuContext &gpu, const Source &src, View &v)
{
    ImGui::Begin("Inspector");

    if (v.selected < 0)
    {
        ImGui::TextDisabled("select an asset on the left");
        ImGui::End();
        return;
    }

    const Entry &e = src.entries[v.selected];
    ImGui::Text("%s", e.path.c_str());
    ImGui::SameLine();
    ImGui::TextDisabled("(%s, %s)", KindTag(e.kind), HumanSize(e.size).c_str());
    ImGui::Separator();

    if (v.isTexture && v.tex.valid)
    {
        const KtxMeta &m = v.meta;
        ImGui::Text("%ux%u  levels:%u  faces:%u  layers:%u%s", m.width, m.height, m.levels,
                    m.faces, m.layers, m.isCubemap ? "  [cube]" : "");
        ImGui::Text("format: %s (%u)   supercompression: %s%s", VkFormatName(m.vkFormat),
                    m.vkFormat, SupercompressionName(m.supercompression),
                    m.wasTranscoded ? "   [transcoded->RGBA8]" : "");

        // Face / layer pickers re-decode (cheap; data already in memory).
        if (m.faces > 1)
        {
            if (ImGui::SliderInt("face", &v.face, 0, (int)m.faces - 1))
                v.dirty = true;
        }
        if (m.layers > 1)
        {
            if (ImGui::SliderInt("layer", &v.layer, 0, (int)m.layers - 1))
                v.dirty = true;
        }

        if ((int)v.tex.mips > 1)
            ImGui::SliderInt("mip", &v.mip, 0, (int)v.tex.mips - 1);
        v.mip = std::clamp(v.mip, 0, (int)v.tex.mips - 1);
        ImGui::SliderFloat("zoom", &v.zoom, 0.1f, 16.0f, "%.2fx");
        ImGui::SameLine();
        if (ImGui::SmallButton("1x"))
            v.zoom = 1.0f;

        const uint32_t mw = std::max(1u, m.width >> v.mip);
        const uint32_t mh = std::max(1u, m.height >> v.mip);
        ImGui::Text("mip %d: %ux%u", v.mip, mw, mh);
        ImGui::Separator();

        ImGui::BeginChild("canvas", ImVec2(0, 0), false,
                          ImGuiWindowFlags_HorizontalScrollbar);
        const ImVec2 sz(mw * v.zoom, mh * v.zoom);
        ImGui::Image((ImTextureID)(uintptr_t)v.tex.mipIds[v.mip], sz);
        ImGui::EndChild();
    }
    else if (v.isTexture)
    {
        ImGui::TextColored(ImVec4(1, 0.5f, 0.4f, 1), "texture decode failed");
        if (!v.meta.error.empty())
            ImGui::TextWrapped("%s", v.meta.error.c_str());
    }
    else if (v.isMesh)
    {
        viewer::DrawMeshPreview(v.mesh, v.meshCam);
    }
    else
    {
        ImGui::TextUnformatted(v.details.c_str());
    }

    ImGui::End();
    (void)gpu;
}

} // namespace

namespace assetc
{

int RunViewer(const std::string &path)
{
    Source src;
    if (!BuildSource(path, src))
        return 1;
    fmtx::Info(fmt::format("ui: {} ({} entries)", src.isPack ? src.packPath : src.rootDir,
                           src.entries.size()));

    viewer::GpuContext gpu;
    if (!gpu.Init("assetc ui", 1280, 760))
    {
        gpu.Shutdown();
        return 1;
    }

    View v;
    // Open on the first texture (else first entry) so there's something on screen.
    for (int i = 0; i < (int)src.entries.size(); ++i)
        if (src.entries[i].kind == PackKind::Texture)
        {
            v.selected = i;
            break;
        }
    if (v.selected < 0 && !src.entries.empty())
        v.selected = 0;
    v.dirty = true;

    while (gpu.BeginFrame())
    {
        if (v.dirty)
        {
            LoadSelection(gpu, src, v);
            v.dirty = false;
        }
        DrawDockspace();
        DrawBrowser(src, v);
        DrawInspector(gpu, src, v);
        gpu.EndFrame();
    }

    if (v.tex.valid)
        gpu.DestroyTexture(v.tex);
    gpu.Shutdown();
    return 0;
}

} // namespace assetc
