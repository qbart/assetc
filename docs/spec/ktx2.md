# `.ktx2` textures

Textures are written as [KTX 2.0](https://registry.khronos.org/KTX/specs/2.0/ktxspec.v2.html)
files. assetc writes only the subset described here; any conforming KTX2
reader can load them.

## Profile (version 1 of assetc's output)

| Property                  | Value                                         |
|---------------------------|-----------------------------------------------|
| `vkFormat`                | `VK_FORMAT_R8G8B8A8_SRGB` (`43`)              |
| `typeSize`                | `1`                                           |
| `pixelWidth`/`pixelHeight`| source image size                             |
| `pixelDepth`              | `0` (2D)                                      |
| `layerCount`              | `0` (not an array)                            |
| `faceCount`               | `1`                                           |
| `levelCount`              | `1` (no mipmaps)                              |
| `supercompressionScheme`  | `0` (none)                                    |
| Orientation               | `rd` (rows top to bottom), the KTX2 default   |
| Alpha                     | straight (not premultiplied)                  |

## File layout

All integers little-endian. Offsets are absolute file offsets.

| Offset | Size | Content                                                                  |
|-------:|-----:|--------------------------------------------------------------------------|
|      0 |   12 | identifier `AB 4B 54 58 20 32 30 BB 0D 0A 1A 0A` (`«KTX 20»\r\n\x1A\n`)  |
|     12 |   36 | header: the nine u32 fields from the table above, in KTX2 order          |
|     48 |   32 | index: `dfdByteOffset` u32, `dfdByteLength` u32, `kvdByteOffset` u32, `kvdByteLength` u32, `sgdByteOffset` u64 (`0`), `sgdByteLength` u64 (`0`) |
|     80 |   24 | level index (1 level): `byteOffset` u64, `byteLength` u64, `uncompressedByteLength` u64 |
|    104 |   92 | data format descriptor, see below                                        |
|    196 |    … | key/value data, see below                                                |
|      … |    … | zero padding to a multiple of 4                                          |
|      … |  w·h·4 | level 0 pixels: rows top to bottom, each pixel `R G B A`, no row padding |

`byteLength` and `uncompressedByteLength` are both `width * height * 4`.

## Data format descriptor (92 bytes)

`dfdTotalSize` u32 = `92`, followed by one Khronos Basic descriptor block:

| Field                           | Value                                    |
|---------------------------------|------------------------------------------|
| vendorId / descriptorType       | `0` / `0` (u32 `0`)                      |
| versionNumber                   | `2` (u16)                                |
| descriptorBlockSize             | `88` (u16)                               |
| colorModel                      | `1` (RGBSDA)                             |
| colorPrimaries                  | `1` (BT.709 / sRGB)                      |
| transferFunction                | `2` (sRGB)                               |
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
