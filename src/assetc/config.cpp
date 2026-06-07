#include "config.hpp"

#include "../deps/fmt.hpp"

#include <yaml-cpp/yaml.h>

#include <filesystem>
#include <fstream>

namespace fs = std::filesystem;

bool assetc::GlobMatch(const std::string &pat, const std::string &str) noexcept
{
    // Iterative wildcard match with backtracking for '*'. '*' spans any chars
    // (including '/'); '?' matches exactly one.
    size_t p = 0, s = 0, star = std::string::npos, mark = 0;
    while (s < str.size())
    {
        if (p < pat.size() && (pat[p] == '?' || pat[p] == str[s]))
        {
            ++p;
            ++s;
        }
        else if (p < pat.size() && pat[p] == '*')
        {
            star = p++;
            mark = s;
        }
        else if (star != std::string::npos)
        {
            p = star + 1;
            s = ++mark;
        }
        else
        {
            return false;
        }
    }
    while (p < pat.size() && pat[p] == '*')
        ++p;
    return p == pat.size();
}

bool assetc::Config::MergeFor(const std::string &relPath) const
{
    for (const auto &r : rules)
        if (r.hasMerge && GlobMatch(r.match, relPath))
            return r.merge;
    return meshMerge;
}

int assetc::LoadConfig(const std::string &startDir, Config &cfg, std::string &foundPath)
{
    foundPath.clear();

    std::error_code ec;
    fs::path        dir = fs::absolute(startDir, ec);
    fs::path        cfgPath;
    for (fs::path d = dir; !d.empty(); d = d.parent_path())
    {
        const auto candidate = d / "assetc.yml";
        if (fs::exists(candidate, ec))
        {
            cfgPath = candidate;
            break;
        }
        if (!d.has_parent_path() || d.parent_path() == d)
            break;
    }
    if (cfgPath.empty())
        return 0; // no config: defaults

    try
    {
        YAML::Node root = YAML::LoadFile(cfgPath.string());
        if (root["input"])
            cfg.input = root["input"].as<std::string>();
        if (root["output"])
            cfg.output = root["output"].as<std::string>();
        if (root["pack"])
            cfg.pack = root["pack"].as<bool>();
        if (root["mesh"] && root["mesh"]["merge"])
            cfg.meshMerge = root["mesh"]["merge"].as<bool>();

        if (root["rules"] && root["rules"].IsSequence())
        {
            for (const auto &rn : root["rules"])
            {
                AssetRule rule;
                if (!rn["match"])
                    continue; // a rule without a pattern matches nothing; skip
                rule.match = rn["match"].as<std::string>();
                if (rn["merge"])
                {
                    rule.hasMerge = true;
                    rule.merge    = rn["merge"].as<bool>();
                }
                cfg.rules.push_back(std::move(rule));
            }
        }
    }
    catch (const YAML::Exception &e)
    {
        fmtx::Error(fmt::format("config: {}: {}", cfgPath.string(), e.what()));
        return 1;
    }

    foundPath = cfgPath.string();
    return 0;
}

int assetc::WriteDefaultConfig(const std::string &path)
{
    // input/output are active (at their defaults); every other knob is shown but
    // commented out so it stays opt-in.
    static constexpr const char *kTemplate = R"(# assetc.yml — asset compiler configuration.
# Searched for in the working directory and its ancestors, so it can live at the
# project root. All keys are optional; CLI flags override them.

# Source and output directories.
input: assets
output: runtime

# Bundle the whole output dir into a single <output>.hpack after building.
# pack: false

# Default mesh processing.
# mesh:
#   merge: true   # bake node transforms into one combined, world-space mesh;
#                 # false keeps geometry in its source-local space. (Skinned
#                 # meshes are never baked; OBJ has no node graph.)

# Per-pattern overrides; the first matching rule wins. `match` is a glob over the
# source path relative to `input` (* spans '/', ? matches one character).
# rules:
#   - match: "props/*.glb"
#     merge: false
)";

    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
    {
        fmtx::Error(fmt::format("config: open failed: {}", path));
        return 1;
    }
    out << kTemplate;
    if (!out.good())
    {
        fmtx::Error(fmt::format("config: write failed: {}", path));
        return 1;
    }
    return 0;
}
