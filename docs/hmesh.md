# .hmesh file format (v2)

Little-endian, tagged-chunk container. Magic `"HMSH"` (`0x4853'4D48`). v2 introduced the `DESC` + `SUBM` chunks, made every data chunk a *pure array* (no inline prelude), and shrank `GpuVertex` from 36 B to 28 B.

The `DESC` chunk is the single source of truth for the mesh's shape: counts, strides, and the index width all live there, so every other data chunk (`VTXS`, `IDXS`, `MLET`, ...) is a bare array the loader can mmap-cast directly using `DESC`.

A glTF/GLB mesh additionally emits a companion material table ([`.hmat`](hmat.md)) plus one `.ktx2` per referenced image; a skinned + animated source also emits clips ([`.hanim`](hanim.md)). A submesh's `materialSlot` indexes straight into the `.hmat` table (`.hmat` row `i` ⇔ `SubMesh::materialSlot i`).

## Top-level layout

```
+--------------------------------+
| FileHeader     (32 B)          |
+--------------------------------+
| ChunkTable     (24 B * N)      |
+--------------------------------+
| Chunk payloads (16 B aligned)  |
+--------------------------------+
```

## FileHeader (32 B)

| Offset | Field        | Type | Notes                                  |
| -----: | ------------ | ---- | -------------------------------------- |
|      0 | `magic`      | u32  | `"HMSH"` (`0x4853'4D48`)               |
|      4 | `version`    | u32  | `2`                                    |
|      8 | `chunkCount` | u32  | number of entries in ChunkTable        |
|     12 | `flags`      | u32  | reserved                               |
|     16 | `_reserved1` | u64  | reserved (was `contentHash`, held)     |
|     24 | `_reserved2` | u64  | reserved                               |

## ChunkEntry (24 B each)

| Offset | Field    | Type | Notes                                |
| -----: | -------- | ---- | ------------------------------------ |
|      0 | `fourcc` | u32  | `ChunkId` value (see below)          |
|      4 | `flags`  | u32  | per-chunk flags (compression hint)   |
|      8 | `offset` | u64  | byte offset from file start          |
|     16 | `size`   | u64  | payload bytes                        |

Chunks are written in any order, padded to 16-byte alignment between payloads. Unknown chunks are ignored by readers (forward compatibility).

## Chunk catalogue (v2)

All data chunks are **pure arrays** — element counts, the vertex stride, and the index width live in `DESC`, not inline in the chunk.

| FourCC | Purpose            | Payload                                                                 |
| ------ | ------------------ | ----------------------------------------------------------------------- |
| `DESC` | mesh descriptor    | `MeshDesc` (32 B): counts (vertex/index/meshlet/submesh/material), `vertexStride` (u16), `indexWidth` (u8, 2 or 4), `flags` (u8), meshlet build params |
| `BNDS` | mesh bounds        | `MeshBounds` (40 B): AABB min/max (`vec3`) + sphere center/radius       |
| `VTXS` | vertex stream      | `GpuVertex[vertexCount]` (28 B stride)                                  |
| `IDXS` | index stream       | `u16[]` or `u32[]` per `DESC.indexWidth`; global indices, all submeshes |
| `MLET` | meshlets           | `Meshlet[]`: per-meshlet vertex/triangle offsets and counts             |
| `MLVR` | meshlet vertices   | `u32[]`: per-meshlet local-to-global vertex index remap                 |
| `MLTR` | meshlet triangles  | `MeshletTriangle[]`: 3 × u8 per triangle, dense (no inter-meshlet pad)  |
| `MLBN` | meshlet bounds     | `MeshletBounds[]`: per-meshlet center/radius + cone-cull axis/cutoff    |
| `MTRL` | material refs      | `u64[materialCount]` (FNV1a64 of material runtime refs; see below)      |
| `SUBM` | submesh table      | `SubMesh[submeshCount]` (64 B): index/meshlet ranges, `materialSlot`, per-submesh bounds |
| `SKIN` | per-vertex skinning | *optional* `GpuSkinVertex[vertexCount]` (24 B): `u16 joints[4]` + `f32 weights[4]` (sum 1); parallel to `VTXS` |
| `SKEL` | skeleton           | *optional* `GpuJoint[jointCount]` (112 B): inverse-bind matrix, bind-pose TRS, parent index |
| `LODI` | LOD index buffer   | *optional* `u32[]` global vertex indices for reduced levels (concatenated)     |
| `LODT` | LOD table          | *optional* `LodTableHeader` (8 B) + `MeshLod[submeshCount*lodCount]` (8 B): per-submesh ranges into `LODI` |

