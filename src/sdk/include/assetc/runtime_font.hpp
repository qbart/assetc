#pragma once

#include "runtime_mesh.hpp" // MakeFourCC

#include <cstdint>
#include <span>
#include <string>
#include <vector>

namespace assetc
{

constexpr uint32_t FontMagic   = MakeFourCC('H', 'F', 'N', 'T');
constexpr uint32_t FontVersion = 1;

// A `.hfont` is the metadata sidecar for a compiled font: glyph metrics, atlas UV
// rects, and kerning. The actual glyph pixels live in a companion single-channel
// SDF `.ktx2` atlas, referenced here by `atlasTex` (FNV1a64 runtime ref, the same
// hash space as GpuMaterial texture refs) and resolved through `assets.hman`.
//
// All glyph/line metrics are in EM units (font-size independent): the engine picks
// a pixel size at draw time and multiplies. Plane bounds are y-up with the origin
// at the pen on the baseline.
//
// SDF decode (single channel, sampled from the atlas .r):
//   d_texels = distanceRange * (sample - edgeValue)   // signed atlas-texel distance
//   d_screen = d_texels * (glyphScreenPx / glyphAtlasPx)
//   coverage = clamp(d_screen + 0.5, 0, 1)
//
// On-disk layout (little-endian):
//   FontFileHeader
//   GpuGlyph[glyphCount]      sorted by codepoint ascending (binary-searchable)
//   KerningPair[kerningCount] sorted by (left, right) ascending
//
// Lookup miss: a codepoint with no glyph resolves to the codepoint-0 entry (the
// font's .notdef box), which is always present as glyph[0].

enum FontFlags : uint32_t
{
    FontFlag_Sdf  = 0u << 0, // single-channel SDF (v1 always this)
    FontFlag_Msdf = 1u << 0, // reserved: 3-channel MSDF atlas
};

#pragma pack(push, 1)

struct FontFileHeader
{
    uint32_t magic;        // == FontMagic
    uint32_t version;      // == FontVersion
    uint32_t glyphCount;   // GpuGlyph records (>= 1: codepoint 0 == .notdef)
    uint32_t kerningCount; // KerningPair records (0 if the font has no 'kern' table)

    uint64_t atlasTex; // FNV1a64 runtime ref of the companion SDF .ktx2

    float ascent;        // em, above baseline (> 0)
    float descent;       // em, below baseline (< 0)
    float lineGap;       // em, extra leading; line advance = ascent - descent + lineGap
    float distanceRange; // SDF spread in atlas texels (see decode above)
    float edgeValue;     // normalized atlas value at the glyph edge (~0.502)

    uint16_t atlasWidth;  // SDF atlas dimensions in texels
    uint16_t atlasHeight;
    uint32_t flags; // FontFlags
};
static_assert(sizeof(FontFileHeader) == 52, "FontFileHeader must be 52 bytes");

// One glyph. Plane bounds are the quad to emit relative to the pen (em, y-up,
// baseline at y=0); a draw places it at [pen + plane*pixelSize]. UV rect is into
// the atlas, normalized [0,1]. A whitespace glyph (e.g. space) has a zero-area
// plane and uv and contributes only `advance`.
struct GpuGlyph
{
    uint32_t codepoint; // Unicode scalar; 0 = default/.notdef fallback
    float    advance;   // em, pen advance after this glyph

    float planeLeft;
    float planeBottom;
    float planeRight;
    float planeTop;

    float uvLeft;
    float uvTop;
    float uvRight;
    float uvBottom;
};
static_assert(sizeof(GpuGlyph) == 40, "GpuGlyph must be 40 bytes");

struct KerningPair
{
    uint32_t left;    // codepoint
    uint32_t right;   // codepoint
    float    advance; // em, added to `left`'s advance when followed by `right`
};
static_assert(sizeof(KerningPair) == 12, "KerningPair must be 12 bytes");

#pragma pack(pop)

// Serialize a font to a `.hfont`. `hdr` supplies line metrics, atlas ref and
// dimensions; glyphCount/kerningCount are set from the spans (magic/version too).
// Glyphs must be sorted by codepoint ascending with glyph[0].codepoint == 0.
// Returns 0 on success, non-zero with a logged error.
int WriteHFont(const std::string &path, FontFileHeader hdr, std::span<const GpuGlyph> glyphs,
               std::span<const KerningPair> kerns);

// Re-read a `.hfont`: checks magic/version and that the file size equals
// header + glyphCount*sizeof(GpuGlyph) + kerningCount*sizeof(KerningPair) and that
// at least the codepoint-0 glyph is present. Returns 0 on success.
int ValidateHFont(const std::string &path);

// Parse a `.hfont` into `outHdr`/`outGlyphs`/`outKerns` (cleared first). Returns 0
// on success, non-zero with a logged error on bad magic/version or truncation.
int ReadHFont(const std::string &path, FontFileHeader &outHdr, std::vector<GpuGlyph> &outGlyphs,
              std::vector<KerningPair> &outKerns);

} // namespace assetc
