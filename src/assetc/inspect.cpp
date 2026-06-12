#include "inspect.hpp"

#include "assetc/pack.hpp"
#include "assetc/runtime_anim.hpp"
#include "assetc/runtime_manifest.hpp"
#include "assetc/runtime_material.hpp"
#include "assetc/runtime_mesh.hpp"

#include "../deps/fmt.hpp"

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <string>
#include <vector>

namespace fs = std::filesystem;

namespace
{

// ---- small IO helpers -------------------------------------------------------

template <typename T> bool ReadAt(std::ifstream &in, uint64_t off, T &out)
{
    in.clear();
    in.seekg(static_cast<std::streamoff>(off));
    return static_cast<bool>(in.read(reinterpret_cast<char *>(&out), sizeof(out)));
}

template <typename T> bool ReadNext(std::ifstream &in, T &out)
{
    return static_cast<bool>(in.read(reinterpret_cast<char *>(&out), sizeof(out)));
}

std::string HumanBytes(uint64_t n)
{
    const char *u[] = {"B", "KB", "MB", "GB"};
    double      v   = static_cast<double>(n);
    int         i   = 0;
    while (v >= 1024.0 && i < 3)
    {
        v /= 1024.0;
        ++i;
    }
    if (i == 0)
        return fmt::format("{} B", n);
    return fmt::format("{:.1f} {}", v, u[i]);
}

uint64_t FileSize(const fs::path &p)
{
    std::error_code ec;
    auto            s = fs::file_size(p, ec);
    return ec ? 0 : static_cast<uint64_t>(s);
}

// ---- aggregate totals -------------------------------------------------------

struct Totals
{
    uint64_t meshFiles = 0, vertices = 0, triangles = 0, indices = 0, meshlets = 0, submeshes = 0,
             meshMaterials = 0;
    uint64_t matFiles = 0, materials = 0;
    uint64_t manFiles = 0, manEntries = 0;
    uint64_t animFiles = 0, clips = 0, channels = 0;
    uint64_t texFiles = 0, texBytes = 0, texPixels = 0;
    uint64_t shaderFiles = 0;
    uint64_t totalBytes = 0;
    uint64_t errors     = 0;
};

// A finished line ready to print, paired with its path so we can sort the section.
struct Line
{
    std::string path;
    std::string text;
};

void Emit(std::vector<Line> &lines, std::string path, std::string text)
{
    lines.push_back({std::move(path), std::move(text)});
}

void PrintSection(const char *title, std::vector<Line> &lines)
{
    if (lines.empty())
        return;
    std::sort(lines.begin(), lines.end(),
              [](const Line &a, const Line &b) { return a.path < b.path; });
    fmt::print("\n{}== {} ({}){}\n", fmtx::CYAN, title, lines.size(), fmtx::RESET);
    for (const auto &l : lines)
        fmt::print("  {}\n    {}\n", l.path, l.text);
}

// ---- .hmesh -----------------------------------------------------------------

void InspectHMesh(const fs::path &p, const std::string &rel, Totals &t, std::vector<Line> &out)
{
    std::ifstream in(p, std::ios::binary);
    assetc::FileHeader hdr{};
    if (!in || !ReadNext(in, hdr) || hdr.magic != assetc::MeshMagic)
    {
        Emit(out, rel, fmt::format("{}bad/unreadable .hmesh{}", fmtx::RED, fmtx::RESET));
        ++t.errors;
        return;
    }

    // Chunk table follows the 32B header; find DESC and BNDS by fourcc.
    assetc::MeshDesc  desc{};
    assetc::MeshBounds bnds{};
    bool              haveDesc = false, haveBnds = false, haveSkin = false;
    uint32_t          jointCount = 0, lodCount = 0;
    for (uint32_t i = 0; i < hdr.chunkCount; ++i)
    {
        assetc::ChunkEntry e{};
        if (!ReadAt(in, sizeof(assetc::FileHeader) + static_cast<uint64_t>(i) * sizeof(e), e))
            break;
        if (e.fourcc == static_cast<uint32_t>(assetc::ChunkId::Desc) && e.size >= sizeof(desc))
            haveDesc = ReadAt(in, e.offset, desc);
        else if (e.fourcc == static_cast<uint32_t>(assetc::ChunkId::Bounds) &&
                 e.size >= sizeof(bnds))
            haveBnds = ReadAt(in, e.offset, bnds);
        else if (e.fourcc == static_cast<uint32_t>(assetc::ChunkId::Skin))
            haveSkin = true;
        else if (e.fourcc == static_cast<uint32_t>(assetc::ChunkId::Skeleton))
            jointCount = static_cast<uint32_t>(e.size / sizeof(assetc::GpuJoint));
        else if (e.fourcc == static_cast<uint32_t>(assetc::ChunkId::LodTable))
        {
            assetc::LodTableHeader lh{};
            if (ReadAt(in, e.offset, lh))
                lodCount = lh.lodCount;
        }
    }
    if (!haveDesc)
    {
        Emit(out, rel, fmt::format("{}missing DESC chunk{}", fmtx::RED, fmtx::RESET));
        ++t.errors;
        return;
    }

    const uint64_t tris = desc.indexCount / 3;
    const uint64_t sz   = FileSize(p);
    std::string    line = fmt::format(
        "v{} | {} verts, {} tris, {} idx@{}B | {} meshlets, {} submeshes, {} mats | "
        "stride {}B | tangents {} | {}",
        hdr.version, desc.vertexCount, tris, desc.indexCount, desc.indexWidth, desc.meshletCount,
        desc.submeshCount, desc.materialCount, desc.vertexStride,
        (desc.flags & assetc::MeshFlag_HasTangents) ? "yes" : "no", HumanBytes(sz));
    if (haveBnds)
        line += fmt::format(" | aabb {:.2f}x{:.2f}x{:.2f}, r={:.2f}",
                            bnds.aabbMax.x - bnds.aabbMin.x, bnds.aabbMax.y - bnds.aabbMin.y,
                            bnds.aabbMax.z - bnds.aabbMin.z, bnds.sphereRadius);
    if (haveSkin || jointCount > 0)
        line += fmt::format(" | skinned ({} joints)", jointCount);
    if (lodCount > 0)
        line += fmt::format(" | +{} LODs", lodCount);
    Emit(out, rel, line);

    ++t.meshFiles;
    t.vertices += desc.vertexCount;
    t.triangles += tris;
    t.indices += desc.indexCount;
    t.meshlets += desc.meshletCount;
    t.submeshes += desc.submeshCount;
    t.meshMaterials += desc.materialCount;
}

// ---- .hmat ------------------------------------------------------------------

void InspectHMat(const fs::path &p, const std::string &rel, Totals &t, std::vector<Line> &out)
{
    std::ifstream      in(p, std::ios::binary);
    assetc::MatFileHeader hdr{};
    if (!in || !ReadNext(in, hdr) || hdr.magic != assetc::MatMagic)
    {
        Emit(out, rel, fmt::format("{}bad/unreadable .hmat{}", fmtx::RED, fmtx::RESET));
        ++t.errors;
        return;
    }

    uint32_t withBase = 0, withMR = 0, withNrm = 0, withOcc = 0, withEmis = 0;
    uint32_t doubleSided = 0, opaque = 0, mask = 0, blend = 0;
    for (uint32_t i = 0; i < hdr.count; ++i)
    {
        assetc::GpuMaterial m{};
        if (!ReadNext(in, m))
            break;
        withBase += m.baseColorTex != 0;
        withMR += m.metallicRoughnessTex != 0;
        withNrm += m.normalTex != 0;
        withOcc += m.occlusionTex != 0;
        withEmis += m.emissiveTex != 0;
        doubleSided += (m.flags & assetc::MatFlag_DoubleSided) != 0;
        switch (m.flags & assetc::MatFlag_AlphaModeBits)
        {
        case assetc::MatFlag_AlphaMask: ++mask; break;
        case assetc::MatFlag_AlphaBlend: ++blend; break;
        default: ++opaque; break;
        }
    }

    Emit(out, rel,
         fmt::format("v{} | {} materials | tex: base {}, mr {}, normal {}, occ {}, emissive {} | "
                     "alpha: {} opaque/{} mask/{} blend | {} double-sided | {}",
                     hdr.version, hdr.count, withBase, withMR, withNrm, withOcc, withEmis, opaque,
                     mask, blend, doubleSided, HumanBytes(FileSize(p))));
    ++t.matFiles;
    t.materials += hdr.count;
}

// ---- .hman ------------------------------------------------------------------

void InspectHMan(const fs::path &p, const std::string &rel, Totals &t, std::vector<Line> &out)
{
    std::ifstream in(p, std::ios::binary);
    uint32_t      magic = 0, version = 0, count = 0, reserved = 0;
    if (!in || !ReadNext(in, magic) || !ReadNext(in, version) || !ReadNext(in, count) ||
        !ReadNext(in, reserved) || magic != assetc::ManMagic)
    {
        Emit(out, rel, fmt::format("{}bad/unreadable .hman{}", fmtx::RED, fmtx::RESET));
        ++t.errors;
        return;
    }

    uint32_t tex = 0, mesh = 0, mat = 0, lut = 0, srgb = 0, linear = 0;
    for (uint32_t i = 0; i < count; ++i)
    {
        uint64_t hash = 0;
        uint8_t  kind = 0, cs = 0;
        uint16_t plen = 0;
        if (!ReadNext(in, hash) || !ReadNext(in, kind) || !ReadNext(in, cs) || !ReadNext(in, plen))
            break;
        in.seekg(plen, std::ios::cur);
        switch (static_cast<assetc::ManKind>(kind))
        {
        case assetc::ManKind::Texture: ++tex; break;
        case assetc::ManKind::Mesh: ++mesh; break;
        case assetc::ManKind::Material: ++mat; break;
        case assetc::ManKind::Lut: ++lut; break;
        }
        if (static_cast<assetc::ManColorSpace>(cs) == assetc::ManColorSpace::Srgb)
            ++srgb;
        else
            ++linear;
    }

    Emit(out, rel,
         fmt::format("v{} | {} entries | kind: {} texture/{} mesh/{} material/{} lut | "
                     "colorspace: {} sRGB/{} linear | {}",
                     version, count, tex, mesh, mat, lut, srgb, linear, HumanBytes(FileSize(p))));
    ++t.manFiles;
    t.manEntries += count;
}

// ---- .hanim -----------------------------------------------------------------

void InspectHAnim(const fs::path &p, const std::string &rel, Totals &t, std::vector<Line> &out)
{
    std::vector<assetc::AnimClip> clips;
    if (assetc::ReadHAnim(p.string(), clips) != 0)
    {
        Emit(out, rel, fmt::format("{}bad/unreadable .hanim{}", fmtx::RED, fmtx::RESET));
        ++t.errors;
        return;
    }
    size_t channels = 0;
    float  maxDur   = 0.0f;
    for (const auto &c : clips)
    {
        channels += c.channels.size();
        maxDur = std::max(maxDur, c.duration);
    }
    std::string names;
    for (size_t i = 0; i < clips.size() && i < 4; ++i)
        names += (i ? ", " : "") + clips[i].name;
    if (clips.size() > 4)
        names += ", ...";
    Emit(out, rel,
         fmt::format("{} clips | {} channels | up to {:.2f}s | [{}] | {}", clips.size(), channels,
                     maxDur, names, HumanBytes(FileSize(p))));
    ++t.animFiles;
    t.clips += clips.size();
    t.channels += channels;
}

// ---- .ktx2 ------------------------------------------------------------------

const char *VkFormatName(uint32_t f)
{
    switch (f)
    {
    case 0: return "UNDEFINED (UASTC/Basis)";
    case 23: return "R8G8B8_UNORM";
    case 37: return "R8G8B8A8_UNORM";
    case 43: return "R8G8B8A8_SRGB";
    case 97: return "R16G16B16A16_SFLOAT";
    case 109: return "R32G32B32A32_SFLOAT";
    default: return nullptr;
    }
}

const char *SupercompressionName(uint32_t s)
{
    switch (s)
    {
    case 0: return "none";
    case 1: return "BasisLZ";
    case 2: return "Zstandard";
    case 3: return "ZLIB";
    default: return "?";
    }
}

void InspectKtx2(const fs::path &p, const std::string &rel, Totals &t, std::vector<Line> &out)
{
    // KTX2 header: 12-byte identifier then 9 little-endian u32 fields.
    std::ifstream in(p, std::ios::binary);
    uint8_t       id[12] = {};
    struct
    {
        uint32_t vkFormat, typeSize, w, h, d, layers, faces, levels, scheme;
    } h{};
    if (!in || !in.read(reinterpret_cast<char *>(id), sizeof(id)) ||
        !in.read(reinterpret_cast<char *>(&h), sizeof(h)) || id[0] != 0xAB || id[1] != 'K' ||
        id[2] != 'T' || id[3] != 'X')
    {
        Emit(out, rel, fmt::format("{}bad/unreadable .ktx2{}", fmtx::RED, fmtx::RESET));
        ++t.errors;
        return;
    }

    const uint32_t faces  = h.faces ? h.faces : 1;
    const uint32_t layers = h.layers ? h.layers : 1;
    const char    *kind   = faces == 6 ? "cubemap" : (h.d > 1 ? "3D" : (layers > 1 ? "array" : "2D"));

    const char *fmtName = VkFormatName(h.vkFormat);
    std::string fmtStr  = fmtName ? fmtName : fmt::format("VkFormat({})", h.vkFormat);

    std::string dims = h.d > 1 ? fmt::format("{}x{}x{}", h.w, h.h, h.d)
                               : fmt::format("{}x{}", h.w, h.h);

    Emit(out, rel,
         fmt::format("{} | {} | {} | {} mips | {} faces, {} layers | supercompress: {} | {}", dims,
                     kind, fmtStr, h.levels ? h.levels : 1, faces, layers,
                     SupercompressionName(h.scheme), HumanBytes(FileSize(p))));

    ++t.texFiles;
    t.texBytes += FileSize(p);
    t.texPixels += static_cast<uint64_t>(h.w) * h.h * std::max(1u, h.d) * faces * layers;
}

// ---- .hpack -----------------------------------------------------------------

// Per-entry on-disk TOC record size for a given path length (mirrors the layout
// written by WritePack: offset u64 + size u64 + kind u8 + pathLen u16 + path).
uint64_t PackTocRecordBytes(size_t pathLen) noexcept
{
    return sizeof(uint64_t) + sizeof(uint64_t) + sizeof(uint8_t) + sizeof(uint16_t) + pathLen;
}

// Labels indexed by PackKind value (Other=0, Mesh=1, ... Font=7).
const char *kPackKindLabels[] = {"other",                "meshes (.hmesh)",   "materials (.hmat)",
                                 "manifests (.hman)",     "animations (.hanim)", "textures (.ktx2)",
                                 "shaders (.spv)",        "fonts (.hfont)"};
constexpr int kPackKindCount = 8;
// Summary display order: assets first, "other" last.
constexpr assetc::PackKind kPackKindOrder[] = {
    assetc::PackKind::Mesh,      assetc::PackKind::Material, assetc::PackKind::Manifest,
    assetc::PackKind::Animation, assetc::PackKind::Texture,  assetc::PackKind::Shader,
    assetc::PackKind::Font,      assetc::PackKind::Other};

const char *PackKindTag(assetc::PackKind k)
{
    switch (k)
    {
    case assetc::PackKind::Mesh: return "mesh";
    case assetc::PackKind::Material: return "material";
    case assetc::PackKind::Manifest: return "manifest";
    case assetc::PackKind::Animation: return "anim";
    case assetc::PackKind::Texture: return "texture";
    case assetc::PackKind::Shader: return "shader";
    case assetc::PackKind::Font: return "font";
    default: return "other";
    }
}

} // namespace

