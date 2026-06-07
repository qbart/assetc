#include "check.hpp"

#include "deps/ktx.hpp"
#include "deps/stb.hpp"

#include <cstdint>
#include <filesystem>
#include <fstream>
#include <vector>

namespace fs = std::filesystem;

// Read the few KTX2 header fields we care about (after the 12-byte identifier).
struct Ktx2Head
{
    uint32_t vkFormat, typeSize, w, h, d, layers, faces, levels, scheme;
};

static bool ReadHead(const std::string &path, Ktx2Head &h)
{
    std::ifstream in(path, std::ios::binary);
    uint8_t       id[12] = {};
    if (!in.read(reinterpret_cast<char *>(id), sizeof(id)))
        return false;
    if (id[0] != 0xAB || id[1] != 'K' || id[2] != 'T' || id[3] != 'X')
        return false;
    return static_cast<bool>(in.read(reinterpret_cast<char *>(&h), sizeof(h)));
}

int main()
{
    const auto dir = fs::temp_directory_path() / "assetc_test_ktx_raw";
    fs::create_directories(dir);

    // 8x8 RGBA gradient.
    std::vector<uint8_t> px(8 * 8 * 4);
    for (int i = 0; i < 8 * 8; ++i)
    {
        px[i * 4 + 0] = static_cast<uint8_t>(i * 4);
        px[i * 4 + 1] = static_cast<uint8_t>(255 - i);
        px[i * 4 + 2] = 128;
        px[i * 4 + 3] = 255;
    }
    stb::Image img = stb::MakeRGBA(px.data(), 8, 8, 4);
    CHECK(img.pixels != nullptr);

    // Color mode -> sRGB raw, Zstd supercompression, not UASTC.
    const auto colorPath = (dir / "color.ktx2").string();
    CHECK_EQ(ktx::FromImageToRawKtx2(img, colorPath, ktx::UASTCMode::Color, 1), KTX_SUCCESS);
    Ktx2Head h{};
    CHECK(ReadHead(colorPath, h));
    CHECK_EQ(h.vkFormat, 43u); // VK_FORMAT_R8G8B8A8_SRGB
    CHECK_EQ(h.w, 8u);
    CHECK_EQ(h.h, 8u);
    CHECK_EQ(h.scheme, 2u); // KTX_SS_ZSTD
    CHECK(h.levels >= 1u);

    // Grayscale/normal modes -> linear UNORM raw.
    const auto linPath = (dir / "lin.ktx2").string();
    CHECK_EQ(ktx::FromImageToRawKtx2(img, linPath, ktx::UASTCMode::Grayscale, 1), KTX_SUCCESS);
    Ktx2Head h2{};
    CHECK(ReadHead(linPath, h2));
    CHECK_EQ(h2.vkFormat, 37u); // VK_FORMAT_R8G8B8A8_UNORM

    fs::remove_all(dir);
    return test::summary();
}
