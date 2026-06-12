#include "encode_font.hpp"

#include "encode_mesh.hpp" // HashAssetRef
#include "runtime_font.hpp"

#include "../deps/fmt.hpp"
#include "../deps/ktx.hpp"
#include "../deps/stb.hpp"

// Declarations only; the implementation is compiled once in src/deps/stb.cpp.
#include "../deps/stb/stb_truetype.h"

#include <algorithm>
#include <cstdint>
#include <fstream>
#include <vector>

namespace assetc
{
namespace
{

// Atlas rasterization parameters. The SDF spread is symmetric (~kPadding texels on
// each side of the edge) at kPxPerEm. Metrics are stored in em units, so this one
// atlas serves any draw size. The SDF is stored losslessly (block compression would
// shred the distance field), so a generous size is cheap after Zstd.
constexpr float         kPxPerEm    = 48.0f;
constexpr int           kPadding    = 6;
constexpr unsigned char kOnedge     = 128;
constexpr uint32_t      kAtlasWidth = 512; // shelf-packed; height grows to fit

uint32_t AlignUp4(uint32_t v) noexcept { return (v + 3u) & ~3u; }

struct RasterGlyph
{
    uint32_t                   codepoint = 0;
    int                        w = 0, h = 0;       // SDF bitmap size (0 = no outline)
    int                        xoff = 0, yoff = 0; // bitmap top-left vs pen, px, y-down
    int                        ax = 0, ay = 0;     // atlas placement (top-left), px
    std::vector<unsigned char> bitmap;
    float                      advanceEm = 0.0f;
};

} // namespace

int EncodeFont(const std::string &srcPath, const std::string &hfontPath,
               const std::string &atlasPath, const std::string &atlasRef, unsigned threadCount)
{
    // Load the whole font file: stb_truetype keeps a pointer into this buffer, so it
    // must outlive every stbtt_* call below.
    std::ifstream in(srcPath, std::ios::binary | std::ios::ate);
    if (!in)
    {
        fmtx::Error(fmt::format("font: cannot open {}", srcPath));
        return 1;
    }
    const auto sz = static_cast<std::streamsize>(in.tellg());
    in.seekg(0);
    std::vector<unsigned char> font(static_cast<size_t>(sz > 0 ? sz : 0));
    if (sz <= 0 || !in.read(reinterpret_cast<char *>(font.data()), sz))
    {
        fmtx::Error(fmt::format("font: read failed: {}", srcPath));
        return 1;
    }

    stbtt_fontinfo info;
    const int      offset = stbtt_GetFontOffsetForIndex(font.data(), 0);
    if (offset < 0 || !stbtt_InitFont(&info, font.data(), offset))
    {
        fmtx::Error(fmt::format("font: not a valid TrueType/OpenType font: {}", srcPath));
        return 1;
    }

    const float scale          = stbtt_ScaleForMappingEmToPixels(&info, kPxPerEm);
    const float emPerFontUnit  = stbtt_ScaleForMappingEmToPixels(&info, 1.0f); // 1 / unitsPerEm
    const float emPerPx        = 1.0f / kPxPerEm;
    const float pixelDistScale = static_cast<float>(kOnedge) / static_cast<float>(kPadding);

    int ascent = 0, descent = 0, lineGap = 0;
    stbtt_GetFontVMetrics(&info, &ascent, &descent, &lineGap);

    // Charset: ASCII printable + Latin-1. Codepoint 0 (the .notdef slot) is added
    // first as the explicit lookup-miss fallback.
    std::vector<uint32_t> codepoints;
    codepoints.push_back(0);
    for (uint32_t c = 0x20; c <= 0x7E; ++c) codepoints.push_back(c);
    for (uint32_t c = 0xA0; c <= 0xFF; ++c) codepoints.push_back(c);

    std::vector<RasterGlyph> glyphs;
    glyphs.reserve(codepoints.size());

    for (uint32_t cp : codepoints)
    {
        const int gi = (cp == 0) ? 0 : stbtt_FindGlyphIndex(&info, static_cast<int>(cp));
        if (cp != 0 && gi == 0)
            continue; // font lacks this codepoint; runtime falls back to glyph 0

        int adv = 0, lsb = 0;
        stbtt_GetGlyphHMetrics(&info, gi, &adv, &lsb);

        RasterGlyph g;
        g.codepoint = cp;
        g.advanceEm = static_cast<float>(adv) * emPerFontUnit;

        int            w = 0, h = 0, xoff = 0, yoff = 0;
        unsigned char *sdf = stbtt_GetGlyphSDF(&info, scale, gi, kPadding, kOnedge, pixelDistScale,
                                               &w, &h, &xoff, &yoff);
        if (sdf && w > 0 && h > 0)
        {
            g.w = w;
            g.h = h;
            g.xoff = xoff;
            g.yoff = yoff;
            g.bitmap.assign(sdf, sdf + static_cast<size_t>(w) * h);
        }
        if (sdf)
            stbtt_FreeSDF(sdf, nullptr);
        glyphs.push_back(std::move(g));
    }

    if (glyphs.empty() || glyphs.front().codepoint != 0)
    {
        fmtx::Error(fmt::format("font: no glyphs encoded: {}", srcPath));
        return 1;
    }

    // Shelf-pack the glyph bitmaps into a kAtlasWidth-wide atlas (height grows).
    uint32_t atlasW = kAtlasWidth;
    for (const auto &g : glyphs)
        atlasW = std::max<uint32_t>(atlasW, static_cast<uint32_t>(g.w));
    atlasW = AlignUp4(atlasW);

    uint32_t penX = 0, penY = 0, rowH = 0;
    for (auto &g : glyphs)
    {
        if (g.w == 0 || g.h == 0)
            continue;
        if (penX + static_cast<uint32_t>(g.w) > atlasW)
        {
            penX = 0;
            penY += rowH;
            rowH = 0;
        }
        g.ax = static_cast<int>(penX);
        g.ay = static_cast<int>(penY);
        penX += static_cast<uint32_t>(g.w);
        rowH = std::max(rowH, static_cast<uint32_t>(g.h));
    }
    const uint32_t atlasH = AlignUp4(penY + rowH);

    if (atlasW == 0 || atlasH == 0 || atlasW > 0xFFFFu || atlasH > 0xFFFFu)
    {
        fmtx::Error(
            fmt::format("font: atlas size out of range ({}x{}): {}", atlasW, atlasH, srcPath));
        return 1;
    }

    // Blit glyph SDFs into a single-channel atlas (0 = far outside the glyph).
    std::vector<unsigned char> atlas(static_cast<size_t>(atlasW) * atlasH, 0);
    for (const auto &g : glyphs)
    {
        for (int y = 0; y < g.h; ++y)
        {
            const unsigned char *srcRow = g.bitmap.data() + static_cast<size_t>(y) * g.w;
            unsigned char       *dstRow =
                atlas.data() + static_cast<size_t>(g.ay + y) * atlasW + g.ax;
            std::copy(srcRow, srcRow + g.w, dstRow);
        }
    }

    // Build the .hfont glyph table: em-space plane bounds (y-up, baseline origin) and
    // normalized atlas UVs.
    std::vector<GpuGlyph> outGlyphs;
    outGlyphs.reserve(glyphs.size());
    const float invW = 1.0f / static_cast<float>(atlasW);
    const float invH = 1.0f / static_cast<float>(atlasH);
    for (const auto &g : glyphs)
    {
        GpuGlyph og{};
        og.codepoint = g.codepoint;
        og.advance   = g.advanceEm;
        if (g.w > 0 && g.h > 0)
        {
            og.planeLeft   = static_cast<float>(g.xoff) * emPerPx;
            og.planeTop    = static_cast<float>(-g.yoff) * emPerPx;
            og.planeRight  = static_cast<float>(g.xoff + g.w) * emPerPx;
            og.planeBottom = static_cast<float>(-(g.yoff + g.h)) * emPerPx;

            og.uvLeft   = static_cast<float>(g.ax) * invW;
            og.uvTop    = static_cast<float>(g.ay) * invH;
            og.uvRight  = static_cast<float>(g.ax + g.w) * invW;
            og.uvBottom = static_cast<float>(g.ay + g.h) * invH;
        }
        outGlyphs.push_back(og);
    }
    std::sort(outGlyphs.begin(), outGlyphs.end(),
              [](const GpuGlyph &a, const GpuGlyph &b) { return a.codepoint < b.codepoint; });

    // Kerning: stb_truetype reads the legacy 'kern' table only (GPOS-only fonts yield
    // none). Emit pairs among encoded codepoints with a nonzero adjustment.
    std::vector<KerningPair> kerns;
    for (const auto &a : outGlyphs)
    {
        if (a.codepoint == 0)
            continue;
        for (const auto &b : outGlyphs)
        {
            if (b.codepoint == 0)
                continue;
            const int k = stbtt_GetCodepointKernAdvance(&info, static_cast<int>(a.codepoint),
                                                         static_cast<int>(b.codepoint));
            if (k != 0)
                kerns.push_back(
                    {a.codepoint, b.codepoint, static_cast<float>(k) * emPerFontUnit});
        }
    }

    // Encode the atlas as a lossless single-channel KTX2 (the SDF must not be block-
    // compressed). MakeRGBA replicates the SDF into RGB; the engine samples .r.
    stb::Image img =
        stb::MakeRGBA(atlas.data(), static_cast<int>(atlasW), static_cast<int>(atlasH), 1);
    if (!img.pixels)
    {
        fmtx::Error(fmt::format("font: atlas alloc failed: {}", srcPath));
        return 1;
    }
    const auto err =
        ktx::FromImageToRawKtx2(img, atlasPath, ktx::UASTCMode::Grayscale, threadCount);
    if (err != KTX_SUCCESS)
    {
        fmtx::Error(fmt::format("font: atlas ktx write failed (code {}): {}",
                                static_cast<int>(err), atlasPath));
        return 1;
    }

    FontFileHeader hdr{};
    hdr.atlasTex      = HashAssetRef(atlasRef);
    hdr.ascent        = static_cast<float>(ascent) * emPerFontUnit;
    hdr.descent       = static_cast<float>(descent) * emPerFontUnit;
    hdr.lineGap       = static_cast<float>(lineGap) * emPerFontUnit;
    hdr.distanceRange = 255.0f / pixelDistScale;
    hdr.edgeValue     = static_cast<float>(kOnedge) / 255.0f;
    hdr.atlasWidth    = static_cast<uint16_t>(atlasW);
    hdr.atlasHeight   = static_cast<uint16_t>(atlasH);
    hdr.flags         = FontFlag_Sdf;

    if (WriteHFont(hfontPath, hdr, outGlyphs, kerns) != 0)
        return 1;

    fmtx::Success(fmt::format("font {} glyphs / {} kerns / {}x{} atlas -> {}", outGlyphs.size(),
                              kerns.size(), atlasW, atlasH, hfontPath));
    return 0;
}

} // namespace assetc