int assetc::InspectPack(const std::string &packPath)
{
    std::vector<assetc::PackEntry> toc;
    if (assetc::ReadPackToc(packPath, toc) != 0)
        return 1;

    uint64_t payload  = 0;
    uint64_t tocBytes = 0;
    uint64_t kindCount[kPackKindCount]{};
    uint64_t kindBytes[kPackKindCount]{};
    for (const auto &e : toc)
    {
        payload += e.size;
        tocBytes += PackTocRecordBytes(e.path.size());
        const int k = static_cast<int>(e.kind);
        ++kindCount[k];
        kindBytes[k] += e.size;
    }
    std::error_code ec;
    const uint64_t  fileSize    = static_cast<uint64_t>(fs::file_size(packPath, ec));
    const uint64_t  headerBytes = sizeof(uint32_t) * 4 + sizeof(uint64_t); // 24
    const uint64_t  padding =
        fileSize >= headerBytes + tocBytes + payload ? fileSize - headerBytes - tocBytes - payload : 0;

    fmt::print("{}== {} (HPAK v{}) — {} entries, {}{}\n", fmtx::CYAN, packPath, assetc::PackVersion,
               toc.size(), HumanBytes(fileSize), fmtx::RESET);
    fmt::print("  layout: {} header + {} toc + {} payload ({} alignment padding)\n",
               HumanBytes(headerBytes), HumanBytes(tocBytes), HumanBytes(payload),
               HumanBytes(padding));
    for (assetc::PackKind pk : kPackKindOrder)
    {
        const int k = static_cast<int>(pk);
        if (kindCount[k] > 0)
            fmt::print("  {:<20} {:>4}  {}\n", kPackKindLabels[k], kindCount[k],
                       HumanBytes(kindBytes[k]));
    }

    // Entries (already path-sorted in the TOC): kind | path | size | offset.
    fmt::print("\n{}== entries{}\n", fmtx::CYAN, fmtx::RESET);
    for (const auto &e : toc)
        fmt::print("  {:<9} {:<44} {:>10}  @{}\n", PackKindTag(e.kind), e.path, HumanBytes(e.size),
                   e.offset);

    return 0;
}

