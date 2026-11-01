# Models

glTF 2.0 files in the Input tree can be inspected and previewed in 3D. They
are not compiled yet: the build ignores them, so they have no output and no
assets.

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
- Auto-rotation, the orbit camera, its controls and the lighting work as for
  textured shapes ([Scene](canvas.md#scene), [Camera](canvas.md#camera)).
- **Loading** runs in the background: the canvas stays empty and the status
  bar shows the [loader](gui.md#layout) until the model is shown. If loading
  fails the canvas shows `Could not load the model: <error>`.
- Each new inspection of the file (selecting it, a rescan) reloads it from
  disk, so changes made in another program show up.

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
  auto-rotation and the orbit camera keep working. Switching to another
  animation or back to Rest pose first returns the model to its rest pose.
- The choice is remembered per model (by path) for the session, and kept
  when the model is reloaded.
- The model is fitted once, in its rest pose; animations can move it out of
  that box.
