#pragma once

#include <stdint.h>
#include <string>

namespace stb
{

struct Image
{
    int w           = 0;
    int h           = 0;
    int channels    = 0;
    uint8_t *pixels = nullptr;

    ~Image();
    int Size() const;
};

std::string ImageError();
Image Load(const std::string &path);
Image &Resize(const Image &img, int w, int h);
} // namespace stb
