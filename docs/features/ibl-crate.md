# IBL crate

assetc's image-based lighting, the part that turns an
[HDR environment map](hdr.md) into its skybox cubemap and its prefiltered
lighting maps, is its own crate, **`assetc-ibl`**, so other programs
(editors, tools, a game baking reflection probes at run time) make exactly
the cubemaps assetc builds: the same faces, mip chains, GGX prefiltering,
irradiance and spherical harmonics, the same floats.

## Crate

- Package `assetc-ibl`, library `assetc_ibl`
  (`use assetc_ibl::{Hdr, specular, irradiance};`). It lives in `ibl/`, a
  member of the Cargo workspace, and is published to
  [crates.io](https://crates.io/crates/assetc-ibl).
- Lightweight: it depends on **nothing but `std`**.
- No file formats: no OpenEXR reading, no `.atex.bin` writing, no texture
  compression. It works on plain values: a linear RGBA float
  latitude-longitude image in, cubemap levels of linear RGBA floats and
  spherical harmonics out. Reading `.exr` files (with imgio), encoding the
  cubemaps to BC6H or half floats and `[environments]` in `assetc.toml`
  stay in the compiler.
- The version is the workspace's, set in `ibl/Cargo.toml` with the others
  (see [Version](release.md#version)).
- Author Bartłomiej Wójtowicz; licensed `MIT OR Apache-2.0`
  (`ibl/LICENSE-MIT`, `ibl/LICENSE-APACHE`). Needs Rust 1.88 or newer
  (`rust-version`).
- `ibl/README.md` is both its crates.io page and the front page of its API
  docs; its example is compiled as a doctest.
- The compiler (`assetc-core`) depends on it by path and makes every
  skybox, specular and irradiance map (and the 3D view's preview faces)
  with it: what the crate holds is not also kept in the compiler.
- The [SDK](sdk.md) does not depend on it; it reads the built
  `.atex.bin` cubemaps.

```toml
[dependencies]
assetc-ibl = "0.4"
```

## Contents

| Item | Holds |
|------|-------|
| `Hdr` | A linear RGBA float image in latitude-longitude layout: `width`, `height`, `pixels` (rows top to bottom); `sample(d)`: the image along direction `d`, bilinearly, wrapping around; `cubemap(size)`: the [cubemap](hdr.md#cubemap) levels, faces of `size` down to 1 × 1, each texel the average of the 2 × 2 under it. |
| `Level` | One cubemap level: `size` and six `faces` (+X, −X, +Y, −Y, +Z, −Z, rows top to bottom, `size × size` texels each); `sample(d)`: bilinearly along direction `d`, across face edges. |
| `face_size(height)` | The skybox's face size: the largest power of two not above `height / 2`, at least 1. |
| `direction(face, x, y, size)` | The direction texel `(x, y)` of a face looks along, as Vulkan picks cube faces (not normalized). |
| `MIN_SOURCE` | 8: the smallest face size the lighting maps read from. |
| `lighting_source(hdr)` | What the lighting maps read: the cubemap at the skybox's face size, at least `MIN_SOURCE`, with its mip chain. |
| `specular(source, max_size, samples)` | The [specular map](hdr.md#specular): faces of `min(source size, max_size)` down to 4 px, level `i` prefiltered with GGX for roughness `i / (levels − 1)` over `samples` Hammersley directions (at least 1). `max_size` is a power of two, at least 8; anything else panics. |
| `irradiance(source, size)` | The [irradiance map](hdr.md#irradiance), one level of faces of `size`, and its 9 spherical harmonics coefficients (RGB each, divided by π, in [`SH9`](../spec/atex.bin.md#sh9) order). |
| `sh_irradiance(sh, d)` | What those coefficients give along unit direction `d`: the irradiance divided by π, as an engine lighting from them computes it. |

Every texel is computed from the input alone, with no random seed, so the
same image and settings always give the same floats; the work is spread
over every core, and the result does not depend on how many there are.

## Publishing

`make publish` publishes `assetc-ibl` (it depends on no other assetc crate)
before the SDK; see [Publishing](colors.md#publishing).
