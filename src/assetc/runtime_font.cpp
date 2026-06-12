#include "runtime_font.hpp"

#include "../deps/fmt.hpp"

#include <bit>
#include <fstream>
#include <ios>

static_assert(std::endian::native == std::endian::little,
              "assetc currently supports little-endian targets only");

namespace
{
// Expected on-disk size for the declared counts.
uint64_t HFontBytes(uint32_t glyphCount, uint32_t kerningCount) noexcept
{
    return sizeof(assetc::FontFileHeader) +
           static_cast<uint64_t>(glyphCount) * sizeof(assetc::GpuGlyph) +
           static_cast<uint64_t>(kerningCount) * sizeof(assetc::KerningPair);
}
} // namespace

int assetc::WriteHFont(const std::string &path, FontFileHeader hdr,
                       std::span<const GpuGlyph> glyphs, std::span<const KerningPair> kerns)
{
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
    {
        fmtx::Error(fmt::format("open failed: {}", path));
        return 1;
    }

    hdr.magic        = FontMagic;
    hdr.version      = FontVersion;
    hdr.glyphCount   = static_cast<uint32_t>(glyphs.size());
    hdr.kerningCount = static_cast<uint32_t>(kerns.size());

    out.write(reinterpret_cast<const char *>(&hdr), sizeof(hdr));
    if (!glyphs.empty())
        out.write(reinterpret_cast<const char *>(glyphs.data()),
                  static_cast<std::streamsize>(glyphs.size() * sizeof(GpuGlyph)));
    if (!kerns.empty())
        out.write(reinterpret_cast<const char *>(kerns.data()),
                  static_cast<std::streamsize>(kerns.size() * sizeof(KerningPair)));

    if (!out.good())
    {
        fmtx::Error(fmt::format("write failed: {}", path));
        return 1;
    }
    return 0;
}

int assetc::ValidateHFont(const std::string &path)
{
    std::ifstream in(path, std::ios::binary | std::ios::ate);
    if (!in)
    {
        fmtx::Error(fmt::format("open failed: {}", path));
        return 1;
    }
    const auto size = static_cast<uint64_t>(in.tellg());
    in.seekg(0);

    FontFileHeader hdr{};
    if (size < sizeof(hdr) || !in.read(reinterpret_cast<char *>(&hdr), sizeof(hdr)))
    {
        fmtx::Error(fmt::format("hfont too small: {}", path));
        return 1;
    }
    if (hdr.magic != FontMagic)
    {
        fmtx::Error(fmt::format("hfont bad magic: {}", path));
        return 1;
    }
    if (hdr.version != FontVersion)
    {
        fmtx::Error(
            fmt::format("hfont bad version {} (want {}): {}", hdr.version, FontVersion, path));
        return 1;
    }
    const uint64_t expect = HFontBytes(hdr.glyphCount, hdr.kerningCount);
    if (size != expect)
    {
        fmtx::Error(fmt::format("hfont size {} != expected {} ({} glyphs, {} kerns): {}", size,
                                expect, hdr.glyphCount, hdr.kerningCount, path));
        return 1;
    }
    if (hdr.glyphCount == 0)
    {
        fmtx::Error(fmt::format("hfont has no glyphs: {}", path));
        return 1;
    }
    return 0;
}

int assetc::ReadHFont(const std::string &path, FontFileHeader &outHdr,
                      std::vector<GpuGlyph> &outGlyphs, std::vector<KerningPair> &outKerns)
{
    outGlyphs.clear();
    outKerns.clear();

    std::ifstream in(path, std::ios::binary | std::ios::ate);
    if (!in)
    {
        fmtx::Error(fmt::format("open failed: {}", path));
        return 1;
    }
    const auto size = static_cast<uint64_t>(in.tellg());
    in.seekg(0);

    if (size < sizeof(outHdr) || !in.read(reinterpret_cast<char *>(&outHdr), sizeof(outHdr)))
    {
        fmtx::Error(fmt::format("hfont too small: {}", path));
        return 1;
    }
    if (outHdr.magic != FontMagic)
    {
        fmtx::Error(fmt::format("hfont bad magic: {}", path));
        return 1;
    }
    if (outHdr.version != FontVersion)
    {
        fmtx::Error(
            fmt::format("hfont bad version {} (want {}): {}", outHdr.version, FontVersion, path));
        return 1;
    }
    if (size != HFontBytes(outHdr.glyphCount, outHdr.kerningCount))
    {
        fmtx::Error(fmt::format("hfont truncated: {}", path));
        return 1;
    }

    outGlyphs.resize(outHdr.glyphCount);
    outKerns.resize(outHdr.kerningCount);
    if (outHdr.glyphCount &&
        !in.read(reinterpret_cast<char *>(outGlyphs.data()),
                 static_cast<std::streamsize>(outGlyphs.size() * sizeof(GpuGlyph))))
    {
        fmtx::Error(fmt::format("hfont glyph read failed: {}", path));
        return 1;
    }
    if (outHdr.kerningCount &&
        !in.read(reinterpret_cast<char *>(outKerns.data()),
                 static_cast<std::streamsize>(outKerns.size() * sizeof(KerningPair))))
    {
        fmtx::Error(fmt::format("hfont kerning read failed: {}", path));
        return 1;
    }
    return 0;
}
