#include "stb.hpp"

#include <memory>

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

std::unique_ptr<stb::Image> Resize(const stb::Image &img, int w, int h)
{
    uint8_t *pixels = (uint8_t *)STBI_MALLOC(w * h * 4);
    stbir_resize_uint8_linear(img.pixels, img.w, img.h, 0, pixels, w, h, 0, STBIR_RGBA);

    auto out = std::make_unique(new stb::Image());
    out.w = w;
    out.h = h;
    out.channels = 4;
    out.pixels = pixels;

    return std::move(out);
}
