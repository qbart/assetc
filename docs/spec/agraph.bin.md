# `.agraph.bin` compiled graphs

A [directed graph](../features/graphs.md) (`*.agraph`) as the build writes
it. A [chunked file](chunks.md) with format `AGRAPH`, version `1`.

## Chunks

| Name      | Required | Payload |
|-----------|----------|---------|
| `DESC`    | yes      | [`GraphDesc`](#graphdesc-16-bytes) |
| `NODES`   | yes      | [`Node`](#node-48-bytes)`[node_count]`, in the file's order |
| `EDGES`   | yes      | [`Edge`](#edge-32-bytes)`[edge_count]`, in the file's order |
| `PROPS`   | no       | [`Property`](amesh.bin.md#property-32-bytes)`[]`: nodes' and edges' data, text in `STRINGS` |
| `STRINGS` | yes      | UTF-8 text back to back: ids, labels, property keys and Text values |

Text is `offset` u32 and `len` u32 (bytes) into `STRINGS`.

## GraphDesc (16 bytes)

| Offset | Size | Field            | Notes |
|-------:|-----:|------------------|-------|
|      0 |    4 | `node_count`     | u32 |
|      4 |    4 | `edge_count`     | u32 |
|      8 |    4 | `property_count` | u32 |
|     12 |    4 | `reserved`       | u32, `0` |

## Node (48 bytes)

| Offset | Size | Field            | Notes |
|-------:|-----:|------------------|-------|
|      0 |    8 | `id_hash`        | u64, FNV-1a-64 of the id |
|      8 |    8 | `id`             | text |
|     16 |    8 | `label`          | text |
|     24 |    4 | `x`              | f32, grid units |
|     28 |    4 | `y`              | f32, grid units, down is positive |
|     32 |    4 | `color`          | bytes `R G B A`; all `0` without a color |
|     36 |    4 | `first_property` | u32, into `PROPS` |
|     40 |    4 | `property_count` | u32 |
|     44 |    4 | `reserved`       | u32, `0` |

## Edge (32 bytes)

| Offset | Size | Field            | Notes |
|-------:|-----:|------------------|-------|
|      0 |    4 | `from`           | u32, index into `NODES` |
|      4 |    4 | `to`             | u32, index into `NODES` |
|      8 |    4 | `weight`         | i32 |
|     12 |    8 | `label`          | text |
|     20 |    4 | `first_property` | u32, into `PROPS` |
|     24 |    4 | `property_count` | u32 |
|     28 |    4 | `reserved`       | u32, `0` |
