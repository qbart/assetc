# Vertex colors

A glTF model's vertex colors are compiled into its
[`.amesh`](../spec/amesh.md#color-sets) as **color sets**: named buffers of
one RGBA color per vertex, alongside `VERTICES`. A mesh can have several
sets; a game picks one by the FNV-1a-64 hash of its name.

Sets come from two places:

- **glTF sets**: the model's `COLOR_n` attributes, see [From glTF](#from-gltf).
  Their names can be changed, see [Renaming](#renaming).
- **Authored sets**: made in assetc from the model's shape (a solid color,
  or a gradient laid over the model), see [Authored sets](#authored-sets).

Both are edited in the Inspector's [Vertex colors tab](#vertex-colors-tab)
and kept in the model's [`.amod`](../spec/amod.md#color_names) sidecar.

## From glTF

- Each `COLOR_n` attribute (`n` = 0, 1, …) on any triangle primitive of
  the model becomes the set named `COLOR_n` (or its new name). Sets are
  written in order of `n`.
- Values are stored as **RGBA, 8 bits per channel, unsigned normalized,
  linear** (as glTF defines them; not sRGB-encoded): floats are clamped to
  0–1 and rounded to the nearest of 255 steps, 16-bit values rounded to 8
  bits, and RGB colors (`VEC3`) get alpha 255.
- A primitive without a set that another primitive has gets white
  (`255, 255, 255, 255`) for its vertices in that set, which leaves its
  material unchanged.
- Vertices are written in the same order as `VERTICES`, so vertex `v` of
  every set belongs to vertex `v` of the mesh. A primitive whose normals
  are generated flat (no `NORMAL`) has its vertices split per triangle;
  each copy keeps its source vertex's colors.
- Models without `COLOR_n` and without authored sets get no color set
  chunks.

## Renaming

A glTF set's name is `COLOR_n` until it is given another one. The new name
is kept in the sidecar's `[color_names]` table, by attribute
(`COLOR_0 = "tint"`); a name set back to `COLOR_n` (or emptied) drops the
entry. Only the name changes: the colors, and the set's place among the
others, stay.

## Authored sets

An authored set gives every vertex a color computed from its position: in
model space, as the vertex is compiled (a skinned mesh's vertices at the bind
pose).

| Field  | Meaning |
|--------|---------|
| Name   | Non-empty, unique among all of the model's sets (glTF ones by their names). |
| Fill   | **Solid**, **Linear** or **Radial**; Linear to start with. |
| Color  | Solid: the one color, sRGB with alpha. |
| From   | Linear and Radial: a point in model space. Linear: where the gradient starts; Radial: its center. |
| To     | Linear and Radial: a point in model space. Linear: where the gradient ends; Radial: a point on its edge, so the radius is the distance from From. |
| Stops  | Linear and Radial: the gradient, as the [gradient editor](gradients.md#gradient-editor) edits it: at least two stops, each a position 0–1 and an sRGB color with alpha. |

Each vertex at `p` gets position `t` along the gradient:

- **Linear**: `t = (p − From) · (To − From) / |To − From|²`, the vertex
  projected onto the line from From to To.
- **Radial**: `t = |p − From| / |To − From|`.
- `t` is clamped to 0–1 (the first stop's color before it, the last's past
  it); From and To at the same point give `t = 0`.

Its color is the gradient's at `t` as a `.gradient` gradient without
dithering computes it ([blending](../spec/gradient.md#image): sRGB values,
premultiplied alpha), then stored like a glTF set: R, G and B converted from
sRGB to linear and rounded to 8 bits, alpha as it is. A Solid set's color is
stored the same way.

Authored sets are written after the glTF sets, in their order in the
sidecar.

## Vertex colors tab

The Inspector's **Vertex colors** tab (`blend`) is shown for a model in the
Input tree:

- **Add color set** (`plus` and text) adds an authored set named `colors`
  (or `colors_<n>`), Linear from the bottom to the top of the model (From
  and To at the middle of the model's bounding box in X and Z, at its lowest
  and highest Y), black (`#000000`) at 0 to white (`#ffffff`) at 1, and
  unfolds it.
- One card per set, glTF sets first, then authored ones; an accordion (one
  unfolded at a time; the unfolded one is the *chosen* set). Header: the
  chevron, a 48 × 12 px preview of the set (3 px corners, over a
  checkerboard): the gradient, first stop on the left, the Solid color, or
  for a glTF set a weak `glTF` label instead; then the name and, unfolded,
  **Delete** for authored sets.
- With no sets at all, a weak `No color sets` line.
- A **glTF set**'s body:
  - **Name** (text): an empty name gives the set `COLOR_n` back; a name
    another set has gets the old one back.
  - A weak line: `From the glTF's COLOR_n`.
- An **authored set**'s body:
  - **Name** (text; an empty or taken name gets the old one back).
  - **Fill**: dropdown Solid / Linear / Radial.
  - Solid: **Color**, the [color picker](gradients.md#selected-stop).
  - Linear and Radial:
    - **Gizmo** (the locators' group with **Move** only): moves the chosen
      end, From or To.
    - **From** and **To** (Radial: **Center** and **Edge**): X, Y, Z drag
      fields (three decimals); clicking a row's label chooses that end (the
      chosen one's label in `ACCENT`). From is chosen to start with.
    - **Fit**: buttons **X**, **Y** and **Z** lay the gradient along that
      axis of the model's bounding box: Linear from its lowest to its
      highest side through the box's middle; Radial centered on the box,
      reaching half its size along the axis.
    - **Swap** (button): From and To trade places.
    - The [gradient editor](gradients.md#gradient-editor) (bar, handles and
      the selected stop's color picker).
- Edits are saved like the locators'.

## 3D view

While the Vertex colors tab shows and a set is chosen, the model's preview
draws the model in that set's colors instead of its materials (lit, a rough
white material times the colors; alpha is not shown), following its
animation. Primitives a glTF set misses are white. This takes the place of
the [regions' colors](regions.md#3d-view) while it lasts.

For a chosen Linear or Radial set, From and To are drawn over the model (on
top of it, like path points) in model space:

- A 1 px `TEXT` line from From to To; for Radial, also a circle of the
  radius around From, facing the camera.
- Each end as a 5 px dot in `TEXT`, the chosen one 7 px with a 2 px `ACCENT`
  ring. Clicking within 8 px of an end chooses it.
- The [gizmo](locators.md#3d-view) in Move mode (and `W`) acts on the chosen
  end, in model axes. `E` does nothing there.

The 3D view shows the first instance's colors where one glTF mesh is drawn
by several nodes (the build computes each instance's own).

## Build

- A `[color_names]` entry for an attribute the model does not have fails the
  build: `<file>.amod: color_names: no COLOR_<n> in the model`.
- Two sets with the same name (glTF sets by their new names) fail it:
  `<file>.amod: color set "<name>": name used twice`.
- Each set's `COLORSET` record says where it came from in `source`: `0`
  glTF, `1` assetc.

## Inspector Info

- A model in the Input tree: a **Color sets** row after Textures: the
  names of the `COLOR_n` attributes its primitives have (`COLOR_0, COLOR_1`),
  or `none`.
- A compiled mesh: a **Color sets** row after Skin: the set names, or
  `none`.
- The [Chunks view](models.md#chunks-view) shows `COLORSET` (with
  `source` as `glTF` or `assetc`) and `COLORS`.

## Previews

- The model's [preview](models.md#preview) shows `COLOR_0` as Bevy's glTF
  loader does: multiplied with the base color.
- The [compiled mesh preview](models.md#compiled-mesh-preview)'s
  **Material** mode does the same with the first glTF set (whatever its
  name) when the mesh has one.

## Not yet

- More fills: baked ambient occlusion, curvature, noise, region colors
  (these need vertices split along region edges).
- Removing a glTF set, or reordering sets.
- A compiled mesh preview mode showing any set on its own, and wider
  formats (16-bit or float channels; the record's `format` leaves room for
  them).
