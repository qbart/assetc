# Models

glTF 2.0 files in the Input tree can be inspected, previewed in 3D, and
compiled into a mesh, its materials and textures, and its animations (see
[Build](#build)).

## Kind

| Tab   | File                         | Kind  |
|-------|------------------------------|-------|
| Input | `*.gltf`, `*.glb` (any case) | Model |

`*.obj` files are not read (see [`DEFERRED.md`](../../DEFERRED.md)); they
stay Ignored, though the file tree shows them with the `torus` icon.

## Inspector

Info rows after Kind and Size, read from the file's JSON (and a `.glb`'s
JSON chunk) with the [`gltf`](https://docs.rs/gltf) crate. Buffers and
images are not read for these rows.

| Row        | Value                                                              |
|------------|--------------------------------------------------------------------|
| Generator  | `asset.generator`, left out when missing                           |
| Version    | `asset.version`, e.g. `2.0`                                        |
| Scenes     | number of scenes                                                   |
| Nodes      | number of nodes                                                    |
| Meshes     | `<meshes> (<primitives> primitives)`                              |
| Vertices   | sum of the `POSITION` accessor counts of all primitives            |
| Triangles  | sum over triangle-list primitives of index count / 3 (vertex count / 3 without indices); strips and fans count n − 2 |
| Materials  | number of materials                                                |
| Textures   | number of textures                                                 |
| Skins      | number of skins                                                    |
| Animations | number of animations                                               |

Counts are per mesh, not per instance: a mesh used by two nodes counts once.
After them, **Meshlet limits** (`64 vertices, 124 triangles`: the values
this model builds with, from `[meshes]` and its `.amod`, or the error), then **Asset path** and
**Output** (the `.amesh`; `(not built)` appended when it does not exist), as
for source textures.

Compiled files in the Output tree:

- **Compiled mesh** (`*.amesh`), every `DESC` value and more: Format
  (`AMESH v1, 14 chunks`), Vertices, Vertex stride (`28 B`), Indices
  (`1728 (u16)`), Triangles, Submeshes, Meshlets, Meshlet limits
  (`64 vertices, 124 triangles`), Materials, Flags (`0x07 (tangents,
  skinned, levels of detail)`, or `0x00 (none)`), Levels of detail (`2` or
  `none`), Skin (`<n> joints` or `none`), Bounds (`min – max`), Asset path,
  Asset ID. A file that does not read shows an Error row.
- **Compiled animation** (`*.aanim`): Clips (names with durations), Channels,
  Keys, Asset path, Asset ID.
A file that fails to parse shows an **Error** row (`not a valid glTF file:
<reason>`) and the canvas shows the message as "No preview".

## Preview

Selecting a model shows it in the canvas' [3D view](canvas.md#3d) instead of
a shape.

- The view bar is empty (no `View` label, no buttons): a model only exists
  in 3D. The toolbar (Shape, Channels) is not shown.
- Loaded with Bevy's glTF loader from an asset source registered at startup
  for the project's `input` directory, so the model's own meshes, materials
  and textures (embedded or next to the file) are used, as Bevy renders them.
  The file's default scene is shown (the first scene if none is marked).
- **Fit**: once loaded, the model is centered on the orbit target and scaled
  so the longest side of its bounding box is 1.5 units (about the size of
  the shapes). The box is that of every mesh in the scene in its rest pose;
  a skinned mesh is measured through its first joint. The model is hidden
  until it has been fitted.
- The model does **not** auto-rotate: it stays as authored, facing the way
  the file has it, and only the orbit camera moves. The camera, its controls
  and the lighting work as for textured shapes ([Scene](canvas.md#scene),
  [Camera](canvas.md#camera)).
- **Loading** runs in the background: the canvas stays empty and the status
  bar shows the [loader](gui.md#layout) until the model is shown. If loading
  fails the canvas shows `Could not load the model: <error>`.
- Each new inspection of the file (selecting it, a rescan) reloads it from
  disk, so changes made in another program show up.

## Parameters tab

The Inspector's **Parameters** tab (`sliders-vertical`) is shown for a model
in the Input tree. It edits the model's [`.amod`](../spec/amod.md) sidecar,
in the [property editor](gui.md#property-editor) look, with one **Meshlets**
section (collapsible, open):

- **Max vertices** (integer field, 3–256) and **Max triangles** (integer
  field, 4–512, rounded to a multiple of 4), drag or type. They show the
  values the model builds with.
- A weak line saying where they come from: `Project settings (assetc.toml)`,
  or `Set for this file (<file>.amod)` when the sidecar sets either value.
- A **Use project settings** button, enabled when the sidecar sets a value:
  it removes the values (and the file when nothing else is left in it).
- Changing a value writes it to the sidecar (creating it), saved like noise
  parameters (see [`noise.md`](noise.md#saving)); the file is then inspected
  again. If `assetc.toml` `[meshes]` or the sidecar is invalid, the tab shows
  the error instead.
- The new limits apply at the next build; check the result in the
  [compiled mesh preview](#compiled-mesh-preview).

## Compiled mesh preview

Selecting a compiled mesh (`*.amesh` in the Output tree) shows it in the
canvas, to check what the build produced. The [view bar](canvas.md#view-bar)
offers **3D** (the default) and **Chunks** (the file's data as tables, see
[Chunks view](#chunks-view)); the choice lasts for the session.

- In 3D the mesh is read from the
  file (vertices, submeshes, meshlets) on the
  [inspector thread](inspector.md#loading), centered and scaled like a
  [model](#preview), and does not auto-rotate. A skinned mesh is shown in its
  bind pose.
- A toolbar group (top right, like the canvas toolbar) picks the display
  mode; the choice lasts for the session:

  | Button      | Icon          | Shows                                                    |
  |-------------|---------------|----------------------------------------------------------|
  | Material    | `box-checker` | the mesh with its compiled materials (default), see below |
  | Wireframe   | `box`         | every triangle edge as a 1 px `TEXT` line, unlit, no faces |
  | Shaded      | `box-shaded`  | the whole mesh in light gray                             |
  | Meshlets    | `squares-unite` | each meshlet in its own color                          |
  | Material ID | `image`       | each material slot in its own color; no material in gray |
  | Submeshes   | `boxes`       | each submesh in its own color                            |

- **Material**: each submesh uses its `.amat` (looked up by the `MATERIAL`
  asset IDs in the output's [`assets.manifest`](../spec/manifest.md)) as a
  physically based material: base color factor and texture, metallic and
  roughness factors with the ORM texture (G roughness, B metallic, R
  occlusion), normal texture, emissive factor × strength and texture, alpha
  mode and cutoff, double-sided. Normal scale and occlusion strength are not
  applied (the 3D view's material has no such settings). Textures are read from their
  `.ktx2` (level 0). A material or texture that cannot be found or read is
  replaced by the defaults (white, roughness 1, no texture), and a submesh
  without a material is light gray.
- **Colors** of the ID modes: index `i` (meshlet, material slot or submesh)
  has hue `i × 0.618034` (fractional part, the golden ratio, so neighbours
  differ), saturation 0.65, value 0.9. Triangles are lit as usual (the
  file's normals), so shapes stay readable.
- **Level of detail**: the view shows the level chosen in the
  [Levels of detail tab](#levels-of-detail-tab) (LOD 0, the full mesh, to
  start with), in every mode. Meshlets exist only for LOD 0: with another
  level chosen, the Meshlets mode shows it in light gray.
- A file that does not read shows its error as the canvas message.

## Chunks view

The **Chunks** view shows the data inside the `.amesh`
[chunked file](../spec/chunks.md), chunk by chunk:

- **Chunk list** on the left (220 px, `BG_PANEL`, 1 px `STROKE` line on its
  right): one 40 px row per chunk in file order with the chunk name
  (13 px monospace, `TEXT`), its description below (12 px, `TEXT_WEAK`), and
  its size on the right (12 px, `TEXT_WEAK`). Rows highlight like other
  choice lists; the chosen chunk (by name, `DESC` to start with, kept for the
  session while the next file has it) is filled with `ACCENT` at 25%.
- **Table** on the right: a header line `<NAME> — <description>   @<offset>
  <size> bytes` (13 px monospace), then the chunk's data as a table with a
  header row (`TEXT_WEAK`) and striped rows, 20 px tall, 12 px monospace;
  long tables scroll (only visible rows are drawn), wide ones scroll
  sideways. Columns size to their content and can be resized.
- Records (`DESC`, `BOUNDS`) show one row per field: **Field**, **Value**
  (the spec's field names; sizes in bytes as `28 B`, flags as `0x07
  (tangents, skinned, levels of detail)`). Arrays show one row per element,
  with **#** (the element's index) first:

  | Chunk      | Description         | Columns after #                                       |
  |------------|---------------------|-------------------------------------------------------|
  | `DESC`     | Mesh descriptor     | (record)                                              |
  | `BOUNDS`   | Mesh bounds         | (record: AABB min, AABB max, sphere center, radius)    |
  | `VERTICES` | Vertices            | Position, Normal, Tangent (`x, y, z, ±1`), UV         |
  | `INDICES`  | Indices             | per triangle: i0, i1, i2                              |
  | `SUBMESH`  | Submeshes           | First index, Index count, First meshlet, Meshlet count, Material (slot or `—`), AABB min, AABB max |
  | `MATERIAL` | Material asset IDs  | Asset ID                                              |
  | `MESHLETS` | Meshlets            | Vertex offset, Vertices, Triangle offset, Triangles   |
  | `MLVERTS`  | Meshlet vertices    | Vertex                                                |
  | `MLTRIS`   | Meshlet triangles   | i0, i1, i2                                            |
  | `MLBOUNDS` | Meshlet bounds      | Center, Radius, Cone axis, Cone cutoff                |
  | `LODIDX`   | LOD indices         | per triangle: i0, i1, i2                              |
  | `LODTABLE` | LOD table           | Submesh, Level, First index, Index count (the header's counts in the header line) |
  | `SKIN`     | Skin weights        | Joints, Weights                                       |
  | `SKELETON` | Skeleton            | Parent (or `—`), Translation, Rotation, Scale          |
  | other      | Unknown chunk       | Offset, Bytes (16 per row, hex)                        |

  Normals and tangents are decoded from their octahedral encoding. Floats
  show three decimals; vectors as `x, y, z`.

## Levels of detail tab

The Inspector's **Levels of detail** tab (`bring-to-front`) is shown for a
compiled mesh (`*.amesh` in the Output tree) with reduced levels (a
`LODTABLE`). It switches which level the
[compiled mesh preview](#compiled-mesh-preview) draws.

- Rows like the [Mipmaps tab](inspector.md#mipmaps-tab)'s: `LOD 0`, `LOD 1`,
  … with the level's triangle count on the right (`576 triangles`).
- A submesh whose level made no progress (`index_count` 0) uses its previous
  level, as a runtime would; a level where every submesh did so says
  `(same as LOD <n>)` after its count.
- LOD 0 is chosen to start with; the choice is remembered per file (by path)
  for the session.

## Animations tab

The Inspector's **Animations** tab (`film`) is shown for a model with at
least one animation. It switches which animation the [preview](#preview)
plays.

- A list of full-width rows, 28 px tall, inside 8 px margins: first
  **Rest pose**, then the file's animations in file order. Each row shows the
  animation's name (14 px, `TEXT`; `Animation <n>` counting from 1 when it
  has none) and, right-aligned, its duration (13 px, `TEXT_WEAK`, two
  decimals, e.g. `1.25 s`).
- The duration is the largest `max` of the animation's sampler input
  accessors (glTF requires them), read with the other
  [Info rows](#inspector).
- Hovering a row fills it with `BG_HEADER`; the chosen row is filled with
  `ACCENT` at 25% opacity, like the file tree's selection. Clicking a row
  chooses it.
- **Rest pose** (the default) shows the model as loaded, without animation.
  Choosing an animation plays it from its start, looping, at normal speed;
  the orbit camera keeps working. Switching to another
  animation or back to Rest pose first returns the model to its rest pose.
- The choice is remembered per model (by path) for the session, and kept
  when the model is reloaded.
- The model is fitted once, in its rest pose; animations can move it out of
  that box.

## Build

Every `*.gltf` / `*.glb` (any case) in `input` compiles into several assets.
With `P` the input-relative path without the extension (e.g. `models/Fox`):

| Output (in `output`)        | Asset kind | Asset path        | When                                   |
|-----------------------------|------------|-------------------|----------------------------------------|
| `P.amesh`                   | mesh       | `P.amesh`         | always ([spec](../spec/amesh.md))       |
| `P/<material>.amat`         | material   | `P/<material>.amat` | per material a submesh uses ([spec](../spec/amat.md)) |
| `P/<image>.ktx2`            | texture    | `P/<image>`       | per image used as base color, normal or emissive texture |
| `P/<material>.orm.ktx2`     | texture    | `P/<material>.orm` | per material with an occlusion or metallic-roughness texture |
| `P.aanim`                   | animation  | `P.aanim`         | when the mesh is skinned and the file has animations ([spec](../spec/aanim.md)) |

Like other assets, each gets the FNV-1a-64 of its asset path as ID and a
manifest entry; collisions are build errors as usual. The build prints one
line per model, e.g.
`models/Fox.glb -> models/Fox.amesh (1728 vertices, 576 triangles, 1 submesh, 12 meshlets, 24 joints, 3 clips, 1 material, 1 texture)`.

### Names

- `<material>` is the glTF material's name when it is non-empty, unique among
  the file's materials and usable as a file name (no `/` or `\`, not `.` or
  `..`); otherwise `material_<index>` (its index in the file). The same rule
  gives `<image>` (`image_<index>`). Case is kept.

### Geometry

- The file's default scene (or its first) is walked from its root nodes.
  Every node with a mesh contributes each of its **triangle** primitives
  (`TRIANGLES`, `TRIANGLE_STRIP`, `TRIANGLE_FAN`, converted to lists; points
  and lines are skipped) as one **submesh**, in traversal order (depth-first,
  children in file order).
- **Static** primitives are baked into world space with the node's world
  transform (normals by the inverse transpose; when the transform mirrors,
  tangent handedness is flipped and each triangle's winding reversed so it
  stays front-facing, as glTF specifies). Primitives of a **skinned** node (one with a
  `skin`) are left in mesh space, as glTF ignores their node transform.
- Attributes: `POSITION` (required); `NORMAL` (missing: flat normals, the
  triangles' own); `TEXCOORD_0` (missing: `0, 0`); `TANGENT` (missing:
  generated with MikkTSpace when there are UVs, otherwise zero and
  `DESC.flags` bit 0 cleared); `JOINTS_0` / `WEIGHTS_0` for skinning.
  Weights are renormalized to sum 1.
- Per submesh: the vertex order is optimized for the GPU's vertex cache, then
  the submesh is split into **meshlets** with their bounds (cone weight
  0.25), of at most `meshlet_max_vertices` vertices and
  `meshlet_max_triangles` triangles: 64 and 124 unless
  [`[meshes]`](../spec/assetc.toml.md#meshes) in `assetc.toml` sets other
  defaults or overrides them for the model, or the model's
  [`.amod`](../spec/amod.md#mesh) sidecar (edited in the
  [Parameters tab](#parameters-tab)) sets its own. The limits used are
  stored in the `.amesh` `DESC`, and simplified into two
  **levels of detail** at about 50% and 25% of its triangles, all with
  [meshoptimizer](https://github.com/zeux/meshoptimizer).
- **Simplification keeps UVs and normals in mind.** A level of detail only
  drops vertices and reuses the remaining ones (with their own UV, normal and
  tangent), so its quality depends on which vertices survive.
  Simplification therefore weighs the vertices' UVs (weight 1) and normals
  (weight 0.5) next to their positions, and may collapse across attribute
  seams (vertices sharing a position but not their UV or normal, as on every
  edge of a flat-shaded mesh) when the attribute error stays within the
  limit. Without that, flat-shaded meshes could not be simplified at all.
  It starts with a 5% error limit and raises it by 5% up to three times
  while it makes no progress. Only then does it fall back to sloppy
  (topology-ignoring) simplification, which merges nearby vertices from
  different faces and so can stretch textures.
- A file without any triangle primitive fails (`no triangles in the default
  scene`).

### Skinning and animations

- The first skin of the file is used (a later skin's primitives are bound to
  it as if rigid); its joints become the `.amesh` skeleton, with the joints'
  bind-pose transforms and inverse bind matrices (identity when missing).
- With a skin and animations, `P.aanim` holds every clip, keeping channels
  that target the skin's joints (translation, rotation, scale). Animations
  of a file without a skin are not compiled.

### Materials and textures

Each used glTF material becomes an `.amat` with:

| `.amat` field          | From glTF                                                   |
|------------------------|-------------------------------------------------------------|
| `base_color`           | `baseColorFactor`                                           |
| `base_color_texture`   | `baseColorTexture` → `P/<image>.ktx2`, sRGB                 |
| `metallic`, `roughness` | `metallicFactor`, `roughnessFactor`                        |
| `orm_texture`          | `P/<material>.orm.ktx2` (linear): R from `occlusionTexture`'s R (255 without one), G and B from `metallicRoughnessTexture` (255 without one); `0` when neither exists |
| `occlusion_strength`   | `occlusionTexture.strength` (1 without one)                 |
| `normal_texture`, `normal_scale` | `normalTexture` → `P/<image>.ktx2`, linear; its `scale` |
| `emissive`, `emissive_texture` | `emissiveFactor`; `emissiveTexture` → `P/<image>.ktx2`, sRGB |
| `emissive_strength`    | `KHR_materials_emissive_strength` (1 without it)            |
| `alpha_mode`, `alpha_cutoff`, double-sided | `alphaMode`, `alphaCutoff` (0.5), `doubleSided` |
| height fields          | none: no height texture, scale 0, midlevel 0.5, parallax    |

- Images are read from the GLB's binary chunk, a data URI or a file next to
  the `.gltf` (PNG or JPEG), decoded to RGBA8 and written with a full mip
  chain. An image used in two color spaces keeps the first use's.
- When occlusion and metallic-roughness are different images they must have
  the same size, otherwise the build fails for the model.
- `texCoord` sets other than 0 and `KHR_texture_transform` are ignored.
- Submeshes without a material have none (`0xFFFFFFFF`).
