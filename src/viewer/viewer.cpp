#include "viewer/viewer.hpp"
#include "viewer/gpu.hpp"
#include "viewer/mesh_view.hpp"

#include "assetc/pack.hpp"
#include "assetc/runtime_anim.hpp"
#include "assetc/runtime_font.hpp"
#include "assetc/runtime_manifest.hpp"
#include "assetc/runtime_material.hpp"
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
// In-memory parsers for the non-texture, non-mesh formats. They read straight
// from the entry bytes (so they work for pack payloads and loose files alike),
// surfacing the same per-file detail `assetc info` prints — no shelling out.
// ---------------------------------------------------------------------------

// One `.hman` record (mirrors the streamed on-disk entry; see docs/hman.md).
struct ManRow
{
    uint64_t    hash;
    uint8_t     kind;
    uint8_t     colorspace;
    std::string path;
};

const char *ManKindName(uint8_t k)
{
    switch (static_cast<assetc::ManKind>(k))
    {
    case assetc::ManKind::Texture:  return "texture";
    case assetc::ManKind::Mesh:     return "mesh";
    case assetc::ManKind::Material: return "material";
    case assetc::ManKind::Lut:      return "lut";
    case assetc::ManKind::Shader:   return "shader";
    case assetc::ManKind::Embed:    return "embed";
    }
    return "?";
}

bool ParseManifest(const std::vector<uint8_t> &b, std::vector<ManRow> &out)
{
    out.clear();
    if (b.size() < 16 || LE32(b, 0) != assetc::ManMagic)
        return false;
    const uint32_t count = LE32(b, 8);
    size_t         off   = 16;
    out.reserve(count);
    for (uint32_t i = 0; i < count; ++i)
    {
        if (off + 12 > b.size())
            break;
        ManRow r;
        std::memcpy(&r.hash, &b[off], 8);
        r.kind       = b[off + 8];
        r.colorspace = b[off + 9];
        uint16_t plen;
        std::memcpy(&plen, &b[off + 10], 2);
        off += 12;
        if (off + plen > b.size())
            break;
        r.path.assign(reinterpret_cast<const char *>(&b[off]), plen);
        off += plen;
        out.push_back(std::move(r));
    }
    return true;
}

bool ParseMaterials(const std::vector<uint8_t> &b, assetc::MatFileHeader &hdr,
                    std::vector<assetc::GpuMaterial> &rows)
{
    rows.clear();
    if (b.size() < sizeof(hdr))
        return false;
    std::memcpy(&hdr, b.data(), sizeof(hdr));
    if (hdr.magic != assetc::MatMagic)
        return false;
    size_t off = sizeof(hdr);
    rows.reserve(hdr.count);
    for (uint32_t i = 0; i < hdr.count; ++i)
    {
        if (off + sizeof(assetc::GpuMaterial) > b.size())
            break;
        assetc::GpuMaterial m;
        std::memcpy(&m, &b[off], sizeof(m));
        rows.push_back(m);
        off += sizeof(m);
    }
    return true;
}

bool ParseFont(const std::vector<uint8_t> &b, assetc::FontFileHeader &hdr,
               std::vector<assetc::GpuGlyph> &glyphs)
{
    glyphs.clear();
    if (b.size() < sizeof(hdr))
        return false;
    std::memcpy(&hdr, b.data(), sizeof(hdr));
    if (hdr.magic != assetc::FontMagic)
        return false;
    size_t off = sizeof(hdr);
    glyphs.reserve(hdr.glyphCount);
    for (uint32_t i = 0; i < hdr.glyphCount; ++i)
    {
        if (off + sizeof(assetc::GpuGlyph) > b.size())
            break;
        assetc::GpuGlyph g;
        std::memcpy(&g, &b[off], sizeof(g));
        glyphs.push_back(g);
        off += sizeof(g);
    }
    return true;
}

struct AnimClipInfo
{
    std::string name;
    float       duration = 0;
    uint32_t    channels = 0;
};

