#pragma once

#include "stb.hpp"
#include <ktx.h>

namespace ktx
{
struct KTX
{
};

KTX_error_code FromImageToASTC(const stb::Image &img, const std::string &destPath);
} // namespace ktx
