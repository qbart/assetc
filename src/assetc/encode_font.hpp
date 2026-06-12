#pragma once

#include <string>

namespace assetc
{

// Compile a TrueType/OpenType font (`srcPath`, .ttf/.otf) into:
//   - `hfontPath` : a `.hfont` metadata sidecar (glyph metrics + kerning)
//   - `atlasPath` : a single-channel SDF glyph atlas as a lossless `.ktx2`
//
// The charset is ASCII printable (U+0020..U+007E) plus Latin-1 (U+00A0..U+00FF);
// codepoints the font lacks are skipped, and the font's .notdef glyph is emitted as
// the codepoint-0 fallback. `atlasRef` is the canonical runtime ref (lowercase, no
// extension) for the atlas; its FNV1a64 is stored in the .hfont's `atlasTex` and
// must match the manifest entry the caller emits for `atlasPath`.
//
// Returns 0 on success, non-zero with a logged error.
int EncodeFont(const std::string &srcPath, const std::string &hfontPath,
               const std::string &atlasPath, const std::string &atlasRef, unsigned threadCount);

} // namespace assetc
