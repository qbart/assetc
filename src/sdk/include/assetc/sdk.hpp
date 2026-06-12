#pragma once

// assetc SDK — the runtime-side API for loading assets compiled by `assetc`.
//
// This is the entire surface an engine needs to consume assetc output: the binary
// format definitions (structs, magics, chunk ids) plus the readers/validators for
// each container, and the ref hashing used to resolve assets through the manifest.
// It depends on nothing but the C++23 standard library — no slang, ktx, fmt, etc.
//
// Typical flow on the engine side:
//   1. Open a `.hpack` (ReadPackToc) or walk the loose output directory.
//   2. Load `assets.hman` (ReadHMan) into a hash -> path map.
//   3. For a material/font/mesh ref, HashAssetRef(refString) -> look up the file.
//   4. mmap/read the `.hmesh` (chunked, zero-copy via the structs here), `.hmat`,
//      `.hanim` (ReadHAnim), `.hfont` (ReadHFont).
//
// Include this single header to pull in everything, or include the individual
// format headers below directly.

#include "assetc/hash.hpp"
#include "assetc/pack.hpp"
#include "assetc/runtime_anim.hpp"
#include "assetc/runtime_font.hpp"
#include "assetc/runtime_manifest.hpp"
#include "assetc/runtime_material.hpp"
#include "assetc/runtime_mesh.hpp"
