#pragma once

#include "runtime_manifest.hpp" // ManifestEntry

#include <cstdint>
#include <string>
#include <unordered_map>
#include <vector>

namespace assetc
{

// Bump when the encoder output format/semantics change in a way that should
// invalidate every cached asset regardless of source content.
constexpr uint32_t kEncoderVersion = 1;

// One cached source asset: the hash of its inputs (content + settings + encoder
// version) and the manifest entries it contributed, so a cache hit can replay
// those into the global manifest without re-encoding.
struct CacheEntry
{
    uint64_t                   inputHash = 0;
    std::string                sourcePath;
    std::vector<ManifestEntry> manifest;
};

using CacheTable = std::unordered_map<std::string, CacheEntry>;

// FNV1a64 over raw bytes. For a directory, hashes each contained regular file as
// (relative-path, bytes) in sorted order so the result is deterministic. Returns
// `seed` folded with `kEncoderVersion`; pass a per-asset salt (e.g. asset type) as
// `seed` so different roles of the same bytes hash differently. 0 on unreadable.
uint64_t HashSource(const std::string &path, uint64_t seed) noexcept;

// Read a `.assetc-cache` into a path-keyed table. Missing/corrupt -> empty table
// (treated as a cold cache; never an error).
CacheTable LoadCache(const std::string &path);

// Serialize entries to `path` (magic 'HCAC'). Returns 0 on success.
int WriteCache(const std::string &path, const std::vector<CacheEntry> &entries);

} // namespace assetc
