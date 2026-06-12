#include "check.hpp"

#include "assetc/runtime_font.hpp"

#include <cstdint>
#include <filesystem>
#include <fstream>
#include <vector>

namespace fs = std::filesystem;
using namespace assetc;

int main()
{
    const auto root = fs::temp_directory_path() / "assetc_test_font";
    fs::remove_all(root);
    fs::create_directories(root);
    const auto path = (root / "f.hfont").string();

    // The on-disk struct sizes are the format contract.
    CHECK_EQ(sizeof(FontFileHeader), 52u);
    CHECK_EQ(sizeof(GpuGlyph), 40u);
    CHECK_EQ(sizeof(KerningPair), 12u);

    std::vector<GpuGlyph> glyphs = {
        GpuGlyph{.codepoint = 0, .advance = 0.5f}, // .notdef fallback
        GpuGlyph{.codepoint = 'A',
                 .advance     = 0.6f,
                 .planeLeft   = 0.0f,
                 .planeBottom = 0.0f,
                 .planeRight  = 0.5f,
                 .planeTop    = 0.7f,
                 .uvLeft      = 0.0f,
                 .uvTop       = 0.0f,
                 .uvRight     = 0.1f,
                 .uvBottom    = 0.1f},
        GpuGlyph{.codepoint = 'B', .advance = 0.6f},
    };
    std::vector<KerningPair> kerns = {KerningPair{'A', 'B', -0.05f}};

    FontFileHeader hdr{};
    hdr.atlasTex      = 0x1234567890abcdefULL;
    hdr.ascent        = 0.8f;
    hdr.descent       = -0.2f;
    hdr.lineGap       = 0.1f;
    hdr.distanceRange = 12.0f;
    hdr.edgeValue     = 0.5f;
    hdr.atlasWidth    = 256;
    hdr.atlasHeight   = 128;
    hdr.flags         = FontFlag_Sdf;

    CHECK_EQ(WriteHFont(path, hdr, glyphs, kerns), 0);
    CHECK_EQ(ValidateHFont(path), 0);

    FontFileHeader           rhdr{};
    std::vector<GpuGlyph>    rglyphs;
    std::vector<KerningPair> rkerns;
    CHECK_EQ(ReadHFont(path, rhdr, rglyphs, rkerns), 0);

    CHECK_EQ(rhdr.magic, FontMagic);
    CHECK_EQ(rhdr.version, FontVersion);
    CHECK_EQ(rhdr.glyphCount, 3u);
    CHECK_EQ(rhdr.kerningCount, 1u);
    CHECK_EQ(rhdr.atlasTex, 0x1234567890abcdefULL);
    CHECK_EQ(rhdr.atlasWidth, uint16_t(256));
    CHECK_EQ(rhdr.atlasHeight, uint16_t(128));
    CHECK_NEAR(rhdr.ascent, 0.8f, 1e-6);

    CHECK_EQ(rglyphs.size(), 3u);
    CHECK_EQ(rglyphs[0].codepoint, 0u);
    CHECK_EQ(rglyphs[1].codepoint, uint32_t('A'));
    CHECK_NEAR(rglyphs[1].planeTop, 0.7f, 1e-6);
    CHECK_NEAR(rglyphs[1].uvRight, 0.1f, 1e-6);

    CHECK_EQ(rkerns.size(), 1u);
    CHECK_EQ(rkerns[0].left, uint32_t('A'));
    CHECK_EQ(rkerns[0].right, uint32_t('B'));
    CHECK_NEAR(rkerns[0].advance, -0.05f, 1e-6);

    // A font with no kerning still round-trips.
    const auto noKern = (root / "g.hfont").string();
    CHECK_EQ(WriteHFont(noKern, hdr, glyphs, {}), 0);
    CHECK_EQ(ValidateHFont(noKern), 0);
    CHECK_EQ(ReadHFont(noKern, rhdr, rglyphs, rkerns), 0);
    CHECK_EQ(rkerns.size(), 0u);

    // Corrupt magic -> validate + read fail.
    {
        std::ofstream bad((root / "bad.hfont").string(), std::ios::binary | std::ios::trunc);
        const char    junk[] = "not a font at all!!";
        bad.write(junk, sizeof(junk));
    }
    CHECK(ValidateHFont((root / "bad.hfont").string()) != 0);
    CHECK(ReadHFont((root / "bad.hfont").string(), rhdr, rglyphs, rkerns) != 0);

    fs::remove_all(root);
    return test::summary();
}
