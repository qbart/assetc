#pragma once

#include <cstdint>
#include <string_view>

namespace assetc
{

// FNV1a64 hash of a runtime-relative asset reference. This is the hash space used
// by every on-disk ref in the format: GpuMaterial texture refs, .hmesh material
// refs, .hfont atlasTex, and the keys in `assets.hman`. The engine computes it on
// a normalized ref string to resolve that ref to a file via the manifest.
//
// Caller-side normalization (lowercase, '/'-separated, no extension, no leading
// "runtime/" prefix) is the authoritative format; this function additionally
// lowercases ASCII A-Z for belt-and-braces consistency.
//
// Note glTF-embedded textures are NOT addressed this way — they are content-hashed
// into "tex/<hash>.ktx2" (see encode_material / runtime_manifest.hpp). HashAssetRef
// still covers name-addressed refs: shader entry points ("<sourceRef>/<entryPoint>"
// plus ".spv") and font SDF atlases ("<sourceRef>" plus ".ktx2").
uint64_t HashAssetRef(std::string_view runtimeRefNoExt) noexcept;

} // namespace assetc
