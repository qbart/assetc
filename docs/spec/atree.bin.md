# `.atree.bin` compiled trees

A [tree](../features/trees.md) (`*.atree`) as the build writes it, nodes in
**breadth-first** order so each node's children are contiguous. A
[chunked file](chunks.md) with format `ATREE`, version `1`.

## Chunks

| Name      | Required | Payload |
|-----------|----------|---------|
| `DESC`    | yes      | [`TreeDesc`](#treedesc-16-bytes) |
| `NODES`   | yes      | [`Node`](#node-64-bytes)`[node_count]`: the root first, then level by level, each node's children in order |
| `PROPS`   | no       | [`Property`](amesh.bin.md#property-32-bytes)`[]`: the nodes' data, text in `STRINGS` |
| `STRINGS` | yes      | UTF-8 text back to back: ids, labels, property keys and Text values |

Text is `offset` u32 and `len` u32 (bytes) into `STRINGS`, as in
[`.agraph.bin`](agraph.bin.md); empty text has length `0`.

## TreeDesc (16 bytes)

| Offset | Size | Field            | Notes |
|-------:|-----:|------------------|-------|
|      0 |    4 | `node_count`     | u32 |
|      4 |    4 | `property_count` | u32 |
|      8 |    4 | `depth`          | u32, levels (the root alone is 1) |
|     12 |    4 | `reserved`       | u32, `0` |

## Node (64 bytes)

| Offset | Size | Field            | Notes |
|-------:|-----:|------------------|-------|
|      0 |    8 | `id_hash`        | u64, FNV-1a-64 of the id |
|      8 |    8 | `id`             | text |
|     16 |    8 | `label`          | text |
|     24 |    4 | `x`              | f32 |
|     28 |    4 | `y`              | f32 |
|     32 |    4 | `color`          | bytes `R G B A`; all `0` without a color |
|     36 |    4 | `parent`         | i32, index into `NODES`; `−1` for the root |
|     40 |    4 | `first_child`    | u32, index of its first child (its children follow one another) |
|     44 |    4 | `child_count`    | u32 |
|     48 |    4 | `first_property` | u32, into `PROPS` |
|     52 |    4 | `property_count` | u32 |
|     56 |    4 | `depth`          | u32, the root is `0` |
|     60 |    4 | `reserved`       | u32, `0` |
