#include "check.hpp"

#include "assetc/config.hpp"

#include <filesystem>
#include <fstream>

namespace fs = std::filesystem;
using namespace assetc;

static void Write(const fs::path &p, const std::string &s)
{
    fs::create_directories(p.parent_path());
    std::ofstream out(p, std::ios::trunc);
    out << s;
}

int main()
{
    // --- glob matcher -----------------------------------------------------------
    CHECK(GlobMatch("*.glb", "models/court.glb"));   // * spans '/'
    CHECK(GlobMatch("models/*.glb", "models/x.glb"));
    CHECK(!GlobMatch("models/*.glb", "props/x.glb"));
    CHECK(GlobMatch("props/?.glb", "props/a.glb"));
    CHECK(!GlobMatch("props/?.glb", "props/ab.glb"));
    CHECK(GlobMatch("*", "anything/at/all"));
    CHECK(GlobMatch("exact.obj", "exact.obj"));
    CHECK(!GlobMatch("exact.obj", "exact.objx"));

    // --- defaults when no config exists ----------------------------------------
    const auto base = fs::temp_directory_path() / "assetc_test_config";
    fs::remove_all(base);
    const auto empty = base / "empty";
    fs::create_directories(empty);
    {
        Config      cfg;
        std::string found;
        CHECK_EQ(LoadConfig(empty.string(), cfg, found), 0);
        CHECK(found.empty());
        CHECK_EQ(cfg.input, std::string("assets"));
        CHECK_EQ(cfg.output, std::string("runtime"));
        CHECK(cfg.pack == false);
        CHECK(cfg.meshMerge == true);
        CHECK(cfg.MergeFor("anything.glb") == true);
    }

    // --- a real config, discovered from a subdirectory (upward search) ----------
    const auto proj = base / "proj";
    Write(proj / "assetc.yml", R"(
input: src_assets
output: out
pack: true
mesh:
  merge: false
rules:
  - match: "props/*.glb"
    merge: true
  - match: "*.obj"
    merge: false
)");
    fs::create_directories(proj / "deep" / "nested");
    {
        Config      cfg;
        std::string found;
        CHECK_EQ(LoadConfig((proj / "deep" / "nested").string(), cfg, found), 0);
        CHECK(!found.empty());
        CHECK_EQ(cfg.input, std::string("src_assets"));
        CHECK_EQ(cfg.output, std::string("out"));
        CHECK(cfg.pack == true);
        CHECK(cfg.meshMerge == false);

        // Default (no rule) -> global meshMerge false.
        CHECK(cfg.MergeFor("models/x.glb") == false);
        // First matching rule wins.
        CHECK(cfg.MergeFor("props/tree.glb") == true);
        CHECK(cfg.MergeFor("crate.obj") == false);
    }

    // --- malformed YAML is reported as an error --------------------------------
    const auto bad = base / "bad";
    Write(bad / "assetc.yml", "input: [unterminated\n  : :");
    {
        Config      cfg;
        std::string found;
        CHECK(LoadConfig(bad.string(), cfg, found) != 0);
    }

    fs::remove_all(base);
    return test::summary();
}
