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
| Color sets | the `COLOR_n` attributes of the primitives (`COLOR_0, COLOR_1`), or `none`; see [`vertex-colors.md`](vertex-colors.md) |
| Skins      | number of skins                                                    |
| Animations | number of animations                                               |

Counts are per mesh, not per instance: a mesh used by two nodes counts once.
After them, **Meshlet limits** (`64 vertices, 124 triangles`) and **Levels
of detail** (`50%, 25%`, or `none`): what this model builds with, from
`[meshes]` and its `.amod` (a **Mesh settings** row with the error instead
when either is invalid), **Locators** (their count in the `.amod`, or
`none`; see [`locators.md`](locators.md)), then **Asset path** and
**Output** (the `.amesh.bin`; `(not built)` appended when it does not exist), as
for source textures.

Compiled files in the Output tree:

- **Compiled mesh** (`*.amesh.bin`), every `DESC` value and more: Format
  (`AMESH v1, 14 chunks`), Vertices, Vertex stride (`28 B`), Indices
  (`1728 (u16)`), Triangles, Submeshes, Meshlets, Meshlet limits
  (`64 vertices, 124 triangles`), Materials, Flags (`0x07 (tangents,
  skinned, levels of detail)`, or `0x00 (none)`), Levels of detail (`2` or
  `none`), Skin (`<n> joints` or `none`), Color sets (see
  [`vertex-colors.md`](vertex-colors.md#inspector-info)), Locators (`<n>` or `none`, see
  [`locators.md`](locators.md#inspector-info)), Bounds (`min – max`), Asset path,
  Asset ID. A file that does not read shows an Error row.
- **Compiled animation** (`*.aanim.bin`): Clips (names with durations), Channels,
  Keys, Asset path, Asset ID.
A file that fails to parse shows an **Error** row (`not a valid glTF file:
<reason>`) and the canvas shows the message as "No preview".

## Preview

Selecting a model shows it in the canvas' [3D view](canvas.md#3d) instead of
a shape.

- The view bar is empty (no `View` label, no buttons): a model only exists
  in 3D. The toolbar (Shape, Channels) is not shown; it has a
  [Show](#show) group, then Color vision.
- The model's [locators](locators.md), [shapes](shapes.md) and
  [regions](regions.md) are drawn over it and edited there, see
  [`locators.md`](locators.md#3d-view).
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

### Show

The toolbar's first group turns the drawing of each kind of
[enrichment](locators.md) on or off, over the Color vision group:

| Button   | Icon          | Shows |
|----------|---------------|-------|
| Locators | `map-pin`     | the visible locators: markers, names, the gizmo and attachment previews |
| Shapes   | `shapes`      | the visible shapes' wireframes and the gizmo on the chosen shape |
| Regions  | `paint-bucket`| the model in its visible regions' colors instead of its materials, and the chosen region's outline ([regions](regions.md#3d-view)) |
| Paths    | `spline`      | the visible paths' curves and points, and the gizmo on the chosen point |
| Grid     | `grid-3x3`    | the [ground grid](#ground-grid) |

- Toggles, Locators, Shapes, Paths and Grid on and Regions off to start with, for the session and every model. *Visible*
  is each item's own **Visible** (eye) in its tab: a button that is on shows
  the items whose eye is on; off hides them all, whatever their eyes, and
  keeps the eyes as they are.
- A button is disabled (and drawn off) when the model has none of its kind;
  its tooltip adds `(none in this model)`. Grid is always enabled.
- A hidden locator cannot be clicked; a hidden locator or shape has no
  gizmo.
- Turning a kind back on by using it: **Add** or **Place on surface** in the
  Locators tab (and `S`) turns Locators on, **Add shape** turns Shapes on,
  **Paint** turns Regions on, **Add path**, **Add point** and **Draw** turn
  Paths on. Turning Regions off turns Paint off; turning Paths off turns
  Draw off.


### Ground grid

The model's preview draws a grid on its ground, the model-space plane
`y = 0` (where a character standing at the origin has its feet), so the
model's height above or below it shows:

- Square cells of `10ⁿ` model units, the largest power of ten no more than
  a quarter of the model's largest side (a 1.6-unit character gets 0.1,
  a 155-unit one 10), out to the first whole cell at or past that side in
  each direction from the origin (at most 50 cells each way).
- 1 px lines in `TEXT_WEAK` at 35% opacity; the line along X through the
  origin in `CHANNEL_R` and the one along Z in `CHANNEL_B`, at 80%.
- Unlit, hidden behind the model where the model is in front (depth
  tested), following the model's fit (not its animation).
- The toolbar's Show **Grid** button turns it off.
## Parameters tab

The Inspector's **Parameters** tab (`sliders-vertical`) is shown for a model
in the Input tree. It edits the model's [`.amod`](../spec/amod.md) sidecar:
the [meshoptimizer settings](../spec/assetc.toml.md#meshes) it builds with,
in the [property editor](gui.md#property-editor) look. Every row shows the
value the model builds with; its label is `ACCENT` when the sidecar sets it
and `TEXT_WEAK` when it comes from the project.

- **Meshlets** section (collapsible, open): **Max vertices** (integer field,
  3–256), **Max triangles** (integer field, 4–512, rounded to a multiple of
  4), **Cone weight** (slider 0–1).
- **Levels of detail** section (collapsible, open): **Levels** (integer field,
  0–8: adding a level appends half the last ratio, removing drops the last),
  one **LOD <n>** row per level (slider 0.01–0.99, each kept below the one
  before), **Error** and **Error step** (sliders 0–1, three decimals),
  **Attempts** (integer field, 0–10), **UV weight** and **Normal weight**
  (sliders 0–10), and checkboxes **Sloppy fallback**, **Permissive**,
  **Lock border**, **Regularize**, **Prune**.
- Hovering a setting's label shows what it does, in plain words:

  | Label           | Hint |
  |-----------------|------|
  | Max vertices    | Most vertices one meshlet may use. Match what your mesh shader outputs per workgroup (64 is common). |
  | Max triangles   | Most triangles one meshlet may hold, a multiple of 4. Match what your mesh shader outputs per workgroup (124 is common). |
  | Cone weight     | 0 packs triangles into as few meshlets as possible. Higher groups triangles that face the same way, so whole meshlets facing away from the camera can be skipped. |
  | Levels          | How many simplified copies of the mesh to build, for drawing it far away. 0 builds none. |
  | LOD *n*         | Size of this level as a share of the full mesh's triangles (0.25 = a quarter). Each level must be smaller than the one before. |
  | Error           | How far simplification may move the surface, as a share of the mesh's size (0.05 = 5%). Lower keeps the shape better but may stop before reaching the target size. |
  | Error step      | How much the allowed error grows on each retry, when a level cannot get smaller. |
  | Attempts        | How many times to retry with more allowed error before giving up (or falling back to sloppy). |
  | UV weight       | How much keeping texture coordinates matters. Higher keeps textures from stretching; lower simplifies more freely. |
  | Normal weight   | How much keeping normals (the shading) matters. Higher keeps the lighting; lower simplifies more freely. |
  | Sloppy fallback | When careful simplification cannot reach the target, merge nearby vertices regardless of how the mesh is built. Always gets smaller, but can stretch textures and leave holes. Off: the level reuses the previous one. |
  | Permissive      | Allow simplifying across seams where UVs or normals split, such as every edge of a flat-shaded model. Flat-shaded models need it; the UV and normal weights keep it in check. |
  | Lock border     | Keep vertices on the mesh's open edges in place, so pieces that meet along their edges (such as terrain tiles) stay sealed. |
  | Regularize      | Prefer evenly sized triangles, at some cost to accuracy. |
  | Prune           | Remove small, disconnected pieces while simplifying, when they are too small to matter. |

- A weak line saying where the values come from: `Project settings
  (assetc.toml)`, or `Set for this file (<file>.amod)` when the sidecar sets
  any.
- A **Use project settings** button, enabled when the sidecar sets a value:
  it removes them (and the file when nothing else is left in it).
- Changing a value writes it to the sidecar (creating it), saved like noise
  parameters (see [`noise.md`](noise.md#saving)); the file is then inspected
  again. If `assetc.toml` `[meshes]` or the sidecar is invalid, the tab shows
  the error instead.
- The new settings apply at the next build; check the result in the
  [compiled mesh preview](#compiled-mesh-preview).

## Compiled mesh preview

Selecting a compiled mesh (`*.amesh.bin` in the Output tree) shows it in the
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
  | Shaded      | `box-shaded`  | the whole mesh in light gray                             |
  | Wireframe   | `box`         | every triangle edge as a 1 px `TEXT` line, unlit, no faces |
  | Meshlets    | `squares-unite` | each meshlet in its own color                          |
  | Material ID | `image`       | each material slot in its own color; no material in gray |
  | Submeshes   | `boxes`       | each submesh in its own color                            |

- **Material**: each submesh uses its `.amat.bin` (looked up by the `MATERIAL`
  asset IDs in the output's [`assetc.aman.bin`](../spec/aman.bin.md)) as a
  physically based material: base color factor and texture, metallic and
  roughness factors with the ORM texture (G roughness, B metallic, R
  occlusion), normal texture, emissive factor × strength and texture, alpha
  mode and cutoff, double-sided. Normal scale and occlusion strength are not
  applied (the 3D view's material has no such settings). Textures are read from their
  `.ktx2` (level 0). A material or texture that cannot be found or read is
  replaced by the defaults (white, roughness 1, no texture), and a submesh
  without a material is light gray. The [color set](vertex-colors.md#compiled-meshes)
  chosen in the Vertex colors tab, if any, multiplies the base color, as in
  glTF; it also colors **Shaded** mode.
- **Colors** of the ID modes: index `i` (meshlet, material slot or submesh)
  has hue `i × 0.618034` (fractional part, the golden ratio, so neighbours
  differ), saturation 0.65, value 0.9. Triangles are lit as usual (the
  file's normals), so shapes stay readable.
- A second toolbar group, below the first, draws **bounds** over the mesh.
  Its buttons are toggles: any number of them can be on at once (none to
  start with), each click turns one on or off, and the choice lasts for the
  session:

  | Button          | Icon            | Draws                                                    |
  |-----------------|-----------------|----------------------------------------------------------|
  | Bounding box    | `scan`          | the mesh's box (`BOUNDS` `aabb_min`–`aabb_max`) in `ACCENT` |
  | Bounding sphere | `circle-dashed` | the mesh's sphere (`BOUNDS` `sphere_center`, `sphere_radius`) in `ACCENT` |
  | Submesh boxes   | `group`         | each submesh's box (its `SUBMESH` `bounds`) in the submesh's color |
  | Meshlet boxes   | `layout-grid`   | each meshlet's box, around the vertices of its triangles, in the meshlet's color |
  | Meshlet spheres | `circle-dot`    | each meshlet's bounding sphere (`MLBOUNDS` `center`, `radius`) in the meshlet's color |
  | Meshlet cones   | `cone`          | each meshlet's normal cone, in the meshlet's color: apex at its sphere's center, axis `cone_axis`, half-angle `asin(cone_cutoff)`, slant `radius` long; meshlets with `cone_cutoff ≥ 1` (no cone, never culled) draw none |
  | Locators        | `map-pin`       | the file's locators as markers, see [`locators.md`](locators.md#compiled-mesh-preview); on to start with, disabled without `LOCATORS` |

- Bounds are drawn in every display mode as unlit 1 px lines, hidden behind
  nearer faces like the mesh's own (switch to Wireframe to see inside).
  Boxes draw their 12 edges; spheres three great circles (in the XY, YZ and
  XZ planes, 48 segments each); cones their axis, the circle at their base
  (32 segments) and four lines from the apex to it. Colors are the ID modes'
  ones (see below), so a meshlet's bounds match its color in the Meshlets
  mode.
- The file's bounds are of LOD 0 and are drawn for every level. Meshlets
  exist only for LOD 0: with another level chosen, the three meshlet buttons
  are disabled (their choice is kept) and draw nothing.
- Left of the toolbar, level with its first button, a label shows the
  level of detail drawn: `LOD 0` to start, plain 13 px `TEXT` without a
  background, centered vertically on the 32 px button and ending 8 px from
  the toolbar.
- **Level of detail**: the view shows the level chosen in the
  [Levels of detail tab](#levels-of-detail-tab) (LOD 0, the full mesh, to
  start with), in every mode. Meshlets exist only for LOD 0: with another
  level chosen, the Meshlets mode shows it in light gray.
- A file that does not read shows its error as the canvas message.

## Compiled animation preview

Selecting a compiled animation (`*.aanim.bin` in the Output tree) plays it, to
check what the build kept: its clips, read from the file, on its companion
mesh, `P.amesh.bin` next to it (same folder and name). The [view bar](canvas.md#view-bar) offers **Play** (the
default) and **Chunks** (the file's data as tables, see
[Chunks view](#chunks-view)); the choice lasts for the session.

- **3D view**: the mesh as the [compiled mesh preview](#compiled-mesh-preview)
  shows LOD 0 (its display modes and overlays, centered and scaled by its
  bind pose's bounds), **posed** from the file's own data on every frame:
  - each joint's local transform comes from the clip's channels at the
    playhead: a `step` channel takes the key at or before it, a `linear`
    one interpolates between the keys around it (translation and scale
    linearly, rotation by spherical interpolation); before the first key
    and after the last the end key holds; a joint without a channel keeps
    its bind transform;
  - joints are placed through their parents, and each vertex follows its
    `SkinVertex` joints and weights through their `inverse_bind`
    matrices, as a game would skin it.
- Without a companion (no such file, a mesh without a skeleton, or one
  whose joint count is not the animation's `joint_count`), the 3D view
  shows a weak `No skinned P.amesh.bin to play on` (or `P.amesh.bin has 24 joints,
  the animation 25`); the timeline still shows.
- A **timeline** panel under the 3D view, as a model's
  [timeline](animation-events.md#timeline), read only:
  1. **Header**: Play / Pause, the time `0.420 / 1.250 s`, and a **Clip**
     dropdown (the clips by name, in file order).
  2. **Ruler** with the playhead; pressing or dragging on it moves the
     playhead (and pauses).
  3. **Track**: the clip's events as markers (a 10 px diamond in `TEXT`,
     the name in 11 px `TEXT_WEAK`), flashing `ACCENT` for 0.25 s when the
     playhead passes them while playing. Hovering a marker shows its name,
     time and properties (`key = value`, one per line).
  4. A weak line: `<n> channels · <m> keys · <k> events`.
- The first clip plays when the file is selected, looping; choosing a
  clip plays it from its start. `Space` (pointer over the panel), `,` /
  `.` and `Shift` + `,` / `.` work as on a model's timeline.

## Chunks view

The **Chunks** view shows the data inside a [chunked file](../spec/chunks.md)
(an `.amesh.bin`, `.aanim.bin`, `.afont.bin`, or [compiled data](inspector.md#compiled-data)),
chunk by chunk:

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
  | `COLORSET`| Color sets          | Name, Hash, Format (`RGBA8`), Source (`glTF`)          |
  | `COLORS`   | Vertex colors       | per vertex: one column per set, `r, g, b, a` (0–255)   |
  | `SKELETON` | Skeleton            | Parent (or `—`), Translation, Rotation, Scale          |
  | `LOCATORS` | Locators            | Name, Hash (`0x` + 16 hex digits), Joint (or `—`), Translation, Rotation, Scale, Tags (joined with `, `) |
  | `LOCTAGS`  | Locator tags        | Name, Hash                                             |
  | `STRINGS`  | Strings             | Offset, Bytes (16 per row, hex)                        |
  | `PROPS`    | Properties          | Key, Type, Value                                       |
  | `SHAPES`   | Shapes              | Name, Kind, Joint (or `—`), Type, Translation, Rotation, Size, Points, Properties |
  | `SHAPEPTS` | Hull points         | Point                                                  |
  | `SHAPEPRP` | Shape properties    | Key, Type, Value                                       |
  | `REGIONS`  | Regions             | Name, Tags, Triangles                                  |
  | `REGTAGS`  | Region tags         | Name, Hash                                             |
  | `TRIREGN`  | Triangle regions    | Regions (the names of its bits, `, `-joined, or `—`)   |
  | `PATHS`    | Paths               | Name, Kind, Joint (or `—`), Closed (`yes` / `no`), Tension, Alpha, Length, Points, Samples, Properties |
  | `PATHPTS`  | Path points         | Point                                                  |
  | `PATHSMP`  | Path samples        | Position, Distance, Tangent, U                         |
  | `PATHPRP`  | Path properties     | Key, Type, Value                                       |
  | `LOCNAMES` | Locator names       | Offset, Bytes (16 per row, hex)                        |
  | other      | Unknown chunk       | Offset, Bytes (16 per row, hex)                        |

  Normals and tangents are decoded from their octahedral encoding. Floats
  show three decimals; vectors as `x, y, z`.

## Levels of detail tab

The Inspector's **Levels of detail** tab (`bring-to-front`) is shown for a
compiled mesh (`*.amesh.bin` in the Output tree) with reduced levels (a
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
plays and lists each one's [events](animation-events.md).

- One card per animation, in file order, like the
  [Gradients tab](gradients.md#gradients-tab)'s cards but never deleted or
  reordered: an accordion, one unfolded at a time. Header: the chevron, the
  animation's name (`Animation <n>` counting from 1 when it has none) and,
  right-aligned and weak, `<n> events · <duration>` (`1 event`; only the
  duration without events; three decimals, e.g. `1.250 s`).
- The duration is the largest `max` of the animation's sampler input
  accessors (glTF requires them), read with the other
  [Info rows](#inspector).
- **Unfolding** a card chooses its animation: it plays from its start,
  looping, at normal speed; the orbit camera keeps working. **Folding** it
  (all cards folded, the default) is the **rest pose**: the model as
  loaded, without animation. Switching to another animation or back to the
  rest pose first returns the model to its rest pose.
- The unfolded card's body is its [event list](animation-events.md#event-list).
- The choice is remembered per model (by path) for the session, and kept
  when the model is reloaded.
- The model is fitted once, in its rest pose; animations can move it out of
  that box.
- While the tab is shown, a [timeline](animation-events.md#timeline) under
  the 3D view plays, pauses and scrubs the chosen animation and places its
  events.

## Build

Every `*.gltf` / `*.glb` (any case) in `input` compiles into several assets.
With `P` the input-relative path without the extension (e.g. `models/Fox`):

| Output (in `output`)        | Asset kind | Asset path        | When                                   |
|-----------------------------|------------|-------------------|----------------------------------------|
| `P.amesh.bin`                   | mesh       | `P.amesh.bin`         | always ([spec](../spec/amesh.bin.md))       |
| `P/<material>.amat.bin`         | material   | `P/<material>.amat.bin` | per material a submesh uses ([spec](../spec/amat.bin.md)) |
| `P/<image>.ktx2`            | texture    | `P/<image>`       | per image used as base color, normal or emissive texture |
| `P/<material>.orm.ktx2`     | texture    | `P/<material>.orm` | per material with an occlusion or metallic-roughness texture |
| `P/<set>.colors.ktx2`       | texture    | `P/<set>.colors`  | per color set exported as Texture + UV buffer or Both, see [`vertex-colors.md`](vertex-colors.md#export) |
| `P.aanim.bin`                   | animation  | `P.aanim.bin`         | when the mesh is skinned and the file has animations ([spec](../spec/aanim.bin.md)) |

The `.amesh.bin` also carries the model's [properties](properties.md),
[shapes](shapes.md), [regions](regions.md) and [paths](paths.md), and its
[locators](locators.md#build): its
sidecar's, its file's `SOCKET_*` nodes and presets' (see
[Inherited locators](locators.md#inherited-locators)).

Like other assets, each gets the FNV-1a-64 of its asset path as ID and a
manifest entry; collisions are build errors as usual. The build prints one
line per model, e.g.
`models/Fox.glb -> models/Fox.amesh.bin (1728 vertices, 576 triangles, 1 submesh, 12 meshlets, 24 joints, 3 clips, 1 material, 1 texture)`.

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
  Weights are renormalized to sum 1. `COLOR_n` become
  [color sets](vertex-colors.md).
- Per submesh: the vertex order is optimized for the GPU's vertex cache, then
  the submesh is split into **meshlets** with their bounds (cone weight
  `meshlet_cone_weight`, 0.25), of at most `meshlet_max_vertices` vertices and
  `meshlet_max_triangles` triangles: 64 and 124 unless
  [`[meshes]`](../spec/assetc.toml.md#meshes) in `assetc.toml` sets other
  defaults or overrides them for the model, or the model's
  [`.amod`](../spec/amod.md#mesh) sidecar (edited in the
  [Parameters tab](#parameters-tab)) sets its own. The limits used are
  stored in the `.amesh.bin` `DESC`, and simplified into **levels of detail**
  at `lod_ratios` of its triangles (two, at about 50% and 25%; none writes
  no `LODIDX` / `LODTABLE`), all with
  [meshoptimizer](https://github.com/zeux/meshoptimizer).
- **Simplification keeps UVs and normals in mind.** A level of detail only
  drops vertices and reuses the remaining ones (with their own UV, normal and
  tangent), so its quality depends on which vertices survive.
  Simplification therefore weighs the vertices' UVs (`lod_uv_weight`, 1)
  and normals (`lod_normal_weight`, 0.5) next to their positions. It may
  also collapse across attribute seams (`lod_permissive`) when the attribute
  error stays within the limit; seams are vertices sharing a position but
  not their UV or normal, as on every edge of a flat-shaded mesh. Without that, flat-shaded meshes could not be simplified at all.
  It starts with a 5% error limit (`lod_error`) and raises it by 5%
  (`lod_error_step`) up to three times (`lod_attempts`) while it makes no
  progress. Only then does it fall back to sloppy (topology-ignoring)
  simplification (`lod_sloppy_fallback`), which merges nearby vertices from
  different faces and so can stretch textures.
- A file without any triangle primitive fails (`no triangles in the default
  scene`).

### Skinning and animations

- The first skin of the file is used (a later skin's primitives are bound to
  it as if rigid); its joints become the `.amesh.bin` skeleton, with the joints'
  bind-pose transforms and inverse bind matrices (identity when missing).
- With a skin and animations, `P.aanim.bin` holds every clip, keeping channels
  that target the skin's joints (translation, rotation, scale). Animations
  of a file without a skin are not compiled.

### Materials and textures

Each used glTF material becomes an `.amat.bin` with:

| `.amat.bin` field          | From glTF                                                   |
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
