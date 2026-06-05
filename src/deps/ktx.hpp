#pragma once

#include "stb.hpp"
#include <cstdint>
#include <ktx.h>
#include <string>

namespace ktx
{
struct KTX
{
};

enum class ASTCMode
{
    Color,     // sRGB OETF, no swizzle
    Normal,    // linear OETF, input swizzle "rg01"
    Grayscale, // linear OETF, input swizzle "r001"
};

enum class UASTCMode
{
    Color,     // sRGB OETF, no swizzle
    Normal,    // linear OETF, input swizzle "rg01"
    Grayscale, // linear OETF, input swizzle "r001"
};

KTX_error_code FromImageToASTC(const stb::Image &img, const std::string &destPath, ASTCMode mode);
KTX_error_code FromImageToUASTC(const stb::Image &img, const std::string &destPath, UASTCMode mode, unsigned threadCount);

// Encode 6 face images as a UASTC-compressed KTX2 cubemap.
// Faces must be passed in libktx/Vulkan face order: +X, -X, +Y, -Y, +Z, -Z.
// All faces must be square and identically sized.
KTX_error_code FromCubemapToUASTC(stb::Image (&faces)[6], const std::string &destPath, unsigned threadCount);

// Write a 3D color-grading LUT as a KTX2 3D texture.
//
//   - format VK_FORMAT_R8G8B8A8_UNORM -> linear DFD, so the GPU does NOT apply
//     an sRGB->linear decode when the LUT is sampled. The stored values pass
//     through untouched; the linear<->LUT-space conversion lives in the shader.
//   - single mip level (LUTs are never mipmapped), clamp-to-edge at sample time.
//   - lossless Zstd supercompression: bytes shrink, values are preserved exactly
//     (never Basis/UASTC — block compression would shred a LUT).
//
// `rgba8` is `size`^3 voxels of RGBA8 in x-fastest order (R varies quickest),
// which is the native order of a .cube file. `rgba8Size` must equal size^3 * 4.
KTX_error_code FromLut3DToKtx2(uint32_t size, const uint8_t *rgba8, size_t rgba8Size,
                               const std::string &destPath);
} // namespace ktx
