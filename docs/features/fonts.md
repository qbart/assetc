# Fonts

Font files in the Input tree can be inspected and previewed. They are not
compiled yet: the build ignores them, so they have no output and no assets.

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

No other Inspector tab applies to fonts.

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
