#include "pack.hpp"

#include "../deps/fmt.hpp"

#include <algorithm>
#include <array>
#include <bit>
#include <filesystem>
#include <fstream>

namespace fs = std::filesystem;

static_assert(std::endian::native == std::endian::little,
              "assetc currently supports little-endian targets only");

namespace
{
constexpr uint64_t kAlign = 16;

uint64_t AlignUp(uint64_t v) noexcept
{
    return (v + kAlign - 1) & ~(kAlign - 1);
}

template <typename T> void Put(std::ofstream &out, const T &v)
{
    out.write(reinterpret_cast<const char *>(&v), sizeof(v));
}
template <typename T> bool Get(std::ifstream &in, T &v)
{
    return static_cast<bool>(in.read(reinterpret_cast<char *>(&v), sizeof(v)));
}

// Per-entry on-disk TOC record size for a given path length.
uint64_t TocRecordBytes(size_t pathLen) noexcept
{
    return sizeof(uint64_t) /*offset*/ + sizeof(uint64_t) /*size*/ + sizeof(uint8_t) /*kind*/ +
           sizeof(uint16_t) /*pathLen*/ + pathLen;
}
} // namespace

assetc::PackKind assetc::PackKindOf(const std::string &path) noexcept
{
    if (path.ends_with(".hmesh")) return PackKind::Mesh;
    if (path.ends_with(".hmat")) return PackKind::Material;
    if (path.ends_with(".hman")) return PackKind::Manifest;
    if (path.ends_with(".hanim")) return PackKind::Animation;
    if (path.ends_with(".ktx2")) return PackKind::Texture;
    if (path.ends_with(".spv")) return PackKind::Shader;
    return PackKind::Other;
}

int assetc::WritePack(const std::string &outputDir, const std::string &packPath)
{
    if (!fs::exists(outputDir) || !fs::is_directory(outputDir))
    {
        fmtx::Error(fmt::format("pack: output dir does not exist: {}", outputDir));
        return 1;
    }

    // Collect eligible files (skip the build cache and any existing pack), sorted
    // by relative path for deterministic output.
    struct Item
    {
        std::string rel;
        std::string abs;
        uint64_t    size;
    };
    std::vector<Item> items;
    std::error_code   ec;
    const auto        packAbs = fs::weakly_canonical(packPath, ec);
    for (fs::recursive_directory_iterator it(outputDir, fs::directory_options::skip_permission_denied,
                                             ec),
         end;
         it != end; it.increment(ec))
    {
        if (ec || !it->is_regular_file())
            continue;
        const auto &p   = it->path();
        const auto  nm  = p.filename().string();
        if (nm == ".assetc-cache" || nm.ends_with(".hpack"))
            continue;
        std::error_code cec;
        if (fs::weakly_canonical(p, cec) == packAbs)
            continue; // never pack ourselves
        const std::string rel = fs::relative(p, outputDir, ec).generic_string();
        items.push_back({rel, p.string(), static_cast<uint64_t>(fs::file_size(p, ec))});
    }
    std::sort(items.begin(), items.end(),
              [](const Item &a, const Item &b) { return a.rel < b.rel; });

    // Layout pass: header + TOC, then 16-aligned payloads.
    uint64_t tocBytes = 0;
    for (const auto &it : items)
        tocBytes += TocRecordBytes(it.rel.size());
    uint64_t cursor = AlignUp(sizeof(uint32_t) * 4 + sizeof(uint64_t) + tocBytes);
    std::vector<uint64_t> offsets(items.size());
    for (size_t i = 0; i < items.size(); ++i)
    {
        offsets[i] = cursor;
        cursor     = AlignUp(cursor + items[i].size);
    }

    std::ofstream out(packPath, std::ios::binary | std::ios::trunc);
    if (!out)
    {
        fmtx::Error(fmt::format("pack: open failed: {}", packPath));
        return 1;
    }

    Put(out, PackMagic);
    Put(out, PackVersion);
    Put(out, static_cast<uint32_t>(items.size()));
    Put(out, static_cast<uint32_t>(0)); // flags
    Put(out, tocBytes);
    for (size_t i = 0; i < items.size(); ++i)
    {
        if (items[i].rel.size() > 0xFFFF)
        {
            fmtx::Error(fmt::format("pack: path too long: {}", items[i].rel));
            return 1;
        }
        Put(out, offsets[i]);
        Put(out, items[i].size);
        Put(out, static_cast<uint8_t>(PackKindOf(items[i].rel)));
        Put(out, static_cast<uint16_t>(items[i].rel.size()));
        out.write(items[i].rel.data(), static_cast<std::streamsize>(items[i].rel.size()));
    }

    // Payload region: copy each file at its aligned offset.
    static const std::array<char, kAlign> zeros{};
    uint64_t                              written = sizeof(uint32_t) * 4 + sizeof(uint64_t) + tocBytes;
    for (size_t i = 0; i < items.size(); ++i)
    {
        if (offsets[i] > written)
            out.write(zeros.data(), static_cast<std::streamsize>(offsets[i] - written));
        std::ifstream in(items[i].abs, std::ios::binary);
        if (!in)
        {
            fmtx::Error(fmt::format("pack: read failed: {}", items[i].abs));
            return 1;
        }
        out << in.rdbuf();
        written = offsets[i] + items[i].size;
    }

    if (!out.good())
    {
        fmtx::Error(fmt::format("pack: write failed: {}", packPath));
        return 1;
    }
    fmtx::Info(fmt::format("packed {} files -> {}", items.size(), packPath));
    return 0;
}

