# Textures

How a texture source is turned into a `.atex.bin` file: the texels in the
GPU's own format, every mip level, ready to upload
(see [`build.md`](build.md) for which sources are textures, and
[`docs/spec/atex.bin.md`](../spec/atex.bin.md) for the output format).

## Settings

Textures used by a [material](materials.md) take their color space from the
material slot instead of `linear`; `no_mipmaps` still applies to them.

Per-texture settings come from the `[textures]` table in
[`assetc.toml`](../spec/assetc.toml.md). Each setting is a list of glob
patterns matched against the source path relative to `input`, `/`-separated,
with its extension (e.g. `textures/rock_n.png`).

```toml
[textures]
linear = ["**/*_n.png", "**/*_orm.png"]
no_mipmaps = ["ui/**"]
```

| Setting      | Texture matching any pattern                 | Otherwise (default)        |
|--------------|----------------------------------------------|----------------------------|
| `linear`     | linear data (normal, roughness, masks, …)    | sRGB color                 |
| `no_mipmaps` | base level only                              | full mip chain             |
| `volume_luts` | color LUT (`*.cube`) as a 3D texture, see [`lut.md`](lut.md#build) | 2D strip |

Two more keys are not glob lists: `format` (see [Format](#format)) and
`quality` (see [Quality](#quality)).

## Format

`[textures] format` picks the GPU format every texture of the build is
written in, one per [target](../spec/assetc.toml.md#targets): a desktop
target takes `bc`, a mobile one `astc`. A build makes the textures of one
target, so they hold only what that GPU reads natively; a game never
transcodes them.

```toml
[textures]
format = "bc"            # the default

[targets.mobile]
textures = { format = "astc" }
```

| `format` | Written as | 8-bit RGBA | R (texture paint) | RG (texture paint) | HDR cubemap |
|----------|------------|------------|-------------------|--------------------|-------------|
| `bc` (default) | BCn blocks, for desktop GPUs | `BC7` | `BC4` | `BC5` | `BC6H` (unsigned) |
| `astc`   | ASTC 4 × 4 blocks, for mobile GPUs | ASTC 4 × 4 LDR | ASTC 4 × 4, R in R, G, B; alpha 1 | ASTC 4 × 4, R in R, G, B; G in alpha | ASTC 4 × 4 HDR |
| `rgba`   | uncompressed | `R8G8B8A8` | `R8` | `R8G8` | `R16G16B16A16_SFLOAT` |
| `rgb`    | uncompressed, alpha dropped | `R8G8B8` | `R8` | `R8G8` | `R16G16B16_SFLOAT` |

- The [color space](#color-space) still applies: sRGB textures get the
  `_SRGB` variant of the format (`BC7_SRGB_BLOCK`, `ASTC_4x4_SRGB_BLOCK`,
  `R8G8B8_SRGB`), linear ones the `_UNORM` variant. R and RG textures are
  always linear. Mip levels are made from the 8-bit images as below, then
  each level is encoded on its own.
- Each texture records where its channels are (its
  [swizzle](../spec/atex.bin.md#swizzle)): an RG texture is read from R and
  G in BC5 and `R8G8`, from R and alpha in ASTC.
- Normal maps are encoded like any other linear RGBA texture (`BC7`, not
  `BC5`), so a shader reads X, Y, Z from R, G, B whatever the format.
- **Lookup textures stay `rgba`** whatever `format` says, since block
  compression would change the exact texels they are read for:
  [gradients](gradients.md), [palettes](palettes.md),
  [color LUTs](lut.md) and a model's
  [vertex color textures](vertex-colors.md#export).
- Encoding uses [imgio](https://crates.io/crates/imgio) (pure Rust ports of
  the Basis Universal and ARM astcenc encoders), straight to the target
  format, at the build's [quality](#quality):
  - `bc`: BC7 with basisu's `bc7f` (`bc7e` at `high` and `best`), BC4 and
    BC5 with basisu's BC4 encoder, BC6H with basisu's BC6H encoder.
  - `astc`: astcenc, 4 × 4 blocks, LDR (sRGB or linear) or HDR.
  - `rgba`, `rgb`: the texels as they are (HDR as IEEE half floats).
- An unknown value is an error when `assetc.toml` is read: `textures.format:
  unknown variant ...`. `uastc`, which older builds took, is one: Basis
  Universal textures need transcoding when a game loads them.
- Changing `format` rebuilds every texture.

## Quality

`[textures] quality` trades build time for how close the blocks come to the
image; it only changes `bc` and `astc` textures.

```toml
[textures]
quality = "default"      # fastest, fast, default, high or best
```

| `quality` | BC7 | BC6H (basisu search) | ASTC (astcenc preset) |
|-----------|-----|-----------------------|-----------------------|
| `fastest` | `bc7f`, fastest | least | `fastest` |
| `fast`    | `bc7f`, fast | less | `fast` |
| `default` | `bc7f`, partly analytical | some | `medium` |
| `high`    | `bc7e`, slow (perceptual for sRGB) | more | `thorough` |
| `best`    | `bc7e`, slowest (perceptual for sRGB) | most | `exhaustive` |

BC4 and BC5 have one quality.

A 1024 × 1024 image takes about 0.01 s in BC7 and 0.1 s in ASTC at
`default`; `high` takes about 20 times longer in BC7 and twice as long in
ASTC, `best` longer still. Changing `quality`
rebuilds every texture; an unknown value is an error when `assetc.toml` is
read.

See [`docs/spec/atex.bin.md`](../spec/atex.bin.md#formats) for what each
format writes.

## Color space

- **sRGB** (default): the pixels are color. The texture is written in the
  `_SRGB` variant of its [format](#format) (`BC7_SRGB_BLOCK`,
  `R8G8B8A8_SRGB`, …), so the GPU decodes it to linear when sampling.
- **linear**: the pixels are data and are used as-is. The texture is
  written in the `_UNORM` variant (`BC7_UNORM_BLOCK`, `R8G8B8A8_UNORM`, …).

Alpha is always linear and straight (not premultiplied).

## Mipmaps

Unless the texture matches `no_mipmaps`, a full mip chain is written, down to
1×1. A texture with an [`.amipmap`](mipmaps.md) file next to it has the
chain that file describes instead: a chosen number of levels, any of them
from an image prepared by hand.

- Level count: `floor(log2(max(width, height))) + 1`.
- Level `n` size: `max(1, floor(width / 2^n))` × `max(1, floor(height / 2^n))`.
- Each level is computed from the previous level (8-bit values) with a box
  filter: texel `(x, y)` of a `w × h` level made from a `W × H` level is the
  average of the source texels with
  `floor(x·W/w) ≤ sx < floor((x+1)·W/w)` and
  `floor(y·H/h) ≤ sy < floor((y+1)·H/h)`. For even sizes that is the usual
  2×2 average; odd sizes fold the extra row/column into the last texel.
- **sRGB** textures are averaged in linear light: R, G, B are decoded with the
  sRGB transfer function, averaged, and encoded back. So a 2×2 checker of black
  and white becomes `188`, not `128`.
- **linear** textures and all alpha channels are averaged as stored.
- Results are rounded to the nearest 8-bit value (halves round up).
- Normal maps are averaged like any other linear data; the vectors are not
  renormalized.

## Modifiers

A `.png` / `.jpg` / `.jpeg` / `.tga` in `input` can carry the same modifier stack as
an [`.anoise`](noise.md#modifiers-tab) file, kept in its
[`.amod`](../spec/amod.md#modifiers) sidecar (`brick.jpg.amod`) and edited
in the Inspector's Modifiers tab.

- The enabled modifiers run, in order, every time assetc loads the image:
  the build (as a plain texture, in a material slot, packed into an ORM
  texture, as an environment-map face), the canvas and the Inspector. What
  the canvas shows is what is exported.
- They work on the stored 8-bit values divided by 255; sRGB images are not
  linearized first. Alpha is kept as it is.
- A grayscale image becomes RGB when an enabled `colorize` is present (the
  Inspector's channels follow).
- The source file itself is never changed.
- A sidecar that does not parse fails the image like an undecodable file
  (`brick.jpg.amod: <problem>`).
- **Modifiers tab**: for images in the Input tree it works exactly as for
  `.anoise` files (see [`noise.md`](noise.md#modifiers-tab)), but saves to the
  sidecar: adding the first modifier creates `<file>.amod`, and removing the
  last deletes it (unless it holds other settings).

## Output

CLI lines show the settings used, for example:

```
textures/Grass.png -> textures/Grass.atex.bin (bc7, srgb, 10 levels)
textures/rock_n.png -> textures/rock_n.atex.bin (bc7, linear, 10 levels)
ui/button.png -> ui/button.atex.bin (bc7, srgb, 1 level)
textures/rock.png -> textures/rock.atex.bin (bc7, srgb, 9 levels, 2 custom)
```

The first word is the format written: `bc7`, `bc4`, `bc5`, `bc6h`,
`astc 4x4`, `astc 4x4 hdr`, `rgba8`, `rgb8`, `r8`,
`rg8`, `rgba16f` or `rgb16f`. The last one has an
[`.amipmap`](mipmaps.md#build): two of its levels come from files.
