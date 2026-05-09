#include "stb.hpp"

#define STB_IMAGE_IMPLEMENTATION
#include "stb/stb_image.h"

#define STB_IMAGE_RESIZE2_IMPLEMENTATION
#include "stb/stb_image_resize2.h"

#define STB_IMAGE_WRITE_IMPLEMENTATION
#include "stb/stb_image_write.h"

std::string stb::ImageError() { return stbi_failure_reason(); }

stb::Image stb::Load(const std::string &path)
{
    Image img;
    uint8_t *pixels = stbi_load(path.c_str(), &img.w, &img.h, &img.channels, STBI_rgb_alpha);
    img.pixels      = pixels;
    return img;
}

stb::Image::~Image()
{
    if (pixels != nullptr)
    {
        stbi_image_free(pixels);
        pixels = nullptr;
    }
}

int stb::Image::Size() const { return w * h * channels; }
