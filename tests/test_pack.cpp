#include "check.hpp"

#include "assetc/pack.hpp"

#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>

namespace fs = std::filesystem;
using namespace assetc;

static void Write(const fs::path &p, const std::string &s)
{
    fs::create_directories(p.parent_path());
    std::ofstream out(p, std::ios::binary | std::ios::trunc);
    out.write(s.data(), static_cast<std::streamsize>(s.size()));
}

static std::string ReadRange(const std::string &file, uint64_t off, uint64_t size)
{
    std::ifstream in(file, std::ios::binary);
    in.seekg(static_cast<std::streamoff>(off));
    std::string s(size, '\0');
    in.read(s.data(), static_cast<std::streamsize>(size));
    return s;
}

int main()
{
    const auto root = fs::temp_directory_path() / "assetc_test_pack";
    fs::remove_all(root);
    const auto outDir = root / "runtime";

    Write(outDir / "a.hmesh", "MESH-AAA");
    Write(outDir / "models" / "b.ktx2", std::string(40, 'B'));
    Write(outDir / "assets.hman", "MANIFEST");
    Write(outDir / ".assetc-cache", "CACHE-should-be-excluded");

    const auto packPath = (root / "runtime.hpack").string();
    CHECK_EQ(WritePack(outDir.string(), packPath), 0);
    CHECK_EQ(ValidatePack(packPath), 0);

    std::vector<PackEntry> toc;
    CHECK_EQ(ReadPackToc(packPath, toc), 0);

    // 3 files; .assetc-cache excluded.
    CHECK_EQ(toc.size(), 3u);

    // pack info reads a valid pack; fails on garbage.
    CHECK_EQ(InspectPack(packPath), 0);

    // Sorted by path ascending.
    for (size_t i = 1; i < toc.size(); ++i)
        CHECK(toc[i - 1].path < toc[i].path);

    // Every entry resolves to its original bytes, and offsets are 16-aligned.
    bool sawHman = false, sawKtx = false;
    for (const auto &e : toc)
    {
        CHECK_EQ(e.offset % 16u, 0u);
        CHECK(e.path != ".assetc-cache");
        const std::string bytes = ReadRange(packPath, e.offset, e.size);
        if (e.path == "assets.hman")
        {
            CHECK_EQ(bytes, std::string("MANIFEST"));
            sawHman = true;
        }
        else if (e.path == "models/b.ktx2")
        {
            CHECK_EQ(bytes, std::string(40, 'B'));
            sawKtx = true;
        }
    }
    CHECK(sawHman);
    CHECK(sawKtx);

    // Determinism: same tree -> identical pack bytes.
    const auto packPath2 = (root / "runtime2.hpack").string();
    CHECK_EQ(WritePack(outDir.string(), packPath2), 0);
    std::ifstream f1(packPath, std::ios::binary), f2(packPath2, std::ios::binary);
    std::string   s1((std::istreambuf_iterator<char>(f1)), {}),
        s2((std::istreambuf_iterator<char>(f2)), {});
    CHECK(s1 == s2);

    // Corrupt magic -> read fails.
    Write(root / "bad.hpack", "not a pack");
    CHECK(ReadPackToc((root / "bad.hpack").string(), toc) != 0);
    CHECK(InspectPack((root / "bad.hpack").string()) != 0);

    fs::remove_all(root);
    return test::summary();
}
