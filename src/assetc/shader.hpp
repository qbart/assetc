#pragma once

#include <string>

namespace assetc
{

// A `.shader` directory bundles a `vertex.slang` and a `fragment.slang`.
// CompileShaderFolder compiles both stages to SPIR-V via the Slang compiler
// API and writes them into `outDir` as `vertex.spv` and `fragment.spv`.
//
// Each stage file must expose a `main` entry point; its stage is implied by
// the file name (vertex.slang -> vertex, fragment.slang -> fragment), so the
// `[shader("...")]` attribute is optional.
//
// Returns 0 on success, non-zero on failure.
int CompileShaderFolder(const std::string &shaderDir, const std::string &outDir);

} // namespace assetc
