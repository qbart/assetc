# `.gradient` file format

A `.gradient` file describes one image holding several color gradients, one
per **cell** of a grid, each running vertically (first stop at the bottom). It is a texture source: the
build compiles the image to KTX2 (see
[`docs/features/gradients.md`](../features/gradients.md#build)). The file is
TOML, UTF-8.

## Written form

```toml
width = 256
height = 256
cell_width = 16
cell_height = 64
layout = "rows"

[[gradients]]
name = "Sunset"
dither = "none"
stops = [
    { at = 0.0, color = "#1b1036" },
    { at = 0.5, color = "#ff6b35" },
    { at = 1.0, color = "#ffe66d" },
]

[[gradients]]
name = "Fade"
dither = "crosshatch"
stops = [
    { at = 0.0, color = "#ffffffff" },
    { at = 1.0, color = "#ffffff00" },
]

[[gradients.modifiers]]
type = "levels"
enabled = true
gamma = 1.5
```

assetc writes files exactly in this form: `width`, `height`, `cell_width`,
`cell_height`, `layout`, a blank line, then one `[[gradients]]` table per gradient (separated by blank
lines) with `name`, `dither` and `stops` (one inline table per line, sorted
by `at`), then its modifiers as `[[gradients.modifiers]]` tables (each after
a blank line: `type`, `enabled`, then the parameters that differ from their
defaults, as `.noise` files write them). Numbers are written in their shortest form that reads back to the
same value (`0.0`, `0.25`, `0.333`).

## Top level

| Key         | Type    | Range        | Default | Meaning                               |
|-------------|---------|--------------|---------|---------------------------------------|
| `width`       | integer | 1–4096       | 256     | image width, px                     |
| `height`      | integer | 1–4096       | 256     | image height, px                    |
| `cell_width`  | integer | 1–`width`    | 16      | width of each gradient's cell, px   |
| `cell_height` | integer | 1–`height`   | 64      | height of each gradient's cell, px  |
| `layout`      | string  | `rows`, `columns` | `rows` | the order cells are filled in, see [Image](#image) |
| `gradients`   | array of tables | —    | empty   | the gradients, in cell order        |

The grid has `columns = floor(width / cell_width)` and
`rows = floor(height / cell_height)`; `gradients.len()` must not exceed
`columns × rows` (`<n> gradients need <n> cells but the grid has <m>`).
Any other key is an error (`unknown key <key>`).

## Gradient

| Key      | Type   | Default  | Meaning                                                  |
|----------|--------|----------|----------------------------------------------------------|
| `name`   | string | `""`     | label shown in the editor; not used by the image          |
| `dither` | string | `"none"` | `none`, `crosshatch` or `pixel`, see [Dithering](#dithering) |
| `stops`  | array of tables | — | at least two stops                                 |
| `modifiers` | array of tables | none | this gradient's [modifiers](noise.md#modifiers), in order; see [Modifiers](#modifiers) |

Any other key is an error (`gradient <n>: unknown key <key>`, counting from
1).

## Stop

| Key     | Type   | Meaning                                                        |
|---------|--------|----------------------------------------------------------------|
| `at`    | float  | position along the gradient, 0 (left) to 1 (right)             |
| `color` | string | `#rrggbb` or `#rrggbbaa`, hex digits in any case, straight (not premultiplied) alpha; `ff` when left out |

Stops are ordered by `at` when read (stable, so stops at the same position
keep their file order). Errors: fewer than two stops, `at` outside 0–1, a
color that is not one of the two forms, a missing key, or any other key.

## Image

The image is `width × height` RGBA8 (straight alpha), starting fully
transparent (`0 0 0 0`). Gradient `i` (from 0) fills one cell, whose
top-left pixel is `(column × cell_width, row × cell_height)`, in the order
`layout` gives:

| `layout`  | Order                              | Cell of gradient `i`                              |
|-----------|------------------------------------|---------------------------------------------------|
| `rows`    | left to right, then top to bottom  | column `i mod columns`, row `floor(i / columns)`  |
| `columns` | top to bottom, then left to right  | row `i mod rows`, column `floor(i / rows)`        |

 Unused cells, and any columns or
rows the grid does not cover, stay transparent.

Gradients run **vertically**, the first stop at the bottom: within a cell,
pixel `(x, y)` (counted from the cell's top-left) has position
`t = (cell_height − 1 − y) / (cell_height − 1)` (`0` when `cell_height` is
1), so the bottom and top rows are exactly the end stops' colors. Every
column of a cell is the same gradient (apart from dithering).

**Segment**: with the stops sorted, `k` is the first index for which
`t ≤ at[k + 1]`, or the last segment (`stops.len() − 2`) if none.
`s = (t − at[k]) / (at[k + 1] − at[k])`, clamped to 0–1, and `0` when both
stops are at the same position. `a` and `b` are stops `k` and `k + 1`.

### Modifiers

A gradient's enabled modifiers run on its own cell, after the cell is
drawn (dithering included), in order, as [`.noise` modifiers](noise.md#modifiers)
run on a noise image: on the cell's **R, G and B** (0–1), the cell's edges
as the image's edges (so `blur` with `wrap` wraps within the cell, and
`normalize` uses the cell's minimum and maximum). **Alpha is never
changed**: every pixel keeps the alpha its stops give it. Other cells are
not affected.

### Dithering

**`pixel`** keeps the palette: every pixel is exactly `a`'s or `b`'s color.
With the 4 × 4 Bayer matrix

```
 0  8  2 10
12  4 14  6
 3 11  1  9
15  7 13  5
```

the pixel is `b` if `s > bayer4[y mod 4][x mod 4] / 16`, else `a`.

**`none`** and **`crosshatch`** blend. Colors are blended in
**premultiplied** alpha, all channels 0–255:

```
A  = a.alpha + (b.alpha − a.alpha) × s
P  = a.rgb × a.alpha / 255 + (b.rgb × b.alpha / 255 − a.rgb × a.alpha / 255) × s
```

The stored alpha is `q(A, 0.5)`. If it is 0 the pixel is `0 0 0 0`;
otherwise each color channel is `q(P × 255 / A, d)`, where `q(v, d)` is
`floor(v + d)` clamped to 0–255 and the dither offset `d` is `0.5`
(`none`, plain rounding) or `bayer8[y mod 8][x mod 8] / 64` (`crosshatch`)
with the 8 × 8 Bayer matrix

```
 0 32  8 40  2 34 10 42
48 16 56 24 50 18 58 26
12 44  4 36 14 46  6 38
60 28 52 20 62 30 54 22
 3 35 11 43  1 33  9 41
51 19 59 27 49 17 57 25
15 47  7 39 13 45  5 37
63 31 55 23 61 29 53 21
```

Dither patterns use cell-local coordinates, so every cell looks the same
wherever it sits in the image.

This matches the `gradient` command of the `img` toolkit in
[SoftKiwiGames/cli](https://github.com/SoftKiwiGames/cli) (`img/gradient.go`):
the same vertical cells (its default cell is 16 × 64), placed on a grid like
its `--position`, stored with straight alpha.
