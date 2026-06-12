#pragma once

#include <string>

namespace assetc
{

// Walk a compiled output directory (default "runtime") and print a human-readable
// report of every recognized artifact: .hmesh geometry stats, .hmat material
// tables, .hman manifest entries, and .ktx2 textures (incl. .lut/.env variants),
// followed by aggregate totals. Returns 0 on success, non-zero if the directory
// is missing. Unreadable/corrupt individual files are reported inline and skipped.
int InspectRuntime(const std::string &dir);

// Print what's inside a `.hpack`: a per-kind summary (meshes/textures/...) and a
// path-sorted entry listing with sizes. Returns 0 on success, non-zero if the
// pack can't be read. (Reporting tool, kept out of the dependency-free SDK.)
int InspectPack(const std::string &packPath);

} // namespace assetc
