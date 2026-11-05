# Custom `.amat.bin` materials

> **Status: design**, see [`custom-materials.md`](../features/custom-materials.md).

A compiled [custom material](../features/custom-materials.md): a material
whose `.amat` names a [shader](ashader.md). Written to
`<output>/<dir>/<name>.amat.bin`, asset path `<dir>/<name>.amat.bin`, like a
[PBR material](amat.bin.md), but in another format and with manifest kind
`14` (`custom material`). Readers tell them apart by the kind, or by the
first bytes: a PBR material starts with `ACMT`, a custom one with the
chunked header's `ACMAT\0\0\0`.

A [chunked file](chunks.md), format `ACMAT`, version `1`.

## Chunks

| Name | Required | Payload |
|------|----------|---------|
| `DESC` | yes | [`Desc`](#desc-32-bytes), one record |
| `PARAMS` | yes | The GPU block, `block_size` bytes, std140, ready to upload as the set 1 binding 0 uniform buffer; empty without a block |
| `TEXTURES` | yes | u64 × binding count: each binding's texture asset ID, in the shader's binding order; `0` when empty (bind the binding's `default`) |
| `STRUCTS`, `SVALUES`, `SREFS`, `SSTRINGS` | yes | [Struct chunks](astruct.md): the params (scope `3`, one record, `item` `0`) when the shader has params, and the attached structs (scope `0`) |

## Desc (32 bytes)

| Offset | Size | Field | Notes |
|-------:|-----:|-------|-------|
| 0 | 8 | `shader` | u64, asset ID of the shader's `.ashader.bin` |
| 8 | 4 | `state` | u32, the shader's [state bits](ashader.bin.md#state-bits) with the material's overrides applied |
| 12 | 4 | `alpha_cutoff` | f32, likewise |
| 16 | 4 | `block_size` | u32, as the shader's |
| 20 | 4 | `texture_count` | u32, as the shader's binding count |
| 24 | 8 | `reserved` | `0` |

The block's fields and each texture's binding number, usage and dimension
are the shader's; the material does not repeat them. Read the
`.ashader.bin` named by `shader` for them.

## Validation

A reader rejects the file when it is not a valid chunked file of format
`ACMAT` version `1`, a required chunk is missing, `PARAMS` is not
`block_size` bytes, or `TEXTURES` is not `texture_count × 8` bytes.
