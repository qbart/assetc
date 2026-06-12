#pragma once

#include "runtime_mesh.hpp" // MakeFourCC

#include <cstdint>
#include <string>
#include <vector>

namespace assetc
{

constexpr uint32_t PackMagic   = MakeFourCC('H', 'P', 'A', 'K');
constexpr uint32_t PackVersion = 2; // v2 added the per-entry `kind` byte

// Asset kind for a packed entry, derived from its path extension at pack time so
// the engine can filter/dispatch by type without sniffing strings.
enum class PackKind : uint8_t
{
    Other     = 0,
    Mesh      = 1, // .hmesh
    Material  = 2, // .hmat
    Manifest  = 3, // .hman
    Animation = 4, // .hanim
    Texture   = 5, // .ktx2
    Shader    = 6, // .spv
    Font      = 7, // .hfont
};

// A `.hpack` bundles every runtime file from the output directory into one blob so
// the engine opens a single file instead of thousands. It is a table of contents
// followed by 16-byte-aligned file payloads.
//
// On-disk layout (little-endian):
//   PackHeader (24 B):
//     magic    u32  == PackMagic ('HPAK')
//     version  u32  == PackVersion (2)
//     count    u32  number of entries
//     flags    u32  reserved (0)
//     tocBytes u64  size of the TOC region following the header
//   TOC: `count` entries, sorted by path ascending:
//     offset   u64  byte offset of the payload from file start (16-aligned)
//     size     u64  payload bytes
//     kind     u8   PackKind (asset type by extension)
//     pathLen  u16
//     path     bytes (UTF-8, forward-slash, relative to the output root)
//   Payload region: each file's bytes at its `offset`, padded to 16-byte alignment.
//
// Excluded from the pack: the build cache (`.assetc-cache`) and any existing
// `.hpack`. Entries are sorted by path so a reader may binary-search; the engine
// resolves a runtime-relative path to (offset,size,kind) and reads/mmaps in place.

struct PackEntry
{
    uint64_t    offset; // from file start
    uint64_t    size;
    PackKind    kind;
    std::string path; // runtime-relative, forward-slash
};

// PackKind for a runtime-relative path (by extension). Exposed so engine-side
// tools share the same mapping.
PackKind PackKindOf(const std::string &path) noexcept;

// Bundle every eligible file under `outputDir` into `packPath`. Returns 0 on
// success, non-zero with a logged error. Deterministic: same tree -> same bytes.
int WritePack(const std::string &outputDir, const std::string &packPath);

// Parse a `.hpack` TOC into `out` (cleared first). Returns 0 on success.
int ReadPackToc(const std::string &packPath, std::vector<PackEntry> &out);

// Validate magic/version, that the TOC parses, and that every entry's
// [offset, offset+size) lies within the file. Returns 0 on success.
int ValidatePack(const std::string &packPath);

} // namespace assetc
