# `assets.aman`

Maps every asset ID (see [`docs/features/build.md`](../features/build.md#asset-ids))
to the output file holding it. Written once per successful build to
`<output>/assets.aman`.

The layout is designed for direct use from a memory-mapped file: entries have
a fixed stride and are sorted by ID, so a reader can binary-search them
without building a map.

## Encoding

All integers are little-endian. No padding other than what is listed.

```
+------------------------------+  offset 0
| Header           (16 bytes)  |
+------------------------------+  offset 16
| Entry[count]     (16 bytes each, sorted by id ascending)
+------------------------------+  offset 16 + 16 * count
| String data      (strings_size bytes)
+------------------------------+
```

### Header

| Offset | Size | Field          | Value                                     |
|-------:|-----:|----------------|-------------------------------------------|
|      0 |    4 | `magic`        | ASCII `AMAN` (`41 4D 41 4E`)              |
|      4 |    4 | `version`      | `1`                                       |
|      8 |    4 | `count`        | number of entries                         |
|     12 |    4 | `strings_size` | byte length of the string data section    |

### Entry

| Offset | Size | Field         | Notes                                                    |
|-------:|-----:|---------------|----------------------------------------------------------|
|      0 |    8 | `id`          | asset ID (u64)                                           |
|      8 |    4 | `path_offset` | offset of the path, relative to the string data start    |
|     12 |    2 | `path_len`    | byte length of the path                                  |
|     14 |    1 | `kind`        | asset kind, see below                                    |
|     15 |    1 | `reserved`    | `0`                                                      |

IDs are unique within a manifest.

### Paths

UTF-8, relative to the output directory, `/`-separated, no leading `/`, not
NUL-terminated. Paths are stored in the string data in entry order, back to
back.

### Asset kinds

| Value | Kind    | File format                  |
|------:|---------|------------------------------|
|     0 | texture | [`ktx2.md`](ktx2.md)         |
|     1 | material | [`amat.md`](amat.md)         |
|     2 | mesh     | [`amesh.md`](amesh.md)       |
|     3 | animation | [`aanim.md`](aanim.md)      |
|     4 | font     | [`afont.md`](afont.md)       |

Other values are reserved. Readers should skip entries with unknown kinds.

## Validation

A reader should reject the file when the magic or version does not match,
when `16 + 16 * count + strings_size` differs from the file size, or when any
`path_offset + path_len` exceeds `strings_size`.

## Example

One texture `textures/dirt` (ID `0x…`) at `textures/dirt.ktx2`:

```
41 4D 41 4E  01 00 00 00  01 00 00 00  12 00 00 00   header: AMAN, v1, 1 entry, 18 bytes of strings
<id u64 LE>  00 00 00 00  12 00  00  00              entry: offset 0, len 18, texture
74 65 78 74 75 72 65 73 2F 64 69 72 74 2E 6B 74 78 32   "textures/dirt.ktx2"
```
