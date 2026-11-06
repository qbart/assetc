# `.acmat.bin` custom materials

> **Status: design**, see [`custom-materials.md`](../features/custom-materials.md).

A compiled [custom material](../features/custom-materials.md)
([`.acmat`](acmat.md)), written to `<output>/<dir>/<name>.acmat.bin`, asset
path `<dir>/<name>.acmat.bin`, manifest kind `14` (`custom material`).

A [chunked file](chunks.md), format `ACMAT`, version `1`.

## Chunks

| Name | Required | Payload |
|------|----------|---------|
| `DESC` | yes | [`Desc`](#desc-32-bytes), one record |
| `PARAMS` | yes | The GPU block, `block_size` bytes, std140, ready to upload as the `Params` uniform buffer; empty without a block |
| `TEXTURES` | yes | u64 × texture count: each texture's asset ID, in the shader's texture order; `0` when empty (bind the texture's `default`) |
| `STRUCTS`, `SVALUES`, `SREFS`, `SSTRINGS` | yes | [Struct chunks](astruct.md): the params (scope `3`, one record, `item` `0`) when the shader has params, and the attached structs (scope `0`) |

## Desc (32 bytes)

| Offset | Size | Field | Notes |
|-------:|-----:|-------|-------|
| 0 | 8 | `shader` | u64, asset ID of the shader's `.ashader.bin` |
| 8 | 4 | `state` | u32, the shader's [state bits](ashader.bin.md#state-bits) with the material's overrides applied |
| 12 | 4 | `alpha_cutoff` | f32, likewise |
| 16 | 4 | `block_size` | u32, as the shader's |
| 20 | 4 | `texture_count` | u32, as the shader's texture count |
| 24 | 8 | `reserved` | `0` |

The block's fields, each texture's usage and dimension and where the stages
bind them are the shader's; the material does not repeat them. Read the
`.ashader.bin` named by `shader` for them.

## Validation

A reader rejects the file when it is not a valid chunked file of format
`ACMAT` version `1`, a required chunk is missing, `PARAMS` is not
`block_size` bytes, or `TEXTURES` is not `texture_count × 8` bytes.
