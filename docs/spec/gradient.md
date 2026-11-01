# `.gradient` file format

A `.gradient` file describes one image holding several color gradients, one
per horizontal **stripe**, stacked top to bottom. It is a texture source: the
build compiles the image to KTX2 (see
[`docs/features/gradients.md`](../features/gradients.md#build)). The file is
TOML, UTF-8.

## Written form

```toml
width = 256
height = 64
stripe = 8

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
```

assetc writes files exactly in this form: `width`, `height`, `stripe`, a
blank line, then one `[[gradients]]` table per gradient (separated by blank
lines) with `name`, `dither` and `stops` (one inline table per line, sorted
by `at`). Numbers are written in their shortest form that reads back to the
same value (`0.0`, `0.25`, `0.333`).

## Top level

| Key         | Type    | Range        | Default | Meaning                               |
|-------------|---------|--------------|---------|---------------------------------------|
| `width`     | integer | 1–4096       | 256     | image width, px                       |
| `height`    | integer | 1–4096       | 64      | image height, px                      |
| `stripe`    | integer | 1–`height`   | 8       | height of each gradient's stripe, px  |
| `gradients` | array of tables | —    | empty   | the gradients, top to bottom          |

`gradients.len() × stripe` must not exceed `height`
(`<n> gradients need <n × stripe> px but height is <height>`). Any other
key is an error (`unknown key <key>`).

## Gradient

| Key      | Type   | Default  | Meaning                                                  |
|----------|--------|----------|----------------------------------------------------------|
| `name`   | string | `""`     | label shown in the editor; not used by the image          |
| `dither` | string | `"none"` | `none`, `crosshatch` or `pixel`, see [Dithering](#dithering) |
| `stops`  | array of tables | — | at least two stops                                 |

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
transparent (`0 0 0 0`). Gradient `i` (from 0) fills rows
`i × stripe … (i + 1) × stripe − 1`, every column. Rows below the last
stripe stay transparent.

Within a stripe, pixel `(x, y)` (`y` counted from the stripe's top row) has
position `t = x / (width − 1)` (`0` when `width` is 1), so the first and last
columns are exactly the end stops' colors.

**Segment**: with the stops sorted, `k` is the first index for which
`t ≤ at[k + 1]`, or the last segment (`stops.len() − 2`) if none.
`s = (t − at[k]) / (at[k + 1] − at[k])`, clamped to 0–1, and `0` when both
stops are at the same position. `a` and `b` are stops `k` and `k + 1`.

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

Dither patterns use stripe-local coordinates, so every stripe looks the same
wherever it sits in the image.

This matches the `gradient` command of the `img` toolkit in
[SoftKiwiGames/cli](https://github.com/SoftKiwiGames/cli) (`img/gradient.go`),
laid out horizontally and stored with straight alpha.
