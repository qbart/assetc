#include "check.hpp"

#include "assetc/cache.hpp"

#include <filesystem>
#include <fstream>
#include <string>
#include <vector>

namespace fs = std::filesystem;
using namespace assetc;

static void Write(const fs::path &p, const std::string &s)
{
    std::ofstream out(p, std::ios::binary | std::ios::trunc);
    out.write(s.data(), static_cast<std::streamsize>(s.size()));
}

int main()
{
    const auto dir = fs::temp_directory_path() / "assetc_test_cache";
    fs::remove_all(dir);
    fs::create_directories(dir);

    // --- HashSource: deterministic, content-sensitive, seed-sensitive ----------
    const auto fileA = dir / "a.bin";
    Write(fileA, "hello world");
    const uint64_t h1 = HashSource(fileA.string(), 0);
    const uint64_t h2 = HashSource(fileA.string(), 0);
    CHECK(h1 != 0);
    CHECK_EQ(h1, h2); // deterministic

    Write(fileA, "hello worlds"); // content change
    CHECK(HashSource(fileA.string(), 0) != h1);

    Write(fileA, "hello world"); // restore -> original hash returns
    CHECK_EQ(HashSource(fileA.string(), 0), h1);

    // Different seed (asset role) -> different hash for identical bytes.
    CHECK(HashSource(fileA.string(), 1) != h1);

    // Missing file -> 0.
    CHECK_EQ(HashSource((dir / "nope.bin").string(), 0), 0ull);

    // Directory hash is order-independent of filesystem enumeration (sorted inside)
    // and changes when a contained file changes.
    const auto sub = dir / "d";
    fs::create_directories(sub);
    Write(sub / "1.txt", "one");
    Write(sub / "2.txt", "two");
    const uint64_t dh = HashSource(sub.string(), 0);
    CHECK(dh != 0);
    CHECK_EQ(HashSource(sub.string(), 0), dh);
    Write(sub / "2.txt", "TWO");
    CHECK(HashSource(sub.string(), 0) != dh);

    // --- cache round trip -------------------------------------------------------
    std::vector<CacheEntry> entries;
    CacheEntry              e1;
    e1.inputHash  = 0xdeadbeefcafef00dull;
    e1.sourcePath = "assets/models/court.glb";
    e1.manifest.push_back({0xAB, ManKind::Texture, ManColorSpace::Srgb, "court/tex_0.ktx2"});
    e1.manifest.push_back({0xCD, ManKind::Texture, ManColorSpace::Linear, "court/tex_1.ktx2"});
    entries.push_back(e1);
    CacheEntry e2;
    e2.inputHash  = 0x1234;
    e2.sourcePath = "assets/tex/wood.png"; // no manifest entries
    entries.push_back(e2);

    const auto cachePath = (dir / ".assetc-cache").string();
    CHECK_EQ(WriteCache(cachePath, entries), 0);

    CacheTable loaded = LoadCache(cachePath);
    CHECK_EQ(loaded.size(), 2u);
    CHECK(loaded.count("assets/models/court.glb") == 1);
    const auto &g = loaded["assets/models/court.glb"];
    CHECK_EQ(g.inputHash, 0xdeadbeefcafef00dull);
    CHECK_EQ(g.manifest.size(), 2u);
    CHECK_EQ(g.manifest[0].path, std::string("court/tex_0.ktx2"));
    CHECK(g.manifest[0].colorspace == ManColorSpace::Srgb);
    CHECK_EQ(loaded["assets/tex/wood.png"].manifest.size(), 0u);

    // Foreign/corrupt cache -> empty table (cold), never throws.
    Write(dir / "bad.cache", "not a cache file at all");
    CHECK_EQ(LoadCache((dir / "bad.cache").string()).size(), 0u);
    CHECK_EQ(LoadCache((dir / "missing.cache").string()).size(), 0u);

    fs::remove_all(dir);
    return test::summary();
}
