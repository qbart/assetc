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
#include <cfloat>
#include <cstdint>
#include <cstring>
#include <filesystem>
#include <functional>
#include <fstream>
#include <string>
#include <unordered_map>
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
    bool                            isPack = false; // entries are read FROM the pack file
    std::string                     rootDir;        // when !isPack
    std::vector<Entry>              entries;        // the browseable asset list

    // A `.hpack` to visualize, shown as a node in the list. Always present when
    // isPack; in directory mode it's the sibling `<dir>.hpack` if one exists.
    bool                            packAvailable = false;
    std::string                     packPath;
    std::vector<assetc::PackEntry>  packToc;
    std::vector<int>                packToList; // packToc index -> entries index, or -1
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

// Map each pack-TOC entry to the matching list entry (by runtime-relative path),
// so a click in the pack viz selects the right asset. Identity when entries came
// straight from the pack.
void BuildPackToList(Source &out)
{
    out.packToList.assign(out.packToc.size(), -1);
    if (out.isPack)
    {
        for (size_t i = 0; i < out.packToc.size(); ++i)
            out.packToList[i] = static_cast<int>(i);
        return;
    }
    std::unordered_map<std::string, int> byPath;
    byPath.reserve(out.entries.size());
    for (int i = 0; i < static_cast<int>(out.entries.size()); ++i)
        byPath.emplace(out.entries[i].path, i);
    for (size_t i = 0; i < out.packToc.size(); ++i)
    {
        auto it = byPath.find(out.packToc[i].path);
        if (it != byPath.end())
            out.packToList[i] = it->second;
    }
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

        // Look for the sibling `<dir>.hpack` so it can be browsed/visualized inline,
        // without the user ever pointing the viewer at the pack file directly.
        fs::path    dd = fs::path(path).lexically_normal();
        std::string dn = dd.filename().string();
        if (dn.empty())
        {
            dd = dd.parent_path();
            dn = dd.filename().string();
        }
        const fs::path pk = dd.parent_path() / (dn + ".hpack");
        if (fs::exists(pk, ec) && assetc::ReadPackToc(pk.generic_string(), out.packToc) == 0)
        {
            out.packAvailable = true;
            out.packPath      = pk.generic_string();
            BuildPackToList(out);
        }
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
    out.isPack        = true;
    out.packAvailable = true;
    out.packPath      = pp;
    for (const auto &pe : out.packToc)
        out.entries.push_back(Entry{pe.path, pe.kind, pe.size});
    BuildPackToList(out);
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
    return "file";
}