// Walk the streamed `.hanim` layout (see runtime_anim.hpp) far enough to recover
// each clip's name/duration/channel count, skipping over keyframe payloads.
bool ParseAnim(const std::vector<uint8_t> &b, uint32_t &version, std::vector<AnimClipInfo> &clips)
{
    clips.clear();
    if (b.size() < 16 || LE32(b, 0) != assetc::AnimMagic)
        return false;
    version              = LE32(b, 4);
    const uint32_t count = LE32(b, 8);
    size_t         off   = 16;
    auto           have  = [&](size_t n) { return off + n <= b.size(); };
    for (uint32_t c = 0; c < count; ++c)
    {
        if (!have(2))
            break;
        uint16_t nameLen;
        std::memcpy(&nameLen, &b[off], 2);
        off += 2;
        if (!have(nameLen))
            break;
        AnimClipInfo ci;
        ci.name.assign(reinterpret_cast<const char *>(&b[off]), nameLen);
        off += nameLen;
        if (!have(8))
            break;
        std::memcpy(&ci.duration, &b[off], 4);
        std::memcpy(&ci.channels, &b[off + 4], 4);
        off += 8;
        for (uint32_t ch = 0; ch < ci.channels; ++ch)
        {
            if (!have(12)) // joint u32, path u8, interp u8, components u8, _pad u8, keyCount u32
                break;
            const uint8_t components = b[off + 6];
            uint32_t      keyCount;
            std::memcpy(&keyCount, &b[off + 8], 4);
            off += 12;
            const size_t keyBytes = static_cast<size_t>(keyCount) * (1u + components) * 4u;
            if (!have(keyBytes))
                break;
            off += keyBytes;
        }
        clips.push_back(std::move(ci));
    }
    return true;
}

struct SpirvInfo
{
    bool     ok    = false;
    uint32_t major = 0, minor = 0, generator = 0, bound = 0;
};

SpirvInfo ParseSpirv(const std::vector<uint8_t> &b)
{
    SpirvInfo s;
    if (b.size() < 20 || LE32(b, 0) != 0x07230203u) // SPIR-V magic (little-endian module)
        return s;
    const uint32_t ver = LE32(b, 4);
    s.major            = (ver >> 16) & 0xff;
    s.minor            = (ver >> 8) & 0xff;
    s.generator        = LE32(b, 8);
    s.bound            = LE32(b, 12);
    s.ok               = true;
    return s;
}

// Heuristic: mostly-printable (incl. UTF-8 high bytes), no embedded NUL -> render
// embeds like scene/level.json as text; otherwise hex-dump them.
bool LooksLikeText(const std::vector<uint8_t> &b)
{
    const size_t n = std::min<size_t>(b.size(), 4096);
    if (n == 0)
        return true;
    size_t printable = 0;
    for (size_t i = 0; i < n; ++i)
    {
        const uint8_t c = b[i];
        if (c == 0)
            return false;
        if (c == '\t' || c == '\n' || c == '\r' || (c >= 0x20 && c < 0x7f) || c >= 0x80)
            ++printable;
    }
    return printable * 100 >= n * 95;
}

// What kind of detail panel the inspector draws for the current non-texture,
// non-mesh selection.
enum class Detail
{
    None,
    Manifest,
    Material,
    Anim,
    Font,
    Shader,
    Blob
};

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
    viewer::MeshRender meshRender;
    std::string        details; // error text for the non-texture pane (empty on success)
    int                mip = 0, face = 0, layer = 0;
    float              zoom = 1.0f;
    char               filter[128] = {0};

    // Parsed detail for the current non-texture, non-mesh selection.
    Detail                           detail = Detail::None;
    std::string                      headerLine; // magic/version/size summary atop the panel
    std::vector<ManRow>              manRows;
    assetc::MatFileHeader            matHdr{};
    std::vector<assetc::GpuMaterial> matRows;
    uint32_t                         animVersion = 0;
    std::vector<AnimClipInfo>        animClips;
    assetc::FontFileHeader           fontHdr{};
    std::vector<assetc::GpuGlyph>    fontGlyphs;
    SpirvInfo                        spirv;
    std::vector<uint8_t>             blob;       // raw bytes for an embed/other preview
    bool                             blobIsText = false;
};

