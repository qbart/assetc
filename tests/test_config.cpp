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
    CHECK(GlobMatch("*.glb", "models/court.glb"));
    CHECK(GlobMatch("models/*.glb", "models/x.glb"));
    CHECK(!GlobMatch("models/*.glb", "props/x.glb"));
    CHECK(GlobMatch("ui/?.png", "ui/a.png"));
    CHECK(!GlobMatch("ui/?.png", "ui/ab.png"));

    const auto base = fs::temp_directory_path() / "assetc_test_config";
    fs::remove_all(base);

    // --- defaults when no config exists ----------------------------------------
    {
        const auto empty = base / "empty";
        fs::create_directories(empty);
        Config      cfg;
        std::string found;
        CHECK_EQ(LoadConfig(empty.string(), cfg, found), 0);
        CHECK(found.empty());
        CHECK_EQ(cfg.input, std::string("assets"));
        CHECK_EQ(cfg.output, std::string("runtime"));
        CHECK(cfg.pack == false);
        CHECK(cfg.outputFor("") == std::string("runtime"));
        auto r = cfg.resolve("anything.glb", "");
        CHECK(r.compress == true);
        CHECK(r.merge == true);
    }

    // --- a layered config, discovered from a subdirectory ----------------------
    const auto proj = base / "proj";
    Write(proj / "assetc.yml", R"(
input: src_assets
output: out
pack: true
preset: desktop
embed:
  - "scene/*.json"
  - "config/*.xml"
default:
  mesh:
    merge: false
  texture:
    compress: true
  rules:
    - match: "ui/*"
      texture:
        compress: false
    - match: "shared/*"
      texture:
        compress: false
presets:
  desktop:
    output: out/desktop
  mobile:
    output: out/mobile
    texture:
      compress: false
    rules:
      - match: "props/*.glb"
        mesh:
          merge: true
      - match: "shared/*"
        texture:
          compress: true
)");
    fs::create_directories(proj / "deep" / "nested");

    Config      cfg;
    std::string found;
    CHECK_EQ(LoadConfig((proj / "deep" / "nested").string(), cfg, found), 0);
    CHECK(!found.empty());
    CHECK_EQ(cfg.input, std::string("src_assets"));
    CHECK_EQ(cfg.output, std::string("out"));
    CHECK(cfg.pack == true);
    CHECK_EQ(cfg.preset, std::string("desktop"));

    // embed: top-level list of glob patterns, parsed in order.
    CHECK_EQ(cfg.embed.size(), 2u);
    CHECK_EQ(cfg.embed[0], std::string("scene/*.json"));
    CHECK_EQ(cfg.embed[1], std::string("config/*.xml"));
    // default config has no embeds.
    CHECK(Config{}.embed.empty());

    // outputFor: preset overrides base; unknown/empty falls back.
    CHECK_EQ(cfg.outputFor(""), std::string("out"));
    CHECK_EQ(cfg.outputFor("desktop"), std::string("out/desktop"));
    CHECK_EQ(cfg.outputFor("mobile"), std::string("out/mobile"));
    CHECK_EQ(cfg.outputFor("nope"), std::string("out"));

    CHECK(cfg.hasPreset("mobile"));
    CHECK(!cfg.hasPreset("nope"));
    CHECK_EQ(cfg.presetNames().size(), 2u);

    // resolve(): default global merge=false applies everywhere absent a rule.
    CHECK(cfg.resolve("models/x.glb", "").merge == false);
    CHECK(cfg.resolve("models/x.glb", "").compress == true);

    // default rule: ui/* forces raw.
    CHECK(cfg.resolve("ui/btn.png", "").compress == false);

    // preset global overlays default: mobile sets compress=false globally.
    CHECK(cfg.resolve("models/x.glb", "mobile").compress == false);
    // preset rule overlays preset global: props/*.glb gets merge=true under mobile.
    CHECK(cfg.resolve("props/tree.glb", "mobile").merge == true);
    CHECK(cfg.resolve("props/tree.glb", "").merge == false); // no preset -> default

    // preset rule beats default rule for the same file: shared/* compress
    // false (default rule) -> true (preset rule) under mobile.
    CHECK(cfg.resolve("shared/x.png", "").compress == false);
    CHECK(cfg.resolve("shared/x.png", "mobile").compress == true);

    // --- init template parses back to defaults ---------------------------------
    {
        const auto initDir = base / "init";
        fs::create_directories(initDir);
        CHECK_EQ(WriteDefaultConfig((initDir / "assetc.yml").string()), 0);
        Config      icfg;
        std::string ifound;
        CHECK_EQ(LoadConfig(initDir.string(), icfg, ifound), 0);
        CHECK(!ifound.empty());
        CHECK_EQ(icfg.input, std::string("assets"));
        CHECK_EQ(icfg.output, std::string("runtime"));
        CHECK(icfg.resolve("x.png", "").compress == true);
        CHECK(icfg.presets.empty());
    }

    // --- malformed YAML is an error --------------------------------------------
    {
        const auto bad = base / "bad";
        Write(bad / "assetc.yml", "input: [unterminated\n  : :");
        Config      bcfg;
        std::string bfound;
        CHECK(LoadConfig(bad.string(), bcfg, bfound) != 0);
    }

    fs::remove_all(base);
    return test::summary();
}
