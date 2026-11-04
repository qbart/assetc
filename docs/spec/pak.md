# `.pak` packs

One file holding many: a [pack](../features/pack.md) of a build's output
files, a contents table followed by each file's bytes, so a game opens one
file and reads (or memory-maps) each asset in place. The layout follows the
`.hpack` of [assetc-cpp](https://github.com/qbart/assetc-cpp), with assetc's
own magic, an alignment chosen per pack and an asset ID per entry.

All integers are little-endian.

## Layout

```
+-------------------------------+  offset 0
| Header        (32 bytes)      |
+-------------------------------+  offset 32
| Contents      (toc_bytes)     |
+-------------------------------+  first entry's offset (aligned)
| Data: each file's bytes at    |
| its offset, gaps zero-filled  |
+-------------------------------+  end of the last entry's bytes
```

## Header (32 bytes)

| Offset | Size | Field       | Notes |
|-------:|-----:|-------------|-------|
|      0 |    4 | `magic`     | ASCII `APAK` |
|      4 |    4 | `version`   | u32, `1` |
|      8 |    4 | `count`     | u32, entries in the contents |
|     12 |    4 | `alignment` | u32, power of two, 16–65536: every entry's offset is a multiple of it |
|     16 |    8 | `toc_bytes` | u64, bytes of the contents, right after the header |
|     24 |    8 | `reserved`  | u64, `0` |

## Contents

`count` records, one after the other with no padding, **sorted by path**
(byte-wise, ascending, so a reader may binary-search):

| Offset | Size | Field      | Notes |
|-------:|-----:|------------|-------|
|      0 |    8 | `offset`   | u64, byte offset of the file's bytes from the start of the pack; a multiple of `alignment` |
|      8 |    8 | `size`     | u64, bytes |
|     16 |    8 | `id`       | u64, asset ID, see below |
|     24 |    1 | `kind`     | u8, see below |
|     25 |    1 | `reserved` | u8, `0` |
|     26 |    2 | `path_len` | u16, bytes of `path` |
|     28 | `path_len` | `path` | UTF-8, `/`-separated, relative to the output directory (`models/Fox.amesh.bin`), unique |

A record is `28 + path_len` bytes.

- **`id`**: the ID the [manifest](aman.bin.md) gives the file (a texture's ID
  hashes its asset path, without `.ktx2`); for a file it does not list,
  `FNV-1a 64` of `path`.
- **`kind`**:

  | Value | Kind | Files |
  |------:|------|-------|
  | 0–9 | as in the [manifest](aman.bin.md): texture, material, mesh, animation, font, schema, data, graph, tree, atlas | `*.ktx2`, `*.amat.bin`, `*.amesh.bin`, `*.aanim.bin`, `*.afont.bin`, `*.aschema.bin`, `*.adata.bin`, `*.agraph.bin`, `*.atree.bin`, `*.aatlas.bin` |
  | 10 | manifest | `assetc.aman.bin` at the root |
  | 11, 12 | as in the manifest: gradient, palette | `*.agradient.bin`, `*.apalette.bin` |
  | 255 | other | anything else |

## Data

Entries are laid out in contents order: the first at the header and contents
rounded up to `alignment`, each next one at the end of the one before rounded
up to `alignment`. Gaps are zero bytes. The file ends right after the last
entry's bytes (with no entries, after the contents). An empty file has
`size` 0 and still an aligned `offset`.

## Reading

1. Read the header; check `magic` and `version`.
2. Read `toc_bytes` bytes of contents and parse `count` records; every
   `offset + size` must lie within the file.
3. Find an entry by path (binary search) or by `id` (a map built once), then
   read or memory-map `size` bytes at `offset`. Each is the very file the
   build wrote, in its own format (a `.ktx2`, an `.amesh.bin`, …).

Packing the same files with the same `alignment` gives the same bytes.
