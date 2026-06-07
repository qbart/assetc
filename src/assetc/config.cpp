#include "config.hpp"

#include "../deps/fmt.hpp"

#include <yaml-cpp/yaml.h>

#include <filesystem>
#include <fstream>
#include <initializer_list>

namespace fs = std::filesystem;

// --- overlay / resolve -------------------------------------------------------

void assetc::TextureSettings::overlay(const TextureSettings &o)
{
    if (o.compress)
        compress = o.compress;
}

void assetc::MeshSettings::overlay(const MeshSettings &o)
{
    if (o.merge)
        merge = o.merge;
}

assetc::Config::Resolved assetc::Config::resolve(const std::string &relPath,
                                                 const std::string &preset) const
{
    TextureSettings tex;
    MeshSettings    mesh;

    // Base globals, then preset globals.
    tex.overlay(defaultLayer.tex);
    mesh.overlay(defaultLayer.mesh);
    const Layer *p = nullptr;
    if (!preset.empty())
    {
        auto it = presets.find(preset);
        if (it != presets.end())
            p = &it->second;
    }
    if (p)
    {
        tex.overlay(p->tex);
        mesh.overlay(p->mesh);
    }

    // Rules cascade: default rules in order, then preset rules (so preset wins).
    auto applyRules = [&](const std::vector<Rule> &rules) {
        for (const auto &r : rules)
            if (GlobMatch(r.match, relPath))
            {
                tex.overlay(r.tex);
                mesh.overlay(r.mesh);
            }
    };
    applyRules(defaultLayer.rules);
    if (p)
        applyRules(p->rules);

    return Resolved{tex.compress.value_or(true), mesh.merge.value_or(true)};
}

std::string assetc::Config::outputFor(const std::string &preset) const
{
    if (!preset.empty())
    {
        auto it = presets.find(preset);
        if (it != presets.end() && it->second.output)
            return *it->second.output;
    }
    return output;
}

bool assetc::Config::hasPreset(const std::string &name) const
{
    return presets.find(name) != presets.end();
}

std::vector<std::string> assetc::Config::presetNames() const
{
    std::vector<std::string> names;
    names.reserve(presets.size());
    for (const auto &[k, _] : presets)
        names.push_back(k);
    return names;
}

// --- glob --------------------------------------------------------------------

