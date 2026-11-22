# `.acollider.bin`

A glTF model's compound [collider](../features/colliders.md): convex hulls
in the model's space, written as `P.acollider.bin` beside the model's
[`P.amesh.bin`](amesh.bin.md) when the model has an
[`.acollider`](acollider.md) sidecar.

It uses the [chunked container](chunks.md) with format name `ACOLL` and
version `1`. All integers are little-endian, floats IEEE 754 `f32`.

## Chunks

In this order:

| Chunk    | Required | Payload |
|----------|----------|---------|
| `DESC`   | yes      | [`Desc`](#desc-64-bytes), one record |
| `HULLS`  | yes      | [`Hull`](#hull-64-bytes)`[]`, `hull_count` records |
| `POINTS` | yes      | `f32[3][]`: every hull's corner points, `point_count` of them |
| `FACES`  | yes      | `u32[3][]`: every hull's triangles, `face_count` of them |

Readers ignore other chunks.

## Space

Points are in the model's space, the space of the mesh's vertices
(`POSITION` in [`amesh.bin.md`](amesh.bin.md)): model space, or mesh space
at the bind pose for a skinned mesh. Units are the model's.

## Desc (64 bytes)

| Offset | Size | Field          | Notes |
|-------:|-----:|----------------|-------|
|      0 |    4 | `hull_count`   | u32, at least 1 |
|      4 |    4 | `point_count`  | u32, records in `POINTS` |
|      8 |    4 | `face_count`   | u32, records in `FACES` |
|     12 |    4 | `reserved`     | u32, `0` |
|     16 |   12 | `aabb_min`     | f32 × 3, the box around every hull |
|     28 |   12 | `aabb_max`     | f32 × 3 |
|     40 |    4 | `volume`       | f32, the hulls' volumes added up (overlaps count twice) |
|     44 |    4 | `model_volume` | f32, the model's volume as voxelized (see [How it is made](../features/colliders.md#how-it-is-made)) |
|     48 |    8 | `mesh`         | u64, asset ID of the model's `P.amesh.bin` |
|     56 |    8 | `reserved`     | u64, `0` |

## Hull (64 bytes)

One per hull, largest volume first.

| Offset | Size | Field          | Notes |
|-------:|-----:|----------------|-------|
|      0 |    4 | `first_point`  | u32, index of its first point in `POINTS` |
|      4 |    4 | `point_count`  | u32, 4–255 |
|      8 |    4 | `first_face`   | u32, index of its first triangle in `FACES` |
|     12 |    4 | `face_count`   | u32 |
|     16 |   12 | `center`       | f32 × 3, the center of its volume (its center of mass at even density) |
|     28 |    4 | `volume`       | f32, > 0 |
|     32 |   12 | `aabb_min`     | f32 × 3 |
|     44 |   12 | `aabb_max`     | f32 × 3 |
|     56 |    8 | `reserved`     | u64, `0` |

Hulls' points and triangles are back to back in hull order: hull `i + 1`'s
`first_point` is hull `i`'s `first_point + point_count`, and the same for
faces.

## Faces

Each triangle is three indices into its hull's points (`0` is the hull's
`first_point`), counter-clockwise seen from outside the hull, so
`(b − a) × (c − a)` points out. Every point is a corner of the hull (none
lies inside it or inside a face). A hull's triangles close it: every edge is
in two of them. Engines that build their own hull from points (Jolt's
`ConvexHullShapeSettings`, PhysX's convex cooking, Rapier's `convex_hull`)
can take the points alone.
