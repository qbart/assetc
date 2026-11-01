# Fonts

Font files in the Input tree can be inspected, previewed, and compiled into
a font with a signed-distance-field atlas (see [Build](#build)).

## Kind

| Tab   | File                                   | Kind |
|-------|----------------------------------------|------|
| Input | `*.ttf`, `*.otf` (any case)            | Font |

Only the first font of a file is read. Web fonts (`*.woff`, `*.woff2`) and
collections (`*.ttc`) are not decoded and stay Ignored (the file tree still
shows them with the `type` icon, see [`gui.md`](gui.md#tree-icons)).

## Inspector

Info rows after Kind and Size:

| Row          | Value                                                           |
|--------------|-----------------------------------------------------------------|
| Family       | typographic family (name ID 16), else family (name ID 1)        |
| Style        | typographic subfamily (name ID 17), else subfamily (name ID 2)  |
| Version      | version string (name ID 5)                                      |
| Outlines     | `TrueType`, `CFF` or `CFF2`, by the outline table present       |
| Glyphs       | number of glyphs                                                |
| Characters   | number of Unicode characters the font maps                      |
| Units per em | font units per em                                               |
| Ascent       | ascender, in font units                                         |
| Descent      | descender, in font units (negative below the baseline)          |
| Line gap     | line gap, in font units                                         |

Names are the first Unicode entry with that ID; a missing name leaves its row
out. A file that is not a valid font shows an **Error** row
(`not a valid font: <reason>`) and the canvas shows "No preview".

After them, **Asset path** (`<path>.afont`) and **Output** (`(not built)`
appended when it does not exist). No other Inspector tab applies to fonts.

## Specimen

Selecting a font shows a **specimen sheet** in the [canvas](canvas.md#2d):
an image rendered from the font, previewed like a 2D image (zoom, pan, fit,
zoom bar, checkerboard behind it).

- The view bar is empty (no `View` label, no buttons): a font has no 2D / 3D
  choice. The toolbar (Tile, Channels) is not shown.
- Rendered on the [inspector thread](inspector.md#loading) with
  [`ab_glyph`](https://docs.rs/ab_glyph): outlines are antialiased, without
  hinting. Glyphs are drawn in `TEXT` with their coverage as alpha, on a
  transparent background.
- Layout is per character: each glyph advances by its horizontal advance plus
  the `kern` table's kerning with the previous glyph. There is no shaping
  (no ligatures, no complex scripts); characters the font lacks show its
  `.notdef` glyph.
- Sizes are em sizes: a 24 px line has an em of 24 image pixels.

From top to bottom, with a 32 px margin on every side:

1. **Waterfall**: the pangram "The quick brown fox jumps over the lazy dog"
   at 12, 16, 24, 32, 48 and 72 px, one line each. Each line is as tall as
   the font's ascent − descent + line gap at that size; the first baseline
   sits one ascent below the top margin.
2. 32 px gap.
3. **Character grid**: the characters the font maps, by code point, skipping
   control and whitespace characters, at most the first 1024. 16 columns of
   64 × 64 px cells, outlined by 1 px lines in `TEXT_WEAK` at 30% opacity.
   Each glyph is drawn at 40 px, centered horizontally on its advance, with
   its ascent − descent centered vertically in the cell.

The sheet is as wide as its widest part (the 72 px line or the 1024 px grid)
plus the margins.

## Build

Every `*.ttf` / `*.otf` in `input` compiles, after assetc-cpp's `.hfont`,
into two assets. With `P` the input-relative path without the extension
(e.g. `ui/inter`):

| Output (in `output`) | Asset kind | Asset path | Holds |
|----------------------|------------|------------|-------|
| `P.afont`            | font       | `P.afont`  | glyph metrics, atlas rectangles, kerning ([spec](../spec/afont.md)) |
| `P.ktx2`             | texture    | `P`        | the SDF atlas, see [spec](../spec/afont.md#atlas) |

The build prints e.g. `ui/inter.ttf -> ui/inter.afont (191 glyphs, 0
kerning pairs, 512 × 160 atlas)`.

- **Characters**: ASCII printable (U+0020–U+007E) and Latin-1 (U+00A0–U+00FF)
  that the font has, plus codepoint 0 for its `.notdef` glyph (drawn for
  anything it lacks).
- **Distance field**: each glyph is drawn at 48 px per em. Its bitmap covers
  its outline's bounding box at that size (rounded out to whole pixels) plus
  6 px on every side. Each texel holds `clamp(round(128 + d × 128 / 6), 0,
  255)`, where `d` is the distance in pixels from the texel's center to the
  outline, positive inside (non-zero winding). Curves are split into 16
  straight segments each for this.
- **Atlas**: glyphs are packed in codepoint order into rows, left to right, a
  new row when the next one does not fit; the atlas is 512 texels wide (or
  the widest glyph), as tall as its rows, both rounded up to a multiple of 4.
  It is written as a linear RGBA8 texture, one mip level; `[textures]`
  settings do not apply.
- **Metrics** are the font's ascent, descent, line gap and advances divided
  by its units per em; **kerning** pairs come from its `kern` table, between
  the included characters.
- A font with no usable glyph fails (`no glyphs`), as does one that does not
  parse.

## Compiled fonts

Selecting a `.afont` in the Output tree shows its Info rows: Format (`AFONT
v1, 3 chunks`), Glyphs, Kerning pairs, Em size (`48 px`), Distance range
(`11.95 texels`), Atlas (`512 × 160 px` and its output path, or `not found`),
Ascent, Descent, Line gap (em, three decimals), Asset path, Asset ID.

The [view bar](canvas.md#view-bar) offers **Atlas** (the default: the SDF
atlas as a 2D image, with zoom and pan but no toolbar) and **Chunks** (the
file's data as tables, like a compiled mesh's
[Chunks view](models.md#chunks-view)): `DESC` (Font descriptor, field by
field), `GLYPHS` (Glyphs: Codepoint as `U+0041 A`, Advance, Plane, UV) and
`KERNING` (Kerning pairs: Left, Right, Advance).