void LoadSelection(viewer::GpuContext &gpu, const Source &src, View &v)
{
    if (v.tex.valid)
        gpu.DestroyTexture(v.tex);
    v.meshRender.Reset(gpu);
    v.isTexture = false;
    v.isMesh    = false;
    v.mesh      = viewer::MeshCpu{};
    v.details.clear();
    v.meta   = KtxMeta{};
    v.detail = Detail::None;
    v.headerLine.clear();
    v.manRows.clear();
    v.matRows.clear();
    v.animClips.clear();
    v.fontGlyphs.clear();
    v.spirv = SpirvInfo{};
    v.blob.clear();
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
        else
        {
            // Surface the decode reason instead of a blank pane.
            v.details = fmt::format("texture decode failed: {}",
                                    v.meta.error.empty() ? "unknown error" : v.meta.error);
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

    // Non-texture, non-mesh: parse the format and surface full detail inline (the
    // same stats `assetc info` prints), so nothing sends you back to the CLI.
    v.headerLine = fmt::format("magic '{}'  version {}   |   {}", FourCC(bytes), LE32(bytes, 4),
                               HumanSize(e.size));

    if (e.kind == assetc::PackKind::Manifest && ParseManifest(bytes, v.manRows))
    {
        v.detail = Detail::Manifest;
        return;
    }
    if (e.kind == assetc::PackKind::Material && ParseMaterials(bytes, v.matHdr, v.matRows))
    {
        v.detail = Detail::Material;
        return;
    }
    if (e.kind == assetc::PackKind::Animation && ParseAnim(bytes, v.animVersion, v.animClips))
    {
        v.detail = Detail::Anim;
        return;
    }
    if (e.kind == assetc::PackKind::Font && ParseFont(bytes, v.fontHdr, v.fontGlyphs))
    {
        v.detail = Detail::Font;
        return;
    }
    if (e.kind == assetc::PackKind::Shader)
    {
        v.spirv      = ParseSpirv(bytes);
        v.headerLine = fmt::format("SPIR-V module   |   {}", HumanSize(e.size));
        v.detail     = Detail::Shader;
        return;
    }

    // Anything else — embeds (scene/level.json, config/path.xml, …) and unknowns:
    // show the bytes as text when they look like text, else a hex dump.
    v.blobIsText = LooksLikeText(bytes);
    v.headerLine = fmt::format("{}   |   {}", HumanSize(e.size), v.blobIsText ? "text" : "binary");
    v.blob       = std::move(bytes);
    v.detail     = Detail::Blob;
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

// A scrolling hex+ASCII dump of the first `cap` bytes, for binary embeds/unknowns.
void DrawHexDump(const std::vector<uint8_t> &b, size_t cap = 4096)
{
    const size_t n = std::min(b.size(), cap);
    ImGui::BeginChild("hex", ImVec2(0, 0), false, ImGuiWindowFlags_HorizontalScrollbar);
    for (size_t row = 0; row < n; row += 16)
    {
        std::string line = fmt::format("{:08x}  ", (unsigned)row);
        std::string ascii;
        for (size_t i = 0; i < 16; ++i)
        {
            if (row + i < n)
            {
                const uint8_t c = b[row + i];
                line += fmt::format("{:02x} ", c);
                ascii += (c >= 0x20 && c < 0x7f) ? (char)c : '.';
            }
            else
                line += "   ";
        }
        ImGui::TextUnformatted(fmt::format("{} {}", line, ascii).c_str());
    }
    if (b.size() > cap)
        ImGui::TextDisabled("... %zu more bytes", b.size() - cap);
    ImGui::EndChild();
}

// Render the full per-file detail for the current non-texture, non-mesh selection.
void DrawDetail(const View &v)
{
    ImGui::TextUnformatted(v.headerLine.c_str());
    ImGui::Separator();

    constexpr ImGuiTableFlags kTable = ImGuiTableFlags_Borders | ImGuiTableFlags_RowBg |
                                       ImGuiTableFlags_ScrollY | ImGuiTableFlags_Resizable |
                                       ImGuiTableFlags_SizingStretchProp;

    switch (v.detail)
    {
    case Detail::Manifest:
    {
        ImGui::Text("%zu entries (hash -> file)", v.manRows.size());
        if (ImGui::BeginTable("man", 4, kTable))
        {
            ImGui::TableSetupColumn("hash", ImGuiTableColumnFlags_WidthFixed, 150.0f);
            ImGui::TableSetupColumn("kind", ImGuiTableColumnFlags_WidthFixed, 70.0f);
            ImGui::TableSetupColumn("colorspace", ImGuiTableColumnFlags_WidthFixed, 70.0f);
            ImGui::TableSetupColumn("path");
            ImGui::TableSetupScrollFreeze(0, 1);
            ImGui::TableHeadersRow();
            for (const auto &r : v.manRows)
            {
                ImGui::TableNextRow();
                ImGui::TableSetColumnIndex(0);
                ImGui::Text("0x%016llx", (unsigned long long)r.hash);
                ImGui::TableSetColumnIndex(1);
                ImGui::TextUnformatted(ManKindName(r.kind));
                ImGui::TableSetColumnIndex(2);
                ImGui::TextUnformatted(r.colorspace ? "sRGB" : "linear");
                ImGui::TableSetColumnIndex(3);
                ImGui::TextUnformatted(r.path.c_str());
            }
            ImGui::EndTable();
        }
        break;
    }
    case Detail::Material:
    {
        ImGui::Text("%u material row(s)  (row i == SubMesh::materialSlot i)", v.matHdr.count);
        if (ImGui::BeginTable("mat", 6, kTable))
        {
            ImGui::TableSetupColumn("#", ImGuiTableColumnFlags_WidthFixed, 26.0f);
            ImGui::TableSetupColumn("baseColor", ImGuiTableColumnFlags_WidthFixed, 150.0f);
            ImGui::TableSetupColumn("metal", ImGuiTableColumnFlags_WidthFixed, 48.0f);
            ImGui::TableSetupColumn("rough", ImGuiTableColumnFlags_WidthFixed, 48.0f);
            ImGui::TableSetupColumn("alpha", ImGuiTableColumnFlags_WidthFixed, 90.0f);
            ImGui::TableSetupColumn("textures");
            ImGui::TableSetupScrollFreeze(0, 1);
            ImGui::TableHeadersRow();
            for (size_t i = 0; i < v.matRows.size(); ++i)
            {
                const assetc::GpuMaterial &m = v.matRows[i];
                ImGui::TableNextRow();
                ImGui::TableSetColumnIndex(0);
                ImGui::Text("%zu", i);
                ImGui::TableSetColumnIndex(1);
                const ImVec4 col(m.baseColorFactor[0], m.baseColorFactor[1], m.baseColorFactor[2],
                                 m.baseColorFactor[3]);
                ImGui::ColorButton("##c", col,
                                   ImGuiColorEditFlags_NoTooltip | ImGuiColorEditFlags_AlphaPreview,
                                   ImVec2(16, 16));
                ImGui::SameLine();
                ImGui::Text("%.2f %.2f %.2f %.2f", col.x, col.y, col.z, col.w);
                ImGui::TableSetColumnIndex(2);
                ImGui::Text("%.2f", m.metallicFactor);
                ImGui::TableSetColumnIndex(3);
                ImGui::Text("%.2f", m.roughnessFactor);
                ImGui::TableSetColumnIndex(4);
                const char *alpha = "opaque";
                switch (m.flags & assetc::MatFlag_AlphaModeBits)
                {
                case assetc::MatFlag_AlphaMask: alpha = "mask"; break;
                case assetc::MatFlag_AlphaBlend: alpha = "blend"; break;
                default: break;
                }
                ImGui::Text("%s%s", alpha,
                            (m.flags & assetc::MatFlag_DoubleSided) ? " 2-sided" : "");
                ImGui::TableSetColumnIndex(5);
                std::string tx;
                if (m.baseColorTex) tx += "base ";
                if (m.metallicRoughnessTex) tx += "mr ";
                if (m.normalTex) tx += "normal ";
                if (m.occlusionTex) tx += "occ ";
                if (m.emissiveTex) tx += "emissive ";
                ImGui::TextUnformatted(tx.empty() ? "(factors only)" : tx.c_str());
            }
            ImGui::EndTable();
        }
        break;
    }
    case Detail::Anim:
    {
        ImGui::Text("%zu clip(s)", v.animClips.size());
        if (ImGui::BeginTable("anim", 3, kTable))
        {
            ImGui::TableSetupColumn("clip");
            ImGui::TableSetupColumn("duration", ImGuiTableColumnFlags_WidthFixed, 90.0f);
            ImGui::TableSetupColumn("channels", ImGuiTableColumnFlags_WidthFixed, 80.0f);
            ImGui::TableSetupScrollFreeze(0, 1);
            ImGui::TableHeadersRow();
            for (const auto &c : v.animClips)
            {
                ImGui::TableNextRow();
                ImGui::TableSetColumnIndex(0);
                ImGui::TextUnformatted(c.name.empty() ? "(unnamed)" : c.name.c_str());
                ImGui::TableSetColumnIndex(1);
                ImGui::Text("%.2fs", c.duration);
                ImGui::TableSetColumnIndex(2);
                ImGui::Text("%u", c.channels);
            }
            ImGui::EndTable();
        }
        break;
    }
    case Detail::Font:
    {
        const assetc::FontFileHeader &h = v.fontHdr;
        ImGui::Text("%u glyphs | %u kerning pairs | atlas %ux%u | atlasTex 0x%016llx", h.glyphCount,
                    h.kerningCount, h.atlasWidth, h.atlasHeight, (unsigned long long)h.atlasTex);
        ImGui::Text("ascent %.3f  descent %.3f  lineGap %.3f  | SDF range %.2f  edge %.3f | %s",
                    h.ascent, h.descent, h.lineGap, h.distanceRange, h.edgeValue,
                    (h.flags & assetc::FontFlag_Msdf) ? "MSDF" : "SDF");
        ImGui::Separator();
        if (ImGui::BeginTable("glyphs", 4, kTable))
        {
            ImGui::TableSetupColumn("codepoint", ImGuiTableColumnFlags_WidthFixed, 90.0f);
            ImGui::TableSetupColumn("char", ImGuiTableColumnFlags_WidthFixed, 50.0f);
            ImGui::TableSetupColumn("advance", ImGuiTableColumnFlags_WidthFixed, 80.0f);
            ImGui::TableSetupColumn("uv (l,t,r,b)");
            ImGui::TableSetupScrollFreeze(0, 1);
            ImGui::TableHeadersRow();
            for (const auto &g : v.fontGlyphs)
            {
                ImGui::TableNextRow();
                ImGui::TableSetColumnIndex(0);
                ImGui::Text("U+%04X", g.codepoint);
                ImGui::TableSetColumnIndex(1);
                if (g.codepoint >= 0x20 && g.codepoint < 0x7f)
                    ImGui::Text("%c", (char)g.codepoint);
                else
                    ImGui::TextDisabled("-");
                ImGui::TableSetColumnIndex(2);
                ImGui::Text("%.3f", g.advance);
                ImGui::TableSetColumnIndex(3);
                ImGui::Text("%.3f %.3f %.3f %.3f", g.uvLeft, g.uvTop, g.uvRight, g.uvBottom);
            }
            ImGui::EndTable();
        }
        break;
    }
    case Detail::Shader:
    {
        if (v.spirv.ok)
        {
            ImGui::Text("SPIR-V %u.%u", v.spirv.major, v.spirv.minor);
            ImGui::Text("generator: 0x%08x", v.spirv.generator);
            ImGui::Text("id bound: %u  (~%u instructions)", v.spirv.bound,
                        v.spirv.bound ? v.spirv.bound - 1 : 0);
        }
        else
        {
            ImGui::TextDisabled("not a recognizable SPIR-V module");
        }
        break;
    }
    case Detail::Blob:
    {
        if (v.blobIsText)
        {
            ImGui::BeginChild("text", ImVec2(0, 0), false, ImGuiWindowFlags_HorizontalScrollbar);
            ImGui::TextUnformatted(reinterpret_cast<const char *>(v.blob.data()),
                                   reinterpret_cast<const char *>(v.blob.data()) + v.blob.size());
            ImGui::EndChild();
        }
        else
        {
            DrawHexDump(v.blob);
        }
        break;
    }
    case Detail::None:
    default:
        ImGui::TextDisabled("no preview for this entry");
        break;
    }
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
        const viewer::MeshCpu &m = v.mesh;
        const uint64_t tris0 = m.lodIndices.empty() ? 0 : m.lodIndices[0].size() / 3;
        ImGui::Text("%u verts | %llu tris | %u submeshes | %u materials | %zu LOD(s)", m.vertexCount,
                    (unsigned long long)tris0, m.submeshCount, m.materialCount, m.lodIndices.size());
        ImGui::Text("aabb %.2f x %.2f x %.2f   r=%.2f", m.aabbMax[0] - m.aabbMin[0],
                    m.aabbMax[1] - m.aabbMin[1], m.aabbMax[2] - m.aabbMin[2], m.radius);
        ImGui::Separator();
        viewer::DrawMeshPreview(gpu, v.mesh, v.meshCam, v.meshRender);
    }
    else if (!v.details.empty())
    {
        // An error from LoadSelection (unreadable bytes / parse failure).
        ImGui::TextColored(ImVec4(1, 0.5f, 0.4f, 1), "%s", v.details.c_str());
    }
    else
    {
        DrawDetail(v);
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
    v.meshRender.Reset(gpu);
    gpu.Shutdown();
    return 0;
}

} // namespace assetc
