#pragma once

#include <memory>
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

// Decode an encoded image (PNG/JPEG/...) held in memory into an owning RGBA8
// Image. channels is forced to 4 (so Size() == w*h*4 matches the pixel buffer).
// On failure pixels == nullptr (see ImageError()).
Image LoadFromMemory(const uint8_t *data, int len);

// Build an owning RGBA8 Image from already-decoded pixels, expanding or
// truncating srcChannels (1..4) to 4. Useful for image data a glTF loader has
// already decoded. channels is set to 4.
Image MakeRGBA(const uint8_t *src, int w, int h, int srcChannels);

std::unique_ptr<Image> Resize(const Image &img, int w, int h);
} // namespace stb
