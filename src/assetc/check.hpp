#pragma once

#include <string>

namespace assetc
{

// Verify the cross-file integrity of a compiled output directory:
//   - each .hmesh/.hmat/.hman is structurally valid (the per-format validators);
//   - every nonzero texture ref in a .hmat resolves via .hman to a file that exists;
//   - every .hman entry points at a file that exists, and texture-entry hashes match
//     HashAssetRef(path-without-".ktx2") (the .hman<->.hmat parity invariant);
//   - each .hmesh material count matches its companion .hmat row count, and every
//     submesh materialSlot is in range.
// Prints each problem; returns 0 if clean, non-zero if any problem was found.
int CheckRuntime(const std::string &dir);

} // namespace assetc