Each `SubMesh` is one drawable section sharing a material: a contiguous `[firstIndex, firstIndex+indexCount)` range into `IDXS` and `[firstMeshlet, firstMeshlet+meshletCount)` into `MLET`, plus a `materialSlot` (index into `MTRL`/`.hmat`, or `kNoMaterial = 0xFFFFFFFF`) and its own AABB+sphere. One glTF primitive → one submesh.

## `GpuVertex` (28 B)

| Offset | Field      | Type      | Notes                                                  |
| -----: | ---------- | --------- | ------------------------------------------------------ |
|      0 | `position` | f32 × 3   | object-local position (unquantized)                    |
|     12 | `normal`   | i16 × 2   | octahedral normal (SNORM)                              |
|     16 | `tangent`  | i16 × 2   | octahedral tangent + handedness in bit 0 of x (SINT)   |
|     20 | `uv`       | f32 × 2   | UV0 (top-left origin, matches Vulkan)                  |

The v1 layout carried 8 bytes of reserved padding (two extra i16 per packed slot); v2 dropped them.

Suggested Vulkan vertex input formats: normal → `VK_FORMAT_R16G16_SNORM`, tangent → `VK_FORMAT_R16G16_SINT` (shader needs raw integer access to extract the handedness bit).

## Material refs

The `MTRL` chunk holds FNV1a64 hashes — **compact**: only materials actually referenced by a submesh, in first-use (dense slot) order. `SubMesh::materialSlot` indexes into this list (or is `kNoMaterial`). The same dense slot order is shared by the companion `.hmat` table, so `MTRL[i]`, `.hmat` row `i`, and `materialSlot == i` all describe the same material.

The hash input is a canonical runtime ref of the form `<sourceRef>/<leaf>` where:

- `sourceRef` is the source asset's runtime path with `assets/` prefix stripped, extension dropped, lowercased, `/`-separated (e.g. `models/chair`).
- `leaf` is the lowercased material name if non-empty AND unique within the source's material array; otherwise `material_<index>` (using the *source* index, so leaf names stay stable regardless of which materials are referenced).

Example: `assets/models/chair.glb` with source materials `["Wood", "", "Leather", "Wood"]`, where submeshes reference source materials `2` then `0` (first-use order), produces `MTRL = [hash("models/chair/leather"), hash("models/chair/material_0")]` — two entries, dense slots `0` and `1`.

## Skinning (`SKIN` + `SKEL`)

A skinned glTF (primitives with `JOINTS_0`/`WEIGHTS_0`, node with a `skin`) additionally emits two optional chunks. Static meshes omit both and are byte-identical to before.

- `SKIN` is parallel to `VTXS` — one `GpuSkinVertex` (`u16 joints[4]`, `f32 weights[4]`) per vertex. `joints` index into `SKEL`; `weights` are renormalized to sum 1 (a weightless vertex falls back to a rigid bind to joint 0).
- `SKEL` is the joint array. Each `GpuJoint` carries the inverse-bind matrix (mesh space → joint bind space, column-major), the joint's local bind-pose TRS (`bindT`/`bindR` quaternion `xyzw`/`bindS`), and a `parent` index (−1 for a root). Joint order follows the glTF skin's `joints` array, so `JOINTS_0` indices map directly.

Per the glTF spec, a skinned mesh node's own transform is ignored — assetc does **not** bake the node world matrix into skinned vertices (only static meshes are baked). Only the first skin (`skin[0]`) is exported; additional skins are warned and ignored.

## Levels of detail (`LODI` + `LODT`)

Every submesh additionally gets reduced LODs via meshoptimizer simplification (default 2 levels at ~50% and ~25% of the triangle count). They live in two optional chunks so the full-res path is untouched: `LODI` is a `u32` index buffer (global vertex indices, same `VTXS`) and `LODT` is a header plus a `MeshLod{firstIndex, indexCount}` per `[submesh][lod]` row. LOD0 is the full-resolution mesh in `IDXS`/`SUBM`/`MLET` (meshlets are LOD0-only); the reduced levels are for classic distance-based indexed draws. A `MeshLod` with `indexCount == 0` means simplification stalled at that level, so the engine should reuse the previous LOD. `ValidateHMesh` checks every range lands inside `LODI`.

## Endianness

Little-endian only. `src/assetc/runtime_mesh.cpp` enforces this with a `static_assert(std::endian::native == std::endian::little)`.
