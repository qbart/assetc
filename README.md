# Asset Compiler

`assetc` compiles source assets under `assets/` (textures, meshes, shaders, materials) into runtime-friendly binary formats written to `runtime/`. Images become UASTC-encoded `.ktx2`, meshes become `.hmesh` with octahedral-packed normals/tangents and meshletized geometry; mesh files reference materials by stable 64-bit hashes so the engine can share resources across assets.

A glTF/GLB mesh additionally emits a **companion material table** (`.hmat`) plus one **`.ktx2` per referenced image**. The three layers are orthogonal: `.hmesh` is geometry, `.hmat` is the small per-source material descriptor table (PBR factors + texture refs), and `.ktx2` is the GPU-ready pixel payload. A submesh's `materialSlot` indexes straight into the `.hmat` table (`.hmat` row `i` ⇔ `SubMesh::materialSlot i`).

Texture refs in `.hmat`/`.hmesh` are stored as 64-bit hashes, not paths. A single global **manifest** (`runtime/assets.hman`) maps each hash back to the `.ktx2` file on disk, so the runtime can content-address textures (load the manifest once into a `hash → path` map, then resolve any `baseColorTex` etc.). See [.hman file format](#hman-file-format-v1).

## Commands

| Command       | What it does                                                                          |
| ------------- | ------------------------------------------------------------------------------------- |
| `assetc`       | Compile everything under `assets/` into the output dir (default `runtime/`).          |
| `assetc info`  | Inspect the *compiled* output dir and print per-file stats + aggregate totals (no recompile). |
| `assetc check` | Verify cross-file integrity of the compiled output dir (exit non-zero on any problem). |

Common flags (apply to `assetc`; `-o` also applies to `info`/`check`):

- `-o, --output <dir>` — output directory (default `runtime`).
- `-j, --jobs <n>` — concurrent jobs.
- `--verify` — re-read each written file and check structural validity.
- `--no-cache` — ignore the incremental build cache and rebuild everything.

### Incremental builds

`assetc` keeps a content cache (`<output>/.assetc-cache`) keyed by each source's bytes + asset type + encoder version. On the next run an asset is skipped when its inputs are unchanged and its primary output still exists; the asset's manifest contributions are replayed from the cache so `assets.hman` stays complete without re-encoding. Bump `kEncoderVersion` (in `src/assetc/cache.hpp`) to invalidate the whole cache when an output format changes; delete the cache file or pass `--no-cache` to force a full rebuild.

`assetc info` reports geometry stats per `.hmesh` (verts / triangles / indices / meshlets / submeshes / materials / bounds), material-table breakdowns per `.hmat` (texture-slot usage, alpha modes, double-sided count), `.hman` manifest entry counts by kind/colorspace, and `.ktx2` dimensions / mips / format / supercompression — then a totals summary across the whole output tree.

`assetc check` validates internal consistency: each `.hmesh`/`.hmat`/`.hman` is structurally valid; every nonzero texture ref in a `.hmat` resolves via `.hman` to a file that exists; every `.hman` entry points at an existing file with a hash matching `HashAssetRef(path-without-".ktx2")`; and each `.hmesh` material count matches its companion `.hmat` row count with all submesh material slots in range. Use it as a CI gate after a build.

## Supported inputs

| Source pattern                         | Asset type | Output                                  |
| -------------------------------------- | ---------- | --------------------------------------- |
| `*.png` (default)                      | Color      | UASTC `.ktx2` (color mode)              |
| `*.n.png`                              | Normal     | UASTC `.ktx2` (normal mode)             |
| `*.ao.png`, `*.h.png`, `*.r.png`       | Grayscale  | UASTC `.ktx2` (grayscale mode)          |
| `*.lut.cube`                           | LUT        | `.lut.ktx2` (3D LUT)                    |
| `*.obj`, `*.gltf`, `*.glb`             | Mesh       | `.hmesh` (container, see below); glTF also emits `.hmat` + `tex_<i>.ktx2` |
| `*.shader/` (directory)                | Shader     | folder with `vertex.spv` / `fragment.spv` |
| `*.env/` (directory)                   | Cubemap    | UASTC `.env.ktx2` (6 faces: `px.png`, `nx.png`, `py.png`, `ny.png`, `pz.png`, `nz.png`) |
| `*.array/` (directory)                 | Array      | `.arr.ktx2` *(planned)*                 |
| `*.mat`                                | Material   | `.hmat` *(planned standalone; today `.hmat` is emitted as a glTF companion)* |

For a glTF source `assets/models/chair.glb` the outputs are:

```
runtime/models/chair.hmesh          geometry + submesh table
runtime/models/chair.hmat           material table, row i == SubMesh::materialSlot i
runtime/models/chair/tex_<i>.ktx2   one UASTC .ktx2 per referenced glTF image i
runtime/models/chair.hanim          animation clips (only if the source is skinned + animated)
runtime/assets.hman                 global hash -> file manifest (all assets, written once)
```

Skinned glTF meshes also embed `SKIN` + `SKEL` chunks in the `.hmesh` (see below).

## .hmesh file format (v2)

Little-endian, tagged-chunk container. Magic `"HMSH"` (`0x4853'4D48`). v2 introduced the `DESC` + `SUBM` chunks, made every data chunk a *pure array* (no inline prelude), and shrank `GpuVertex` from 36 B to 28 B.

The `DESC` chunk is the single source of truth for the mesh's shape: counts, strides, and the index width all live there, so every other data chunk (`VTXS`, `IDXS`, `MLET`, ...) is a bare array the loader can mmap-cast directly using `DESC`.

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
|      4 | `version`    | u32  | `2`                                    |
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

### Chunk catalogue (v2)

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

### `GpuVertex` (28 B)

| Offset | Field      | Type      | Notes                                                  |
| -----: | ---------- | --------- | ------------------------------------------------------ |
|      0 | `position` | f32 × 3   | object-local position (unquantized)                    |
|     12 | `normal`   | i16 × 2   | octahedral normal (SNORM)                              |
|     16 | `tangent`  | i16 × 2   | octahedral tangent + handedness in bit 0 of x (SINT)   |
|     20 | `uv`       | f32 × 2   | UV0 (top-left origin, matches Vulkan)                  |

The v1 layout carried 8 bytes of reserved padding (two extra i16 per packed slot); v2 dropped them.

Suggested Vulkan vertex input formats: normal → `VK_FORMAT_R16G16_SNORM`, tangent → `VK_FORMAT_R16G16_SINT` (shader needs raw integer access to extract the handedness bit).

### Material refs

The `MTRL` chunk holds FNV1a64 hashes — **compact**: only materials actually referenced by a submesh, in first-use (dense slot) order. `SubMesh::materialSlot` indexes into this list (or is `kNoMaterial`). The same dense slot order is shared by the companion `.hmat` table, so `MTRL[i]`, `.hmat` row `i`, and `materialSlot == i` all describe the same material.

The hash input is a canonical runtime ref of the form `<sourceRef>/<leaf>` where:

- `sourceRef` is the source asset's runtime path with `assets/` prefix stripped, extension dropped, lowercased, `/`-separated (e.g. `models/chair`).
- `leaf` is the lowercased material name if non-empty AND unique within the source's material array; otherwise `material_<index>` (using the *source* index, so leaf names stay stable regardless of which materials are referenced).

Example: `assets/models/chair.glb` with source materials `["Wood", "", "Leather", "Wood"]`, where submeshes reference source materials `2` then `0` (first-use order), produces `MTRL = [hash("models/chair/leather"), hash("models/chair/material_0")]` — two entries, dense slots `0` and `1`.

### Skinning (`SKIN` + `SKEL`)

A skinned glTF (primitives with `JOINTS_0`/`WEIGHTS_0`, node with a `skin`) additionally emits two optional chunks. Static meshes omit both and are byte-identical to before.

- `SKIN` is parallel to `VTXS` — one `GpuSkinVertex` (`u16 joints[4]`, `f32 weights[4]`) per vertex. `joints` index into `SKEL`; `weights` are renormalized to sum 1 (a weightless vertex falls back to a rigid bind to joint 0).
- `SKEL` is the joint array. Each `GpuJoint` carries the inverse-bind matrix (mesh space → joint bind space, column-major), the joint's local bind-pose TRS (`bindT`/`bindR` quaternion `xyzw`/`bindS`), and a `parent` index (−1 for a root). Joint order follows the glTF skin's `joints` array, so `JOINTS_0` indices map directly.

Per the glTF spec, a skinned mesh node's own transform is ignored — assetc does **not** bake the node world matrix into skinned vertices (only static meshes are baked). Only the first skin (`skin[0]`) is exported; additional skins are warned and ignored.

### Levels of detail (`LODI` + `LODT`)

Every submesh additionally gets reduced LODs via meshoptimizer simplification (default 2 levels at ~50% and ~25% of the triangle count). They live in two optional chunks so the full-res path is untouched: `LODI` is a `u32` index buffer (global vertex indices, same `VTXS`) and `LODT` is a header plus a `MeshLod{firstIndex, indexCount}` per `[submesh][lod]` row. LOD0 is the full-resolution mesh in `IDXS`/`SUBM`/`MLET` (meshlets are LOD0-only); the reduced levels are for classic distance-based indexed draws. A `MeshLod` with `indexCount == 0` means simplification stalled at that level, so the engine should reuse the previous LOD. `ValidateHMesh` checks every range lands inside `LODI`.

### Endianness

Little-endian only. `src/assetc/runtime_mesh.cpp` enforces this with a `static_assert(std::endian::native == std::endian::little)`.

## .hanim file format (v1)

Little-endian. Magic `"HANM"`. A skinned, animated glTF emits a companion `.hanim` (`runtime/<name>.hanim`) alongside its `.hmesh`, holding every animation clip for that source. Channels index joints in the companion `.hmesh` `SKEL` array.

```
magic u32 'HANM' | version u32 1 | clipCount u32 | reserved u32
per clip:
    nameLen u16, name bytes
    duration f32                      // seconds (max key time)
    channelCount u32
    per channel:
        joint u32                     // index into SKEL
        path u8        (0=translation vec3, 1=rotation vec4 xyzw, 2=scale vec3)
        interp u8      (0=STEP, 1=LINEAR)
        components u8  (3 or 4)
        _pad u8
        keyCount u32
        per key: time f32, value[components] f32
```

Morph-target (`weights`) channels and channels targeting non-joint nodes are skipped. `CUBICSPLINE` samplers are degraded to `LINEAR` (the value keyframe of each tangent triple is kept) with a warning. `assetc info` lists clips/channels/duration; `assetc check` validates that every channel's joint index is within the companion skeleton.

## .hmat file format (v1)

Little-endian flat material table. Magic `"HMAT"`. One `.hmat` is emitted per glTF source, alongside its `.hmesh`. It is a fixed-stride array of material rows in the **same dense slot order** as the mesh's `MTRL`/`SUBM` tables, so `SubMesh::materialSlot` indexes straight into it.

### Layout

```
+--------------------------------+
| MatFileHeader (16 B)           |
+--------------------------------+
| GpuMaterial[count] (96 B each) |
+--------------------------------+
```

The file is exactly `16 + count * 96` bytes — no chunk table, directly mmappable as a `GpuMaterial[]`.

### MatFileHeader (16 B)

| Offset | Field     | Type | Notes                                            |
| -----: | --------- | ---- | ------------------------------------------------ |
|      0 | `magic`   | u32  | `"HMAT"`                                          |
|      4 | `version` | u32  | `1`                                              |
|      8 | `count`   | u32  | material rows (matches the companion `.hmesh` material count) |
|     12 | `flags`   | u32  | reserved                                         |

### GpuMaterial (96 B)

| Offset | Field                  | Type    | Notes                                                       |
| -----: | ---------------------- | ------- | ----------------------------------------------------------- |
|      0 | `baseColorFactor`      | f32 × 4 | linear RGBA multiplier                                       |
|     16 | `emissiveFactor`       | f32 × 3 | linear RGB                                                   |
|     28 | `metallicFactor`       | f32     |                                                             |
|     32 | `roughnessFactor`      | f32     |                                                             |
|     36 | `normalScale`          | f32     | glTF `normalTexture.scale`                                   |
|     40 | `occlusionStrength`    | f32     | glTF `occlusionTexture.strength`                             |
|     44 | `alphaCutoff`          | f32     | used when alpha mode == `MASK`                               |
|     48 | `flags`                | u32     | bit 0 = double-sided; bits 1-2 = alpha mode                 |
|     52 | `_pad`                 | u32     | aligns the texture refs to 8 bytes                          |
|     56 | `baseColorTex`         | u64     | FNV1a64 runtime ref of the `.ktx2`, `0` = none              |
|     64 | `metallicRoughnessTex` | u64     | ORM/MR packing: occlusion=R, roughness=G, metallic=B        |
|     72 | `normalTex`            | u64     |                                                             |
|     80 | `occlusionTex`         | u64     | equals `metallicRoughnessTex` when packed as ORM            |
|     88 | `emissiveTex`          | u64     |                                                             |

**Alpha mode** (flags bits 1-2): `0` = OPAQUE, `1` = MASK, `2` = BLEND.

Factors carry glTF defaults when a field is absent in the source (base color `1,1,1,1`; metallic/roughness `1`; emissive `0,0,0`; `normalScale`/`occlusionStrength` `1`; `alphaCutoff` `0.5`; OPAQUE, single-sided).

### Texture refs

Each `*Tex` field is an FNV1a64 hash of the texture's runtime ref, or `0` when the material has no texture in that slot. The ref is `<sourceRef>/tex_<imageIndex>` (lowercased, like material refs), and the matching file is written to `runtime/<rel-no-ext>/tex_<imageIndex>.ktx2`. Textures are **deduplicated by glTF image index** — one `.ktx2` per source image, even when shared across materials or slots.

### Texture color space & encoder mode

Each image is transcoded to UASTC `.ktx2` with the color space / swizzle dictated by the slot it is first used in:

| Slot                  | Mode          | OETF   | Swizzle | Notes                                  |
| --------------------- | ------------- | ------ | ------- | -------------------------------------- |
| baseColor, emissive   | `Color`       | sRGB   | none    | color data                             |
| metallicRoughness, occlusion | `LinearColor` | linear | none    | keeps all channels (ORM: O=R, R=G, M=B) |
| normal                | `Normal`      | linear | `rg01`  | X→R, Y→G; Z reconstructed in-shader     |

If the same image is pulled into conflicting color spaces by different materials, the first use wins and a warning is logged.

### Endianness

Little-endian only, same as `.hmesh` (`src/assetc/runtime_material.cpp` carries the matching `static_assert`).

## .hman file format (v1)

Little-endian. Magic `"HMAN"`. A **single global** `runtime/assets.hman` is written once per build (not per source), after every asset has compiled successfully. It maps each texture's FNV1a64 runtime ref — the exact hash already stored in `.hmat`/`.hmesh` — to its `.ktx2` path, so the runtime can resolve `baseColorTex` and friends to bytes on disk.

The engine loads it once into a `hash → path` map and does `root + "/" + path` to open each texture. Entries are sorted by hash ascending, so a reader may `lower_bound` instead of building a map.

### Layout

```
+-------------------------------------+
| ManHeader (16 B)                    |
+-------------------------------------+
| ManEntry[count] (variable length)   |
+-------------------------------------+
```

### ManHeader (16 B)

| Offset | Field      | Type | Notes                       |
| -----: | ---------- | ---- | --------------------------- |
|      0 | `magic`    | u32  | `"HMAN"`                    |
|      4 | `version`  | u32  | `1`                         |
|      8 | `count`    | u32  | number of entries           |
|     12 | `reserved` | u32  | `0`                         |

### ManEntry (variable length)

| Field        | Type           | Notes                                                          |
| ------------ | -------------- | -------------------------------------------------------------- |
| `hash`       | u64            | FNV1a64 ref — byte-identical to the `.hmat`/`.hmesh` ref hash  |
| `kind`       | u8             | `0` = texture (`1` mesh, `2` material, `3` lut reserved)       |
| `colorspace` | u8             | `0` = linear, `1` = sRGB                                       |
| `pathLen`    | u16            | byte length of `path`                                          |
| `path`       | `u8[pathLen]`  | UTF-8, forward-slash, **relative to the runtime root**         |

Entries are streamed (not a fixed stride), so the file is parsed sequentially rather than mmap-cast.

### Canonical hashed string

The `hash` is `HashAssetRef("<sourceRef>/tex_<imageIndex>")` — the same canonical ref documented under [.hmat texture refs](#texture-refs): `sourceRef` is the source path relative to `assets/`, extension stripped, lowercased, `/`-separated; no extension and no `runtime/` prefix. For a texture the on-disk `path` equals that ref plus `.ktx2`, so `hash == HashAssetRef(path-without-".ktx2")`.

### Scope and guarantees

- **Textures only.** Only `.ktx2` images referenced by an emitted `.hmat` appear. The standalone `*.png`-compiled textures (Color/Normal/Grayscale) and LUTs are not listed; `kind` reserves room to add them later.
- **Colorspace** mirrors what `assetc` baked into the `.ktx2` (sRGB for the `Color` slot, linear otherwise) — authoritative, so the runtime need not re-derive it from the material slot.
- **Deduplicated.** A ref reachable from multiple slots/assets appears once; identical `(hash, path)` pairs collapse.
- **Collision-checked.** If two distinct paths hash equal the build fails loudly rather than emit an ambiguous entry.
- **Deterministic.** Same inputs → byte-identical `assets.hman` (sorted output, no timestamps).

### Endianness

Little-endian only (`src/assetc/runtime_manifest.cpp` carries the matching `static_assert`).
