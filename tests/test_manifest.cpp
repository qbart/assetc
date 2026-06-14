#include "check.hpp"

#include "assetc/runtime_manifest.hpp"

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <vector>

namespace fs = std::filesystem;
using namespace assetc;

static std::vector<uint8_t> ReadAll(const std::string &p)
{
    std::ifstream in(p, std::ios::binary);
    return {std::istreambuf_iterator<char>(in), std::istreambuf_iterator<char>()};
}

int main()
{
    const auto dir  = fs::temp_directory_path() / "assetc_test_manifest";
    fs::create_directories(dir);
    const auto path = (dir / "a.hman").string();

    // Round trip: unsorted input with a duplicate ref should write, sort by hash,
    // and collapse the identical (hash, path) pair.
    std::vector<ManifestEntry> entries = {
        {0x30, ManKind::Texture, ManColorSpace::Linear, "models/c/tex_0.ktx2"},
        {0x10, ManKind::Texture, ManColorSpace::Srgb, "models/a/tex_0.ktx2"},
        {0x20, ManKind::Texture, ManColorSpace::Srgb, "models/b/tex_0.ktx2"},
        {0x10, ManKind::Texture, ManColorSpace::Srgb, "models/a/tex_0.ktx2"}, // exact dup
    };
    CHECK_EQ(WriteHMan(path, entries), 0);
    CHECK_EQ(ValidateHMan(path), 0);

    // Path-addressed entries (embeds + the by-path asset kinds) round-trip alongside
    // textures and read back with kind/path intact.
    {
        const auto epath = (dir / "embed.hman").string();
        std::vector<ManifestEntry> emb = {
            {0x11, ManKind::Texture, ManColorSpace::Srgb, "models/a/tex_0.ktx2"},
            {0x22, ManKind::Embed, ManColorSpace::Linear, "scene/level.json"},
            {0x33, ManKind::Embed, ManColorSpace::Linear, "config/path.xml"},
            {0x44, ManKind::Mesh, ManColorSpace::Linear, "models/chair.hmesh"},
            {0x55, ManKind::Material, ManColorSpace::Linear, "models/chair.hmat"},
            {0x66, ManKind::Animation, ManColorSpace::Linear, "models/chair.hanim"},
            {0x77, ManKind::Font, ManColorSpace::Linear, "fonts/roboto.hfont"},
        };
        CHECK_EQ(WriteHMan(epath, emb), 0);
        CHECK_EQ(ValidateHMan(epath), 0);
        std::vector<ManifestEntry> back;
        CHECK_EQ(ReadHMan(epath, back), 0);
        CHECK_EQ(back.size(), 7u);
        int embeds = 0, mesh = 0, mat = 0, anim = 0, font = 0;
        for (const auto &e : back)
        {
            if (e.kind == ManKind::Embed) ++embeds;
            if (e.kind == ManKind::Mesh) { ++mesh; CHECK(e.path == "models/chair.hmesh"); }
            if (e.kind == ManKind::Material) ++mat;
            if (e.kind == ManKind::Animation) ++anim;
            if (e.kind == ManKind::Font) ++font;
        }
        CHECK_EQ(embeds, 2);
        CHECK_EQ(mesh, 1);
        CHECK_EQ(mat, 1);
        CHECK_EQ(anim, 1);
        CHECK_EQ(font, 1);
    }

    // Parse it back and assert structure: 3 entries, sorted ascending by hash.
    auto bytes = ReadAll(path);
    CHECK(bytes.size() >= 16);
    uint32_t magic, version, count, reserved;
    std::memcpy(&magic, &bytes[0], 4);
    std::memcpy(&version, &bytes[4], 4);
    std::memcpy(&count, &bytes[8], 4);
    std::memcpy(&reserved, &bytes[12], 4);
    CHECK_EQ(magic, ManMagic);
    CHECK_EQ(version, ManVersion);
    CHECK_EQ(count, 3u);
    CHECK_EQ(reserved, 0u);

    // Walk entries, confirm hashes come out 0x10,0x20,0x30 in order.
    size_t   off  = 16;
    uint64_t prev = 0;
    for (uint32_t i = 0; i < count; ++i)
    {
        uint64_t hash;
        uint16_t plen;
        std::memcpy(&hash, &bytes[off], 8);
        std::memcpy(&plen, &bytes[off + 10], 2);
        CHECK(hash >= prev);
        prev = hash;
        off += 12 + plen;
    }
    CHECK_EQ(off, bytes.size()); // exact consumption

    // Determinism: same input -> identical bytes.
    const auto path2 = (dir / "b.hman").string();
    CHECK_EQ(WriteHMan(path2, entries), 0);
    CHECK(ReadAll(path) == ReadAll(path2));

    // Collision: same hash, different paths -> hard failure (no file trust).
    std::vector<ManifestEntry> colliding = {
        {0x99, ManKind::Texture, ManColorSpace::Srgb, "models/x/tex_0.ktx2"},
        {0x99, ManKind::Texture, ManColorSpace::Srgb, "models/y/tex_0.ktx2"},
    };
    CHECK(WriteHMan((dir / "c.hman").string(), colliding) != 0);

    fs::remove_all(dir);
    return test::summary();
}
