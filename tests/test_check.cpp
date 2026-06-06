#include "check.hpp"

#include "assetc/check.hpp"
#include "assetc/encode_mesh.hpp" // HashAssetRef
#include "assetc/runtime_manifest.hpp"
#include "assetc/runtime_material.hpp"

#include <filesystem>
#include <fstream>
#include <vector>

namespace fs = std::filesystem;
using namespace assetc;

static void Touch(const fs::path &p)
{
    fs::create_directories(p.parent_path());
    std::ofstream(p, std::ios::binary).put('\0');
}

// Build a minimal valid output dir: one texture, manifest entry, and a .hmat row
// whose baseColorTex points at it. Returns the texture-ref hash.
static uint64_t MakeGoodTree(const fs::path &dir)
{
    fs::remove_all(dir);
    fs::create_directories(dir);

    const std::string relTex = "m/tex_0.ktx2";
    const std::string ref     = "m/tex_0";
    const uint64_t    hash    = HashAssetRef(ref);

    Touch(dir / relTex);

    std::vector<ManifestEntry> entries = {{hash, ManKind::Texture, ManColorSpace::Srgb, relTex}};
    WriteHMan((dir / "assets.hman").string(), entries);

    GpuMaterial row{};
    row.baseColorTex = hash;
    std::vector<GpuMaterial> rows = {row};
    WriteHMat((dir / "m.hmat").string(), rows);
    return hash;
}

int main()
{
    const auto base = fs::temp_directory_path() / "assetc_test_check";

    // --- good tree passes -------------------------------------------------------
    const auto good = base / "good";
    MakeGoodTree(good);
    CHECK_EQ(CheckRuntime(good.string()), 0);

    // --- missing texture file is caught ----------------------------------------
    const auto missing = base / "missing";
    MakeGoodTree(missing);
    fs::remove(missing / "m/tex_0.ktx2");
    CHECK(CheckRuntime(missing.string()) != 0);

    // --- .hmat ref absent from manifest is caught ------------------------------
    const auto dangling = base / "dangling";
    MakeGoodTree(dangling);
    {
        GpuMaterial row{};
        row.baseColorTex = 0xDEADBEEF; // not in the manifest
        std::vector<GpuMaterial> rows = {row};
        WriteHMat((dangling / "m.hmat").string(), rows);
    }
    CHECK(CheckRuntime(dangling.string()) != 0);

    // --- manifest entry pointing at a missing file is caught -------------------
    const auto badman = base / "badman";
    fs::remove_all(badman);
    fs::create_directories(badman);
    {
        const uint64_t             h = HashAssetRef("z/tex_0");
        std::vector<ManifestEntry> e = {{h, ManKind::Texture, ManColorSpace::Srgb, "z/tex_0.ktx2"}};
        WriteHMan((badman / "assets.hman").string(), e); // file z/tex_0.ktx2 never created
    }
    CHECK(CheckRuntime(badman.string()) != 0);

    fs::remove_all(base);
    return test::summary();
}
