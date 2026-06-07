#pragma once

#include "runtime_mesh.hpp" // MakeFourCC

#include <cstdint>
#include <string>
#include <vector>

namespace assetc
{

constexpr uint32_t PackMagic   = MakeFourCC('H', 'P', 'A', 'K');
constexpr uint32_t PackVersion = 1;

// A `.hpack` bundles every runtime file from the output directory into one blob so
// the engine opens a single file instead of thousands. It is a table of contents
// followed by 16-byte-aligned file payloads.
//
// On-disk layout (little-endian):
//   PackHeader (24 B):
//     magic    u32  == PackMagic ('HPAK')
//     version  u32  == PackVersion
//     count    u32  number of entries
//     flags    u32  reserved (0)
//     tocBytes u64  size of the TOC region following the header
//   TOC: `count` entries, sorted by path ascending:
//     offset   u64  byte offset of the payload from file start (16-aligned)
//     size     u64  payload bytes
//     pathLen  u16
//     path     bytes (UTF-8, forward-slash, relative to the output root)
//   Payload region: each file's bytes at its `offset`, padded to 16-byte alignment.
//
// Excluded from the pack: the build cache (`.assetc-cache`) and any existing
// `.hpack`. Entries are sorted by path so a reader may binary-search; the engine
// resolves a runtime-relative path to (offset,size) and reads/mmaps in place.

struct PackEntry
{
    uint64_t    offset; // from file start
    uint64_t    size;
    std::string path; // runtime-relative, forward-slash
};

// Bundle every eligible file under `outputDir` into `packPath`. Returns 0 on
// success, non-zero with a logged error. Deterministic: same tree -> same bytes.
int WritePack(const std::string &outputDir, const std::string &packPath);

// Parse a `.hpack` TOC into `out` (cleared first). Returns 0 on success.
int ReadPackToc(const std::string &packPath, std::vector<PackEntry> &out);

// Validate magic/version, that the TOC parses, and that every entry's
// [offset, offset+size) lies within the file. Returns 0 on success.
int ValidatePack(const std::string &packPath);

// Print what's inside a `.hpack`: a per-kind summary (meshes/textures/...) and a
// path-sorted entry listing with sizes. Returns 0 on success, non-zero if the
// pack can't be read.
int InspectPack(const std::string &packPath);

} // namespace assetc
