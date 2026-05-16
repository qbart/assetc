# Asset Compiler

`assetc` compiles source assets under `assets/` (textures, meshes, shaders, materials) into runtime-friendly binary formats written to `runtime/`. Images become UASTC-encoded `.ktx2`, meshes become `.hmesh` with octahedral-packed normals/tangents and meshletized geometry; mesh files reference materials by stable 64-bit hashes so the engine can share resources across assets.

## .hmesh file format (v1)

Little-endian, tagged-chunk container. Magic `"HMSH"` (`0x4853'4D48`).

### Top-level layout

```
+--------------------------------+
| FileHeader     (32 B)          |
+--------------------------------+
| ChunkTable     (24 B * N)      |
+--------------------------------+
| Chunk payloads (16 B aligned)  |
+--------------------------------+
```

### FileHeader (32 B)

| Offset | Field        | Type | Notes                                  |
| -----: | ------------ | ---- | -------------------------------------- |
|      0 | `magic`      | u32  | `"HMSH"` (`0x4853'4D48`)               |
|      4 | `version`    | u32  | `1`                                    |
|      8 | `chunkCount` | u32  | number of entries in ChunkTable        |
|     12 | `flags`      | u32  | reserved                               |
|     16 | `_reserved1` | u64  | reserved (was `contentHash`, held)     |
|     24 | `_reserved2` | u64  | reserved                               |

### ChunkEntry (24 B each)

| Offset | Field    | Type | Notes                                |
| -----: | -------- | ---- | ------------------------------------ |
|      0 | `fourcc` | u32  | `ChunkId` value (see below)          |
|      4 | `flags`  | u32  | per-chunk flags (compression hint)   |
|      8 | `offset` | u64  | byte offset from file start          |
|     16 | `size`   | u64  | payload bytes                        |

Chunks are written in any order, padded to 16-byte alignment between payloads. Unknown chunks are ignored by readers (forward compatibility).

### Chunk catalogue (v1)

| FourCC | Purpose            | Payload                                                                 |
| ------ | ------------------ | ----------------------------------------------------------------------- |
| `BNDS` | mesh bounds        | `MeshBounds` (40 B): AABB min/max (`vec3`) + sphere center/radius       |
| `VTXS` | vertex stream      | `u32 count`, `u32 stride`, then `count * stride` bytes (`GpuVertex[]`)  |
| `IDXS` | index stream       | `u32 count`, `u32 size`, then `count * size` bytes (`size` = 2 or 4)    |
| `MLET` | meshlets           | `Meshlet[]`: per-meshlet vertex/triangle offsets and counts             |
| `MLVR` | meshlet vertices   | `u32[]`: per-meshlet local-to-global vertex index remap                 |
| `MLTR` | meshlet triangles  | `MeshletTriangle[]`: 3 × u8 per triangle, dense (no inter-meshlet pad)  |
| `MLBN` | meshlet bounds     | `MeshletBounds[]`: per-meshlet center/radius + cone-cull axis/cutoff    |
| `MTRL` | material refs      | `u32 count`, then `count * u64` (FNV1a64 of material runtime refs)      |

### `GpuVertex` (36 B)

| Offset | Field      | Type      | Notes                                                  |
| -----: | ---------- | --------- | ------------------------------------------------------ |
|      0 | `position` | f32 × 3   | object-local position (no quantization in v1)          |
|     12 | `normal`   | i16 × 4   | `[0..1]` octahedral normal (SNORM); `[2..3]` reserved  |
|     20 | `tangent`  | i16 × 4   | `[0..1]` octahedral tangent + handedness in bit 0;     |
|        |            |           | `[2..3]` reserved                                      |
|     28 | `uv`       | f32 × 2   | UV0 (top-left origin, matches Vulkan)                  |

Suggested Vulkan vertex input formats: normal → `VK_FORMAT_R16G16_SNORM`, tangent → `VK_FORMAT_R16G16_SINT` (shader needs raw integer access to extract the handedness bit).

### Material refs

The `MTRL` chunk holds FNV1a64 hashes — one per material in the source file, in source-index order. The hash input is a canonical runtime ref of the form `<sourceRef>/<leaf>` where:

- `sourceRef` is the source asset's runtime path with `assets/` prefix stripped, extension dropped, lowercased, `/`-separated (e.g. `models/chair`).
- `leaf` is the lowercased material name if non-empty AND unique within the source's material array; otherwise `material_<index>`.

Example: `assets/models/chair.glb` with materials `["Wood", "", "Leather", "Wood"]` produces `MTRL = [hash("models/chair/material_0"), hash("models/chair/material_1"), hash("models/chair/leather"), hash("models/chair/material_3")]`.

### Endianness

Little-endian only. `src/assetc/runtime_mesh.cpp` enforces this with a `static_assert(std::endian::native == std::endian::little)`.
