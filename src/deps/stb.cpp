#include "stb.hpp"

#define STB_IMAGE_IMPLEMENTATION
#include "stb/stb_image.h"

#define STB_IMAGE_RESIZE2_IMPLEMENTATION
#include "stb/stb_image_resize2.h"

#define STB_IMAGE_WRITE_IMPLEMENTATION
#include "stb/stb_image_write.h"

std::string stb::ImageError()
{
    const char *reason = stbi_failure_reason();
    return reason ? std::string(reason) : std::string("(no error)");
}

stb::Image stb::Load(const std::string &path)
{
    Image img;
    uint8_t *pixels = stbi_load(path.c_str(), &img.w, &img.h, &img.channels, STBI_rgb_alpha);
    img.pixels      = pixels;
    return img;
}

stb::Image stb::LoadFromMemory(const uint8_t *data, int len)
{
    Image img;
    int   srcChannels = 0;
    img.pixels        = stbi_load_from_memory(data, len, &img.w, &img.h, &srcChannels, STBI_rgb_alpha);
    img.channels      = 4; // STBI_rgb_alpha forces a 4-channel buffer regardless of source
    return img;
}

stb::Image stb::MakeRGBA(const uint8_t *src, int w, int h, int srcChannels)
{
    Image img;
    img.w        = w;
    img.h        = h;
    img.channels = 4;
    auto *dst    = static_cast<uint8_t *>(STBI_MALLOC(static_cast<size_t>(w) * h * 4));
    img.pixels   = dst;
    if (!dst)
        return img;

    const size_t px = static_cast<size_t>(w) * h;
    for (size_t i = 0; i < px; ++i)
    {
        const uint8_t *s = src + i * srcChannels;
        uint8_t       *d = dst + i * 4;
        switch (srcChannels)
        {
        case 1: // grayscale -> RGB, opaque
            d[0] = d[1] = d[2] = s[0];
            d[3]               = 255;
            break;
        case 2: // grayscale + alpha
            d[0] = d[1] = d[2] = s[0];
            d[3]               = s[1];
            break;
        case 3: // RGB, opaque
            d[0] = s[0];
            d[1] = s[1];
            d[2] = s[2];
            d[3] = 255;
            break;
        default: // 4 (or anything else, read 4)
            d[0] = s[0];
            d[1] = s[1];
            d[2] = s[2];
            d[3] = s[3];
            break;
        }
    }
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

std::unique_ptr<stb::Image> stb::Resize(const stb::Image &img, int w, int h)
{
    uint8_t *pixels = (uint8_t *)STBI_MALLOC(w * h * 4);
    stbir_resize_uint8_linear(img.pixels, img.w, img.h, 0, pixels, w, h, 0, STBIR_RGBA);

    auto out      = std::make_unique<stb::Image>();
    out->w        = w;
    out->h        = h;
    out->channels = 4;
    out->pixels   = pixels;

    return std::move(out);
}
