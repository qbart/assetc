# Color LUTs

Preview of 3D color lookup tables in the Adobe / Resolve `.cube` format, e.g.
`samples/blue-and-pink.lut.cube`, and their compilation to a KTX2 texture
(see [Build](#build)).

## Format

Text, one statement per line; blank lines and lines starting with `#` are
ignored. Supported:

| Line                        | Meaning                                           |
|-----------------------------|---------------------------------------------------|
| `TITLE "…"`                 | optional title                                    |
| `LUT_3D_SIZE N`             | required, `2 ≤ N ≤ 256`                           |
| `DOMAIN_MIN r g b`          | optional, default `0 0 0`                         |
| `DOMAIN_MAX r g b`          | optional, default `1 1 1`; each must exceed min   |
| `r g b`                     | one output color; exactly N³ of them, red changing fastest, then green, then blue |

`LUT_1D_SIZE` (1D LUTs) and any other keyword are errors ("unsupported").

**Applying** a LUT to a color: each channel is mapped from the domain to
`0..N-1`, clamped, and the output is trilinearly interpolated between the 8
surrounding entries, then clamped to `0..1`. Images are graded on their stored
8-bit values (`value / 255`), the usual convention for display-referred LUTs;
alpha is kept.

## Inspector

Kind `Color LUT` for `*.cube` (any case) in the Input tree, with rows
**Title** (or `—`), **LUT size** `N × N × N` (the file size stays in **Size**), **Domain** `min – max` (e.g.
`0, 0, 0 – 1, 1, 1`), then **Asset path**, **Asset ID** and **Output** as
for source textures (see [`inspector.md`](inspector.md#details-by-file-kind)),
or an **Error** row if the file does not parse. A file that fails to parse
shows its error as the canvas message.

## Build

`*.cube` (any case) files in `input` are compiled to a texture, in one of two
layouts chosen per file by the `[textures] volume_luts` globs (see
[`assetc.toml`](../spec/assetc.toml.md#textures)):

```toml
[textures]
volume_luts = ["grades/**"]   # these become 3D textures; the rest strips
```

| Layout | When | Texture |
|--------|------|---------|
| **Strip** (default) | the file matches no `volume_luts` glob | 2D, `N² × N` pixels: `N` slices of `N × N` side by side, one per **blue** entry (left to right); within a slice **red** grows left to right and **green** top to bottom. Pixel `(b × N + r, g)` holds entry `(r, g, b)`. The layout game engines commonly use; works where 3D textures do not. |
| **Volume** | the file matches a `volume_luts` glob | 3D, `N × N × N` (`pixelDepth` `N`): **red** along x, **green** along y (top to bottom), **blue** along z (slice). Sampled directly with hardware trilinear filtering. |

Layout details in [`ktx2.md`](../spec/ktx2.md#color-luts). Both:

- Asset path: the input-relative path without `.cube`, e.g.
  `grades/blue-and-pink.lut.cube` → `grades/blue-and-pink.lut`, output
  `<asset path>.ktx2`, a `texture` in the manifest.
- Each entry is stored as is: `round(clamp(v, 0, 1) × 255)` per channel,
  alpha 255, `R8G8B8A8_UNORM` (the values are already display encoded, so the
  GPU must not convert them), one mip level (mip levels would blend unrelated
  entries).
- The domain is not stored: a runtime maps its input from `DOMAIN_MIN`…
  `DOMAIN_MAX` (usually 0…1) to the texture itself.
- Strips are limited to `N` ≤ 128 (a 16384-pixel-wide strip, the common GPU
  maximum); a larger LUT fails with `LUT_3D_SIZE <N> is too large to compile
  as a strip (at most 128; volume_luts allows up to 256)`. Volumes take any
  size the format allows (up to 256). A file that does not parse fails like
  an undecodable image.
- The other `[textures]` settings (`linear`, `no_mipmaps`) do not apply.
- The build prints the layout, e.g. `33³ LUT strip, linear, 1 level` or
  `33³ LUT volume, linear, 1 level`.

A compiled volume in the Output tree is previewed as its slices side by side
(the same picture as the strip), and the Info tab's Dimensions read
`W × H × D px`.


## Views

With a LUT selected, the [view bar](canvas.md#view-bar) offers three views
instead of 2D / 3D (same look; **Compare** is the default; the choice is
global for the session):

```
View  [ Compare | Cube | Curves ]
```

### Compare

A reference image graded by the LUT, split in two: **original on the left,
graded on the right** of a vertical divider.

- The image is fitted to the canvas (24 px margin, like 2D fit), drawn with
  linear filtering, no zoom or pan.
- The divider starts in the middle. It is a 2 px `TEXT` line with small
  "Before" / "After" labels (12 px, `TEXT` on a `BG_BAR` pill) at the top of
  each side, and a **grab handle** centered on it vertically (within the
  image): a 24 × 40 px pill (12 px radius) with `BG_BAR` fill, a 1 px
  `STROKE` border and a 16 px `grip-vertical` icon in `TEXT`; while hovered
  or dragged the border and icon turn `ACCENT`.
- Dragging the handle, or the line (8 px wide hit area), with the left button
  moves the divider with the pointer (clamped to the image); the cursor is a
  grab hand (closed while dragging). Clicking elsewhere on the canvas does
  nothing.
- **Source** — a toolbar group (top-right, like the canvas toolbar):

  | Button | Icon      | Image                                              |
  |--------|-----------|----------------------------------------------------|
  | Chart  | `palette` | the built-in reference chart (default)             |
  | Image  | `image`   | the last image selected in the file browser, tooltip `Image (<name>)`; disabled ("no image selected yet") until one has been selected this session |

- The graded image is computed once per LUT and source and cached.

#### Reference chart

Generated by assetc (no external image, so no license), 768 × 512 px,
sRGB values, top to bottom:

1. **Hue sweep**, 768 × 256: x is hue 0°→360°, y is HSL lightness from 1
   (white, top) to 0 (black, bottom), saturation 1.
2. **Gray ramp**, 768 × 48: x from 0 to 255, equal R = G = B.
3. **ColorChecker**, 768 × 208: the 24 patches in 6 columns × 4 rows (each
   128 × 52 px), using the widely published sRGB values of the X-Rite
   ColorChecker Classic (dark skin `115 82 68` … black `52 52 52`).

### Cube

A 3D view (same [camera and controls](canvas.md#3d-controls), auto-rotation
and no tonemapping as the 3D image view) of the RGB cube transformed by the
LUT: a 1.2-unit cube with red along +X, green along +Y and blue along +Z, its
six faces subdivided into N × N quads, each vertex colored with the LUT output
for its input color (unlit). An identity LUT shows the plain RGB cube; a grade
shows where it moves colors.

- **Before / After** — a toolbar group (top-right): **Before** (`eye-off`)
  shows the plain, untransformed RGB cube; **After** (`eye`, the default)
  shows the LUT's cube. The choice keeps the camera and rotation, so flipping
  between them shows where colors move. It is global for the session.
- Holding **Space** shows Before while held (when no text field has focus),
  whatever the toolbar says.

### Curves

How neutral gray is mapped: for input `x` from 0 to 1 (gray `x, x, x`), the
output's R, G and B plotted as three curves (`CHANNEL_R`, `CHANNEL_G`,
`CHANNEL_B`, 2 px).

- Square plot, fitted to the canvas with a 48 px margin; `BG_PANEL` fill,
  1 px `STROKE` grid at quarters, the identity diagonal in `TEXT_WEAK`.
- Axis labels in `TEXT_WEAK`, 12 px: `0` and `1` at the ends, `Input` below,
  `Output` at the left.
- Sampled at 256 points.

## Keybindings

Compare: left click / drag sets the divider. Cube: as the 3D view. See
[`KEYS.md`](../../KEYS.md).
