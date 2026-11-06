# `.ashader.bin` shaders

A compiled [shader interface](ashader.md): its GPU params block layout,
textures, render state, the resources its SPIR-V stages declare and the
stages themselves. Written to
`<output>/<dir>/<name>.ashader.bin`, asset path `<dir>/<name>.ashader.bin`,
manifest kind `13` (`shader`).

A [chunked file](chunks.md), format `ASHADER`, version `1`. Text is
`offset` u32 and `len` u32 (bytes) into `STRINGS`.

## Chunks

| Name | Required | Payload |
|------|----------|---------|
| `DESC` | yes | [`Desc`](#desc-64-bytes), one record |
| `FIELDS` | yes | [`Field`](#field-32-bytes)`[]`: the GPU block, in offset order (empty without a block) |
| `TEXTURES` | yes | [`Texture`](#texture-48-bytes)`[]`, in the shader's order |
| `INPUTS` | yes | [`Input`](#input-16-bytes)`[]`: the packed textures' channels |
| `BINDINGS` | yes | [`Binding`](#bindings)`[]`: the resources the stages declare, and where (empty without stages) |
| `SPIRVV` | no | the vertex stage's SPIR-V words (u32, little-endian), as in the `.spv` |
| `SPIRVF` | no | the fragment stage's SPIR-V words |
| `STRINGS` | yes | UTF-8 text back to back |

## Desc (64 bytes)

| Offset | Size | Field | Notes |
|-------:|-----:|-------|-------|
| 0 | 8 | `params_schema` | u64, asset ID of the params schema's `.aschema.bin`; `0` without params |
| 8 | 8 | `params_type_hash` | u64, FNV-1a-64 of the params struct's name; `0` without params |
| 16 | 8 | `params_type` | text |
| 24 | 4 | `block_size` | u32, bytes, a multiple of 16; `0` without a GPU block |
| 28 | 4 | `state` | u32, [state bits](#state-bits) |
| 32 | 4 | `alpha_cutoff` | f32 |
| 36 | 8 | `vertex_entry` | text; empty without `SPIRVV` |
| 44 | 8 | `fragment_entry` | text; empty without `SPIRVF` |
| 52 | 8 | `doc` | text |
| 60 | 4 | `reserved` | `0` |

### State bits

| Bits | Meaning |
|------|---------|
| 0–2 | blend: `0` opaque, `1` mask, `2` blend, `3` add, `4` multiply |
| 3–4 | cull: `0` back, `1` front, `2` none |
| 5 | depth write |
| 6–7 | depth test: `0` less, `1` less_equal, `2` equal, `3` always |
| 8–31 | reserved, `0` |

[Custom `.amat.bin`](acmat.bin.md) uses the same bits for its state.

## Field (32 bytes)

One per GPU value in the block, nested struct fields flattened (their path
joined with `.`: `foam.amount`).

| Offset | Size | Field | Notes |
|-------:|-----:|-------|-------|
| 0 | 8 | `path_hash` | u64, FNV-1a-64 of `path` |
| 8 | 8 | `path` | text |
| 16 | 4 | `offset` | u32, bytes into the block |
| 20 | 4 | `type` | u32: `0` u32, `1` i32, `2` f32, `3` vec3 (f32 × 3), `4` vec4 (f32 × 4) |
| 24 | 4 | `source` | u32, the schema type it came from: `0` bool, `1` integer, `2` float, `3` enum, `4` rgb, `5` rgba |
| 28 | 4 | `reserved` | `0` |

## Texture (48 bytes)

| Offset | Size | Field | Notes |
|-------:|-----:|-------|-------|
| 0 | 8 | `name_hash` | u64, FNV-1a-64 of `name` |
| 8 | 8 | `name` | text |
| 16 | 4 | `reserved` | `0` |
| 20 | 1 | `usage` | u8: `0` color, `1` data, `2` normal, `3` height, `4` noise, `5` gradient, `6` palette, `7` lut, `8` environment, `9` atlas |
| 21 | 1 | `dimension` | u8: `0` 2d, `1` 3d, `2` cube |
| 22 | 1 | `default` | u8: `0` white, `1` black, `2` gray, `3` normal, `4` transparent |
| 23 | 1 | `flags` | u8: bit 0 sRGB, bit 1 required, bit 2 bare, bit 3 mipmaps, bit 4 packed |
| 24 | 4 | `reserved` | `0` |
| 28 | 4 | `inputs` | u32, index of the texture's first `INPUTS` record (`0` when not packed) |
| 32 | 4 | `input_count` | u32, `4` when packed, else `0` |
| 36 | 4 | `fill` | u8 × 4, R G B A fill values; `0` when not packed |
| 40 | 8 | `reserved` | `0` |

## Input (16 bytes)

Four per packed texture, for its R, G, B and A channels.

| Offset | Size | Field | Notes |
|-------:|-----:|-------|-------|
| 0 | 8 | `name` | text; empty when no input fills the channel |
| 8 | 4 | `channel` | u32, the input's channel: `0` r, `1` g, `2` b, `3` a |
| 12 | 4 | `reserved` | `0` |

## Bindings

One 32-byte record per resource of the [interface](../features/custom-materials.md#the-interface-in-spir-v)
that at least one stage declares, in this order: `Params`, `Frame`,
`Object`, then each texture followed by its sampler, in the shader's texture
order. A resource no stage declares has no record. Its set and binding are
the same in every stage that declares it (the build checks).

| Offset | Size | Field | Notes |
|-------:|-----:|-------|-------|
| 0 | 4 | `kind` | u32: `0` params, `1` frame, `2` object, `3` texture, `4` sampler |
| 4 | 4 | `texture` | u32, index into `TEXTURES` for kinds `3` and `4`, else `0` |
| 8 | 4 | `set` | u32, descriptor set (WGSL `@group`) |
| 12 | 4 | `binding` | u32 (WGSL `@binding`) |
| 16 | 4 | `stages` | u32 bits: `1` vertex, `2` fragment |
| 20 | 4 | `size` | u32, the block's size in bytes as the stage declares it, for kinds `0`–`2`; else `0` |
| 24 | 8 | `name` | text, the name it was found by |

## Validation

A reader rejects the file when it is not a valid chunked file of format
`ASHADER` version `1`, a required chunk is missing, a payload size is not a
multiple of its record size, text or an `INPUTS` range runs past its chunk,
or a SPIR-V chunk's size is not a multiple of 4 or does not start with the
magic `0x07230203`.
