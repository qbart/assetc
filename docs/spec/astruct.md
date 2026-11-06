# Struct chunks

The values of [attached structs](../features/structs.md), as the build
writes them into an asset's binary: four chunks of a
[chunked file](chunks.md), the same in every format that carries them
([`.agraph.bin`](agraph.bin.md), [`.atree.bin`](atree.bin.md),
[`.amesh.bin`](amesh.bin.md), [`.aatlas.bin`](aatlas.bin.md),
[`.agradient.bin`](agradient.bin.md), [`.apalette.bin`](apalette.bin.md)),
and in the `ASTRUCT` file after a material's record (see
[`.amat.bin`](amat.bin.md#struct-data)).

The chunks have their own names and string table, so they never change the
offsets of the format's other chunks. All four are present when the asset
has at least one attached struct, and none when it has none; a
[`.acmat.bin`](acmat.bin.md) always has them, empty when there is nothing
to hold.

## Chunks

| Name       | Payload |
|------------|---------|
| `STRUCTS`  | [`Attached`](#attached-40-bytes)`[]` (payload size / 40) |
| `SVALUES`  | [`Value`](adata.bin.md#value-24-bytes)`[]` (payload size / 24): the values, as in `.adata.bin` |
| `SREFS`    | [`Target`](adata.bin.md#target-24-bytes)`[]` (payload size / 24): the instances refs point at, each once, in order of first use |
| `SSTRINGS` | UTF-8 text back to back: struct names, text, enum values and ref ids |

Text is `offset` u32 and `len` u32 (bytes) into `SSTRINGS`; values' text
offsets (`kind` 4, 8 and 9) point into `SSTRINGS` too, and a ref's target
index (`kind` 8) into `SREFS`.

## Attached (40 bytes)

One per owner and struct: every owner of a scope has one record per struct
attached to the scope, even when it sets no values.

| Offset | Size | Field       | Notes |
|-------:|-----:|-------------|-------|
|      0 |    4 | `scope`     | u32, see below |
|      4 |    4 | `item`      | u32, the owner's index in the format's chunk of that scope; `0` for scope `0` |
|      8 |    8 | `type_hash` | u64, FNV-1a-64 of the struct's name |
|     16 |    8 | `type`      | text: the struct's name |
|     24 |    8 | `schema`    | u64, [asset ID](../features/build.md#asset-ids) of the struct's schema's `.aschema.bin` |
|     32 |    4 | `value`     | u32, index into `SVALUES` of the struct value (`kind` 6), its fields in the struct's field order, every field resolved; no two records share a value |
|     36 |    4 | `reserved`  | u32, `0` |

Records are sorted by `scope`, then `item`, then the scope's attachment
order, so an owner's records are next to one another.

| `scope` | Owner | `item` indexes |
|--------:|-------|----------------|
| 0 | the asset itself | — |
| 1 | each node (`.agraph.bin`, `.atree.bin`), gradient (`.agradient.bin`) or color (`.apalette.bin`) | `NODES` (a tree's breadth-first order), `GRADIENT`, `COLORS` |
| 2 | each edge (`.agraph.bin`) | `EDGES` |
| 3 | a [custom material](acmat.bin.md)'s params | — |

## `ASTRUCT` files

Where a format is not a chunked file (a material's fixed record), the
chunks travel in a chunked file of their own: format `ASTRUCT`, version
`1`, the four chunks above in that order and nothing else. Its chunk
offsets count from the start of the `ASTRUCT` file, not of the file it
follows.

## Reading

To read the values of owner `(scope, item)`: find its records in `STRUCTS`
(binary search, or a scan: they are sorted), then read each record's struct
value at `value` in `SVALUES` as `.adata.bin` values are read, field names
by their hashes (the schema's `.aschema.bin` names them). A reader that
does not know these chunks ignores them, as for any chunk.
