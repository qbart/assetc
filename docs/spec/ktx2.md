# `.ktx2` textures

Textures are written as [KTX 2.0](https://registry.khronos.org/KTX/specs/2.0/ktxspec.v2.html)
files by [ktx-rs](https://crates.io/crates/ktx-rs) (a Rust port of
KTX-Software's libktx: its texture writer, encoders and data format
descriptors). assetc writes only the subset described here; any conforming
KTX2 reader can load them, and every file passes `ktx validate` (with
warning 7010 for the custom `assetc.usage` key).

## Profile

| Property                  | Value                                         |
|---------------------------|-----------------------------------------------|
| `vkFormat`                | per [format](#formats)                        |
| `typeSize`                | `1`; `2` for the half-float formats           |
| `pixelWidth`/`pixelHeight`| source image size                             |
| `pixelDepth`              | `0` (2D); `N` for color LUT volumes, see [Color LUTs](#color-luts) |
| `layerCount`              | `0` (not an array)                            |
| `faceCount`               | `1`; `6` for [cubemaps](#cubemaps)            |
| `levelCount`              | `1` without mipmaps, else `floor(log2(max(w, h))) + 1` (full chain to 1×1) |
| `supercompressionScheme`  | `0` (none); `2` (zstd) for `uastc`            |
| Orientation               | `rd` (rows top to bottom), the KTX2 default   |
| Alpha                     | straight (not premultiplied)                  |

Color space, mipmaps and the format are chosen per texture and per build,
see [`docs/features/textures.md`](../features/textures.md), which also
defines how mip levels are computed and which encoder makes each format.

## Formats

The `[textures] format` setting (see
[`textures.md`](../features/textures.md#format)) picks the row; the image
kind picks the column. `S`/`U` is the sRGB or linear variant, from the
texture's color space (R, RG and HDR are always linear).

| `format` | RGBA (and RGB) images | R | RG | HDR cubemap |
|----------|-----------------------|---|----|-------------|
| `bc`     | `BC7_SRGB_BLOCK` (146) / `BC7_UNORM_BLOCK` (145) | `BC4_UNORM_BLOCK` (139) | `BC5_UNORM_BLOCK` (141) | `BC6H_UFLOAT_BLOCK` (143) |
| `astc`   | `ASTC_4x4_SRGB_BLOCK` (158) / `ASTC_4x4_UNORM_BLOCK` (157) | `ASTC_4x4_UNORM_BLOCK` | `ASTC_4x4_UNORM_BLOCK` | `ASTC_4x4_SFLOAT_BLOCK` (1000066000) |
| `uastc`  | `UNDEFINED` (0), UASTC LDR 4×4, zstd | as RGBA, linear | as RGBA, linear | `ASTC_4x4_SFLOAT_BLOCK`, UASTC HDR 4×4, zstd |
| `rgba`   | `R8G8B8A8_SRGB` (43) / `R8G8B8A8_UNORM` (37) | `R8_UNORM` (9) | `R8G8_UNORM` (16) | `R16G16B16A16_SFLOAT` (97) |
| `rgb`    | `R8G8B8_SRGB` (29) / `R8G8B8_UNORM` (23) | `R8_UNORM` | `R8G8_UNORM` | `R16G16B16_SFLOAT` (90) |

Texels in the block formats:

- **R** in ASTC and UASTC: `R R R 1` (as libktx swizzles one channel);
  read R. In BC4: R.
- **RG** in ASTC and UASTC: `R R R G`; read R and alpha. In BC5: R, G.
- **RGBA**: R, G, B, A in every format; normal maps are RGBA too (X, Y, Z
  in R, G, B), not BC5.
- `rgb` drops alpha from RGBA images (`R8G8B8`, `R16G16B16_SFLOAT`).
- Lookup textures (gradients, palettes, color LUTs, a model's vertex color
  textures) are written as `rgba` whatever the format.

## File layout

All integers little-endian. Offsets are absolute file offsets. `L` is
`levelCount`.

| Offset    | Size   | Content                                                                  |
|----------:|-------:|--------------------------------------------------------------------------|
|         0 |     12 | identifier `AB 4B 54 58 20 32 30 BB 0D 0A 1A 0A` (`«KTX 20»\r\n\x1A\n`)  |
|        12 |     36 | header: the nine u32 fields from the table above, in KTX2 order          |
|        48 |     32 | index: `dfdByteOffset` u32, `dfdByteLength` u32, `kvdByteOffset` u32, `kvdByteLength` u32, `sgdByteOffset` u64 (`0`), `sgdByteLength` u64 (`0`) |
|        80 |   24·L | level index, level 0 (largest) first: `byteOffset` u64, `byteLength` u64, `uncompressedByteLength` u64 |
|   80+24·L |      … | data format descriptor, see below                                        |
|         … |      … | key/value data, see below                                                |
|         … |      … | zero padding to the level alignment                                      |
|         … |      … | level images, **smallest level first** (level `L-1` … level 0), as KTX2 requires |

- Uncompressed levels: the texels, rows top to bottom, no row padding:
  `byteLength` = `uncompressedByteLength` = `w_n · h_n · texel size`
  (4 for `R8G8B8A8`, 3 for `R8G8B8`, 8 / 6 for the half-float formats).
- Block levels: 4×4 blocks, rows of blocks top to bottom,
  `ceil(w_n / 4) · ceil(h_n / 4)` blocks of 16 bytes (8 for `BC4`); a level
  smaller than a block still takes one block per 4×4 started.
- `uastc`: each level is zstd-compressed on its own: `byteLength` is the
  compressed size, `uncompressedByteLength` the UASTC size (16 bytes a
  block).
- Each level image starts at a multiple of the level alignment: the least
  common multiple of the texel (or block) size and 4 (`1` with
  supercompression), with zero padding before it.

## Data format descriptor

A Khronos Basic descriptor block, as libktx makes it for the `vkFormat`
(`vk2dfd`); for `uastc`, the UASTC (`166`) or UASTC HDR 4×4 color model
libktx's Basis encoder writes. Block formats have `texelBlockDimension`
`3, 3, 0, 0` (4×4), one sample over the whole block and `transferFunction`
`2` (sRGB) for the `_SRGB` formats and sRGB UASTC, else `1` (linear).

The uncompressed RGBA formats' descriptor in full (92 bytes):

`dfdTotalSize` u32 = `92`, followed by one Khronos Basic descriptor block.
The two formats differ only in `transferFunction`:

| Field                           | Value                                    |
|---------------------------------|------------------------------------------|
| vendorId / descriptorType       | `0` / `0` (u32 `0`)                      |
| versionNumber                   | `2` (u16)                                |
| descriptorBlockSize             | `88` (u16)                               |
| colorModel                      | `1` (RGBSDA)                             |
| colorPrimaries                  | `1` (BT.709 / sRGB)                      |
| transferFunction                | `2` (sRGB) for `_SRGB`, `1` (linear) for `_UNORM` |
| flags                           | `0` (straight alpha)                     |
| texelBlockDimension[0..3]       | `0, 0, 0, 0` (1×1×1×1)                   |
| bytesPlane[0..7]                | `4, 0, 0, 0, 0, 0, 0, 0`                 |

Then four 16-byte samples, one per channel:

| Channel | bitOffset | bitLength | channelType            | samplePosition | sampleLower | sampleUpper |
|---------|----------:|----------:|------------------------|----------------|------------:|------------:|
| R       |         0 |    7 (8 bits) | `0x00`             | 0,0,0,0        |           0 |         255 |
| G       |         8 |    7      | `0x01`                 | 0,0,0,0        |           0 |         255 |
| B       |        16 |    7      | `0x02`                 | 0,0,0,0        |           0 |         255 |
| A       |        24 |    7      | `0x1F` for `_SRGB`, `0x0F` for `_UNORM` | 0,0,0,0 |      0 |         255 |

Alpha is channel `0x0F`. With the sRGB transfer function it carries the
`LINEAR` qualifier (`0x10`), marking alpha as not sRGB-encoded; with the linear
transfer function the qualifier is omitted, matching the Khronos reference
descriptors.

Sample layout: `bitOffset` u16, `bitLength` u8, `channelType` u8,
`samplePosition` 4×u8, `sampleLower` u32, `sampleUpper` u32.

## One and two channels

[Texture paints](../features/texture-paint.md#build) in Channels mode with R
or RG are written with fewer channels (formats `rgba` and `rgb`; the block
formats are in [Formats](#formats)); everything else above stays the same:

| Property | R | RG |
|----------|---|----|
| `vkFormat` | `VK_FORMAT_R8_UNORM` (`9`) | `VK_FORMAT_R8G8_UNORM` (`16`) |
| Level image | `w_n · h_n` bytes, one per texel | `w_n · h_n · 2` bytes, `R G` per texel |
| `dfdTotalSize` | `60` | `76` |
| descriptorBlockSize | `56` | `72` |
| transferFunction | `1` (linear) | `1` (linear) |
| bytesPlane[0] | `1` | `2` |
| Samples | R only | R, G |

The samples are R's and G's rows of the table above (`bitOffset` 0 and 8).
A level's `byteLength` need not be a multiple of 4 here (a 1 × 1 R level is
1 byte). KTX2 aligns each level image to the least common multiple of the
texel size and 4, which is 4 for both formats: zero bytes pad each level
image to the next multiple of 4, `byteOffset` points at the image and
`byteLength` does not count the padding.

## Key/value data

Two entries, sorted by key as KTX2 requires: `KTXwriter` =
`assetc <version> / libktx <version>` (`assetc 0.2.2 / libktx v4.4.2-2`:
assetc's, then the libktx version ktx-rs ports), then `assetc.usage` = what
the texture is for. Each is

```
keyAndValueByteLength u32   = len(key) + 1 + len(value) + 1
key                         "<key>\0"
value                       "<value>\0"
padding                     zero bytes to a multiple of 4
```

`kvdByteLength` includes the padding.

`assetc.usage` tells a game, or a tool, what the texture holds, so it does
not have to guess from the name or the format:

| Value | Textures |
|-------|----------|
| `noise` | generated from an `.anoise` file |
| `gradient` | generated from an `.agradient` file |
| `palette` | generated from an `.apalette` file |
| `normal` | normal maps: a material's `normal` slot, a glTF material's normal texture |
| `orm` | packed occlusion, roughness, metallic (`<name>.orm.ktx2`, a glTF material's) |
| `height` | a material's `height` slot |
| `atlas` | an atlas's texture |
| `font` | a font's signed distance field atlas |
| `lut` | a color LUT, strip or volume |
| `environment` | an HDR environment map (cubemap) |
| `color` | any other sRGB texture (images, base color, emissive) |
| `data` | any other linear texture |

The first that applies, in this order, wins: a noise used as a normal map
is `noise`.

## Cubemaps

An [HDR environment map](../features/hdr.md) is written with this profile
instead:

| Property      | Value |
|---------------|-------|
| `vkFormat`    | the HDR column of [Formats](#formats); `R16G16B16A16_SFLOAT` (`97`) for `rgba` |
| `typeSize`    | `2` for the half-float formats, else `1` |
| `pixelWidth` / `pixelHeight` | the face size (square, a power of two) |
| `pixelDepth`, `layerCount` | `0` |
| `faceCount`   | `6` |
| `levelCount`  | `log2(size) + 1` (full chain to 1 × 1) |

Each level image holds the six faces back to back in the order +X, −X,
+Y, −Y, +Z, −Z, each face its rows (of texels or blocks) top to bottom. In
`rgba`, each texel is `R G B A` as IEEE half floats: `byteLength` =
`6 × w_n × h_n × 8`, the level images starting at a multiple of 8 bytes
(the texel size).

For `R16G16B16A16_SFLOAT`, the data format descriptor is the same 92 bytes
with these differences:
`transferFunction` `1` (linear), `bytesPlane[0]` `8`, and each sample has
`bitOffset` `0`, `16`, `32`, `48`, `bitLength` `15` (16 bits), the channel
(`0x00`, `0x01`, `0x02`, `0x0F`) with the `SIGNED` (`0x40`) and `FLOAT`
(`0x80`) qualifiers, `sampleLower` `0xBF800000` (−1.0) and `sampleUpper`
`0x3F800000` (1.0).

## Color LUTs

A compiled `.cube` LUT of size `N` (see [`lut.md`](../features/lut.md#build))
uses the same profile with `VK_FORMAT_R8G8B8A8_UNORM` and `levelCount` `1`,
as a strip or a volume. Entries are stored as `round(clamp(value, 0, 1) × 255)`
per channel with alpha `255`.

### Volume

`pixelWidth`, `pixelHeight` and `pixelDepth` are `N` (a 3D texture). The
level's data is `N` slices of `N` rows of `N` texels: texel `(x, y, z)` holds
the entry for input `r = x`, `g = y`, `b = z`, so the bytes follow the
`.cube` table order (red fastest, then green, then blue). Level 0's
`byteLength` is `N³ × 4`.

### Strip

`pixelWidth` `N²`, `pixelHeight` `N`, `pixelDepth` `0`. Pixel `(x, y)`
(`y` from the top row) holds the LUT entry for input

```
r = x mod N,  b = floor(x / N),  g = y        (indices 0 … N − 1)
```

To sample it, a shader picks the two blue slices around `b × (N − 1)`, samples each
bilinearly at `(r, g)` (keeping half a texel inside each slice) and blends
them.
