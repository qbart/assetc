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
| `LOCATORS` | no       | [`Locator`](#locator-64-bytes)`[]` (payload size / 64), see [Locators](#locators) |
| `LOCNAMES` | no       | `u8[]`: the locators' names and tags, UTF-8, back to back         |
| `LOCTAGS`  | no       | [`LocatorTag`](#locatortag-16-bytes)`[]` (payload size / 16), the locators' tags |
| `COLORSET` | no       | [`ColorSet`](#colorset-32-bytes)`[set_count]`, see [Color sets](#color-sets) |
| `COLORS`   | no       | `u8[4][set_count × vertex_count]`: every set's colors, set after set |
| `STRINGS`  | no       | `u8[]`: UTF-8 text of color sets, properties, shapes, regions and paths, back to back, see [Strings](#strings) |
| `PROPS`    | no       | [`Property`](#property-32-bytes)`[]`: the model's properties, sorted by key |
| `SHAPES`   | no       | [`Shape`](#shape-96-bytes)`[]`, see [Shapes](#shapes) |
| `SHAPEPTS` | no       | `f32[3][]`: the hulls' points |
| `SHAPEPRP` | no       | [`Property`](#property-32-bytes)`[]`: the shapes' properties |
| `REGIONS`  | no       | [`Region`](#region-32-bytes)`[]`, see [Regions](#regions) |
| `REGTAGS`  | no       | [`Tag`](#tag-16-bytes)`[]`: the regions' tags |
| `TRIREGN`  | no       | `u32[index_count / 3]`: per triangle of `INDICES`, its regions as bits |
| `PATHS`    | no       | [`Path`](#path-80-bytes)`[]`, see [Paths](#paths) |
| `PATHPTS`  | no       | `f32[3][]`: the paths' control points |
| `PATHSMP`  | no       | [`PathSample`](#pathsample-32-bytes)`[]`: the paths' samples |
| `PATHPRP`  | no       | [`Property`](#property-32-bytes)`[]`: the paths' properties |

assetc writes the chunks in the order above; `LODIDX` and `LODTABLE` appear
together, as do `SKIN` and `SKELETON` (only for skinned meshes), and
`LOCATORS` and `LOCNAMES` (only for models with locators); `LOCTAGS`
follows them when a locator has tags. `COLORSET` and `COLORS` come
together, when the mesh has color sets. `STRINGS` comes when color sets,
properties, shapes, regions or paths do; then `PROPS` (with properties), `SHAPES`, `SHAPEPTS`
and `SHAPEPRP` (with shapes; `SHAPEPTS` only with a hull, `SHAPEPRP` only
when a shape has properties), `REGIONS`, `REGTAGS` (when a region has tags)
and `TRIREGN` (with regions), `PATHS`, `PATHPTS`, `PATHSMP` and `PATHPRP`
(with paths; `PATHPRP` only when a path has properties).

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
resolve through [`assets.aman`](aman.md).

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

## Locators

Named transforms authored in the model's
[`.amod`](amod.md#locators), see
[`docs/features/locators.md`](../features/locators.md). `LOCATORS` holds one
record per locator in the sidecar's order; `LOCNAMES` holds their names
(and their tags' text, after them); `LOCTAGS` their tags.

A locator's transform is `T · R · S` (translation, rotation, scale) relative
to its parent: the joint's current transform (as animations pose it,
`SKELETON` space) when `joint ≥ 0`, otherwise the mesh's own space (that of
`VERTICES`). Its world transform is the parent's world transform × that.

Look a locator up by `name_hash`, the FNV-1a-64 of its UTF-8 name (the same
hash as [asset IDs](../features/build.md#asset-ids)); hashes are unique
within a file. The names are there for tools and debugging.

### Locator (64 bytes)

| Offset | Size | Field         | Notes                                                       |
|-------:|-----:|---------------|-------------------------------------------------------------|
|      0 |    8 | `name_hash`   | u64, FNV-1a-64 of the name                                  |
|      8 |    4 | `name_offset` | u32, byte offset of the name in `LOCNAMES`                   |
|     12 |    4 | `name_len`    | u32, byte length of the name                                |
|     16 |    4 | `joint`       | i32, index into `SKELETON`; `−1`: mesh space                 |
|     20 |   12 | `translation` | f32 × 3                                                     |
|     32 |   16 | `rotation`    | f32 × 4, unit quaternion `x y z w`                          |
|     48 |   12 | `scale`       | f32 × 3                                                     |
|     60 |    2 | `first_tag`   | u16, index of its first tag in `LOCTAGS` (`0` without tags)  |
|     62 |    2 | `tag_count`   | u16, number of its tags, consecutive in `LOCTAGS`            |

`first_tag` and `tag_count` were a reserved `u32` of `0` before tags: a file
without `LOCTAGS` reads as locators without tags.

### LocatorTag (16 bytes)

The tags of every locator, the first locator's first, in the order the
sidecar lists them. A tag used by several locators appears once per locator.

| Offset | Size | Field         | Notes                                                       |
|-------:|-----:|---------------|-------------------------------------------------------------|
|      0 |    8 | `hash`        | u64, FNV-1a-64 of the tag's UTF-8 text                      |
|      8 |    4 | `name_offset` | u32, byte offset of the text in `LOCNAMES`                  |
|     12 |    4 | `name_len`    | u32, byte length of the text                                |

## Strings

`STRINGS` holds the text of color sets (names), properties (keys and Text values), shapes
(names and kinds), regions (names and tags) and paths (names and kinds), UTF-8, back to back, without
terminators. Records point into it with a byte offset and length. Locators
keep theirs in `LOCNAMES`.

## Tag (16 bytes)

| Offset | Size | Field         | Notes                                           |
|-------:|-----:|---------------|-------------------------------------------------|
|      0 |    8 | `hash`        | u64, FNV-1a-64 of the tag's UTF-8 text           |
|      8 |    4 | `text_offset` | u32, byte offset of the text in `STRINGS`        |
|     12 |    4 | `text_len`    | u32, byte length of the text                     |

## Properties

Key / value pairs, see
[`docs/features/properties.md`](../features/properties.md). `PROPS` holds the
model's, sorted by key; `SHAPEPRP` the shapes' and `PATHPRP` the paths',
each one's run sorted by key. Look one up by `key_hash`, the FNV-1a-64 of its UTF-8 key.

### Property (32 bytes)

| Offset | Size | Field        | Notes                                                      |
|-------:|-----:|--------------|------------------------------------------------------------|
|      0 |    8 | `key_hash`   | u64, FNV-1a-64 of the key                                  |
|      8 |    4 | `key_offset` | u32, byte offset of the key in `STRINGS`                    |
|     12 |    4 | `key_len`    | u32, byte length of the key                                |
|     16 |    4 | `type`       | u32: `0` Bool, `1` Int, `2` Float, `3` Text                 |
|     20 |    4 | `reserved`   | u32, `0`                                                   |
|     24 |    8 | `value`      | Bool: u64 `0` or `1`; Int: i64; Float: f64; Text: u32 byte offset in `STRINGS`, then u32 byte length |

## Shapes

Simple volumes, see [`docs/features/shapes.md`](../features/shapes.md).
`SHAPES` holds one record per shape in the sidecar's order. A shape's
transform is `T · R` relative to its parent, the joint (`joint ≥ 0`) or the
mesh's own space, as for [locators](#locators). A hull's points are in the
parent's space, without `T · R`.

### Shape (96 bytes)

| Offset | Size | Field            | Notes                                                       |
|-------:|-----:|------------------|-------------------------------------------------------------|
|      0 |    8 | `name_hash`      | u64, FNV-1a-64 of the name                                  |
|      8 |    4 | `name_offset`    | u32, in `STRINGS`                                           |
|     12 |    4 | `name_len`       | u32                                                         |
|     16 |    8 | `kind_hash`      | u64, FNV-1a-64 of the kind (of the empty text when none)    |
|     24 |    4 | `kind_offset`    | u32, in `STRINGS`                                           |
|     28 |    4 | `kind_len`       | u32, `0` when none                                          |
|     32 |    4 | `joint`          | i32, index into `SKELETON`; `−1`: mesh space                 |
|     36 |    4 | `type`           | u32: `0` box, `1` sphere, `2` capsule, `3` hull              |
|     40 |   12 | `translation`    | f32 × 3 (zero for a hull)                                   |
|     52 |   16 | `rotation`       | f32 × 4, unit quaternion `x y z w` (identity for a hull)    |
|     68 |   12 | `size`           | f32 × 3: box: full size along X, Y, Z; sphere: radius, 0, 0; capsule: radius, height, 0; hull: 0, 0, 0 |
|     80 |    4 | `first_point`    | u32, index of its first point in `SHAPEPTS` (`0` but a hull) |
|     84 |    4 | `point_count`    | u32, its points (hull only)                                 |
|     88 |    4 | `first_property` | u32, index of its first property in `SHAPEPRP`               |
|     92 |    4 | `property_count` | u32                                                         |

A capsule's `height` is the distance between the centers of its two
half-spheres, along its local Y axis; its full length is `height + 2 ·
radius`.

## Regions

Named sets of triangles, see
[`docs/features/regions.md`](../features/regions.md). `REGIONS` holds at
most 32 records, in the sidecar's order; region `i` is bit `i` (`1 << i`) of
`TRIREGN`, whose entry `t` is triangle `t` of `INDICES` (indices `3t` to
`3t + 2`, across submeshes). Reduced levels of detail (`LODIDX`) have no
regions.

### Region (32 bytes)

| Offset | Size | Field            | Notes                                                    |
|-------:|-----:|------------------|----------------------------------------------------------|
|      0 |    8 | `name_hash`      | u64, FNV-1a-64 of the name                               |
|      8 |    4 | `name_offset`    | u32, in `STRINGS`                                        |
|     12 |    4 | `name_len`       | u32                                                      |
|     16 |    2 | `first_tag`      | u16, index of its first tag in `REGTAGS` (`0` without tags) |
|     18 |    2 | `tag_count`      | u16                                                      |
|     20 |    4 | `triangle_count` | u32, triangles of `INDICES` in it                         |
|     24 |    8 | `reserved`       | `0`                                                      |

## Color sets

Per-vertex colors, see
[`docs/features/vertex-colors.md`](../features/vertex-colors.md).
`COLORSET` holds one record per set; `COLORS` holds the sets' colors one
set after another, each `vertex_count` entries long and parallel to
`VERTICES`: the color of vertex `v` in set `s` is the 4 bytes at
`(s × vertex_count + v) × 4`. Find a set by `name_hash`. glTF sets come
first (in order of `n`), then authored ones.

### ColorSet (32 bytes)

| Offset | Size | Field         | Notes                                                 |
|-------:|-----:|---------------|-------------------------------------------------------|
|      0 |    8 | `name_hash`   | u64, FNV-1a-64 of the name                            |
|      8 |    4 | `name_offset` | u32, in `STRINGS`                                     |
|     12 |    4 | `name_len`    | u32                                                   |
|     16 |    4 | `format`      | u32: `0` RGBA8 (4 × u8, unsigned normalized, linear; `R8G8B8A8_UNORM`) |
|     20 |    4 | `source`      | u32: `0` the glTF's `COLOR_n` attribute, `1` authored in assetc (a solid color or gradient, see [Authored sets](../features/vertex-colors.md#authored-sets)) |
|     24 |    8 | `reserved`    | `0`                                                   |

Readers skip sets whose `format` they do not know; every format's entries
are 4 bytes in this version.

## Paths

Curves, see [`docs/features/paths.md`](../features/paths.md#curve). `PATHS`
holds one record per path in the sidecar's order. Points and samples are in
the path's parent space: its joint (`joint ≥ 0`) or the mesh's own space,
as for [locators](#locators).

The curve is a cardinal spline through `PATHPTS`: segment `i` (from point
`i` to point `i + 1`, or from the last to the first closing a loop) is the
cubic Hermite curve `p₁ → p₂` with neighbors `p₀`, `p₃` (wrapping when
closed; mirrored `2 p₀ − p₁` and `2 pₙ₋₁ − pₙ₋₂` past the ends when open)
and tangents, with `dᵢⱼ = max(|pⱼ − pᵢ|^alpha, 1e-5)`:

```
m₁ = ((p₁ − p₀) / d₀₁ − (p₂ − p₀) / (d₀₁ + d₁₂) + (p₂ − p₁) / d₁₂) · d₁₂ · (1 − tension)
m₂ = ((p₂ − p₁) / d₁₂ − (p₃ − p₁) / (d₁₂ + d₂₃) + (p₃ − p₂) / d₂₃) · d₁₂ · (1 − tension)
```

(matma's `cardinal`). The spline parameter `u` runs from `0` to the
segment count; its integer part is the segment, its fraction the segment's
`t`.

### Path (80 bytes)

| Offset | Size | Field            | Notes                                                    |
|-------:|-----:|------------------|----------------------------------------------------------|
|      0 |    8 | `name_hash`      | u64, FNV-1a-64 of the name                               |
|      8 |    4 | `name_offset`    | u32, in `STRINGS`                                        |
|     12 |    4 | `name_len`       | u32                                                      |
|     16 |    8 | `kind_hash`      | u64, FNV-1a-64 of the kind (of the empty text when none) |
|     24 |    4 | `kind_offset`    | u32, in `STRINGS`                                        |
|     28 |    4 | `kind_len`       | u32, `0` when none                                       |
|     32 |    4 | `joint`          | i32, index into `SKELETON`; `−1`: mesh space              |
|     36 |    4 | `flags`          | u32, bit 0: closed                                       |
|     40 |    4 | `tension`        | f32, 0 to 1                                              |
|     44 |    4 | `alpha`          | f32, 0 to 1                                              |
|     48 |    4 | `length`         | f32, the curve's length                                  |
|     52 |    4 | `first_point`    | u32, index of its first point in `PATHPTS`                |
|     56 |    4 | `point_count`    | u32, at least 2 (3 when closed)                          |
|     60 |    4 | `first_sample`   | u32, index of its first sample in `PATHSMP`               |
|     64 |    4 | `sample_count`   | u32, `16 · segments + 1`                                 |
|     68 |    4 | `first_property` | u32, index of its first property in `PATHPRP` (`0` without) |
|     72 |    4 | `property_count` | u32                                                      |
|     76 |    4 | `reserved`       | `0`                                                      |

### PathSample (32 bytes)

Samples evenly spaced by distance: sample `k` is at `length · k / (count −
1)` along the curve, the first on the first point, the last on the last
point (closed: on the first point again).

| Offset | Size | Field      | Notes                                                  |
|-------:|-----:|------------|--------------------------------------------------------|
|      0 |   12 | `position` | f32 × 3                                                |
|     12 |    4 | `distance` | f32, from the start along the curve                    |
|     16 |   12 | `tangent`  | f32 × 3, unit direction of travel (zero where the curve stops, e.g. at coincident points) |
|     28 |    4 | `u`        | f32, the spline parameter                              |
