# `.agradient.bin` gradients

The gradients of an [`.agradient`](agradient.md) file as data, beside the
image the build bakes from them (its `.ktx2`), so a game can sample a
gradient itself, look one up by name, or read the
[structs](../features/structs.md) attached to it. A
[chunked file](chunks.md) with format `AGRAD`, version `1`; see
[`gradients.md`](../features/gradients.md#build).

## Chunks

| Name       | Required | Payload |
|------------|----------|---------|
| `DESC`     | yes      | [`GradDesc`](#graddesc-48-bytes) |
| `GRADIENT` | yes      | [`Gradient`](#gradient-48-bytes)`[gradient_count]`, in the file's (cell) order |
| `STOPS`    | yes      | `f32[stop_count]`: every stop's position `at` (0–1), gradient after gradient, each gradient's sorted by `at` |
| `RGBA8`    | yes      | `u8[4][stop_count]`: every stop's color as written, `R G B A`, sRGB, straight alpha |
| `RGBAF`    | yes      | `f32[4][stop_count]`: the same colors as **linear** floats: R, G and B decoded from sRGB, A as written / 255 |
| `STRINGS`  | yes      | UTF-8 gradient names back to back |
| [`STRUCTS`, `SVALUES`, `SREFS`, `SSTRINGS`](astruct.md) | no | [attached structs](../features/structs.md): scope `0` the file, scope `1` each gradient (index into `GRADIENT`) |

`STOPS`, `RGBA8` and `RGBAF` are parallel: stop `k` is entry `k` of each,
so the byte and the float form of a color share its index. Text is `offset`
u32 and `len` u32 (bytes) into `STRINGS`.

## GradDesc (48 bytes)

| Offset | Size | Field            | Notes |
|-------:|-----:|------------------|-------|
|      0 |    4 | `gradient_count` | u32 |
|      4 |    4 | `stop_count`     | u32, all gradients' stops |
|      8 |    4 | `width`          | u32, the image's size in px |
|     12 |    4 | `height`         | u32 |
|     16 |    4 | `cell_width`     | u32, px |
|     20 |    4 | `cell_height`    | u32, px |
|     24 |    4 | `columns`        | u32, the grid's |
|     28 |    4 | `rows`           | u32 |
|     32 |    4 | `layout`         | u32: `0` rows, `1` columns |
|     36 |    4 | `reserved`       | u32, `0` |
|     40 |    8 | `texture`        | u64, asset ID of the image (the file's `.ktx2`) |

## Gradient (48 bytes)

| Offset | Size | Field        | Notes |
|-------:|-----:|--------------|-------|
|      0 |    8 | `name_hash`  | u64, FNV-1a-64 of the name (of `""` for none) |
|      8 |    8 | `name`       | text |
|     16 |    4 | `first_stop` | u32, index of its first stop in `STOPS`, `RGBA8` and `RGBAF` |
|     20 |    4 | `stop_count` | u32, at least 2 |
|     24 |    4 | `dither`     | u32: `0` none, `1` crosshatch, `2` pixel |
|     28 |    4 | `reserved`   | u32, `0` |
|     32 |   16 | `uv`         | f32 × 4: `u0 v0 u1 v1`, its cell as 0–1 texture coordinates (top-left origin); the first stop is at the bottom (`v1`) |

The stops are the authored ones; a gradient's [modifiers](agradient.md#modifiers)
change only the image. Sampling at `t` blends the two stops around it as
[the image does](agradient.md#image) (without dithering).