bool assetc::GlobMatch(const std::string &pat, const std::string &str) noexcept
{
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

// --- load --------------------------------------------------------------------

namespace
{
void WarnUnknownKeys(const YAML::Node &map, std::initializer_list<const char *> known,
                     const std::string &where, const std::string &file)
{
    if (!map.IsMap())
        return;
    for (const auto &kv : map)
    {
        const auto key = kv.first.as<std::string>();
        bool       ok  = false;
        for (const char *k : known)
            if (key == k)
            {
                ok = true;
                break;
            }
        if (!ok)
            fmtx::Warn(fmt::format("config {}: unknown key '{}' in {}", file, key, where));
    }
}

void ParseTexture(const YAML::Node &n, assetc::TextureSettings &tex, const std::string &where,
                  const std::string &file)
{
    if (!n)
        return;
    if (n["compress"])
        tex.compress = n["compress"].as<bool>();
    WarnUnknownKeys(n, {"compress"}, where + ".texture", file);
}

void ParseMesh(const YAML::Node &n, assetc::MeshSettings &mesh, const std::string &where,
               const std::string &file)
{
    if (!n)
        return;
    if (n["merge"])
        mesh.merge = n["merge"].as<bool>();
    WarnUnknownKeys(n, {"merge"}, where + ".mesh", file);
}

// Parse a `default:`/preset layer. `isPreset` controls whether `output` is allowed.
assetc::Layer ParseLayer(const YAML::Node &n, bool isPreset, const std::string &where,
                         const std::string &file)
{
    assetc::Layer L;
    if (n["output"])
    {
        if (isPreset)
            L.output = n["output"].as<std::string>();
        else
            fmtx::Warn(fmt::format(
                "config {}: 'output' in default: is ignored; set it at the top level", file));
    }
    ParseTexture(n["texture"], L.tex, where, file);
    ParseMesh(n["mesh"], L.mesh, where, file);

    if (n["rules"] && n["rules"].IsSequence())
    {
        size_t i = 0;
        for (const auto &rn : n["rules"])
        {
            ++i;
            if (!rn["match"])
            {
                fmtx::Warn(fmt::format("config {}: rule #{} in {} has no 'match'; skipped", file, i,
                                       where));
                continue;
            }
            assetc::Rule r;
            r.match            = rn["match"].as<std::string>();
            const auto rwhere  = fmt::format("{}.rules[{}]", where, i - 1);
            ParseTexture(rn["texture"], r.tex, rwhere, file);
            ParseMesh(rn["mesh"], r.mesh, rwhere, file);
            WarnUnknownKeys(rn, {"match", "texture", "mesh"}, rwhere, file);
            L.rules.push_back(std::move(r));
        }
    }

    WarnUnknownKeys(n, {"output", "texture", "mesh", "rules"}, where, file);
    return L;
}
} // namespace

int assetc::LoadConfig(const std::string &startDir, Config &cfg, std::string &foundPath)
{
    foundPath.clear();

    std::error_code ec;
    fs::path        cfgPath;
    for (fs::path d = fs::absolute(startDir, ec); !d.empty(); d = d.parent_path())
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

    const std::string file = cfgPath.string();
    try
    {
        YAML::Node root = YAML::LoadFile(file);
        if (root["input"])
            cfg.input = root["input"].as<std::string>();
        if (root["output"])
            cfg.output = root["output"].as<std::string>();
        if (root["pack"])
            cfg.pack = root["pack"].as<bool>();
        if (root["preset"])
            cfg.preset = root["preset"].as<std::string>();

        if (root["default"])
            cfg.defaultLayer = ParseLayer(root["default"], /*isPreset=*/false, "default", file);

        if (root["presets"] && root["presets"].IsMap())
        {
            for (const auto &pn : root["presets"])
            {
                const auto name = pn.first.as<std::string>();
                cfg.presets[name] =
                    ParseLayer(pn.second, /*isPreset=*/true, "presets." + name, file);
            }
        }

        WarnUnknownKeys(root, {"input", "output", "pack", "preset", "default", "presets"},
                        "(top level)", file);
    }
    catch (const YAML::Exception &e)
    {
        fmtx::Error(fmt::format("config: {}: {}", file, e.what()));
        return 1;
    }

    foundPath = file;
    return 0;
}

int assetc::WriteDefaultConfig(const std::string &path)
{
    static constexpr const char *kTemplate = R"(# assetc.yml — asset compiler configuration.
# Searched for in the working directory and its ancestors, so it can live at the
# project root. All keys are optional; CLI flags override them.

# Source and base output directories.
input: assets
output: runtime

# Bundle the whole output dir into a single <output>.hpack after building.
# pack: false

# Preset used when --preset is not passed.
# preset: desktop

# Base layer applied to every build: per-asset defaults + pattern rules.
# default:
#   mesh:
#     merge: true        # bake glTF node transforms into one combined mesh;
#                        # false keeps geometry source-local. (Skinned meshes are
#                        # never baked; OBJ has no node graph.)
#   texture:
#     compress: true     # UASTC-encode (default). false = store raw, lossless.
#   rules:               # first-to-last; later matches override. `match` is a glob
#                        # (* spans '/', ? one char) over the source-relative path.
#     - match: "ui/*"
#       texture:
#         compress: false   # keep UI / data textures pixel-exact
#     - match: "props/*.glb"
#       mesh:
#         merge: false

# Named presets overlay `default` when selected with --preset <name>. Each may
# redirect `output` so targets don't clobber each other.
# presets:
#   desktop:
#     output: runtime/desktop
#   mobile:
#     output: runtime/mobile
#     rules:
#       - match: "decals/*"
#         texture:
#           compress: false
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
