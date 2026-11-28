# Colors crate

assetc's color logic, the part that makes and changes colors, is its own
crate, **`assetc-colors`**, so other programs can use exactly what assetc
uses: the same gradients, the same sRGB conversion, the same color
modifiers, LUTs and color vision simulation, with the same results to the
byte.

## Crate

- Package `assetc-colors`, library `assetc_colors`
  (`use assetc_colors::gradient;`). It lives in `colors/`, a member of the
  Cargo workspace, and is published to
  [crates.io](https://crates.io/crates/assetc-colors).
- **No dependencies** but `std`: no file formats (TOML, PNG, textures), no UI,
  no engine. It works on plain values: colors as `[u8; 4]` or floats,
  images as RGBA8 bytes.
- The version is the workspace's, set in `colors/Cargo.toml` with
  the others (see [Version](release.md#version)).
- Author Bartłomiej Wójtowicz; licensed `MIT OR Apache-2.0`
  (`colors/LICENSE-MIT`, `colors/LICENSE-APACHE`). Needs Rust 1.88 or newer
  (`rust-version`).
- `colors/README.md` is both its crates.io page and the front page of its
  API docs; its example is compiled as a doctest.
- The compiler (`assetc-core`) depends on it by path and uses it for all of
  the below: what the crate holds is not also kept in the compiler. The
  compiler keeps reading and writing assetc's files (`.agradient`,
  `.apalette`, `.anoise`, `.amod`), the UI, and the noise patterns of
  `.anoise` files (their color modifiers are the crate's). The one text
  format in the crate is `.cube`, read by `Lut::parse`, as it is part of
  what a LUT is.
- The [SDK](sdk.md) does not depend on it, so a game that only reads assets
  still builds nothing but the SDK.

```toml
[dependencies]
assetc-colors = "0.4"
```

## Contents

| Module | Holds |
|--------|-------|
| `assetc_colors` (root) | `Image`: an RGBA8 image, rows top to bottom (`width`, `height`, `pixels`); `new` (transparent black), `filled`, `from_fn`, `dimensions`, `pixel` / `set_pixel`, `texels` / `texels_mut` (the pixels as `[u8; 4]`), `crop`. |
| `space` | `ColorSpace` (`Srgb`, `Linear`); `srgb_to_linear` / `linear_to_srgb` (0–1 floats, the sRGB transfer functions), `srgb_byte_to_linear` (a byte through a table) and `linear_to_srgb_byte`; `rgb_to_hsv` / `hsv_to_rgb`, `hsl_to_rgb`; `luminance` (Rec. 709 weights); `parse_hex` (`#rrggbb` or `#rrggbbaa`, the `#` optional, any case) and `format_hex` (`#rrggbb`, `#rrggbbaa` when not opaque). |
| `gradient` | `Stop` (`at`, straight RGBA `color`), `Dither` (`None`, `Crosshatch`, `Pixel`); `color_at(stops, t)`, `pixel(stops, dither, t, x, y)`, `render(stops, dither, width, height)` (a cell of a gradient image, first stop at the bottom) and `render_sideways` (first stop on the left), as [gradients](gradients.md) are drawn: blended premultiplied, dithered with 4×4 or 8×8 Bayer matrices. |
| `grid` | `Layout` (`Rows`, `Columns`), `CellGrid`: an image split into equal cells filled in layout order (`columns`, `rows`, `cell`, `capacity`, `validate`, `render_cells`), as gradient and [palette](palettes.md) images are; `MAX_SIZE` (4096). |
| `modifier` | Color modifiers as [`.anoise` files, `.amod` sidecars and gradients](noise.md) apply them: `ModifierKind` (`Invert`, `Levels`, `BrightnessContrast`, `Threshold`, `Posterize`, `Blur`, `Normalize`, `Colorize`), `Modifier` (kind, `enabled`, `params`), `apply_modifiers(modifiers, rgb, width, height)` on 0–1 RGB. Their parameters: `ParamSpec` (key, label, `ParamKind` with range and default), `Params`, `Value`, and the `int`, `float` and `choice` constructors, which noise generators use too. |
| `blend` | `BlendMode` (`Mix`, `Multiply`, `Screen`, `Overlay`, `Add`) and `blend(mode, base, top, amount)`: how a [vertex color set](vertex-colors.md#blending) blends over another, on sRGB 0–1 RGBA. |
| `lut` | `Lut`: a 3D color LUT from `.cube` text (`parse`), `apply` to one color (trilinear), `grade` an image, and the [compiled forms](lut.md): `strip` (an image, blue slices side by side) and `volume` (RGBA8 voxels). |
| `vision` | `Vision`: [color vision](canvas.md#color-vision) deficiencies (`Protanopia`, `Deuteranopia`, `Tritanopia`, `Monochromacy`) with `name`, `hint`, `matrix` and `simulate` (RGBA8 bytes, in linear light). |
| `heat` | `heat(value)`: the [heatmap](texture-paint.md#heatmap) color of a 0–255 weight. |

Names, keys (`"crosshatch"`, `"multiply"`) and labels are part of each type,
as the files and the Inspector use them.

## Publishing

`make publish` publishes every published crate, `assetc-colors`, then
[`assetc-atlas`](atlas-crate.md) and [`assetc-noise`](noise-crate.md)
(which depend on it), [`assetc-collider`](collider-crate.md),
[`assetc-ibl`](ibl-crate.md), then the SDK `assetc`; each also has a target of its
own. A `-check` target does everything but the upload
(`cargo publish --dry-run`); the atlas and noise checks need the
`assetc-colors` version they name already on crates.io, as packaging
resolves it from there:

| Target | Runs |
|--------|------|
| `make publish` | `publish-colors`, `publish-atlas`, `publish-noise`, `publish-collider`, `publish-ibl`, then `publish-sdk` |
| `make publish-colors` | `cargo publish -p assetc-colors` |
| `make publish-atlas` | `cargo publish -p assetc-atlas` |
| `make publish-noise` | `cargo publish -p assetc-noise` |
| `make publish-collider` | `cargo publish -p assetc-collider` |
| `make publish-ibl` | `cargo publish -p assetc-ibl` |
| `make publish-sdk` | `cargo publish -p assetc` |
| `make publish-check` | `publish-check-colors`, `publish-check-atlas`, `publish-check-noise`, `publish-check-collider`, `publish-check-ibl`, then `publish-check-sdk` |
| `make publish-check-colors` | `cargo publish -p assetc-colors --dry-run` |
| `make publish-check-atlas` | `cargo publish -p assetc-atlas --dry-run` |
| `make publish-check-noise` | `cargo publish -p assetc-noise --dry-run` |
| `make publish-check-collider` | `cargo publish -p assetc-collider --dry-run` |
| `make publish-check-ibl` | `cargo publish -p assetc-ibl --dry-run` |
| `make publish-check-sdk` | `cargo publish -p assetc --dry-run` |

Publishing needs a crates.io token (`cargo login`) and versions not
published yet.
