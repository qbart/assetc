#include "runtime_manifest.hpp"

#include "../deps/fmt.hpp"

#include <algorithm>
#include <bit>
#include <fstream>
#include <ios>

static_assert(std::endian::native == std::endian::little,
              "assetc currently supports little-endian targets only");

namespace
{

template <typename T> void Put(std::ofstream &out, const T &v)
{
    out.write(reinterpret_cast<const char *>(&v), sizeof(v));
}

template <typename T> bool Get(std::ifstream &in, T &v)
{
    return static_cast<bool>(in.read(reinterpret_cast<char *>(&v), sizeof(v)));
}

} // namespace

int assetc::WriteHMan(const std::string &path, std::vector<ManifestEntry> entries)
{
    // Deterministic order: by hash, then path so duplicate detection is a linear scan.
    std::sort(entries.begin(), entries.end(), [](const ManifestEntry &a, const ManifestEntry &b) {
        if (a.hash != b.hash)
            return a.hash < b.hash;
        return a.path < b.path;
    });

    std::vector<ManifestEntry> deduped;
    deduped.reserve(entries.size());
    for (auto &e : entries)
    {
        if (!deduped.empty() && deduped.back().hash == e.hash)
        {
            if (deduped.back().path == e.path)
                continue; // same ref emitted from multiple slots/assets: collapse
            fmtx::Error(fmt::format(
                "hman hash collision: 0x{:016x} maps to both \"{}\" and \"{}\"", e.hash,
                deduped.back().path, e.path));
            return 1;
        }
        deduped.push_back(std::move(e));
    }

    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
    {
        fmtx::Error(fmt::format("open failed: {}", path));
        return 1;
    }

    Put(out, ManMagic);
    Put(out, ManVersion);
    Put(out, static_cast<uint32_t>(deduped.size()));
    Put(out, static_cast<uint32_t>(0)); // reserved

    for (const auto &e : deduped)
    {
        if (e.path.size() > 0xFFFF)
        {
            fmtx::Error(fmt::format("hman path too long ({} bytes): {}", e.path.size(), e.path));
            return 1;
        }
        Put(out, e.hash);
        Put(out, static_cast<uint8_t>(e.kind));
        Put(out, static_cast<uint8_t>(e.colorspace));
        Put(out, static_cast<uint16_t>(e.path.size()));
        out.write(e.path.data(), static_cast<std::streamsize>(e.path.size()));
    }

    if (!out.good())
    {
        fmtx::Error(fmt::format("write failed: {}", path));
        return 1;
    }
    return 0;
}

int assetc::ValidateHMan(const std::string &path)
{
    std::ifstream in(path, std::ios::binary | std::ios::ate);
    if (!in)
    {
        fmtx::Error(fmt::format("open failed: {}", path));
        return 1;
    }
    const auto size = static_cast<uint64_t>(in.tellg());
    in.seekg(0);

    uint32_t magic = 0, version = 0, count = 0, reserved = 0;
    if (size < 16 || !Get(in, magic) || !Get(in, version) || !Get(in, count) || !Get(in, reserved))
    {
        fmtx::Error(fmt::format("hman too small: {}", path));
        return 1;
    }
    if (magic != ManMagic)
    {
        fmtx::Error(fmt::format("hman bad magic: {}", path));
        return 1;
    }
    if (version != ManVersion)
    {
        fmtx::Error(
            fmt::format("hman bad version {} (want {}): {}", version, ManVersion, path));
        return 1;
    }

    uint64_t prevHash = 0;
    for (uint32_t i = 0; i < count; ++i)
    {
        uint64_t hash    = 0;
        uint8_t  kind    = 0;
        uint8_t  colorsp = 0;
        uint16_t pathLen = 0;
        if (!Get(in, hash) || !Get(in, kind) || !Get(in, colorsp) || !Get(in, pathLen))
        {
            fmtx::Error(fmt::format("hman truncated header for entry {}: {}", i, path));
            return 1;
        }
        if (i > 0 && hash < prevHash)
        {
            fmtx::Error(fmt::format("hman not sorted by hash at entry {}: {}", i, path));
            return 1;
        }
        prevHash = hash;
        in.seekg(pathLen, std::ios::cur);
        if (!in.good())
        {
            fmtx::Error(fmt::format("hman truncated path for entry {}: {}", i, path));
            return 1;
        }
    }

    if (static_cast<uint64_t>(in.tellg()) != size)
    {
        fmtx::Error(fmt::format("hman trailing bytes after {} entries: {}", count, path));
        return 1;
    }
    return 0;
}
