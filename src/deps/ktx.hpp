#pragma once

#include "stb.hpp"
#include <ktx.h>

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

KTX_error_code FromImageToASTC(const stb::Image &img, const std::string &destPath, ASTCMode mode);
} // namespace ktx
