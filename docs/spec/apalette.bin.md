# `.apalette.bin` palettes

The colors of an [`.apalette`](apalette.md) file as data, beside the image
the build bakes from them (its `.ktx2`), so a game can use a color by name
or index without sampling the image, or read the
[structs](../features/structs.md) attached to it. A
[chunked file](chunks.md) with format `APALETTE`, version `1`; see
[`palettes.md`](../features/palettes.md#build).

## Chunks

| Name      | Required | Payload |
|-----------|----------|---------|
| `DESC`    | yes      | [`PaletteDesc`](#palettedesc-48-bytes) |
| `COLORS`  | yes      | [`Color`](#color-32-bytes)`[color_count]`, in the file's (cell) order |
| `RGBA8`   | yes      | `u8[4][color_count]`: each color as written, `R G B A`, sRGB, straight alpha |
| `RGBAF`   | yes      | `f32[4][color_count]`: the same colors as **linear** floats: R, G and B decoded from sRGB, A as written / 255 |
| `STRINGS` | yes      | UTF-8 color names back to back |
| [`STRUCTS`, `SVALUES`, `SREFS`, `SSTRINGS`](astruct.md) | no | [attached structs](../features/structs.md): scope `0` the file, scope `1` each color (index into `COLORS`) |

`COLORS`, `RGBA8` and `RGBAF` are parallel: color `i` is entry `i` of each,
so the byte and the float form of a color share its index. Text is `offset`
u32 and `len` u32 (bytes) into `STRINGS`.

## PaletteDesc (48 bytes)

| Offset | Size | Field         | Notes |
|-------:|-----:|---------------|-------|
|      0 |    4 | `color_count` | u32 |
|      4 |    4 | `reserved`    | u32, `0` |
|      8 |    4 | `width`       | u32, the image's size in px |
|     12 |    4 | `height`      | u32 |
|     16 |    4 | `cell_width`  | u32, px |
|     20 |    4 | `cell_height` | u32, px |
|     24 |    4 | `columns`     | u32, the grid's |
|     28 |    4 | `rows`        | u32 |
|     32 |    4 | `layout`      | u32: `0` rows, `1` columns |
|     36 |    4 | `reserved`    | u32, `0` |
|     40 |    8 | `texture`     | u64, asset ID of the image (the file's `.ktx2`) |

## Color (32 bytes)

| Offset | Size | Field       | Notes |
|-------:|-----:|-------------|-------|
|      0 |    8 | `name_hash` | u64, FNV-1a-64 of the name (of `""` for none) |
|      8 |    8 | `name`      | text |
|     16 |   16 | `uv`        | f32 × 4: `u0 v0 u1 v1`, its cell as 0–1 texture coordinates (top-left origin) |
