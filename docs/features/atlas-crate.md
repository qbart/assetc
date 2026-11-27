# Atlas crate

assetc's atlas logic, the part that lays frames out and draws the
[atlas](atlases.md) image, is its own crate, **`assetc-atlas`**, so other
programs (editors, tools, a game packing sprites at run time) can pack
atlases exactly as assetc does: the same grid and shelf layouts, trimming,
padding and extrusion, the same frame rectangles to the pixel.

## Crate

- Package `assetc-atlas`, library `assetc_atlas`
  (`use assetc_atlas::{pack, Settings};`). It lives in `atlas/`, a member of
  the Cargo workspace, and is published to
  [crates.io](https://crates.io/crates/assetc-atlas).
- Depends on [`assetc-colors`](colors.md) for its `Image` (re-exported as
  `assetc_atlas::Image`) and on `imgio` (no default features: no file
  formats) for the Lanczos-3 resize of frames scaled into grid cells.
- No file formats: no `atlas.toml`, no `.aatlas.bin`, no image files, no
  folders. It works on plain values: named RGBA8 images in, an image and
  frame rectangles out.
- The version is the workspace's, set in `atlas/Cargo.toml` with the
  others (see [Version](release.md#version)).
- Author Bartłomiej Wójtowicz; licensed `MIT OR Apache-2.0`
  (`atlas/LICENSE-MIT`, `atlas/LICENSE-APACHE`). Needs Rust 1.88 or newer
  (`rust-version`).
- `atlas/README.md` is both its crates.io page and the front page of its
  API docs; its example is compiled as a doctest.
- The compiler (`assetc-core`) depends on it by path and packs every atlas
  with it: what the crate holds is not also kept in the compiler. The
  compiler keeps the atlas folder (finding and decoding its frames),
  `atlas.toml` (with its [structs](structs.md)), the `.aatlas.bin` writer,
  the Inspector and the Atlas tab. The Atlas tab takes its ranges from the
  crate.
- Font glyph atlases ([`fonts.md`](fonts.md#build)) are not packed with
  it: glyphs go in codepoint order with no padding, a layout of their own.
- The [SDK](sdk.md) does not depend on it; it reads `.aatlas.bin` files on
  its own.

```toml
[dependencies]
assetc-atlas = "0.4"
```

## Contents

| Item | Holds |
|------|-------|
| `Image` | `assetc_colors::Image`, re-exported. |
| `Packing` | `Grid`, `Pack`; `ALL`, `key` (`"grid"`, `"pack"`, as `atlas.toml` writes it), `name` (`Grid`, `Pack`) and `from_key`. |
| `Settings` | The [atlas settings](atlases.md#settings): `packing`, `cell_width`, `cell_height`, `columns`, `padding`, `extrude`, `trim`, `power_of_two`, `max_size`, with the defaults of that table (`Default`), and `validate`: the first value out of range, as `atlas.toml` reports it (`padding must be 0–64`, `extrude must not exceed padding`, `max_size must be 1–16384`). |
| `MAX_CELL`, `MAX_COLUMNS`, `MAX_PADDING`, `MAX_SIZE` | The ranges: cells up to 4096 px, 1024 columns, 64 px of padding (and extrusion), atlases up to 16384 px. |
| `Frame` | Where a frame is: `name`, `x`, `y`, `width`, `height` (in the atlas, px), `source_width`, `source_height` (the image it came from), `offset_x`, `offset_y` (where a trimmed frame starts in it); `uv(width, height)`: `[u0, v0, u1, v1]`, the rectangle as 0–1 coordinates of an atlas that size, top-left origin, as `.aatlas.bin` stores it. |
| `Atlas` | A packed atlas: `image`, `frames` (in the order the images were given), `packing`, `columns` and `cell` (the grid's; `0` and `(0, 0)` for pack); `frame(name)`, `uv(index)` and `summary` (`grid 4 × 3` or `pack`, as the CLI and Inspector show it). |
| `pack(settings, images)` | Packs named images, as [Settings](atlases.md#settings) describes: grid cells or shelves, trimmed, padded, extruded, rounded to powers of two. Fails with no images (`no frames`), invalid settings, or an atlas over `max_size` (`the atlas would be 600 × 300 px, larger than max_size 512`). |
| `trim_bounds(image)` | The rectangle `(x, y, width, height)` of the pixels with alpha above 0; the whole image when there are none. |
| `blit(atlas, image, x, y, extrude)` | Draws `image` into `atlas` at `(x, y)`, its edge pixels repeated `extrude` px outward; what falls outside the atlas is dropped. |

The compiler gives `pack` the frames in name order, so frame indices are
their places in the folder's name-sorted list; the crate keeps whatever
order it is given.

## Publishing

`make publish` publishes `assetc-atlas` after `assetc-colors` (which it
depends on) and before the SDK; see [Publishing](colors.md#publishing).
