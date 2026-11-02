# HDR environment maps

An **HDR environment map** is an OpenEXR image (`*.exr`, any case) in the
input directory holding a whole surrounding in **latitude-longitude**
(equirectangular) layout, as HDRI sites publish them: the full 360° around
from left to right, straight up at the top row, straight down at the bottom.
The build compiles it into a [KTX2 cubemap](../spec/ktx2.md#cubemaps) with
half-float colors, for skyboxes and image-based lighting. It is read with the
pure-Rust [`exr`](https://crates.io/crates/exr) crate.

(Folders of six faces, `*.env`, stay [preview-only](canvas.md#environment-maps).)

## Reading

- The first RGBA layer of the file is read (RGB without alpha reads as alpha
  1), as linear floats. Deep data, and files the crate cannot read, are an
  error.
- Any size works; `width = 2 × height` is what the layout expects (other
  sizes are stretched).

## Cubemap

- **Face size**: the largest power of two not above `height / 2` (a 2048 ×
  1024 image gives 512), at least 1.
- **Faces**, in KTX2 order: +X, −X, +Y, −Y, +Z, −Z. Texel `(x, y)` of a face,
  with `u = 2 (x + 0.5) / size − 1` and `v = 2 (y + 0.5) / size − 1`, looks
  along (Vulkan's cube face directions):

  | Face | Direction       |
  |------|-----------------|
  | +X   | `( 1, −v, −u)`  |
  | −X   | `(−1, −v,  u)`  |
  | +Y   | `( u,  1,  v)`  |
  | −Y   | `( u, −1, −v)`  |
  | +Z   | `( u, −v,  1)`  |
  | −Z   | `(−u, −v, −1)`  |

- **Sampling**: a direction `d` (normalized) reads the image at
  `s = 0.5 + atan2(d.x, −d.z) / 2π` (across, wrapping) and
  `t = acos(d.y) / π` (down), bilinearly: the image's middle column looks
  along −Z, +X is a quarter to the right.
- **Mipmaps**: a full chain down to 1 × 1; each level's texel is the average
  of the 2 × 2 texels under it, per face.
- Colors are kept linear and unclamped, written as half floats
  (`R16G16B16A16_SFLOAT`); values beyond the half-float range become its
  largest finite value.

## Build

| Source | Asset kind | Output |
|--------|------------|--------|
| `*.exr` (any case) | texture (cubemap) | `<asset path>.ktx2`, asset path the input-relative path without the extension |

The CLI line reads `<source> -> <output> (cubemap 512², rgba16f, 10 levels)`.

## Preview and Inspector

- The file tree shows `.exr` files with the `sun-medium` icon.
- Selecting one shows the cubemap in the 3D view as an
  [environment map](canvas.md#environment-maps) does (background, light,
  the mirror shape), its faces made at most 512 px and shown as 8-bit sRGB:
  colors clamped to 0–1 (bright skies clip in the preview, not in the
  build).
- **Info**: Kind `HDR environment map`, Size, **Dimensions** (`W × H px`),
  **Face size** (`512 px`), **Mip levels**, **Asset path**, **Asset ID**,
  **Output** (as for textures, `(not built)` appended when missing).
- A compiled cubemap (`.ktx2` with six faces) in the Output tree shows
  Dimensions, Format `R16G16B16A16_SFLOAT`, **Faces** `6` and Mip levels; it
  has no 2D preview.

## Not yet

- Prefiltered lighting maps (specular mip levels by roughness, diffuse
  irradiance), and cubemaps from `*.env` folders.
