# Chunked files

assetc's own binary formats for structured data ([`.amesh.bin`](amesh.bin.md),
[`.aanim.bin`](aanim.bin.md), [`.afont.bin`](afont.bin.md), [`.aschema.bin`](aschema.bin.md),
[`.adata.bin`](adata.bin.md), [`.agraph.bin`](agraph.bin.md), [`.atree.bin`](atree.bin.md),
[`.aatlas.bin`](aatlas.bin.md)) share one
container: a header, a table of chunks, then
the chunks' payloads. Each payload is a plain array (or one record) that a
loader can memory-map and cast directly, using the counts and strides a
descriptor chunk gives. The layout follows the `.hmesh` container of
[assetc-cpp](https://github.com/qbart/assetc-cpp), with **8-byte chunk names**
instead of 4-byte FourCCs, and an 8-byte format name in the header.

All integers are little-endian. Floats are IEEE 754 `f32`.

## Layout

```
+-----------------------------------+  offset 0
| Header            (32 bytes)      |
+-----------------------------------+  offset 32
| Chunk table       (32 bytes × N)  |
+-----------------------------------+  offset 32 + 32 × N (16-aligned)
| Payloads, each starting 16-aligned |
+-----------------------------------+
```

## Header (32 bytes)

| Offset | Size | Field         | Notes                                                  |
|-------:|-----:|---------------|--------------------------------------------------------|
|      0 |    8 | `format`      | ASCII format name, zero-padded: `AMESH`, `AANIM`, `AFONT`, `ASCHEMA`, `ADATA`, `AGRAPH`, `ATREE`, `AATLAS` |
|      8 |    4 | `version`     | u32, per format (see each format's spec)               |
|     12 |    4 | `chunk_count` | u32, entries in the chunk table (`N`)                  |
|     16 |    4 | `flags`       | u32, reserved, `0`                                     |
|     20 |    4 | `reserved`    | u32, `0`                                               |
|     24 |    8 | `reserved`    | u64, `0`                                               |

## Chunk entry (32 bytes)

| Offset | Size | Field      | Notes                                                       |
|-------:|-----:|------------|-------------------------------------------------------------|
|      0 |    8 | `name`     | ASCII chunk name, 1–8 characters (`A`–`Z`, `0`–`9`), zero-padded |
|      8 |    4 | `flags`    | u32, reserved, `0`                                          |
|     12 |    4 | `reserved` | u32, `0`                                                    |
|     16 |    8 | `offset`   | u64, payload offset from the start of the file             |
|     24 |    8 | `size`     | u64, payload size in bytes (may be 0)                      |

## Rules

- Payloads start at offsets that are multiples of 16; the bytes between
  payloads (and between the table and the first payload) are zero.
- Payloads appear in chunk-table order and do not overlap. The file ends
  with the last payload (no trailing padding).
- A chunk name appears at most once per file.
- Readers look chunks up by name and **ignore unknown chunks**, so formats
  can add chunks without breaking older readers. A format's spec says which
  chunks are required.
- A reader rejects a file whose format name or version it does not support,
  whose table runs past the end of the file, or whose payload ranges do.
