# `.arec` compiled data

A [data file](../features/instances.md) (`*.adata`) as the build writes it:
its instances with **every field resolved** (defaults applied, as
[§4.2](../aschema.md#42-sparse-storage) resolves them), as a tree of typed
values a game reads without its schema's defaults. A
[chunked file](chunks.md) with format `AREC`, version `1`.

## Chunks

| Name        | Required | Payload |
|-------------|----------|---------|
| `DESC`      | yes      | [`RecDesc`](#recdesc-16-bytes) |
| `TYPES`     | yes      | [`TypeRef`](#typeref-16-bytes)`[type_count]`: the structs the instances are of, in order of first use |
| `INSTANCE` | yes      | [`Instance`](#instance-32-bytes)`[instance_count]`, in the file's order |
| `VALUES`    | yes      | [`Value`](#value-24-bytes)`[value_count]` |
| `STRINGS`   | yes      | UTF-8 text back to back: type names, ids, text values |

Text is `offset` u32 and `len` u32 (bytes) into `STRINGS`. Hashes are
FNV-1a-64 of the UTF-8 text.

## RecDesc (16 bytes)

| Offset | Size | Field            | Notes |
|-------:|-----:|------------------|-------|
|      0 |    4 | `instance_count` | u32 |
|      4 |    4 | `value_count`    | u32 |
|      8 |    4 | `type_count`     | u32 |
|     12 |    4 | `reserved`       | u32, `0` |

## TypeRef (16 bytes)

| Offset | Size | Field       | Notes |
|-------:|-----:|-------------|-------|
|      0 |    8 | `name_hash` | u64 |
|      8 |    8 | `name`      | text |

## Instance (32 bytes)

| Offset | Size | Field      | Notes |
|-------:|-----:|------------|-------|
|      0 |    4 | `type`     | u32, index into `TYPES` |
|      4 |    4 | `value`    | u32, index into `VALUES` of its struct value |
|      8 |    8 | `id_hash`  | u64 |
|     16 |    8 | `id`       | text |
|     24 |    8 | `reserved` | u64, `0` |

## Value (24 bytes)

| Offset | Size | Field       | Notes |
|-------:|-----:|-------------|-------|
|      0 |    8 | `name_hash` | u64: the field's name hash in a struct; `0` for a list item |
|      8 |    4 | `kind`      | u32, see below |
|     12 |    4 | `count`     | u32: a struct's fields or a list's items; a text's, ref's or enum's byte length |
|     16 |    8 | `payload`   | per kind |

| `kind` | Type | `payload` |
|-------:|------|-----------|
| 0 | null (an optional field without a value) | `0` |
| 1 | `bool` | u64 `0` or `1` |
| 2 | integers | i64 |
| 3 | floats | f64 |
| 4 | `string` | u32 offset into `STRINGS`, then u32 `0` |
| 5 | `rgb`, `rgba` | bytes `R G B A` (alpha `255` for `rgb`), then 4 zero bytes |
| 6 | a struct | u32 index of its first field in `VALUES`, then u32 `0`; its `count` fields follow one another in field order |
| 7 | `list<T>`, `curve`, `gradient` | u32 index of its first item, then u32 `0`; its `count` items follow one another |
| 8 | `ref<S>` | u32 offset of the target's id in `STRINGS`, then u32 `0` |
| 9 | an enum | u32 offset of the value's name in `STRINGS`, then u32 the value's index in its enum |
| 10 | `asset<K>` | u64 asset ID of the file's main output (`0` for `""`), see below |

An `asset<K>` value is the [asset ID](../features/build.md#asset-ids) the
build gives the referenced file's main output, so a game finds it in
`assets.aman`: its input-relative path without the extension for
`texture`, `noise`, `gradient`, `palette`, `lut` and `hdr` (their `.ktx2`);
with `.amesh` for `model`, `.afont` for `font`, `.aatlas` for `atlas` (the
folder's path without `.atlas`), `.agph` for `graph`, `.atre` for `tree`,
`.arec` for `data`; and `<dir>/<name>.amat` for a `material`
`<dir>/<name>.mat.toml`.

A `curve` is a list of points, each a list of four floats `x y in out`; a
`gradient` a list of stops, each a list of a float `t` and a color.
Children are placed after their parent, so `VALUES` starts with the first
instance's struct value.