int assetc::InspectRuntime(const std::string &dir)
{
    if (!fs::exists(dir) || !fs::is_directory(dir))
    {
        fmtx::Error(fmt::format("output dir does not exist: {}", dir));
        return 1;
    }

    Totals             t{};
    std::vector<Line>  meshes, mats, mans, textures, shaders, anims;

    std::error_code ec;
    for (fs::recursive_directory_iterator it(dir, fs::directory_options::skip_permission_denied, ec),
         end;
         it != end; it.increment(ec))
    {
        if (ec || !it->is_regular_file())
            continue;
        const fs::path &p   = it->path();
        const std::string rel = fs::relative(p, dir, ec).generic_string();
        const std::string name = p.filename().string();

        t.totalBytes += FileSize(p);

        if (name.ends_with(".hmesh"))
            InspectHMesh(p, rel, t, meshes);
        else if (name.ends_with(".hmat"))
            InspectHMat(p, rel, t, mats);
        else if (name.ends_with(".hman"))
            InspectHMan(p, rel, t, mans);
        else if (name.ends_with(".hanim"))
            InspectHAnim(p, rel, t, anims);
        else if (name.ends_with(".ktx2")) // covers tex_*, .lut.ktx2, .env.ktx2
            InspectKtx2(p, rel, t, textures);
        else if (name.ends_with(".spv"))
        {
            Emit(shaders, rel, HumanBytes(FileSize(p)));
            ++t.shaderFiles;
        }
    }

    fmt::print("{}Inspecting {}{}\n", fmtx::BLUE, dir, fmtx::RESET);
    PrintSection("meshes (.hmesh)", meshes);
    PrintSection("materials (.hmat)", mats);
    PrintSection("manifests (.hman)", mans);
    PrintSection("animations (.hanim)", anims);
    PrintSection("textures (.ktx2)", textures);
    PrintSection("shaders (.spv)", shaders);

    fmt::print("\n{}== summary{}\n", fmtx::GREEN, fmtx::RESET);
    fmt::print("  meshes:    {} files | {} verts | {} tris | {} idx | {} meshlets | {} submeshes\n",
               t.meshFiles, t.vertices, t.triangles, t.indices, t.meshlets, t.submeshes);
    fmt::print("  materials: {} files | {} material rows ({} mesh-referenced slots)\n", t.matFiles,
               t.materials, t.meshMaterials);
    fmt::print("  manifests: {} files | {} entries\n", t.manFiles, t.manEntries);
    fmt::print("  anims:     {} files | {} clips | {} channels\n", t.animFiles, t.clips,
               t.channels);
    fmt::print("  textures:  {} files | {} | ~{} texels\n", t.texFiles, HumanBytes(t.texBytes),
               t.texPixels);
    fmt::print("  shaders:   {} .spv files\n", t.shaderFiles);
    fmt::print("  on disk:   {} total\n", HumanBytes(t.totalBytes));
    if (t.errors)
        fmt::print("  {}{} file(s) failed to parse{}\n", fmtx::RED, t.errors, fmtx::RESET);

    return t.errors ? 1 : 0;
}
