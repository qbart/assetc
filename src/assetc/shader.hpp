#pragma once

#include <string>
#include <vector>

namespace assetc
{

// A `.shader` directory bundles one or more `*.slang` files that together form a
// single pipeline. CompileShaderFolder enumerates the *defined entry points*
// across those files via the Slang reflection API and compiles each one to its
// own SPIR-V module, written into `outDir` as `<entryPoint>.spv`.
//
// The stage of each entry point comes from its `[shader("vertex"|"fragment"|...)]`
// attribute (NOT from the file name), so a single file may declare several entry
// points of different stages, and a folder may emit any mix of stages (e.g. a
// depth pass with only a vertex entry point, or a bloom pass with two fragment
// entry points `down` and `up`).
//
// Entry-point names must be unique within the folder (it maps to one pipeline);
// a collision is a hard error. On success `outEntryPoints`, if non-null, receives
// the emitted entry-point names in deterministic order (the `.spv` stems).
//
// Returns 0 on success, non-zero on failure.
int CompileShaderFolder(const std::string &shaderDir, const std::string &outDir,
                        std::vector<std::string> *outEntryPoints = nullptr);

} // namespace assetc
