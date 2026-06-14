#pragma once

#include "runtime_mesh.hpp" // MakeFourCC

#include <cstdint>
#include <mutex>
#include <string>
#include <vector>

namespace assetc
{

constexpr uint32_t ManMagic   = MakeFourCC('H', 'M', 'A', 'N');
constexpr uint32_t ManVersion = 1;

// `runtime/assets.hman` is the global hash -> file table the runtime uses to
// resolve an asset reference (e.g. GpuMaterial::baseColorTex) to bytes on disk.
//
// The hash is the SAME FNV1a64 already stored in .hmat/.hmesh for that ref.
// glTF-embedded textures are content-addressed: the hash is FNV1a64 of the image
// bytes salted by encoder mode, and the on-disk path is the shared flat store
//
//     "tex/<hash>.ktx2"               (hash as 16 lowercase hex digits)
//
// so the same texture in two sources collapses to one file. Font SDF atlases are
// name-addressed instead: hash == HashAssetRef("<sourceRef>") and path == ref +
// ".ktx2", where sourceRef is the asset path relative to `assets/`, extension
// stripped and lowercased (see Asset::SourceRef).
//
// A shader entry point follows the name-addressed shape: its canonical ref is
//
//     "<sourceRef>/<entryPoint>"       (e.g. "shaders/bloom/down")
//
// where sourceRef is the `.shader` folder relative to `assets/` with its `.shader`
// suffix stripped, and <entryPoint> is the Slang entry-point name. The on-disk path
// is that ref plus ".spv". Entry-point names are unique within a folder.
//
// An embed (assetc.yml `embed:`) is a raw file copied verbatim into the runtime
// tree. Its `kind` is Embed, its `path` is the runtime-relative path WITH extension
// (e.g. "scene/level.json"), and its hash is HashEmbedRef(path) — i.e. the same
// FNV1a64, but the extension is part of the ref so path and extension both matter.
//
// Meshes (`.hmesh`), material tables (`.hmat`), animation clips (`.hanim`), and
// font metadata (`.hfont`) are registered the SAME way: hash == HashEmbedRef(their
// runtime-relative path, extension kept), kind Mesh/Material/Animation/Font. So
// every asset — not just textures — resolves through one uniform id -> path lookup,
// and a mesh and its sibling `.hmat`/`.hanim` get distinct ids from their distinct
// extensions.
//
// On-disk layout (little-endian), entries sorted by hash ascending:
//
//     magic       u32   == ManMagic
//     version     u32   == ManVersion
//     count       u32
//     reserved    u32   == 0
//     count entries:
//         hash        u64
//         kind        u8    ManKind
//         colorspace  u8    ManColorSpace
//         pathLen     u16
//         path        pathLen bytes, UTF-8, forward-slash, relative to runtime root
enum class ManKind : uint8_t
{
    Texture   = 0,
    Mesh      = 1, // `.hmesh`:  hash == HashEmbedRef(runtime path)
    Material  = 2, // `.hmat`:   hash == HashEmbedRef(runtime path)
    Lut       = 3, // reserved, not emitted
    Shader    = 4, // SPIR-V entry point: ref "<sourceRef>/<entryPoint>", path + ".spv"
    Embed     = 5, // `embed:`-ed raw file: hash == HashEmbedRef(path), path kept verbatim
    Animation = 6, // `.hanim`:  hash == HashEmbedRef(runtime path)
    Font      = 7, // `.hfont`:  hash == HashEmbedRef(runtime path)
};

enum class ManColorSpace : uint8_t
{
    Linear = 0,
    Srgb   = 1,
};

// In-memory manifest record; serialized field-by-field (variable length).
struct ManifestEntry
{
    uint64_t      hash;
    ManKind       kind;
    ManColorSpace colorspace;
    std::string   path; // runtime-relative, forward-slash
};

// Thread-safe accumulator: assets compile on a worker pool, so entries arrive
// concurrently and in nondeterministic order. WriteHMan sorts before writing.
class ManifestSink
{
public:
    void Add(ManifestEntry e)
    {
        std::lock_guard<std::mutex> lk(mu_);
        entries_.push_back(std::move(e));
    }

    // Move out the accumulated entries (call after all workers have joined).
    std::vector<ManifestEntry> Take()
    {
        std::lock_guard<std::mutex> lk(mu_);
        return std::move(entries_);
    }

private:
    std::mutex                 mu_;
    std::vector<ManifestEntry> entries_;
};

// Sort by hash, collapse identical (hash, path) duplicates to one entry, and fail
// loud if two distinct paths share a hash. Then serialize to `path`. Returns 0 on
// success, non-zero with a logged error.
int WriteHMan(const std::string &path, std::vector<ManifestEntry> entries);

// Re-read a written `.hman`: check magic/version and that every entry parses and
// the bytes are consumed exactly. Returns 0 on success.
int ValidateHMan(const std::string &path);

// Parse a `.hman` into `out` (cleared first). Returns 0 on success, non-zero with
// a logged error on bad magic/version or truncation.
int ReadHMan(const std::string &path, std::vector<ManifestEntry> &out);

} // namespace assetc
