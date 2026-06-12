# .hfont file format (v1)

Little-endian. Magic `"HFNT"`. A TrueType/OpenType font (`*.ttf`/`*.otf`) compiles to a `.hfont` metadata sidecar plus a companion **single-channel SDF atlas** `.ktx2`. The `.hfont` carries glyph metrics, atlas UV rects, and kerning; the atlas holds the signed-distance-field pixels. Together they render resolution-independent, smoothly-antialiased text from one atlas at any size — in screen space (HUD/UI) or in the 3D world (labels, signage, billboards) — without the aliasing of a fixed-size bitmap font.

For a source `assets/ui/inter.ttf` the outputs are:

```
runtime/ui/inter.hfont    glyph metrics + kerning
runtime/ui/inter.ktx2     single-channel SDF atlas (lossless R8G8B8A8 + Zstd)
runtime/assets.hman       the atlas is registered here (Texture kind), keyed by HashAssetRef("ui/inter")
```

The atlas is referenced from the header by `atlasTex` — the same FNV1a64 hash space as material texture refs — and resolved through [`assets.hman`](hman.md) exactly like a material's `baseColorTex`, so the engine reuses its existing texture-loading path.

## Charset & SDF parameters

The charset is ASCII printable (U+0020–U+007E) plus Latin-1 (U+00A0–U+00FF). Codepoints the font lacks are skipped; the font's `.notdef` glyph is emitted as the **codepoint-0 fallback** so a lookup miss always has a glyph to draw. Glyphs are rasterized at 48 px/em with a ~6-texel SDF spread (via `stb_truetype`'s `stbtt_GetGlyphSDF`) and shelf-packed into the atlas. The SDF is stored **losslessly** — block compression would shred the distance field — which Zstd keeps small.

## Layout

```
+-----------------------------------+
| FontFileHeader (52 B)             |
+-----------------------------------+
| GpuGlyph[glyphCount]  (40 B each) |  sorted by codepoint ascending
+-----------------------------------+
| KerningPair[kerningCount] (12 B)  |  sorted by (left, right)
+-----------------------------------+
```

The file is exactly `52 + glyphCount * 40 + kerningCount * 12` bytes — directly mmappable.

## FontFileHeader (52 B)

| Offset | Field          | Type | Notes                                                       |
| -----: | -------------- | ---- | ----------------------------------------------------------- |
|      0 | `magic`        | u32  | `"HFNT"`                                                    |
|      4 | `version`      | u32  | `1`                                                         |
|      8 | `glyphCount`   | u32  | ≥ 1 (`glyph[0].codepoint == 0` is the `.notdef` fallback)   |
|     12 | `kerningCount` | u32  | `0` if the font has no legacy `kern` table                  |
|     16 | `atlasTex`     | u64  | FNV1a64 runtime ref of the companion SDF `.ktx2`            |
|     24 | `ascent`       | f32  | em, above baseline                                          |
|     28 | `descent`      | f32  | em, below baseline (negative)                               |
|     32 | `lineGap`      | f32  | em; line advance = `ascent - descent + lineGap`             |
|     36 | `distanceRange`| f32  | SDF spread in atlas texels (drives screen-space AA)         |
|     40 | `edgeValue`    | f32  | normalized atlas value at the glyph edge (~0.502)           |
|     44 | `atlasWidth`   | u16  | atlas texels                                                |
|     46 | `atlasHeight`  | u16  | atlas texels                                                |
|     48 | `flags`        | u32  | bit 0 = MSDF (reserved; v1 is single-channel SDF)           |

All glyph/line metrics are in **em units** (font-size independent): multiply by the target pixel size (screen) or world size (3D) at draw time.

## GpuGlyph (40 B)

| Offset | Field         | Type | Notes                                                      |
| -----: | ------------- | ---- | ---------------------------------------------------------- |
|      0 | `codepoint`   | u32  | Unicode scalar; `0` = `.notdef` fallback                   |
|      4 | `advance`     | f32  | em, pen advance after this glyph                           |
|      8 | `planeLeft`   | f32  | em, quad bounds, **y-up**, origin at the pen on the baseline |
|     12 | `planeBottom` | f32  |                                                            |
|     16 | `planeRight`  | f32  |                                                            |
|     20 | `planeTop`    | f32  |                                                            |
|     24 | `uvLeft`      | f32  | normalized atlas UV (top-left origin)                      |
|     28 | `uvTop`       | f32  |                                                            |
|     32 | `uvRight`     | f32  |                                                            |
|     36 | `uvBottom`    | f32  |                                                            |

A whitespace glyph (e.g. space) has a zero-area plane and uv and contributes only `advance`. Glyphs are sorted by `codepoint`, so a reader may binary-search.

## KerningPair (12 B)

| Offset | Field     | Type | Notes                                                       |
| -----: | --------- | ---- | ----------------------------------------------------------- |
|      0 | `left`    | u32  | codepoint                                                   |
|      4 | `right`   | u32  | codepoint                                                   |
|      8 | `advance` | f32  | em, added to `left`'s advance when followed by `right`      |

`stb_truetype` reads the legacy `kern` table only; fonts that kern exclusively via GPOS produce `kerningCount == 0`.

## Rendering (SDF decode)

Build one quad per glyph from its `plane*`/`uv*` rects, advancing the pen by `advance` (plus any `KerningPair`). Place quads in pixel coords under an orthographic projection for HUD/UI, or in world units under the MVP (optionally billboarded) for in-world text — the asset is placement-agnostic. The atlas stores signed distance in `.r`; sample with **linear filtering + mips**, then convert distance to coverage. `distanceRange`/`edgeValue` make the antialiasing correct at any scale (`fwidth` handles perspective, so the same shader serves 2D and 3D):

```glsl
// distanceRange, edgeValue from FontFileHeader; uAtlas sampled linear + mips
float sd   = texture(uAtlas, uv).r;
vec2  unit = vec2(distanceRange) / vec2(textureSize(uAtlas, 0));
float pxr  = max(0.5 * dot(unit, vec2(1.0) / fwidth(uv)), 1.0);  // screen-px range
float cov  = clamp(pxr * (sd - edgeValue) + 0.5, 0.0, 1.0);       // text coverage / alpha
```

Outlines, glow, and soft shadows come free by thresholding `sd` away from `edgeValue` (the spread gives ~6 texels of room). Single-channel SDF rounds very sharp corners under extreme magnification; the `flags` MSDF bit is reserved to add a 3-channel atlas later without a format break.

## Endianness

Little-endian only (`src/assetc/runtime_font.cpp` carries the matching `static_assert`).
