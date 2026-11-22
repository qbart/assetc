# Colliders

A **collider** is a glTF model's shape for physics, made of a few **convex
hulls** that together follow the model closely: a compound collider. Physics
engines (Jolt, PhysX, Rapier, Bullet, Havok, …) collide convex shapes fast,
but a chair, a rock arch or a car is not convex; one hull around all of it
fills the space under the chair's seat. A collider splits the model into
pieces that each are close to convex and wraps each in its own hull: the
*convex decomposition* V-HACD and CoACD do, built into assetc.

Colliders are made **per model**: a model gets one when its collider
sidecar, [`<model file>.acollider`](../spec/acollider.md), exists next to it
(`Fox.glb.acollider` for `Fox.glb`). The sidecar holds the settings; the
build makes the hulls and writes them to their own file,
[`P.acollider.bin`](../spec/acollider.bin.md), beside the model's
`P.amesh.bin`, so a game (or a server that never draws the model) loads the
collider without the mesh.

[Shapes](shapes.md) stay what they are: simple volumes placed by hand (a
hitbox on a joint, a trigger). A collider is made from the whole model's
triangles, for its physics body.

## Settings

| Setting        | Key            | Default | Range       | Meaning |
|----------------|----------------|---------|-------------|---------|
| Max hulls      | `max_hulls`    | `32`    | 1–256       | Most hulls in the collider. Fewer is faster to simulate; more follows the model closer. |
| Max vertices   | `max_vertices` | `32`    | 4–255       | Most corner points of one hull (most engines want 255 or fewer; 32–64 is common). |
| Error          | `error`        | `0.01`  | 0.001–0.5   | How much empty space the hulls may hold in all, as a share of the model's volume (0.01 = 1%). Lower splits the model into more hulls. |
| Resolution     | `resolution`   | `64`    | 16–256      | Voxels along the model's longest side. Higher sees thinner gaps and smaller details, and takes longer. |

Each is optional in the sidecar; an empty sidecar makes a collider with the
defaults.

## How it is made

The same steps in the [build](#build) and the [preview](#3d-view), so they
make the same hulls:

1. **Triangles**: every triangle of the model's default scene, each instance
   of a mesh, in model space (mesh space at the bind pose for a skinned
   model), as for a [convex hull shape](shapes.md#shape) in model space.
2. **Voxels**: the model's box is cut into cubes, `resolution` along its
   longest side; every cube a triangle touches is part of the model and so
   is every cube its surface closes in (a closed model is solid inside; an
   open one, such as a single wall, is its surface's cubes only).
3. **Splitting**: pieces of cubes that do not touch (not even by a corner)
   are separate parts from the start. Then the part whose hull holds the
   most empty cubes (counted by their centers) is cut in two by a plane
   across one axis, between two layers of cubes: the plane after which the
   two sides' hulls hold the fewest empty cubes, the middle one of several
   that are as good. Each side's separate pieces become parts. Splitting
   stops when all hulls together hold no more than `error` of the model's
   cubes, or when there are twice `max_hulls` parts.
4. **Hulls**: each part's hull is made around the model's own triangles
   inside its box (cut to it), not around its cubes, so it fits the surface
   and not the voxel steps. A cutting plane moves onto a corner of the
   model up to two cubes away, so hulls end where the model's faces do. A
   part without triangles inside it (deep inside a solid model) is
   wrapped around its cubes. A hull with more than `max_vertices` corners
   keeps the ones that keep the most of it: starting from four, the corner
   farthest outside the hull so far, until there are `max_vertices`.
5. **Merging**: while there are more than `max_hulls` hulls, the two whose
   joint hull grows the least become one. Then, while two can be merged
   without the empty cubes going over `error`, they are: a part that was
   split for nothing is put back together.
6. Hulls are listed largest first.

The result depends only on the triangles and the settings: building twice
gives the same file.

## Collider tab

The Inspector's **Collider** tab (`shield`) is shown for a model in the
Input tree:

- Without a sidecar: a weak line `No collider. A collider is a few convex
  hulls that follow the model, for physics.` and a **Make collider** button
  (`plus` and text), which writes an empty `<model file>.acollider` (the
  defaults) and shows the hulls in the 3D view.
- With one, in the [property editor](gui.md#property-editor) look, as the
  [Parameters tab](models.md#parameters-tab): **Max hulls** (integer field,
  1–256), **Max vertices** (integer field, 4–255), **Error** (slider
  0.001–0.5, logarithmic, shown as a percentage with one decimal, `1.0%`),
  **Resolution** (integer field, 16–256). A row's label is `ACCENT` when the
  sidecar sets it and `TEXT_WEAK` when it is the default. Hovering a label
  shows the setting's meaning (the [table](#settings)'s last column).
- Below them, the result of the [preview](#3d-view): `Making the collider…`
  with the loader while it runs, then `<n> hulls, <p> points`, and a second
  weak line `Hulls hold <v>% of the model's volume` (the hulls' volume over
  the voxelized model's, no decimals) and the time it took (`0.21 s`). A
  model whose collider cannot be made shows the error in `CHANNEL_R`.
- **Use defaults** (enabled when the sidecar sets a value) empties the
  sidecar. **Remove collider** (`trash-2` and text) deletes it; the model
  then builds no collider.
- Changing a setting writes the sidecar, saved like the Parameters tab's
  values, and makes the preview again.

## 3D view

The model's preview draws its collider's hulls as wireframes over the
model, on top of it like [shapes](shapes.md#3d-view): the edges of each
hull's faces, 1 px, hull `i` in the ID color of index `i` (hue `i ×
0.618034`, saturation 0.65, value 0.9, as the
[compiled mesh preview's](models.md#compiled-mesh-preview)).

- The hulls are made in the background, the [same way](#how-it-is-made) as
  the build makes them, when the model is inspected and after each change
  of a setting (a change while one runs starts another once it ends).
- They follow the model's fit; they do not follow an animation (a
  collider is made at the bind pose).
- The toolbar's [Show](models.md#show) group has a **Colliders** button
  (`shield`) after Paths: on to start with, disabled (`Colliders (none in
  this model)`) without a sidecar. **Make collider** turns it on.

## Build

A model with an `.acollider` sidecar builds one more output,
`P.acollider.bin` (see [`acollider.bin.md`](../spec/acollider.bin.md)), its
own asset (kind `15`, collider, in the [manifest](../spec/aman.bin.md)),
listed after the model's mesh with the detail `<n> hulls, <p> points`. It
is built again when the model, its folder (for a `.gltf`) or the sidecar
changes.

- An invalid sidecar (unknown key, wrong type, value out of range) fails the
  model's collider and mesh: `<file>.acollider: <problem>`.
- A model without triangles fails its collider: `the model has no
  triangles to make a collider of`.

## Inspector Info

- A model in the Input tree: a **Collider** row after Shapes: `<max hulls>
  hulls at most, resolution <r>` from the sidecar, or `none`.
- A compiled collider (`*.acollider.bin` in the Output tree): Format
  (`ACOLL v1, 4 chunks`), Hulls, Points, Faces, Volume (`<v> (<p>% of the
  model's)`, three decimals), Bounds (`min – max`), Mesh (the asset ID of
  its `.amesh.bin`), Asset path, Asset ID. A file that does not read shows
  an Error row. Its canvas offers the [Chunks view](models.md#chunks-view).

## Not yet

- Colliders per joint, for ragdolls (a skinned model's collider is of its
  bind pose).
- Cutting planes that are not across an axis.
- Drawing a compiled collider in 3D in the Output tree.