int assetc::ReadPackToc(const std::string &packPath, std::vector<PackEntry> &out)
{
    out.clear();
    std::ifstream in(packPath, std::ios::binary);
    uint32_t      magic = 0, version = 0, count = 0, flags = 0;
    uint64_t      tocBytes = 0;
    if (!in || !Get(in, magic) || !Get(in, version) || !Get(in, count) || !Get(in, flags) ||
        !Get(in, tocBytes))
    {
        fmtx::Error(fmt::format("pack: too small: {}", packPath));
        return 1;
    }
    if (magic != PackMagic)
    {
        fmtx::Error(fmt::format("pack: bad magic: {}", packPath));
        return 1;
    }
    if (version != PackVersion)
    {
        fmtx::Error(fmt::format("pack: bad version {} (want {}): {}", version, PackVersion, packPath));
        return 1;
    }
    out.reserve(count);
    for (uint32_t i = 0; i < count; ++i)
    {
        PackEntry e;
        uint8_t   kind    = 0;
        uint16_t  pathLen = 0;
        if (!Get(in, e.offset) || !Get(in, e.size) || !Get(in, kind) || !Get(in, pathLen))
        {
            fmtx::Error(fmt::format("pack: truncated TOC entry {}: {}", i, packPath));
            return 1;
        }
        e.kind = static_cast<PackKind>(kind);
        e.path.resize(pathLen);
        if (pathLen && !in.read(e.path.data(), pathLen))
        {
            fmtx::Error(fmt::format("pack: truncated path {}: {}", i, packPath));
            return 1;
        }
        out.push_back(std::move(e));
    }
    return 0;
}

namespace
{
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
    return i == 0 ? fmt::format("{} B", n) : fmt::format("{:.1f} {}", v, u[i]);
}

// Labels indexed by PackKind value (Other=0, Mesh=1, ... Shader=6).
const char *kKindLabels[] = {"other",               "meshes (.hmesh)",  "materials (.hmat)",
                             "manifests (.hman)",    "animations (.hanim)", "textures (.ktx2)",
                             "shaders (.spv)"};
constexpr int kKindCount = 7;
// Summary display order: assets first, "other" last.
constexpr assetc::PackKind kKindOrder[] = {
    assetc::PackKind::Mesh,    assetc::PackKind::Material,  assetc::PackKind::Manifest,
    assetc::PackKind::Animation, assetc::PackKind::Texture, assetc::PackKind::Shader,
    assetc::PackKind::Other};

// Short per-entry tag.
const char *KindTag(assetc::PackKind k)
{
    switch (k)
    {
    case assetc::PackKind::Mesh: return "mesh";
    case assetc::PackKind::Material: return "material";
    case assetc::PackKind::Manifest: return "manifest";
    case assetc::PackKind::Animation: return "anim";
    case assetc::PackKind::Texture: return "texture";
    case assetc::PackKind::Shader: return "shader";
    default: return "other";
    }
}
} // namespace

int assetc::InspectPack(const std::string &packPath)
{
    std::vector<PackEntry> toc;
    if (ReadPackToc(packPath, toc) != 0)
        return 1;

    uint64_t payload = 0;
    uint64_t tocBytes = 0;
    uint64_t kindCount[kKindCount]{};
    uint64_t kindBytes[kKindCount]{};
    for (const auto &e : toc)
    {
        payload += e.size;
        tocBytes += TocRecordBytes(e.path.size());
        const int k = static_cast<int>(e.kind);
        ++kindCount[k];
        kindBytes[k] += e.size;
    }
    std::error_code ec;
    const uint64_t  fileSize = static_cast<uint64_t>(fs::file_size(packPath, ec));
    const uint64_t  headerBytes = sizeof(uint32_t) * 4 + sizeof(uint64_t); // 24
    const uint64_t  padding =
        fileSize >= headerBytes + tocBytes + payload ? fileSize - headerBytes - tocBytes - payload : 0;

    fmt::print("{}== {} (HPAK v{}) — {} entries, {}{}\n", fmtx::CYAN, packPath, PackVersion,
               toc.size(), HumanBytes(fileSize), fmtx::RESET);
    fmt::print("  layout: {} header + {} toc + {} payload ({} alignment padding)\n",
               HumanBytes(headerBytes), HumanBytes(tocBytes), HumanBytes(payload),
               HumanBytes(padding));
    for (PackKind pk : kKindOrder)
    {
        const int k = static_cast<int>(pk);
        if (kindCount[k] > 0)
            fmt::print("  {:<20} {:>4}  {}\n", kKindLabels[k], kindCount[k],
                       HumanBytes(kindBytes[k]));
    }

    // Entries (already path-sorted in the TOC): kind | path | size | offset.
    fmt::print("\n{}== entries{}\n", fmtx::CYAN, fmtx::RESET);
    for (const auto &e : toc)
        fmt::print("  {:<9} {:<44} {:>10}  @{}\n", KindTag(e.kind), e.path, HumanBytes(e.size),
                   e.offset);

    return 0;
}

int assetc::ValidatePack(const std::string &packPath)
{
    std::vector<PackEntry> toc;
    if (ReadPackToc(packPath, toc) != 0)
        return 1;
    std::error_code ec;
    const uint64_t  fileSize = static_cast<uint64_t>(fs::file_size(packPath, ec));
    if (ec)
        return 1;
    for (const auto &e : toc)
    {
        if (e.offset > fileSize || e.size > fileSize - e.offset)
        {
            fmtx::Error(fmt::format("pack: entry \"{}\" [{},{}) past EOF {}: {}", e.path, e.offset,
                                    e.offset + e.size, fileSize, packPath));
            return 1;
        }
    }
    return 0;
}
