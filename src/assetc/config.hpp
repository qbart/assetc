#pragma once

#include <string>
#include <vector>

namespace assetc
{

// One pattern-based processing rule. `match` is a glob (`*` matches any run of
// characters including `/`, `?` matches one) tested against a source file's path
// relative to the input dir, forward-slash, e.g. "models/court.glb".
struct AssetRule
{
    std::string match;
    bool        hasMerge = false; // whether `merge` was specified
    bool        merge    = true;  // mesh: bake node transforms into one combined mesh
};

// Minimal project config, loaded from the nearest `assetc.yml` searching upward
// from the working directory. Everything has a sensible default so a missing
// config reproduces today's behavior.
struct Config
{
    std::string input  = "assets";
    std::string output = "runtime";
    bool        pack   = false; // bundle the output into <output>.hpack after build

    bool                   meshMerge = true; // default merge for meshes
    std::vector<AssetRule> rules;            // first matching rule wins

    // Resolve the effective mesh `merge` for a source file (path relative to the
    // input dir): first matching rule's `merge`, else the global default.
    bool MergeFor(const std::string &relPath) const;
};

// Glob match: `*` matches any run (incl. `/`), `?` matches exactly one char.
bool GlobMatch(const std::string &pattern, const std::string &str) noexcept;

// Search `startDir` and its ancestors for `assetc.yml`. On success fills `cfg`,
// sets `foundPath` to the file used, and returns 0. Returns 0 with defaults (and
// empty `foundPath`) when no config exists; returns non-zero only on a malformed
// config (logged).
int LoadConfig(const std::string &startDir, Config &cfg, std::string &foundPath);

// Write a starter `assetc.yml` to `path`: input/output set to their defaults and
// active, every other key present but commented out. Overwrites unconditionally
// (callers guard against clobbering). Returns 0 on success.
int WriteDefaultConfig(const std::string &path);

} // namespace assetc
