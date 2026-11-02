# `.atype` compiled schemas

A [schema](../features/schemas.md) (`*.aschema`) as the build writes it:
its types, fields and enum values as fixed-size records, so a game can look
types up by name hash and read every field's settings. A
[chunked file](chunks.md) with format `ATYPE`, version `1`.

## Chunks

| Name      | Required | Payload |
|-----------|----------|---------|
| `DESC`    | yes      | [`TypeDesc`](#typedesc-16-bytes) |
| `TYPES`   | yes      | [`Type`](#type-32-bytes)`[type_count]`, in the schema's order |
| `FIELDS`  | yes      | [`Field`](#field-112-bytes)`[field_count]`: every struct's fields, struct after struct, each in field order |
| `VALUES`  | yes      | [`Value`](#value-16-bytes)`[value_count]`: every enum's values, enum after enum |
| `STRINGS` | yes      | UTF-8 text back to back (not terminated): names, type expressions, labels, groups, docs, defaults, conditions, widgets |

Text is referenced as `offset` u32 and `len` u32 (bytes) into `STRINGS`;
`len` `0` is empty. Name hashes are FNV-1a-64 of the UTF-8 name.

## TypeDesc (16 bytes)

| Offset | Size | Field         | Notes |
|-------:|-----:|---------------|-------|
|      0 |    4 | `type_count`  | u32 |
|      4 |    4 | `field_count` | u32 |
|      8 |    4 | `value_count` | u32 |
|     12 |    4 | `reserved`    | u32, `0` |

## Type (32 bytes)

| Offset | Size | Field       | Notes |
|-------:|-----:|-------------|-------|
|      0 |    8 | `name_hash` | u64 |
|      8 |    8 | `name`      | text |
|     16 |    4 | `kind`      | u32: `0` struct, `1` enum |
|     20 |    4 | `first`     | u32: a struct's first field in `FIELDS`, an enum's first value in `VALUES` |
|     24 |    4 | `count`     | u32: its fields or values |
|     28 |    4 | `reserved`  | u32, `0` |

## Field (112 bytes)

| Offset | Size | Field       | Notes |
|-------:|-----:|-------------|-------|
|      0 |    8 | `name_hash` | u64 |
|      8 |    8 | `name`      | text |
|     16 |    8 | `type`      | text: the type expression (`list<ref<Weapon>>`) |
|     24 |    8 | `label`     | text |
|     32 |    8 | `group`     | text |
|     40 |    8 | `doc`       | text |
|     48 |    8 | `default`   | text: the default as compact JSON in its [canonical form](../aschema.md#3-type-expressions) (`10`, `"#c8c8c8"`, `{"x":0.5}`); empty without one |
|     56 |    8 | `show_if`   | text |
|     64 |    8 | `hide_if`   | text |
|     72 |    8 | `widget`    | text |
|     80 |    8 | `min`       | f64, when flag `has_min` |
|     88 |    8 | `max`       | f64, when flag `has_max` |
|     96 |    8 | `step`      | f64, when flag `has_step` |
|    104 |    4 | `flags`     | u32 bits: `1` optional, `2` readonly, `4` has_min, `8` has_max, `16` has_step, `32` has_default, `64` has_order |
|    108 |    4 | `order`     | f32, when flag `has_order` |

## Value (16 bytes)

| Offset | Size | Field       | Notes |
|-------:|-----:|-------------|-------|
|      0 |    8 | `name_hash` | u64 |
|      8 |    8 | `name`      | text |
