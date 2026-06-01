#pragma once

#include <string>

namespace assetc
{

// Parse an Adobe/Resolve `.cube` 3D color-grading LUT and write it as a KTX2
// 3D texture (see ktx::FromLut3DToKtx2 for the container details).
//
// Intended for a post-tonemap grade LUT in a game postprocessing stack: the
// tonemapper (e.g. ACES) is analytic shader math, and this LUT applies after
// it on sRGB-encoded display values. The table is stored verbatim (sRGB-in /
// sRGB-out, interpolated in its authored space) as 8-bit UNORM — assetc does
// NOT bake the LUT into linear. The linear<->LUT-space conversion is the
// shader's job; assetc only stores values and tags the texture linear so the
// GPU doesn't auto-decode them.
//
// Only 3D LUTs (LUT_3D_SIZE) are supported; a 1D LUT is rejected. Returns 0 on
// success, non-zero on any parse/IO/encode failure.
int EncodeLutCube(const std::string &srcPath, const std::string &destPath);

} // namespace assetc
