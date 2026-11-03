# `.aatlas` atlas metadata

Where each frame of an [atlas](../features/atlases.md) is in its texture. A
[chunked file](chunks.md) with format `AATLAS`, version `1`.

## Chunks

| Name      | Required | Payload |
|-----------|----------|---------|
| `DESC`    | yes      | [`AtlasDesc`](#atlasdesc-48-bytes) |
| `FRAMES`  | yes      | [`Frame`](#frame-64-bytes)`[frame_count]`, in name order |
| `STRINGS` | yes      | UTF-8 frame names back to back |

## AtlasDesc (48 bytes)

| Offset | Size | Field         | Notes |
|-------:|-----:|---------------|-------|
|      0 |    4 | `width`       | u32, px |
|      4 |    4 | `height`      | u32, px |
|      8 |    4 | `frame_count` | u32 |
|     12 |    4 | `packing`     | u32: `0` grid, `1` pack |
|     16 |    4 | `columns`     | u32, grid: cells per row; `0` for pack |
|     20 |    4 | `cell_width`  | u32, grid; `0` for pack |
|     24 |    4 | `cell_height` | u32, grid; `0` for pack |
|     28 |    4 | `padding`     | u32 |
|     32 |    8 | `texture`     | u64, asset ID of the atlas texture (`P`) |
|     40 |    8 | `reserved`    | u64, `0` |

## Frame (64 bytes)

| Offset | Size | Field           | Notes |
|-------:|-----:|-----------------|-------|
|      0 |    8 | `name_hash`     | u64, FNV-1a-64 of the name |
|      8 |    4 | `name_offset`   | u32, in `STRINGS` |
|     12 |    4 | `name_len`      | u32 |
|     16 |    4 | `x`             | u32, the frame's pixels in the atlas: left |
|     20 |    4 | `y`             | u32, top |
|     24 |    4 | `width`         | u32 |
|     28 |    4 | `height`        | u32 |
|     32 |    4 | `source_width`  | u32, the image file's size |
|     36 |    4 | `source_height` | u32 |
|     40 |    4 | `offset_x`      | u32, where the frame's pixels start in the source (trimmed, or `0`) |
|     44 |    4 | `offset_y`      | u32 |
|     48 |   16 | `uv`            | f32 × 4: `u0 v0 u1 v1`, the frame's rectangle as 0–1 texture coordinates (top-left origin) |

A grid frame scaled into its cell has `width` / `height` of its scaled
size, centered in the cell; `source_*` stay the file's size.
