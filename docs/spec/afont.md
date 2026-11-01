# `.afont` fonts

A compiled font: glyph metrics, atlas rectangles and kerning for text drawn
from a signed-distance-field (SDF) atlas, a companion texture. See
[`docs/features/fonts.md`](../features/fonts.md#build). A
[chunked file](chunks.md) with format name `AFONT`, version `1`; the content
follows assetc-cpp's `.hfont` v1.

All metrics are in **em** (font-size independent, y up): multiply by the
size to draw at (pixels on screen, units in the world).

## Chunks

| Name      | Required | Payload                                                 |
|-----------|----------|---------------------------------------------------------|
| `DESC`    | yes      | [`FontDesc`](#fontdesc-56-bytes)                         |
| `GLYPHS`  | yes      | [`Glyph`](#glyph-40-bytes)`[glyph_count]`, by codepoint ascending |
| `KERNING` | yes      | [`KerningPair`](#kerningpair-12-bytes)`[kerning_count]`, by (left, right) ascending; may be empty |

## FontDesc (56 bytes)

| Offset | Size | Field            | Notes                                                    |
|-------:|-----:|------------------|----------------------------------------------------------|
|      0 |    4 | `glyph_count`    | u32, at least 1 (glyph 0 is codepoint 0, the fallback)   |
|      4 |    4 | `kerning_count`  | u32                                                      |
|      8 |    8 | `atlas`          | u64, asset ID of the SDF atlas texture                   |
|     16 |    4 | `ascent`         | f32, em above the baseline                               |
|     20 |    4 | `descent`        | f32, em below the baseline (negative)                    |
|     24 |    4 | `line_gap`       | f32, em; line advance = `ascent − descent + line_gap`    |
|     28 |    4 | `distance_range` | f32, atlas texels the stored distance spans from 0 to 1 (`255 × 6 / 128`, about 11.95) |
|     32 |    4 | `edge_value`     | f32, normalized atlas value on the outline (`128 / 255`) |
|     36 |    4 | `em_size`        | f32, atlas pixels per em the glyphs were drawn at (`48`) |
|     40 |    4 | `atlas_width`    | u32, texels                                              |
|     44 |    4 | `atlas_height`   | u32, texels                                              |
|     48 |    4 | `flags`          | u32, `0` (bit 0 is reserved for a multi-channel atlas)   |
|     52 |    4 | `reserved`       | u32, `0`                                                 |

## Glyph (40 bytes)

| Offset | Size | Field          | Notes                                                       |
|-------:|-----:|----------------|-------------------------------------------------------------|
|      0 |    4 | `codepoint`    | u32, Unicode scalar; `0` is the font's `.notdef`, for characters it lacks |
|      4 |    4 | `advance`      | f32, em, pen advance after the glyph                        |
|      8 |   16 | `plane`        | f32 × 4: left, bottom, right, top; em, y up, from the pen on the baseline |
|     24 |   16 | `uv`           | f32 × 4: left, top, right, bottom; normalized atlas coordinates, top-left origin |

A glyph without an outline (space) has an empty plane and UV rectangle and
only advances the pen.

## KerningPair (12 bytes)

| Offset | Size | Field     | Notes                                                    |
|-------:|-----:|-----------|----------------------------------------------------------|
|      0 |    4 | `left`    | u32, codepoint                                           |
|      4 |    4 | `right`   | u32, codepoint                                           |
|      8 |    4 | `advance` | f32, em, added to `left`'s advance before `right`        |

Pairs come from the font's `kern` table; a font that kerns only through GPOS
has none.

## Atlas

The companion texture (asset ID `atlas`) is a [KTX2](ktx2.md) in
`R8G8B8A8_UNORM`, one mip level, holding the distance in R, G and B (alpha
255); a renderer reads R. A texel value `v` means a distance of
`(v / 255 − edge_value) × distance_range` texels from the outline, positive
inside the glyph. To draw text, build one quad per glyph from `plane` and
`uv`, sample the atlas with linear filtering, and turn distance into
coverage, e.g.

```glsl
float sd  = texture(atlas, uv).r;
vec2 unit = vec2(distance_range) / vec2(textureSize(atlas, 0));
float px  = max(0.5 * dot(unit, vec2(1.0) / fwidth(uv)), 1.0);
float cov = clamp(px * (sd - edge_value) + 0.5, 0.0, 1.0);
```
