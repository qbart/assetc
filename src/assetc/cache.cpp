#include "cache.hpp"

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

constexpr uint64_t kFnvOffset = 0xcbf29ce484222325ULL;
constexpr uint64_t kFnvPrime  = 0x00000100000001b3ULL;
constexpr uint32_t kCacheMagic =
    static_cast<uint32_t>('H') | (static_cast<uint32_t>('C') << 8) |
    (static_cast<uint32_t>('A') << 16) | (static_cast<uint32_t>('C') << 24);
constexpr uint32_t kCacheVersion = 1;

void FnvBytes(uint64_t &h, const void *data, size_t n) noexcept
{
    const auto *p = static_cast<const uint8_t *>(data);
    for (size_t i = 0; i < n; ++i)
    {
        h ^= p[i];
        h *= kFnvPrime;
    }
}

// Stream a regular file's bytes into the running hash. Returns false if unreadable.
bool FnvFile(uint64_t &h, const fs::path &p) noexcept
{
    std::ifstream in(p, std::ios::binary);
    if (!in)
        return false;
    std::array<char, 1 << 16> buf;
    while (in)
    {
        in.read(buf.data(), static_cast<std::streamsize>(buf.size()));
        FnvBytes(h, buf.data(), static_cast<size_t>(in.gcount()));
    }
    return true;
}

template <typename T> void Put(std::ofstream &out, const T &v)
{
    out.write(reinterpret_cast<const char *>(&v), sizeof(v));
}

template <typename T> bool Get(std::ifstream &in, T &v)
{
    return static_cast<bool>(in.read(reinterpret_cast<char *>(&v), sizeof(v)));
}

} // namespace

uint64_t assetc::HashSource(const std::string &path, uint64_t seed) noexcept
{
    uint64_t h = kFnvOffset;
    FnvBytes(h, &seed, sizeof(seed));
    const uint32_t ver = kEncoderVersion;
    FnvBytes(h, &ver, sizeof(ver));

    std::error_code ec;
    if (fs::is_directory(path, ec))
    {
        // Deterministic: collect relative paths, sort, hash (path, bytes) each.
        std::vector<fs::path> files;
        for (fs::recursive_directory_iterator it(path, fs::directory_options::skip_permission_denied,
                                                 ec),
             end;
             it != end; it.increment(ec))
        {
            if (!ec && it->is_regular_file())
                files.push_back(it->path());
        }
        std::sort(files.begin(), files.end());
        for (const auto &f : files)
        {
            const auto rel = fs::relative(f, path, ec).generic_string();
            FnvBytes(h, rel.data(), rel.size());
            if (!FnvFile(h, f))
                return 0;
        }
        return h;
    }

    return FnvFile(h, path) ? h : 0;
}

assetc::CacheTable assetc::LoadCache(const std::string &path)
{
    CacheTable    table;
    std::ifstream in(path, std::ios::binary);
    if (!in)
        return table;

    uint32_t magic = 0, version = 0, count = 0, reserved = 0;
    if (!Get(in, magic) || !Get(in, version) || !Get(in, count) || !Get(in, reserved) ||
        magic != kCacheMagic || version != kCacheVersion)
        return table; // cold/foreign cache

    auto readStr = [&](std::string &s, uint16_t len) {
        s.resize(len);
        return static_cast<bool>(in.read(s.data(), len));
    };

    for (uint32_t i = 0; i < count; ++i)
    {
        CacheEntry e;
        uint16_t   srcLen = 0;
        uint32_t   manCount = 0;
        if (!Get(in, e.inputHash) || !Get(in, srcLen) || !readStr(e.sourcePath, srcLen) ||
            !Get(in, manCount))
            return table; // truncated: keep what we have (all-or-nothing per entry)
        bool ok = true;
        for (uint32_t j = 0; j < manCount; ++j)
        {
            ManifestEntry m;
            uint8_t       kind = 0, cs = 0;
            uint16_t      pathLen = 0;
            if (!Get(in, m.hash) || !Get(in, kind) || !Get(in, cs) || !Get(in, pathLen) ||
                !readStr(m.path, pathLen))
            {
                ok = false;
                break;
            }
            m.kind       = static_cast<ManKind>(kind);
            m.colorspace = static_cast<ManColorSpace>(cs);
            e.manifest.push_back(std::move(m));
        }
        if (!ok)
            return table;
        table.emplace(e.sourcePath, std::move(e));
    }
    return table;
}

int assetc::WriteCache(const std::string &path, const std::vector<CacheEntry> &entries)
{
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
    {
        fmtx::Error(fmt::format("open failed: {}", path));
        return 1;
    }

    Put(out, kCacheMagic);
    Put(out, kCacheVersion);
    Put(out, static_cast<uint32_t>(entries.size()));
    Put(out, static_cast<uint32_t>(0)); // reserved

    for (const auto &e : entries)
    {
        Put(out, e.inputHash);
        Put(out, static_cast<uint16_t>(e.sourcePath.size()));
        out.write(e.sourcePath.data(), static_cast<std::streamsize>(e.sourcePath.size()));
        Put(out, static_cast<uint32_t>(e.manifest.size()));
        for (const auto &m : e.manifest)
        {
            Put(out, m.hash);
            Put(out, static_cast<uint8_t>(m.kind));
            Put(out, static_cast<uint8_t>(m.colorspace));
            Put(out, static_cast<uint16_t>(m.path.size()));
            out.write(m.path.data(), static_cast<std::streamsize>(m.path.size()));
        }
    }

    if (!out.good())
    {
        fmtx::Error(fmt::format("write failed: {}", path));
        return 1;
    }
    return 0;
}
