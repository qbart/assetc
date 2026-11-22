# `.atex.bin` textures

Every texture assetc builds: images, generated textures (noise, gradients,
palettes), atlases, font atlases, color LUTs, HDR environment cubemaps and a
model's textures. The texels are already in the GPU format of the build's
[target](assetc.toml.md#targets) (BC blocks for desktop, ASTC blocks for
mobile, or plain texels), with every mip level, so a game uploads each level
as it is: no transcoding, no inflating, and no KTX library.

It uses the [chunked container](chunks.md) with format name `ATEX` and
version `1`. All integers are little-endian.

Which format each texture gets, its color space and how its mip levels are
made are in [`docs/features/textures.md`](../features/textures.md).

## Chunks

In this order:

| Chunk    | Required | Payload |
|----------|----------|---------|
| `DESC`   | yes      | [`Desc`](#desc-64-bytes), one record |
| `LEVELS` | yes      | [`Level`](#level-32-bytes)`[]`, `level_count` records, level 0 (largest) first |
| `DATA`   | yes      | the level images, see [Data](#data) |

Readers ignore other chunks.

## Desc (64 bytes)

| Offset | Size | Field          | Notes |
|-------:|-----:|----------------|-------|
|      0 |    4 | `format`       | u32, the Vulkan `VkFormat` value, see [Formats](#formats) |
|      4 |    4 | `width`        | u32, level 0's width in texels, ≥ 1 |
|      8 |    4 | `height`       | u32, level 0's height in texels, ≥ 1 |
|     12 |    4 | `depth`        | u32, level 0's depth: `1`, or `N` for a [volume](#color-luts) |
|     16 |    4 | `faces`        | u32, `1`, or `6` for a [cubemap](#cubemaps) |
|     20 |    4 | `layers`       | u32, array layers: `1` (assetc writes no arrays) |
|     24 |    4 | `level_count`  | u32, mip levels, ≥ 1 |
|     28 |    4 | `usage`        | u32, what the texture is for, see [Usage](#usage) |
|     32 |    4 | `flags`        | u32, bit 0 `SRGB`: the color channels are sRGB-encoded (set exactly for the `_SRGB` formats); other bits `0` |
|     36 |    4 | `swizzle`      | u8 × 4, see [Swizzle](#swizzle) |
|     40 |    1 | `block_width`  | u8, texels across a block: `4` for the block formats, `1` for plain texels |
|     41 |    1 | `block_height` | u8, likewise down a block |
|     42 |    1 | `block_depth`  | u8, `1` |
|     43 |    1 | `reserved`     | u8, `0` |
|     44 |    4 | `block_bytes`  | u32, bytes of one block (or one texel): `16` for BC7, BC5, BC6H and ASTC, `8` for BC4, `4` for `R8G8B8A8`, … |
|     48 |   16 | `reserved`     | `0` |

`block_*` repeat what the format implies, so a loader can size and upload
levels without a table of formats.

## Level (32 bytes)

| Offset | Size | Field      | Notes |
|-------:|-----:|------------|-------|
|      0 |    8 | `offset`   | u64, from the start of the `DATA` payload, a multiple of 16 |
|      8 |    8 | `size`     | u64, bytes of the level image |
|     16 |    4 | `width`    | u32, `max(1, width >> n)` for level `n` |
|     20 |    4 | `height`   | u32, `max(1, height >> n)` |
|     24 |    4 | `depth`    | u32, `max(1, depth >> n)` |
|     28 |    4 | `reserved` | u32, `0` |

## Data

The level images, level 0 first, each starting at its `offset` (a multiple
of 16 from the start of `DATA`, which itself starts 16-aligned in the file),
zero bytes between them.

A level image holds, outermost first: each layer, each face (cubemaps: +X,
−X, +Y, −Y, +Z, −Z), each depth slice; and within one of those, rows of
blocks (or texels) top to bottom, each row left to right, with no row
padding. Its `size` is

```
ceil(w / block_width) × ceil(h / block_height) × d × faces × layers × block_bytes
```

for the level's `w`, `h`, `d`. A level smaller than a block (`2 × 2` in BC7)
still takes one whole block. Readers reject a level whose `size` differs, or
whose range runs past `DATA`.

## Formats

The `[textures] format` setting of the target (see
[`textures.md`](../features/textures.md#format)) picks the row; what the
image holds picks the column. `SRGB` / `UNORM` follow the texture's color
space; R, RG and HDR textures are always linear.

| `format` | RGBA images | R | RG | HDR cubemap |
|----------|-------------|---|----|-------------|
| `bc`     | `BC7_SRGB_BLOCK` / `BC7_UNORM_BLOCK` | `BC4_UNORM_BLOCK` | `BC5_UNORM_BLOCK` | `BC6H_UFLOAT_BLOCK` |
| `astc`   | `ASTC_4x4_SRGB_BLOCK` / `ASTC_4x4_UNORM_BLOCK` | `ASTC_4x4_UNORM_BLOCK` | `ASTC_4x4_UNORM_BLOCK` | `ASTC_4x4_SFLOAT_BLOCK` |
| `rgba`   | `R8G8B8A8_SRGB` / `R8G8B8A8_UNORM` | `R8_UNORM` | `R8G8_UNORM` | `R16G16B16A16_SFLOAT` |
| `rgb`    | `R8G8B8_SRGB` / `R8G8B8_UNORM` | `R8_UNORM` | `R8G8_UNORM` | `R16G16B16_SFLOAT` |

Lookup textures (gradients, palettes, color LUTs, a model's vertex color
textures) are `R8G8B8A8` whatever the format.

Every format assetc writes, with the values of other APIs for loaders that
do not use Vulkan (`—` where the API has no such format):

| Format (`VK_FORMAT_…`) | `format` value | `block_bytes` | DXGI | wgpu |
|------------------------|---------------:|--------------:|-----:|------|
| `R8_UNORM`             | 9   | 1 | 61 `R8_UNORM` | `R8Unorm` |
| `R8G8_UNORM`           | 16  | 2 | 49 `R8G8_UNORM` | `Rg8Unorm` |
| `R8G8B8_UNORM`         | 23  | 3 | — | — |
| `R8G8B8_SRGB`          | 29  | 3 | — | — |
| `R8G8B8A8_UNORM`       | 37  | 4 | 28 `R8G8B8A8_UNORM` | `Rgba8Unorm` |
| `R8G8B8A8_SRGB`        | 43  | 4 | 29 `R8G8B8A8_UNORM_SRGB` | `Rgba8UnormSrgb` |
| `R16G16B16_SFLOAT`     | 90  | 6 | — | — |
| `R16G16B16A16_SFLOAT`  | 97  | 8 | 10 `R16G16B16A16_FLOAT` | `Rgba16Float` |
| `BC4_UNORM_BLOCK`      | 139 | 8 | 80 `BC4_UNORM` | `Bc4RUnorm` |
| `BC5_UNORM_BLOCK`      | 141 | 16 | 83 `BC5_UNORM` | `Bc5RgUnorm` |
| `BC6H_UFLOAT_BLOCK`    | 143 | 16 | 95 `BC6H_UF16` | `Bc6hRgbUfloat` |
| `BC7_UNORM_BLOCK`      | 145 | 16 | 98 `BC7_UNORM` | `Bc7RgbaUnorm` |
| `BC7_SRGB_BLOCK`       | 146 | 16 | 99 `BC7_UNORM_SRGB` | `Bc7RgbaUnormSrgb` |
| `ASTC_4x4_UNORM_BLOCK` | 157 | 16 | — | `Astc { B4x4, Unorm }` |
| `ASTC_4x4_SRGB_BLOCK`  | 158 | 16 | — | `Astc { B4x4, UnormSrgb }` |
| `ASTC_4x4_SFLOAT_BLOCK` | 1000066000 | 16 | — | `Astc { B4x4, Hdr }` |

The three-channel formats (`R8G8B8*`, `R16G16B16_SFLOAT`, from `rgb`) are
for engines that keep them; most GPUs need a fourth channel added on upload.
Half floats are IEEE 754 binary16. Plain texels store their channels in
order (`R G B A`), one byte each (two for half floats).

## Swizzle

Where each of the texture's channels ends up when the GPU samples it:
`swizzle[i]` for the texture's channel `i` (R, G, B, A) is the component of
the sampled value that holds it (`0` R, `1` G, `2` B, `3` A), or `255` when
the texture has no such channel. Its channels are those before the first
`255`.

| Texture | Formats | `swizzle` |
|---------|---------|-----------|
| RGBA    | all `R8G8B8A8`, BC7, ASTC LDR | `0 1 2 3` |
| RGB (alpha dropped) | `R8G8B8`, `R16G16B16_SFLOAT` | `0 1 2 255` |
| R       | `R8`, BC4, ASTC (stored as `R R R 1`) | `0 255 255 255` |
| RG      | `R8G8`, BC5 | `0 1 255 255` |
| RG      | ASTC (stored as `R R R G`, which ASTC compresses best) | `0 3 255 255` |
| HDR     | `R16G16B16A16_SFLOAT`, ASTC HDR (alpha 1) | `0 1 2 3` |
| HDR     | BC6H (no alpha) | `0 1 2 255` |

So a shader reading an RG texture paint takes `.rg` from BC5 and `.ra` from
ASTC; normal maps are RGBA (X, Y, Z in R, G, B) in every format.

## Usage

What the texture holds, so a game or a tool does not have to guess from the
name or the format. The first that applies, in this order, wins: a noise
used as a normal map is `noise`.

| Value | Name | Textures |
|------:|------|----------|
|  5 | `noise` | generated from an `.anoise` file |
|  6 | `gradient` | generated from an `.agradient` file |
|  7 | `palette` | generated from an `.apalette` file |
|  2 | `normal` | normal maps: a material's `normal` slot, a glTF material's normal texture |
|  3 | `orm` | packed occlusion, roughness, metallic (`<name>.orm.atex.bin`, a glTF material's) |
|  4 | `height` | a material's `height` slot |
|  8 | `atlas` | an atlas's texture |
|  9 | `font` | a font's signed distance field atlas |
| 10 | `lut` | a color LUT, strip or volume |
| 11 | `environment` | an HDR environment map (cubemap) |
|  0 | `color` | any other sRGB texture (images, base color, emissive) |
|  1 | `data` | any other linear texture |

Readers treat a value they do not know as `data`.

## Cubemaps

An [HDR environment map](../features/hdr.md) has `faces` `6`, square faces
(`width` = `height`, a power of two), `depth` `1` and a full chain of
`log2(width) + 1` levels, in the HDR column of [Formats](#formats). Each
level image holds the six faces back to back in the order +X, −X, +Y, −Y,
+Z, −Z.

## Color LUTs

A compiled `.cube` LUT of size `N` (see [`lut.md`](../features/lut.md#build))
is `R8G8B8A8_UNORM` with one level, as a strip or a volume. Entries are
stored as `round(clamp(value, 0, 1) × 255)` per channel with alpha `255`.

### Volume

`width`, `height` and `depth` are `N` (a 3D texture). The level's data is
`N` slices of `N` rows of `N` texels: texel `(x, y, z)` holds the entry for
input `r = x`, `g = y`, `b = z`, so the bytes follow the `.cube` table order
(red fastest, then green, then blue). Its `size` is `N³ × 4`.

### Strip

`width` `N²`, `height` `N`, `depth` `1`. Texel `(x, y)` (`y` from the top
row) holds the LUT entry for input

```
r = x mod N,  b = floor(x / N),  g = y        (indices 0 … N − 1)
```

To sample it, a shader picks the two blue slices around `b × (N − 1)`,
samples each bilinearly at `(r, g)` (keeping half a texel inside each slice)
and blends them.

## Example

A 5 × 3 sRGB image, format `bc`, full chain: `DESC` has `format` `146`,
`width` `5`, `height` `3`, `depth` `1`, `faces` `1`, `layers` `1`,
`level_count` `3`, `usage` `0`, `flags` `1`, `swizzle` `0 1 2 3`, blocks
`4 4 1` of `16` bytes. `LEVELS`:

| Level | `offset` | `size` | Size |
|------:|---------:|-------:|------|
| 0 | 0  | 32 (2 × 1 blocks) | 5 × 3 |
| 1 | 32 | 16 | 2 × 1 |
| 2 | 48 | 16 | 1 × 1 |

`DATA` is those 64 bytes.
