# `.ktx2` textures

Textures are written as [KTX 2.0](https://registry.khronos.org/KTX/specs/2.0/ktxspec.v2.html)
files. assetc writes only the subset described here; any conforming KTX2
reader can load them.

## Profile

| Property                  | Value                                         |
|---------------------------|-----------------------------------------------|
| `vkFormat`                | `VK_FORMAT_R8G8B8A8_SRGB` (`43`) for sRGB textures, `VK_FORMAT_R8G8B8A8_UNORM` (`37`) for linear ones |
| `typeSize`                | `1`                                           |
| `pixelWidth`/`pixelHeight`| source image size                             |
| `pixelDepth`              | `0` (2D)                                      |
| `layerCount`              | `0` (not an array)                            |
| `faceCount`               | `1`                                           |
| `levelCount`              | `1` without mipmaps, else `floor(log2(max(w, h))) + 1` (full chain to 1×1) |
| `supercompressionScheme`  | `0` (none)                                    |
| Orientation               | `rd` (rows top to bottom), the KTX2 default   |
| Alpha                     | straight (not premultiplied)                  |

Color space and mipmaps are chosen per texture, see
[`docs/features/textures.md`](../features/textures.md), which also defines how
mip levels are computed.

## File layout

All integers little-endian. Offsets are absolute file offsets. `L` is
`levelCount`.

| Offset    | Size   | Content                                                                  |
|----------:|-------:|--------------------------------------------------------------------------|
|         0 |     12 | identifier `AB 4B 54 58 20 32 30 BB 0D 0A 1A 0A` (`«KTX 20»\r\n\x1A\n`)  |
|        12 |     36 | header: the nine u32 fields from the table above, in KTX2 order          |
|        48 |     32 | index: `dfdByteOffset` u32, `dfdByteLength` u32, `kvdByteOffset` u32, `kvdByteLength` u32, `sgdByteOffset` u64 (`0`), `sgdByteLength` u64 (`0`) |
|        80 |   24·L | level index, level 0 (largest) first: `byteOffset` u64, `byteLength` u64, `uncompressedByteLength` u64 |
|   80+24·L |     92 | data format descriptor, see below                                        |
|  172+24·L |      … | key/value data, see below                                                |
|         … |      … | zero padding to a multiple of 4                                          |
|         … |      … | level images, **smallest level first** (level `L-1` … level 0), as KTX2 requires |

Each level image is its pixels, rows top to bottom, each pixel `R G B A`, no
row padding: `byteLength` = `uncompressedByteLength` = `w_n · h_n · 4`.
Images follow each other with no gaps; since every size is a multiple of 4,
every `byteOffset` is 4-aligned.

## Data format descriptor (92 bytes)

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
| A       |        24 |    7      | `0x1F` (alpha, linear) | 0,0,0,0        |           0 |         255 |

Sample layout: `bitOffset` u16, `bitLength` u8, `channelType` u8,
`samplePosition` 4×u8, `sampleLower` u32, `sampleUpper` u32.

## Key/value data

One entry, `KTXwriter` = `assetc <version>`:

```
keyAndValueByteLength u32   = len("KTXwriter") + 1 + len("assetc <version>") + 1
key                         "KTXwriter\0"
value                       "assetc <version>\0"
padding                     zero bytes to a multiple of 4
```

`kvdByteLength` includes the padding.
