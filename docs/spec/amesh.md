# `.amesh` meshes

One compiled mesh: every triangle primitive of a glTF model's scene, with
meshlets, bounds, levels of detail and (when skinned) its skeleton; see
[`docs/features/models.md`](../features/models.md#build). A
[chunked file](chunks.md) with format name `AMESH`, version `1`. The layout of
the data follows assetc-cpp's `.hmesh` v2.

## Chunks

Every data chunk is a plain array; counts, the vertex stride and the index
width are in `DESC`.

| Name       | Required | Payload                                                         |
|------------|----------|-----------------------------------------------------------------|
| `DESC`     | yes      | [`MeshDesc`](#meshdesc-32-bytes) (32 bytes)                      |
| `BOUNDS`   | yes      | [`Bounds`](#bounds-40-bytes) of the whole mesh                    |
| `VERTICES` | yes      | [`Vertex`](#vertex-28-bytes)`[vertex_count]`                      |
| `INDICES`  | yes      | `u16[index_count]` or `u32[index_count]` (`DESC.index_width`); triangle lists, global into `VERTICES` |
| `SUBMESH`  | yes      | [`SubMesh`](#submesh-64-bytes)`[submesh_count]`                   |
| `MATERIAL` | yes      | `u64[material_count]`: asset IDs of the materials (`.amat`), see [Materials](#materials) |
| `MESHLETS` | yes      | [`Meshlet`](#meshlet-16-bytes)`[meshlet_count]`                   |
| `MLVERTS`  | yes      | `u32[]`: each meshlet's vertices, as global vertex indices        |
| `MLTRIS`   | yes      | `u8[3][]`: each meshlet's triangles, as indices into its own `MLVERTS` range |
| `MLBOUNDS` | yes      | [`MeshletBounds`](#meshletbounds-32-bytes)`[meshlet_count]`       |
| `LODIDX`   | no       | `u32[]`: index data of the reduced levels of detail, global into `VERTICES` |
| `LODTABLE` | no       | [`LodTable`](#lodtable) header plus ranges into `LODIDX`          |
| `SKIN`     | no       | [`SkinVertex`](#skinvertex-24-bytes)`[vertex_count]`, parallel to `VERTICES` |
| `SKELETON` | no       | [`Joint`](#joint-112-bytes)`[joint_count]`                        |

assetc writes the chunks in the order above; `LODIDX` and `LODTABLE` appear
together, as do `SKIN` and `SKELETON` (only for skinned meshes).

## MeshDesc (32 bytes)

| Offset | Size | Field                   | Notes                                        |
|-------:|-----:|-------------------------|----------------------------------------------|
|      0 |    4 | `vertex_count`          | u32                                          |
|      4 |    4 | `index_count`           | u32, all submeshes (a multiple of 3)         |
|      8 |    4 | `meshlet_count`         | u32                                          |
|     12 |    4 | `submesh_count`         | u32, at least 1                              |
|     16 |    4 | `material_count`        | u32, entries in `MATERIAL`                   |
|     20 |    2 | `vertex_stride`         | u16, `28`                                    |
|     22 |    1 | `index_width`           | u8, `2` when `vertex_count ≤ 65535`, else `4` |
|     23 |    1 | `flags`                 | u8: bit 0 tangents valid, bit 1 skinned (`SKIN`/`SKELETON` present), bit 2 levels of detail present |
|     24 |    4 | `meshlet_max_vertices`  | u32, build limit (`assetc.toml` `[meshes]`, default `64`) |
|     28 |    4 | `meshlet_max_triangles` | u32, build limit (default `124`)             |

## Bounds (40 bytes)

| Offset | Size | Field           | Notes                                              |
|-------:|-----:|-----------------|----------------------------------------------------|
|      0 |   12 | `aabb_min`      | f32 × 3                                            |
|     12 |   12 | `aabb_max`      | f32 × 3                                            |
|     24 |   12 | `sphere_center` | f32 × 3, the box's center                          |
|     36 |    4 | `sphere_radius` | f32, largest distance from the center to a vertex  |

For a skinned mesh the bounds are of the bind pose.

## Vertex (28 bytes)

| Offset | Size | Field      | Notes                                                            |
|-------:|-----:|------------|------------------------------------------------------------------|
|      0 |   12 | `position` | f32 × 3                                                          |
|     12 |    4 | `normal`   | i16 × 2, [octahedral](#octahedral-encoding), SNORM               |
|     16 |    4 | `tangent`  | i16 × 2, octahedral; bit 0 of x is the handedness (`1` = negative `w`) |
|     20 |    8 | `uv`       | f32 × 2, `TEXCOORD_0`, top-left origin (as glTF and Vulkan)      |

Suggested vertex input formats: `R32G32B32_SFLOAT`, `R16G16_SNORM`,
`R16G16_SINT` (the shader reads the handedness bit from the raw integer),
`R32G32_SFLOAT`.

### Octahedral encoding

A unit vector `(x, y, z)` is divided by `|x| + |y| + |z|`; if `z < 0`, `x`
and `y` become `(1 − |y|) × sign(x)` and `(1 − |x|) × sign(y)` (with
`sign(0) = 1`). Each is stored as `round(clamp(v, −1, 1) × 32767)`. For a
tangent, bit 0 of the stored x is then set to `1` when the tangent's `w` is
negative and cleared otherwise. A zero vector encodes as `(0, 0)` (a zero
normal decodes to `+Z`).

## SubMesh (64 bytes)

One glTF primitive (as placed in the scene): a contiguous range of indices
and of meshlets sharing one material. Submeshes follow each other in both
`INDICES` and `MESHLETS`.

| Offset | Size | Field           | Notes                                                 |
|-------:|-----:|-----------------|-------------------------------------------------------|
|      0 |    4 | `first_index`   | u32, into `INDICES`                                   |
|      4 |    4 | `index_count`   | u32, a multiple of 3                                  |
|      8 |    4 | `first_meshlet` | u32, into `MESHLETS`                                  |
|     12 |    4 | `meshlet_count` | u32                                                   |
|     16 |    4 | `material`      | u32, index into `MATERIAL`, or `0xFFFFFFFF` for none  |
|     20 |    4 | `reserved`      | u32, `0` (indices are global)                         |
|     24 |   40 | `bounds`        | [`Bounds`](#bounds-40-bytes) of this submesh          |

Draw a submesh with `drawIndexed(index_count, first_index)`, or dispatch its
meshlets `first_meshlet … first_meshlet + meshlet_count − 1`.

## Meshlet (16 bytes)

| Offset | Size | Field            | Notes                                         |
|-------:|-----:|------------------|-----------------------------------------------|
|      0 |    4 | `vertex_offset`  | u32, into `MLVERTS`                           |
|      4 |    4 | `vertex_count`   | u32, at most `meshlet_max_vertices`           |
|      8 |    4 | `triangle_offset`| u32, into `MLTRIS` (in triangles)             |
|     12 |    4 | `triangle_count` | u32, at most `meshlet_max_triangles`          |

Triangle `t` of a meshlet is `MLTRIS[triangle_offset + t]`: three `u8`
indices `i`, each naming vertex `MLVERTS[vertex_offset + i]`. Meshlets never
cross submeshes.

## MeshletBounds (32 bytes)

| Offset | Size | Field         | Notes                                                       |
|-------:|-----:|---------------|-------------------------------------------------------------|
|      0 |   12 | `center`      | f32 × 3, bounding sphere                                    |
|     12 |    4 | `radius`      | f32                                                         |
|     16 |   12 | `cone_axis`   | f32 × 3, normal cone for back-face culling                  |
|     28 |    4 | `cone_cutoff` | f32: with `d = center − camera`, the meshlet faces away from the camera when `dot(d, cone_axis) ≥ cone_cutoff × length(d) + radius` |

As computed by meshoptimizer's `meshopt_computeMeshletBounds`.

## Materials

`MATERIAL` lists the asset IDs of the `.amat` files the submeshes use, each
once, in order of first use; `SubMesh.material` indexes into it. The IDs
resolve through [`assets.manifest`](manifest.md).

## LodTable

Reduced levels of detail, for classic indexed draws (meshlets exist for the
full mesh only). `LODTABLE` is an 8-byte header followed by
`submesh_count × lod_count` ranges, row-major `[submesh][level]`:

| Offset | Size | Field           | Notes                                       |
|-------:|-----:|-----------------|---------------------------------------------|
|      0 |    4 | `lod_count`     | u32, reduced levels per submesh (`2`)       |
|      4 |    4 | `submesh_count` | u32, equals `DESC.submesh_count`            |
|      8 |  8 × n | ranges        | `first_index` u32 (into `LODIDX`), `index_count` u32 |

Level 0 of the table is the first reduced level (the full mesh is `SUBMESH`).
An `index_count` of `0` means no simpler level was reached; use the previous
one.

## SkinVertex (24 bytes)

| Offset | Size | Field     | Notes                                                    |
|-------:|-----:|-----------|----------------------------------------------------------|
|      0 |    8 | `joints`  | u16 × 4, indices into `SKELETON`                          |
|      8 |   16 | `weights` | f32 × 4, summing to 1                                    |

A vertex without weights (e.g. from an unskinned primitive in a skinned
mesh) is bound rigidly to joint 0 (weights `1, 0, 0, 0`).

## Joint (112 bytes)

| Offset | Size | Field          | Notes                                                    |
|-------:|-----:|----------------|----------------------------------------------------------|
|      0 |   64 | `inverse_bind` | f32 × 16, column-major, mesh space → joint bind space    |
|     64 |   12 | `bind_translation` | f32 × 3, the joint's local bind-pose transform       |
|     76 |   16 | `bind_rotation`| f32 × 4, unit quaternion `x y z w`                        |
|     92 |   12 | `bind_scale`   | f32 × 3                                                  |
|    104 |    4 | `parent`       | i32, index of the parent joint, `−1` for a root          |
|    108 |    4 | `reserved`     | u32, `0`                                                 |

Joints are in the order of the glTF skin's `joints`, so `SkinVertex.joints`
and animation channels index them directly.