// The reference name the engine content-addresses this asset by — i.e. what you
// pass to the SDK hasher to resolve it through `assets.hman`. Mirrors how the
// manifest is keyed:
//   - content-store textures "tex/<hash>.ktx2" -> the content hash (filename stem)
//   - shaders (.spv) and name-addressed atlases (.ktx2) -> path minus extension
//     (engine: HashAssetRef on this)
//   - embeds and everything else -> the runtime-relative path, extension kept
//     (engine: HashEmbedRef on this)
std::string ContentRef(const std::string &path)
{
    // "tex/<16 lowercase hex>.ktx2": the stem IS the content address.
    if (path.size() == 4 + 16 + 5 && path.compare(0, 4, "tex/") == 0 && path.ends_with(".ktx2"))
    {
        bool hex = true;
        for (size_t i = 4; i < 4 + 16; ++i)
        {
            const char c = path[i];
            if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f')))
            {
                hex = false;
                break;
            }
        }
        if (hex)
            return path.substr(4, 16);
    }
    if (path.ends_with(".spv"))
        return path.substr(0, path.size() - 4);
    if (path.ends_with(".ktx2"))
        return path.substr(0, path.size() - 5);
    return path;
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
    case assetc::ManKind::Texture:   return "texture";
    case assetc::ManKind::Mesh:      return "mesh";
    case assetc::ManKind::Material:  return "material";
    case assetc::ManKind::Lut:       return "lut";
    case assetc::ManKind::Shader:    return "shader";
    case assetc::ManKind::Embed:     return "embed";
    case assetc::ManKind::Animation: return "anim";
    case assetc::ManKind::Font:      return "font";
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

    // For a mesh: the raw `.hmesh` bytes + chunk table, so the inspector can show one
    // tab per chunk decoded into tables (alongside the 3D Preview tab).
    std::vector<uint8_t>            meshBytes;
    std::vector<assetc::ChunkEntry> meshChunks;
    assetc::MeshDesc                meshDesc{}; // counts/strides to interpret array chunks
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
    v.meshBytes.clear();
    v.meshChunks.clear();
    v.meshDesc = assetc::MeshDesc{};
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

        // Keep the raw bytes + chunk table so the inspector can show a tab per chunk.
        if (bytes.size() >= sizeof(assetc::FileHeader))
        {
            assetc::FileHeader fh{};
            std::memcpy(&fh, bytes.data(), sizeof(fh));
            const size_t tableOff   = sizeof(fh);
            const size_t tableBytes = static_cast<size_t>(fh.chunkCount) * sizeof(assetc::ChunkEntry);
            if (fh.magic == assetc::MeshMagic && tableOff + tableBytes <= bytes.size())
            {
                v.meshChunks.resize(fh.chunkCount);
                std::memcpy(v.meshChunks.data(), bytes.data() + tableOff, tableBytes);
                for (const auto &ce : v.meshChunks)
                    if (ce.fourcc == static_cast<uint32_t>(assetc::ChunkId::Desc) &&
                        ce.size >= sizeof(assetc::MeshDesc) &&
                        ce.offset + sizeof(assetc::MeshDesc) <= bytes.size())
                        std::memcpy(&v.meshDesc, bytes.data() + ce.offset, sizeof(assetc::MeshDesc));
            }
        }
        v.meshBytes = std::move(bytes);

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

// v.selected sentinel for "the pack itself is selected" (its overview opens in the
// Inspector, like any other entry). -1 stays "nothing selected".
constexpr int kPackSelected = -2;

void DrawBrowser(Source &src, View &v)
{
    ImGui::Begin("Assets");

    ImGui::TextUnformatted(src.isPack ? src.packPath.c_str() : src.rootDir.c_str());
    ImGui::Text("%zu entries", src.entries.size());
    ImGui::InputTextWithHint("##filter", "filter path...", v.filter, sizeof(v.filter));
    ImGui::Separator();

    ImGui::BeginChild("list");
    const std::string flt = v.filter;

    constexpr ImGuiTableFlags kListFlags = ImGuiTableFlags_RowBg | ImGuiTableFlags_ScrollY |
                                           ImGuiTableFlags_Resizable |
                                           ImGuiTableFlags_BordersInnerV;
    if (ImGui::BeginTable("entries", 3, kListFlags))
    {
        ImGui::TableSetupColumn("kind", ImGuiTableColumnFlags_WidthFixed, 38.0f);
        ImGui::TableSetupColumn("path", ImGuiTableColumnFlags_WidthStretch, 0.55f);
        // The reference the engine content-addresses each asset by (see ContentRef).
        ImGui::TableSetupColumn("engine ref", ImGuiTableColumnFlags_WidthStretch, 0.45f);
        ImGui::TableSetupScrollFreeze(0, 1);
        ImGui::TableHeadersRow();

        const ImVec4 gold(0.93f, 0.78f, 0.36f, 1.0f);

        // The pack file is itself a first-class, gold node: selecting it opens the
        // pack overview/visualization in the Inspector.
        if (src.packAvailable)
        {
            ImGui::TableNextRow();
            ImGui::TableSetColumnIndex(0);
            ImGui::PushStyleColor(ImGuiCol_Text, gold);
            const bool sel = ImGui::Selectable("pack##__pack__", v.selected == kPackSelected,
                                               ImGuiSelectableFlags_SpanAllColumns);
            ImGui::PopStyleColor();
            if (sel && v.selected != kPackSelected)
            {
                v.selected = kPackSelected;
                v.dirty    = true;
            }
            ImGui::TableSetColumnIndex(1);
            ImGui::TextColored(gold, "%s", fs::path(src.packPath).filename().string().c_str());
            ImGui::TableSetColumnIndex(2);
            ImGui::TextDisabled("(whole pack)");
        }

        for (int i = 0; i < (int)src.entries.size(); ++i)
        {
            const Entry      &e   = src.entries[i];
            const std::string ref = ContentRef(e.path);
            if (!flt.empty() && e.path.find(flt) == std::string::npos &&
                ref.find(flt) == std::string::npos)
                continue;

            ImGui::TableNextRow();
            ImGui::TableSetColumnIndex(0);
            const std::string id = fmt::format("{}##{}", KindTag(e.kind), i);
            if (ImGui::Selectable(id.c_str(), v.selected == i, ImGuiSelectableFlags_SpanAllColumns))
            {
                if (v.selected != i)
                {
                    v.selected = i;
                    v.mip = v.face = v.layer = 0;
                    v.zoom                   = 1.0f;
                    v.dirty                  = true;
                }
            }
            ImGui::TableSetColumnIndex(1);
            ImGui::TextUnformatted(e.path.c_str());
            ImGui::TableSetColumnIndex(2);
            ImGui::TextUnformatted(ref.c_str());
        }
        ImGui::EndTable();
    }
    ImGui::EndChild();
    ImGui::End();
}

// A scrolling hex+ASCII dump of the first `cap` bytes, for binary embeds/unknowns.
void DrawHexSpan(const uint8_t *b, size_t len, size_t cap = 4096)
{
    const size_t n = std::min(len, cap);
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
    if (len > cap)
        ImGui::TextDisabled("... %zu more bytes", len - cap);
    ImGui::EndChild();
}

void DrawHexDump(const std::vector<uint8_t> &b, size_t cap = 4096)
{
    DrawHexSpan(b.data(), b.size(), cap);
}

// ---------------------------------------------------------------------------
// .hmesh chunk inspector: one tab per chunk, each decoding its payload into a
// table. Reads straight from the kept raw bytes (View::meshBytes), with a virtual
// (clipped) table so even million-row vertex/index chunks stay responsive.
// ---------------------------------------------------------------------------

std::string FourCCStr(uint32_t f)
{
    char c[5] = {static_cast<char>(f & 0xff), static_cast<char>((f >> 8) & 0xff),
                 static_cast<char>((f >> 16) & 0xff), static_cast<char>((f >> 24) & 0xff), 0};
    for (int i = 0; i < 4; ++i)
        if (c[i] < 32 || c[i] >= 127)
            c[i] = '.';
    return c;
}

constexpr ImGuiTableFlags kChunkTable = ImGuiTableFlags_Borders | ImGuiTableFlags_RowBg |
                                        ImGuiTableFlags_ScrollY | ImGuiTableFlags_ScrollX |
                                        ImGuiTableFlags_Resizable;

// A scrollable, clipped table: `cols` headers, `count` rows, `row(i)` fills the
// columns for row i (via TableSetColumnIndex/Text).
void ArrayTable(const char *id, std::initializer_list<const char *> cols, int count,
                const std::function<void(int)> &row)
{
    if (count <= 0)
    {
        ImGui::TextDisabled("(empty)");
        return;
    }
    if (ImGui::BeginTable(id, static_cast<int>(cols.size()), kChunkTable))
    {
        for (const char *c : cols)
            ImGui::TableSetupColumn(c);
        ImGui::TableSetupScrollFreeze(0, 1);
        ImGui::TableHeadersRow();
        ImGuiListClipper clip;
        clip.Begin(count);
        while (clip.Step())
            for (int i = clip.DisplayStart; i < clip.DisplayEnd; ++i)
            {
                ImGui::TableNextRow();
                row(i);
            }
        ImGui::EndTable();
    }
}

// Render one chunk's payload. `v.meshBytes` holds the whole file; `e` is the chunk.
void DrawMeshChunk(const View &v, const assetc::ChunkEntry &e)
{
    using namespace assetc;
    ImGui::Text("%s   |   @%llu   |   %llu bytes", FourCCStr(e.fourcc).c_str(),
                (unsigned long long)e.offset, (unsigned long long)e.size);
    ImGui::Separator();

    if (e.offset + e.size > v.meshBytes.size())
    {
        ImGui::TextColored(ImVec4(1, 0.5f, 0.4f, 1), "chunk range past end of file");
        return;
    }
    const uint8_t *p  = v.meshBytes.data() + e.offset;
    const ChunkId  id = static_cast<ChunkId>(e.fourcc);

    auto kv = [](const char *k, const std::string &val) {
        ImGui::TableNextRow();
        ImGui::TableSetColumnIndex(0);
        ImGui::TextUnformatted(k);
        ImGui::TableSetColumnIndex(1);
        ImGui::TextUnformatted(val.c_str());
    };
    auto beginKV = [] {
        return ImGui::BeginTable("kv", 2,
                                 ImGuiTableFlags_Borders | ImGuiTableFlags_RowBg |
                                     ImGuiTableFlags_SizingStretchProp);
    };

    switch (id)
    {
    case ChunkId::Desc:
    {
        MeshDesc d{};
        std::memcpy(&d, p, std::min(sizeof(d), static_cast<size_t>(e.size)));
        if (beginKV())
        {
            kv("vertexCount", fmt::format("{}", d.vertexCount));
            kv("indexCount", fmt::format("{}", d.indexCount));
            kv("meshletCount", fmt::format("{}", d.meshletCount));
            kv("submeshCount", fmt::format("{}", d.submeshCount));
            kv("materialCount", fmt::format("{}", d.materialCount));
            kv("vertexStride", fmt::format("{} B", d.vertexStride));
            kv("indexWidth", fmt::format("{} B", d.indexWidth));
            kv("flags", fmt::format("0x{:02x}{}", d.flags,
                                    (d.flags & MeshFlag_HasTangents) ? " (tangents)" : ""));
            kv("meshletMaxVerts", fmt::format("{}", d.meshletMaxVerts));
            kv("meshletMaxTris", fmt::format("{}", d.meshletMaxTris));
            ImGui::EndTable();
        }
        break;
    }
    case ChunkId::Bounds:
    {
        MeshBounds b{};
        std::memcpy(&b, p, std::min(sizeof(b), static_cast<size_t>(e.size)));
        if (beginKV())
        {
            kv("aabbMin", fmt::format("{:.3f} {:.3f} {:.3f}", b.aabbMin.x, b.aabbMin.y, b.aabbMin.z));
            kv("aabbMax", fmt::format("{:.3f} {:.3f} {:.3f}", b.aabbMax.x, b.aabbMax.y, b.aabbMax.z));
            kv("sphereCenter",
               fmt::format("{:.3f} {:.3f} {:.3f}", b.sphereCenter.x, b.sphereCenter.y,
                           b.sphereCenter.z));
            kv("sphereRadius", fmt::format("{:.3f}", b.sphereRadius));
            ImGui::EndTable();
        }
        break;
    }
    case ChunkId::Vertices:
    {
        const uint32_t stride =
            v.meshDesc.vertexStride ? v.meshDesc.vertexStride : static_cast<uint32_t>(sizeof(GpuVertex));
        const int count = static_cast<int>(e.size / stride);
        ImGui::Text("%d vertices, stride %u B (normal/tangent are octahedral int16)", count, stride);
        ArrayTable("vtxs", {"#", "position", "uv", "normal", "tangent"}, count, [&](int i) {
            GpuVertex gv{};
            std::memcpy(&gv, p + static_cast<size_t>(i) * stride, sizeof(gv));
            ImGui::TableSetColumnIndex(0);
            ImGui::Text("%d", i);
            ImGui::TableSetColumnIndex(1);
            ImGui::Text("%.3f %.3f %.3f", gv.position[0], gv.position[1], gv.position[2]);
            ImGui::TableSetColumnIndex(2);
            ImGui::Text("%.3f %.3f", gv.uv[0], gv.uv[1]);
            ImGui::TableSetColumnIndex(3);
            ImGui::Text("%d %d", gv.normal[0], gv.normal[1]);
            ImGui::TableSetColumnIndex(4);
            ImGui::Text("%d %d", gv.tangent[0], gv.tangent[1]);
        });
        break;
    }
    case ChunkId::Indices:
    {
        const uint32_t iw    = v.meshDesc.indexWidth == 4 ? 4u : 2u;
        const int      total = static_cast<int>(e.size / iw);
        const int      tris  = total / 3;
        ImGui::Text("%d indices (%d tris), %u-bit", total, tris, iw * 8);
        auto at = [&](int k) {
            uint32_t x = 0;
            std::memcpy(&x, p + static_cast<size_t>(k) * iw, iw);
            return x;
        };
        ArrayTable("idxs", {"tri", "i0", "i1", "i2"}, tris, [&](int t) {
            ImGui::TableSetColumnIndex(0);
            ImGui::Text("%d", t);
            ImGui::TableSetColumnIndex(1);
            ImGui::Text("%u", at(t * 3 + 0));
            ImGui::TableSetColumnIndex(2);
            ImGui::Text("%u", at(t * 3 + 1));
            ImGui::TableSetColumnIndex(3);
            ImGui::Text("%u", at(t * 3 + 2));
        });
        break;
    }
    case ChunkId::Meshlets:
    {
        const int count = static_cast<int>(e.size / sizeof(Meshlet));
        ArrayTable("mlet", {"#", "vtxOffset", "vtxCount", "triOffset", "triCount"}, count, [&](int i) {
            Meshlet m{};
            std::memcpy(&m, p + static_cast<size_t>(i) * sizeof(m), sizeof(m));
            ImGui::TableSetColumnIndex(0);
            ImGui::Text("%d", i);
            ImGui::TableSetColumnIndex(1);
            ImGui::Text("%u", m.vertexOffset);
            ImGui::TableSetColumnIndex(2);
            ImGui::Text("%u", m.vertexCount);
            ImGui::TableSetColumnIndex(3);
            ImGui::Text("%u", m.triangleOffset);
            ImGui::TableSetColumnIndex(4);
            ImGui::Text("%u", m.triangleCount);
        });
        break;
    }
    case ChunkId::MeshletVertices:
    {
        const int count = static_cast<int>(e.size / sizeof(uint32_t));
        ArrayTable("mlvr", {"#", "vertex"}, count, [&](int i) {
            uint32_t x = 0;
            std::memcpy(&x, p + static_cast<size_t>(i) * 4, 4);
            ImGui::TableSetColumnIndex(0);
            ImGui::Text("%d", i);
            ImGui::TableSetColumnIndex(1);
            ImGui::Text("%u", x);
        });
        break;
    }
    case ChunkId::MeshletTriangles:
    {
        const int count = static_cast<int>(e.size / sizeof(MeshletTriangle));
        ArrayTable("mltr", {"#", "i0", "i1", "i2"}, count, [&](int i) {
            MeshletTriangle t{};
            std::memcpy(&t, p + static_cast<size_t>(i) * sizeof(t), sizeof(t));
            ImGui::TableSetColumnIndex(0);
            ImGui::Text("%d", i);
            ImGui::TableSetColumnIndex(1);
            ImGui::Text("%u", t.i0);
            ImGui::TableSetColumnIndex(2);
            ImGui::Text("%u", t.i1);
            ImGui::TableSetColumnIndex(3);
            ImGui::Text("%u", t.i2);
        });
        break;
    }
    case ChunkId::MeshletBounds:
    {
        const int count = static_cast<int>(e.size / sizeof(MeshletBounds));
        ArrayTable("mlbn", {"#", "center", "radius", "coneAxis", "coneCutoff"}, count, [&](int i) {
            MeshletBounds b{};
            std::memcpy(&b, p + static_cast<size_t>(i) * sizeof(b), sizeof(b));
            ImGui::TableSetColumnIndex(0);
            ImGui::Text("%d", i);
            ImGui::TableSetColumnIndex(1);
            ImGui::Text("%.2f %.2f %.2f", b.center.x, b.center.y, b.center.z);
            ImGui::TableSetColumnIndex(2);
            ImGui::Text("%.3f", b.radius);
            ImGui::TableSetColumnIndex(3);
            ImGui::Text("%.2f %.2f %.2f", b.coneAxis.x, b.coneAxis.y, b.coneAxis.z);
            ImGui::TableSetColumnIndex(4);
            ImGui::Text("%.3f", b.coneCutoff);
        });
        break;
    }
    case ChunkId::Materials:
    {
        const int count = static_cast<int>(e.size / sizeof(uint64_t));
        ArrayTable("mtrl", {"slot", "material hash"}, count, [&](int i) {
            uint64_t h = 0;
            std::memcpy(&h, p + static_cast<size_t>(i) * 8, 8);
            ImGui::TableSetColumnIndex(0);
            ImGui::Text("%d", i);
            ImGui::TableSetColumnIndex(1);
            ImGui::Text("0x%016llx", (unsigned long long)h);
        });
        break;
    }
    case ChunkId::SubMeshes:
    {
        const int count = static_cast<int>(e.size / sizeof(SubMesh));
        ArrayTable("subm",
                   {"#", "firstIdx", "idxCount", "firstMlet", "mletCount", "matSlot", "baseVtx",
                    "aabbMin", "aabbMax"},
                   count, [&](int i) {
                       SubMesh s{};
                       std::memcpy(&s, p + static_cast<size_t>(i) * sizeof(s), sizeof(s));
                       ImGui::TableSetColumnIndex(0);
                       ImGui::Text("%d", i);
                       ImGui::TableSetColumnIndex(1);
                       ImGui::Text("%u", s.firstIndex);
                       ImGui::TableSetColumnIndex(2);
                       ImGui::Text("%u", s.indexCount);
                       ImGui::TableSetColumnIndex(3);
                       ImGui::Text("%u", s.firstMeshlet);
                       ImGui::TableSetColumnIndex(4);
                       ImGui::Text("%u", s.meshletCount);
                       ImGui::TableSetColumnIndex(5);
                       if (s.materialSlot == kNoMaterial)
                           ImGui::TextDisabled("none");
                       else
                           ImGui::Text("%u", s.materialSlot);
                       ImGui::TableSetColumnIndex(6);
                       ImGui::Text("%u", s.baseVertex);
                       ImGui::TableSetColumnIndex(7);
                       ImGui::Text("%.2f %.2f %.2f", s.bounds.aabbMin.x, s.bounds.aabbMin.y,
                                   s.bounds.aabbMin.z);
                       ImGui::TableSetColumnIndex(8);
                       ImGui::Text("%.2f %.2f %.2f", s.bounds.aabbMax.x, s.bounds.aabbMax.y,
                                   s.bounds.aabbMax.z);
                   });
        break;
    }
    case ChunkId::Skin:
    {
        const int count = static_cast<int>(e.size / sizeof(GpuSkinVertex));
        ArrayTable("skin", {"#", "joints", "weights"}, count, [&](int i) {
            GpuSkinVertex s{};
            std::memcpy(&s, p + static_cast<size_t>(i) * sizeof(s), sizeof(s));
            ImGui::TableSetColumnIndex(0);
            ImGui::Text("%d", i);
            ImGui::TableSetColumnIndex(1);
            ImGui::Text("%u %u %u %u", s.joints[0], s.joints[1], s.joints[2], s.joints[3]);
            ImGui::TableSetColumnIndex(2);
            ImGui::Text("%.3f %.3f %.3f %.3f", s.weights[0], s.weights[1], s.weights[2],
                        s.weights[3]);
        });
        break;
    }
    case ChunkId::Skeleton:
    {
        const int count = static_cast<int>(e.size / sizeof(GpuJoint));
        ArrayTable("skel", {"joint", "parent", "bindT", "bindR (xyzw)", "bindS"}, count, [&](int i) {
            GpuJoint j{};
            std::memcpy(&j, p + static_cast<size_t>(i) * sizeof(j), sizeof(j));
            ImGui::TableSetColumnIndex(0);
            ImGui::Text("%d", i);
            ImGui::TableSetColumnIndex(1);
            if (j.parent < 0)
                ImGui::TextDisabled("root");
            else
                ImGui::Text("%d", j.parent);
            ImGui::TableSetColumnIndex(2);
            ImGui::Text("%.3f %.3f %.3f", j.bindT[0], j.bindT[1], j.bindT[2]);
            ImGui::TableSetColumnIndex(3);
            ImGui::Text("%.3f %.3f %.3f %.3f", j.bindR[0], j.bindR[1], j.bindR[2], j.bindR[3]);
            ImGui::TableSetColumnIndex(4);
            ImGui::Text("%.3f %.3f %.3f", j.bindS[0], j.bindS[1], j.bindS[2]);
        });
        break;
    }
    case ChunkId::LodIndices:
    {
        const int count = static_cast<int>(e.size / sizeof(uint32_t));
        ArrayTable("lodi", {"#", "vertex index"}, count, [&](int i) {
            uint32_t x = 0;
            std::memcpy(&x, p + static_cast<size_t>(i) * 4, 4);
            ImGui::TableSetColumnIndex(0);
            ImGui::Text("%d", i);
            ImGui::TableSetColumnIndex(1);
            ImGui::Text("%u", x);
        });
        break;
    }
    case ChunkId::LodTable:
    {
        LodTableHeader lh{};
        if (e.size >= sizeof(lh))
            std::memcpy(&lh, p, sizeof(lh));
        ImGui::Text("lodCount %u, submeshCount %u  (rows are [submesh][lod])", lh.lodCount,
                    lh.submeshCount);
        const uint8_t *arr   = p + sizeof(lh);
        const int      count = static_cast<int>((e.size - sizeof(lh)) / sizeof(MeshLod));
        ArrayTable("lodt", {"#", "firstIndex", "indexCount"}, count, [&](int i) {
            MeshLod ml{};
            std::memcpy(&ml, arr + static_cast<size_t>(i) * sizeof(ml), sizeof(ml));
            ImGui::TableSetColumnIndex(0);
            ImGui::Text("%d", i);
            ImGui::TableSetColumnIndex(1);
            ImGui::Text("%u", ml.firstIndex);
            ImGui::TableSetColumnIndex(2);
            ImGui::Text("%u%s", ml.indexCount, ml.indexCount == 0 ? "  (reuse prev LOD)" : "");
        });
        break;
    }
    default:
        ImGui::TextDisabled("no decoder for this chunk — raw bytes:");
        DrawHexSpan(p, static_cast<size_t>(e.size));
        break;
    }
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
                // Scope per-row widget ids so the baseColor ColorButton (same "##c"
                // label every row) doesn't collide and assert on hover.
                ImGui::PushID(static_cast<int>(i));
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
                ImGui::PopID();
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

// Pack visualization, defined below; the Inspector renders it when the pack node is
// selected, so the .hpack opens like any other entry.
struct PackStats;
void DrawPackTabs(const Source &src, const PackStats &s, View &v);

void DrawInspector(viewer::GpuContext &gpu, const Source &src, const PackStats &packStats, View &v)
{
    ImGui::Begin("Inspector");

    // The pack node: show its overview/visualization right here in the Inspector.
    if (v.selected == kPackSelected)
    {
        ImGui::Text("%s", fs::path(src.packPath).filename().string().c_str());
        ImGui::SameLine();
        ImGui::TextDisabled("(pack)");
        ImGui::Separator();
        DrawPackTabs(src, packStats, v);
        ImGui::End();
        return;
    }

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
        // Preview tab (3D view + stats) plus one tab per .hmesh chunk (decoded tables).
        if (ImGui::BeginTabBar("meshtabs"))
        {
            if (ImGui::BeginTabItem("Preview"))
            {
                const viewer::MeshCpu &m     = v.mesh;
                const uint64_t         tris0 = m.lodIndices.empty() ? 0 : m.lodIndices[0].size() / 3;
                ImGui::Text("%u verts | %llu tris | %u submeshes | %u materials | %zu LOD(s)",
                            m.vertexCount, (unsigned long long)tris0, m.submeshCount,
                            m.materialCount, m.lodIndices.size());
                ImGui::Text("aabb %.2f x %.2f x %.2f   r=%.2f", m.aabbMax[0] - m.aabbMin[0],
                            m.aabbMax[1] - m.aabbMin[1], m.aabbMax[2] - m.aabbMin[2], m.radius);
                ImGui::Separator();
                viewer::DrawMeshPreview(gpu, v.mesh, v.meshCam, v.meshRender);
                ImGui::EndTabItem();
            }
            for (size_t ci = 0; ci < v.meshChunks.size(); ++ci)
            {
                const auto       &e     = v.meshChunks[ci];
                const std::string label = fmt::format("{}##chunk{}", FourCCStr(e.fourcc), ci);
                if (ImGui::BeginTabItem(label.c_str()))
                {
                    DrawMeshChunk(v, e);
                    ImGui::EndTabItem();
                }
            }
            ImGui::EndTabBar();
        }
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

// ---------------------------------------------------------------------------
// Pack visualization: a "Pack" window (shown only when the source is a .hpack)
// with three tabs — Summary (stats + per-kind bars), Layout (a byte-accurate
// memory map), and Treemap (entries sized by bytes, grouped by kind). Everything
// is derived from the in-memory TOC, and clicking any block selects that entry so
// it loads in the Inspector.
// ---------------------------------------------------------------------------

constexpr assetc::PackKind kPackKindOrder[8] = {
    assetc::PackKind::Mesh,      assetc::PackKind::Material, assetc::PackKind::Manifest,
    assetc::PackKind::Animation, assetc::PackKind::Texture,  assetc::PackKind::Shader,
    assetc::PackKind::Font,      assetc::PackKind::Other};

ImVec4 PackKindColor(assetc::PackKind k)
{
    using K = assetc::PackKind;
    switch (k)
    {
    case K::Mesh:      return ImVec4(0.35f, 0.59f, 0.92f, 1.0f);
    case K::Material:  return ImVec4(0.92f, 0.59f, 0.27f, 1.0f);
    case K::Manifest:  return ImVec4(0.71f, 0.47f, 0.86f, 1.0f);
    case K::Animation: return ImVec4(0.35f, 0.78f, 0.47f, 1.0f);
    case K::Texture:   return ImVec4(0.27f, 0.75f, 0.78f, 1.0f);
    case K::Shader:    return ImVec4(0.88f, 0.80f, 0.35f, 1.0f);
    case K::Font:      return ImVec4(0.88f, 0.47f, 0.67f, 1.0f);
    case K::Other:     break;
    }
    return ImVec4(0.51f, 0.51f, 0.51f, 1.0f);
}

const char *PackKindWord(assetc::PackKind k)
{
    using K = assetc::PackKind;
    switch (k)
    {
    case K::Mesh:      return "meshes";
    case K::Material:  return "materials";
    case K::Manifest:  return "manifests";
    case K::Animation: return "animations";
    case K::Texture:   return "textures";
    case K::Shader:    return "shaders";
    case K::Font:      return "fonts";
    case K::Other:     break;
    }
    return "other";
}

// Per-entry TOC record size (mirrors WritePack: offset u64 + size u64 + kind u8 +
// pathLen u16 + path), so we can show the TOC region's true byte cost.
uint64_t PackTocRecordBytes(size_t pathLen)
{
    return sizeof(uint64_t) + sizeof(uint64_t) + sizeof(uint8_t) + sizeof(uint16_t) + pathLen;
}

struct PackStats
{
    bool             valid        = false;
    uint64_t         fileSize     = 0;
    uint64_t         headerBytes  = 24; // magic+version+count+flags (4xu32) + tocBytes (u64)
    uint64_t         tocBytes     = 0;
    uint64_t         payloadBytes = 0;
    uint64_t         paddingBytes = 0;
    uint64_t         kindCount[8] = {};
    uint64_t         kindBytes[8] = {};
    std::vector<int> byKind[8]; // packToc indices per kind, sorted by size descending
};

PackStats ComputePackStats(const Source &src)
{
    PackStats s;
    if (!src.packAvailable)
        return s;
    std::error_code ec;
    s.fileSize = static_cast<uint64_t>(fs::file_size(src.packPath, ec));
    if (ec || s.fileSize == 0)
        return s;
    for (size_t i = 0; i < src.packToc.size(); ++i)
    {
        const auto &pe = src.packToc[i];
        s.tocBytes += PackTocRecordBytes(pe.path.size());
        s.payloadBytes += pe.size;
        int k = static_cast<int>(pe.kind);
        if (k < 0 || k >= 8)
            k = 0;
        ++s.kindCount[k];
        s.kindBytes[k] += pe.size;
        s.byKind[k].push_back(static_cast<int>(i));
    }
    for (int k = 0; k < 8; ++k)
        std::sort(s.byKind[k].begin(), s.byKind[k].end(),
                  [&](int a, int b) { return src.packToc[a].size > src.packToc[b].size; });
    const uint64_t used = s.headerBytes + s.tocBytes + s.payloadBytes;
    s.paddingBytes      = s.fileSize > used ? s.fileSize - used : 0;
    s.valid             = true;
    return s;
}

// The pack viz indexes into packToc; selecting maps that to the browseable list.
void SelectPackEntry(const Source &src, View &v, int packIdx);

// Select an entry from a viz click, resetting the per-selection texture/zoom state
// like the browser does, so the Inspector reloads it.
void SelectEntry(View &v, int idx)
{
    if (v.selected == idx)
        return;
    v.selected = idx;
    v.mip = v.face = v.layer = 0;
    v.zoom                   = 1.0f;
    v.dirty                  = true;
}

void SelectPackEntry(const Source &src, View &v, int packIdx)
{
    if (packIdx < 0 || packIdx >= static_cast<int>(src.packToList.size()))
        return;
    const int li = src.packToList[packIdx];
    if (li >= 0)
        SelectEntry(v, li);
}

// Is the pack-TOC entry `packIdx` the one currently selected in the list?
bool PackEntrySelected(const Source &src, const View &v, int packIdx)
{
    return packIdx >= 0 && packIdx < static_cast<int>(src.packToList.size()) &&
           src.packToList[packIdx] == v.selected && v.selected >= 0;
}

void DrawPackSummary(const Source &src, const PackStats &s, View &v)
{
    (void)v;
    ImGui::Text("%s", fs::path(src.packPath).filename().string().c_str());
    ImGui::SameLine();
    ImGui::TextDisabled("HPAK v%u  |  %s  |  %zu entries", assetc::PackVersion,
                        HumanSize(s.fileSize).c_str(), src.packToc.size());

    // Region bar: header / TOC / payload / padding, proportional to bytes.
    const ImVec2 p0 = ImGui::GetCursorScreenPos();
    const float  w  = ImGui::GetContentRegionAvail().x;
    const float  h  = 14.0f;
    ImGui::InvisibleButton("regionbar", ImVec2(w, h));
    ImDrawList *dl = ImGui::GetWindowDrawList();
    struct Seg
    {
        uint64_t    bytes;
        ImU32       col;
    } segs[] = {
        {s.headerBytes, IM_COL32(120, 120, 128, 255)},
        {s.tocBytes, IM_COL32(95, 95, 150, 255)},
        {s.payloadBytes, IM_COL32(80, 140, 90, 255)},
        {s.paddingBytes, IM_COL32(58, 58, 64, 255)},
    };
    float x = p0.x;
    for (const auto &sg : segs)
    {
        const float sw = w * static_cast<float>((double)sg.bytes / (double)s.fileSize);
        dl->AddRectFilled(ImVec2(x, p0.y), ImVec2(x + sw, p0.y + h), sg.col);
        x += sw;
    }
    dl->AddRect(p0, ImVec2(p0.x + w, p0.y + h), IM_COL32(0, 0, 0, 120));
    ImGui::Text("header %s   TOC %s   payload %s   padding %s (%.1f%% overhead)",
                HumanSize(s.headerBytes).c_str(), HumanSize(s.tocBytes).c_str(),
                HumanSize(s.payloadBytes).c_str(), HumanSize(s.paddingBytes).c_str(),
                s.fileSize ? 100.0 * (s.headerBytes + s.tocBytes + s.paddingBytes) / s.fileSize : 0.0);
    ImGui::Separator();

    // Per-kind size bars.
    if (ImGui::BeginTable("kinds", 3,
                          ImGuiTableFlags_RowBg | ImGuiTableFlags_SizingStretchProp |
                              ImGuiTableFlags_ScrollY))
    {
        ImGui::TableSetupColumn("kind", ImGuiTableColumnFlags_WidthFixed, 110.0f);
        ImGui::TableSetupColumn("share");
        ImGui::TableSetupColumn("size", ImGuiTableColumnFlags_WidthFixed, 140.0f);
        for (assetc::PackKind k : kPackKindOrder)
        {
            const int ki = static_cast<int>(k);
            if (!s.kindCount[ki])
                continue;
            ImGui::TableNextRow();
            ImGui::TableSetColumnIndex(0);
            ImGui::TextColored(PackKindColor(k), "%s", PackKindWord(k));
            ImGui::TableSetColumnIndex(1);
            const float frac =
                s.payloadBytes ? static_cast<float>((double)s.kindBytes[ki] / s.payloadBytes) : 0.0f;
            ImGui::PushStyleColor(ImGuiCol_PlotHistogram, PackKindColor(k));
            ImGui::ProgressBar(frac, ImVec2(-FLT_MIN, 0), "");
            ImGui::PopStyleColor();
            ImGui::TableSetColumnIndex(2);
            ImGui::Text("%s (%llu)", HumanSize(s.kindBytes[ki]).c_str(),
                        (unsigned long long)s.kindCount[ki]);
        }
        ImGui::EndTable();
    }
}

// A small colored legend chip + label, laid out inline.
void LegendChip(ImU32 col, const char *label, bool &first)
{
    if (!first)
        ImGui::SameLine();
    first          = false;
    const ImVec2 q = ImGui::GetCursorScreenPos();
    ImGui::Dummy(ImVec2(12, 12));
    ImGui::GetWindowDrawList()->AddRectFilled(q, ImVec2(q.x + 12, q.y + 12), col);
    ImGui::SameLine();
    ImGui::TextUnformatted(label);
}

void DrawPackLayout(const Source &src, const PackStats &s, View &v)
{
    ImGui::TextDisabled("offset 0 -> %s   (width = bytes, gaps = alignment padding; click to inspect)",
                        HumanSize(s.fileSize).c_str());

    const ImVec2 p0   = ImGui::GetCursorScreenPos();
    const float  w    = ImGui::GetContentRegionAvail().x;
    const float  barH = 48.0f;
    ImGui::InvisibleButton("bytemap", ImVec2(w, barH));
    const bool   hov     = ImGui::IsItemHovered();
    const bool   clicked = hov && ImGui::IsMouseClicked(ImGuiMouseButton_Left);
    const ImVec2 mouse   = ImGui::GetIO().MousePos;
    ImDrawList  *dl      = ImGui::GetWindowDrawList();

    auto X = [&](uint64_t pos) {
        return p0.x + w * static_cast<float>((double)pos / (double)s.fileSize);
    };

    dl->AddRectFilled(p0, ImVec2(p0.x + w, p0.y + barH), IM_COL32(30, 30, 34, 255)); // bg = padding
    dl->AddRectFilled(p0, ImVec2(X(s.headerBytes), p0.y + barH), IM_COL32(120, 120, 128, 255));
    dl->AddRectFilled(ImVec2(X(s.headerBytes), p0.y),
                      ImVec2(X(s.headerBytes + s.tocBytes), p0.y + barH), IM_COL32(95, 95, 150, 255));

    int hovered = -1;
    for (size_t i = 0; i < src.packToc.size(); ++i)
    {
        const auto &pe = src.packToc[i];
        float       x0 = X(pe.offset);
        float       x1 = X(pe.offset + pe.size);
        if (x1 < x0 + 1.0f)
            x1 = x0 + 1.0f; // keep sub-pixel entries visible
        ImU32 col = PackEntrySelected(src, v, static_cast<int>(i))
                        ? IM_COL32(255, 255, 255, 255)
                        : ImGui::GetColorU32(PackKindColor(pe.kind));
        dl->AddRectFilled(ImVec2(x0, p0.y), ImVec2(x1, p0.y + barH), col);
        // Clear divider at each item's start, so adjacent (esp. same-kind) blocks
        // read as distinct entries rather than one fused band.
        dl->AddLine(ImVec2(x0, p0.y), ImVec2(x0, p0.y + barH), IM_COL32(16, 16, 20, 235), 1.0f);
        if (hov && mouse.x >= x0 && mouse.x < x1)
            hovered = static_cast<int>(i);
    }
    dl->AddRect(p0, ImVec2(p0.x + w, p0.y + barH), IM_COL32(0, 0, 0, 150));

    if (hovered >= 0)
    {
        const auto &pe = src.packToc[hovered];
        ImGui::BeginTooltip();
        ImGui::TextColored(PackKindColor(pe.kind), "%s", PackKindWord(pe.kind));
        ImGui::TextUnformatted(pe.path.c_str());
        ImGui::Text("%s   @%llu", HumanSize(pe.size).c_str(), (unsigned long long)pe.offset);
        ImGui::EndTooltip();
        if (clicked)
            SelectPackEntry(src, v, hovered);
    }

    ImGui::Spacing();
    bool first = true;
    LegendChip(IM_COL32(120, 120, 128, 255), "header", first);
    LegendChip(IM_COL32(95, 95, 150, 255), "TOC", first);
    for (assetc::PackKind k : kPackKindOrder)
        if (s.kindCount[static_cast<int>(k)])
            LegendChip(ImGui::GetColorU32(PackKindColor(k)), PackKindWord(k), first);
}

void DrawPackTreemap(const Source &src, const PackStats &s, View &v)
{
    ImGui::TextDisabled("payload only; columns = kind by size, tiles = entries by size (click to inspect)");

    const ImVec2 p0    = ImGui::GetCursorScreenPos();
    ImVec2       avail = ImGui::GetContentRegionAvail();
    const float  W     = avail.x;
    const float  H     = std::max(avail.y, 60.0f);
    ImGui::InvisibleButton("treemap", ImVec2(W, H));
    const bool   hov     = ImGui::IsItemHovered();
    const bool   clicked = hov && ImGui::IsMouseClicked(ImGuiMouseButton_Left);
    const ImVec2 mouse   = ImGui::GetIO().MousePos;
    ImDrawList  *dl      = ImGui::GetWindowDrawList();

    int   hovered = -1;
    float x       = p0.x;
    for (assetc::PackKind k : kPackKindOrder)
    {
        const int ki = static_cast<int>(k);
        if (!s.kindBytes[ki])
            continue;
        const float colW =
            W * (s.payloadBytes ? static_cast<float>((double)s.kindBytes[ki] / s.payloadBytes) : 0.0f);
        const ImVec4 base = PackKindColor(k);
        float        y    = p0.y;
        int          n    = 0;
        for (int ei : s.byKind[ki]) // ei indexes packToc
        {
            const auto &pe = src.packToc[ei];
            float       th = H * static_cast<float>((double)pe.size / (double)s.kindBytes[ki]);
            if (y + th > p0.y + H)
                th = p0.y + H - y;
            const ImVec2 a(x, y), b(x + colW, y + th);
            ImVec4       c = base;
            if (n & 1) { c.x *= 0.82f; c.y *= 0.82f; c.z *= 0.82f; }
            const ImU32 col = PackEntrySelected(src, v, ei) ? IM_COL32(255, 255, 255, 255)
                                                            : ImGui::GetColorU32(c);
            dl->AddRectFilled(a, b, col);
            dl->AddRect(a, b, IM_COL32(18, 18, 22, 180));
            if (colW > 46.0f && th > 16.0f)
            {
                const std::string nm = fs::path(pe.path).filename().string();
                dl->PushClipRect(a, b, true);
                dl->AddText(ImVec2(a.x + 3, a.y + 2), IM_COL32(12, 12, 16, 255), nm.c_str());
                dl->PopClipRect();
            }
            if (hov && mouse.x >= a.x && mouse.x < b.x && mouse.y >= a.y && mouse.y < b.y)
                hovered = ei;
            y += th;
            ++n;
        }
        if (colW > 40.0f)
        {
            dl->PushClipRect(ImVec2(x, p0.y), ImVec2(x + colW, p0.y + H), true);
            dl->AddText(ImVec2(x + 3, p0.y + 2), IM_COL32(255, 255, 255, 235), PackKindWord(k));
            dl->PopClipRect();
        }
        x += colW;
    }

    if (hovered >= 0)
    {
        const auto &pe = src.packToc[hovered];
        ImGui::BeginTooltip();
        ImGui::TextColored(PackKindColor(pe.kind), "%s", PackKindWord(pe.kind));
        ImGui::TextUnformatted(pe.path.c_str());
        ImGui::TextUnformatted(HumanSize(pe.size).c_str());
        ImGui::EndTooltip();
        if (clicked)
            SelectPackEntry(src, v, hovered);
    }
}

// Draws the three pack-visualization tabs inline (no window) into the current
// window — the Inspector calls this when the pack node is selected.
void DrawPackTabs(const Source &src, const PackStats &s, View &v)
{
    if (!s.valid)
    {
        ImGui::TextDisabled("pack size unavailable");
        return;
    }
    if (ImGui::BeginTabBar("packtabs"))
    {
        if (ImGui::BeginTabItem("Summary"))
        {
            DrawPackSummary(src, s, v);
            ImGui::EndTabItem();
        }
        if (ImGui::BeginTabItem("Layout"))
        {
            DrawPackLayout(src, s, v);
            ImGui::EndTabItem();
        }
        if (ImGui::BeginTabItem("Treemap"))
        {
            DrawPackTreemap(src, s, v);
            ImGui::EndTabItem();
        }
        ImGui::EndTabBar();
    }
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

    // Pack layout/size stats, computed once from the TOC (empty for a loose dir).
    const PackStats packStats = ComputePackStats(src);

    View v;
    // A pack opens on its own overview node; a loose dir opens on the first texture
    // (else first entry) so there's something on screen.
    if (src.isPack)
        v.selected = kPackSelected;
    else
    {
        for (int i = 0; i < (int)src.entries.size(); ++i)
            if (src.entries[i].kind == PackKind::Texture)
            {
                v.selected = i;
                break;
            }
        if (v.selected < 0 && !src.entries.empty())
            v.selected = 0;
    }
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
        DrawInspector(gpu, src, packStats, v);
        gpu.EndFrame();
    }

    if (v.tex.valid)
        gpu.DestroyTexture(v.tex);
    v.meshRender.Reset(gpu);
    gpu.Shutdown();
    return 0;
}

} // namespace assetc
