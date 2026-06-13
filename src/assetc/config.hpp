#pragma once

#include <map>
#include <optional>
#include <string>
#include <vector>

namespace assetc
{

// Per-asset settings are optional at every layer and overlaid by specificity
// (built-in defaults < default block < preset < default rules < preset rules).
// A layer only overrides the fields it actually sets.
struct TextureSettings
{
    std::optional<bool> compress; // false = store raw R8G8B8A8 (no UASTC)
    void                overlay(const TextureSettings &o);
};

struct MeshSettings
{
    std::optional<bool> merge; // bake glTF node transforms into one combined mesh
    void                overlay(const MeshSettings &o);
};

// A pattern-based override. `match` is a glob (`*` spans `/`, `?` one char) tested
// against the source path relative to `input`, e.g. "models/court.glb".
struct Rule
{
    std::string     match;
    TextureSettings tex;
    MeshSettings    mesh;
};

// `default:` and each entry of `presets:` share this shape.
struct Layer
{
    std::optional<std::string> output; // only meaningful for presets (overrides base)
    TextureSettings            tex;
    MeshSettings               mesh;
    std::vector<Rule>          rules;
};

// Project config from the nearest assetc.yml (searched upward). Everything is
// optional; built-in defaults reproduce today's behavior.
struct Config
{
    std::string input  = "assets";   // top-level: source tree (same for every preset)
    std::string output = "runtime";  // top-level base output (a preset may override)
    bool        pack   = false;      // top-level
    std::string preset;              // top-level `preset:` (default when --preset absent)

    // `embed:` — top-level glob patterns (over the source path relative to `input`)
    // of raw files to copy verbatim into the output tree and register in the manifest
    // (ManKind::Embed) by HashEmbedRef(runtime-relative path). Path + extension matter.
    std::vector<std::string> embed;

    Layer                        defaultLayer; // `default:` (mesh/texture/rules; output ignored)
    std::map<std::string, Layer> presets;      // `presets:`

    // Fully-resolved per-file settings under the active preset ("" = none).
    struct Resolved
    {
        bool compress;
        bool merge;
    };
    Resolved resolve(const std::string &relPath, const std::string &preset) const;

    // Output dir for the active preset: preset.output ?: base output.
    std::string outputFor(const std::string &preset) const;

    bool                     hasPreset(const std::string &name) const;
    std::vector<std::string> presetNames() const;
};

// Glob match: `*` matches any run (incl. `/`), `?` matches exactly one char.
bool GlobMatch(const std::string &pattern, const std::string &str) noexcept;

// Search `startDir` and ancestors for assetc.yml. Fills `cfg`, sets `foundPath`,
// returns 0. Returns 0 with defaults (empty `foundPath`) when none exists;
// non-zero only on malformed YAML (logged). Unknown keys are warned, not fatal.
int LoadConfig(const std::string &startDir, Config &cfg, std::string &foundPath);

// Write a starter assetc.yml to `path` (overwrites). Returns 0 on success.
int WriteDefaultConfig(const std::string &path);

} // namespace assetc
