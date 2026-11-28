# HDR environment maps

An **HDR environment map** is an OpenEXR image (`*.exr`, any case) in the
input directory holding a whole surrounding in **latitude-longitude**
(equirectangular) layout, as HDRI sites publish them: the full 360° around
from left to right, straight up at the top row, straight down at the bottom.
The build compiles it into a [cubemap texture](../spec/atex.bin.md#cubemaps) with
half-float colors for the skybox, and two prefiltered
[lighting maps](#lighting-maps) for image-based lighting of PBR materials; the
cubemaps and lighting maps are the [`assetc-ibl`](ibl-crate.md) crate, so
other programs can make the same ones. It is read with
[imgio](https://crates.io/crates/imgio) (pure Rust, through the `exr`
crate): the main layer of the first part, R, G, B and A (alpha 1 without
one; a luminance-only image as gray).

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
- **Faces**, in the texture's order: +X, −X, +Y, −Y, +Z, −Z. Texel `(x, y)` of a face,
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
- Colors are kept linear and unclamped as half floats; values beyond the
  half-float range become its largest finite value. The levels are then
  written in the build's [texture format](textures.md#format): `BC6H`
  (unsigned: negative values become 0) for `bc`, ASTC 4 × 4 HDR for
  `astc`, `R16G16B16A16_SFLOAT` for `rgba`,
  `R16G16B16_SFLOAT` for `rgb`.

## Lighting maps

The skybox cubemap's mipmaps are box-filtered: right for drawing the sky,
wrong for lighting. Two more cubemaps are written for that, both filtered
by direction over the whole environment (never per face, so there are no
seams at face edges), with the faces, texel directions and HDR format of the
skybox, linear and unclamped. Their **source** is the skybox cubemap with
its mip chain (built at a face size of at least 8, so a tiny image still
gives two specular levels); a source texel covers `4π / (6 F²)` steradians,
`F` the source's face size.

### Specular

`<asset path>.specular.atex.bin`: the environment prefiltered for glossy
reflections, the split-sum approximation of Karis ("Real Shading in Unreal
Engine 4").

- **Face size** `S = min(F, specular_size)` (default 256).
- **Levels**: `S`, `S/2`, … down to 4 px faces, so `log2(S) − 1` levels
  (7 for 256): smaller levels are below one compressed block and never
  useful.
- **Roughness**: level `i` of `n` holds the environment for perceptual
  roughness `r = i / (n − 1)`, so an engine samples at
  `lod = roughness × (n − 1)` (level 0 is mirror-like, the last is fully
  rough).
- Level 0 (`r = 0`) is the source resampled to `S`: the source's level of
  that size, unfiltered.
- Each texel of a level above 0, for its direction `R`, with `N = V = R`:
  importance-samples the GGX distribution with `α = r²` over
  `samples` directions (default 1024) from the Hammersley sequence
  (`x_k = k / samples`, `y_k` the bits of `k` reversed), no random seed, so
  two builds are byte-identical:

  ```
  φ = 2π x_k,  cos θ_H = sqrt((1 − y_k) / (1 + (α² − 1) y_k))
  H = (sin θ_H cos φ, sin θ_H sin φ, cos θ_H) around N
  L = 2 (V·H) H − V
  ```

  and averages the radiance along every `L` with `N·L > 0`, weighted by
  `N·L` (divided by the sum of the `N·L`).
- **No sun dots**: each sample reads the source at the mip matching its
  solid angle, trilinearly:
  `lod = 0.5 × log2(Ω_s / Ω_p) + 1` (at least 0), where
  `Ω_s = 1 / (samples × pdf)` is the sample's solid angle, with
  `pdf = D(H) / 4` (`D` the GGX distribution:
  `α² / (π ((N·H)² (α² − 1) + 1)²)`), and `Ω_p = 4π / (6 F²)` a source
  texel's. The `+ 1` (as in Colbert and Křivánek, "GPU-Based Importance
  Sampling") reads one level blurrier than the sample's own footprint:
  without it neighbouring texels, whose samples land differently, still
  catch a sun unevenly and show dots. Bilinear reads near a face edge take
  their texels across it, on the neighbouring face.
- The tangent frame around `N` is built from `+Z` (from `+X` when `N` is
  within 2.6° of ±Z).

### Irradiance

`<asset path>.irradiance.atex.bin`: the diffuse light arriving from every
direction.

- **Face size** `irradiance_size` (default 32), one level.
- Each texel, for its direction `n`, stores the **cosine-weighted average**
  of the environment over the hemisphere around `n`:

  ```
  E(n) = (1/π) ∫ L(ω) max(0, n·ω) dω
  ```

  that is the irradiance divided by π, so an engine's diffuse ambient light
  is `albedo × texel`, with no further factor. A constant environment of
  color `c` gives `c`.
- Computed by direct integration over the source's level of face size
  `min(F, 32)`: every source texel, at its center direction, weighted by
  its exact solid angle times `max(0, n·ω)`; the sum is divided by the sum of
  the weights (so a constant environment is exact).
- The file also holds the 9 **spherical harmonics** coefficients of `E`
  (27 floats, the `SH9` chunk, see
  [atex.bin.md](../spec/atex.bin.md#sh9)), for engines that light without a
  cubemap lookup: projected from the same source texels, then convolved
  with the cosine lobe and divided by π.

### Settings

`[environments]` in [`assetc.toml`](../spec/assetc.toml.md#environments),
also per [target](../spec/assetc.toml.md#targets):

| Key | Default | Meaning |
|-----|---------|---------|
| `specular_size` | `256` | Largest face size of the specular map; a power of two, 8–4096 |
| `irradiance_size` | `32` | Face size of the irradiance map; a power of two, 1–4096 |
| `samples` | `1024` | GGX samples per specular texel, 1–65536 |
| `lighting` | `true` | `false` writes only the skybox cubemap |

## Build

| Source | Asset kind | Output | Asset path |
|--------|------------|--------|------------|
| `*.exr` (any case) | texture (cubemap) | `<asset path>.atex.bin` (usage `environment`) | the input-relative path without the extension (`sky`) |
| | texture (cubemap) | `<asset path>.specular.atex.bin` (usage `environment_specular`) | `<asset path>.specular` (`sky.specular`) |
| | texture (cubemap) | `<asset path>.irradiance.atex.bin` (usage `environment_irradiance`) | `<asset path>.irradiance` (`sky.irradiance`) |

The last two only with `lighting` on. IDs follow the usual rule (FNV-1a 64 of
the asset path), so an engine that knows `sky` derives `sky.specular` and
`sky.irradiance`. All three are listed in the manifest and packed, and are
rebuilt when the `.exr`, the texture format or quality, or (for the lighting
maps) `[environments]` change.

The CLI lines read:

```
sky.exr -> sky.atex.bin (cubemap 512², bc6h, 10 levels)
sky.exr -> sky.specular.atex.bin (cubemap 256², bc6h, 7 levels)
sky.exr -> sky.irradiance.atex.bin (cubemap 32², bc6h, 1 level)
```

## Preview and Inspector

- The file tree shows `.exr` files with the `sun-medium` icon.
- Selecting one shows the cubemap in the 3D view as an
  [environment map](canvas.md#environment-maps) does (background, light,
  the mirror shape), its faces made at most 512 px and shown as 8-bit sRGB:
  colors clamped to 0–1 (bright skies clip in the preview, not in the
  build).
- **Info**: Kind `HDR environment map`, Size, **Dimensions** (`W × H px`),
  **Face size** (`512 px`), **Mip levels**, **Asset path**, **Asset ID**,
  **Output** (as for textures, `(not built)` appended when missing), and
  while [`lighting`](#settings) is on **Specular map**
  (`sky.specular.atex.bin (256 px, 7 levels)`) and **Irradiance map**
  (`sky.irradiance.atex.bin (32 px)`), each with `(not built)` appended when
  missing.
- A compiled cubemap (`.atex.bin` with six faces) in the Output tree shows
  Dimensions, Format (`BC6H_UFLOAT_BLOCK`, …), **Faces** `6` and Mip levels; it
  has no 2D preview.

## Not yet

- Cubemaps from `*.env` folders.
- The BRDF lookup table of the split-sum method: it does not depend on the
  environment, so engines make it themselves.
