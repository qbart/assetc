# Texture paint

A **paint** is a texture painted straight onto a glTF model in the 3D view,
with a brush. Unlike [regions](regions.md), which mark whole triangles, a
paint is an image laid over the model through its unwrap (its first UV set,
`TEXCOORD_0`), so a stroke can end anywhere on a triangle and fade out.

A paint is either:

- **Color**: an sRGB color with alpha per texel (decals, dirt, hand-painted
  detail), or
- **Channels**: up to four independent masks, R, G, B and A, each painted on
  its own (wetness in R, damage in G, ...). How many there are (R, RG, RGB
  or RGBA) is chosen per paint and sets the texture it compiles to, or
- **Weights**: the same masks painted as weights: the brush does not set a
  value, it **builds weight up** the longer it is held over a spot (and
  takes it away again), and the model shows the weight as a
  [heatmap](#heatmap) that can be read back as a number. For blend
  weights, wind or cloth influence, wear, density: anything where the
  amount matters, not just where it is, or
- **Distance**: the same masks again, not painted but **measured**: a
  point is picked on the model and a channel is filled with how far every
  spot of the model is from it, [along the surface or in a straight
  line](#distance-map). For showing distances on an object, and for
  shaders that treat what is farther from a point differently (a tree
  swaying more toward its tips, a dissolve spreading from a wound).

Paints are an *enrichment* like [locators](locators.md): their settings are
kept in the model's [`.amod`](../spec/amod.md#paints) sidecar, their texels in
an [`.apaint`](../spec/apaint.md) file next to the model, they are edited in
the Inspector's [Texture paint tab](#texture-paint-tab) and painted in the
[3D view](#painting), and each compiles to a [texture](#build) a game samples
with the mesh's `uv`.

## Paint

| Field    | Meaning |
|----------|---------|
| Name     | Non-empty, unique among the model's paints, made of ASCII letters, digits, `_` and `-` (it names files). |
| Mode     | **Color**, **Channels**, **Weights** or **Distance**; Color to start with. |
| Size     | Width and height in texels: 256, 512, 1024, 2048 or 4096; 1024 to start with. |
| Channels | Every mode but Color: **R**, **RG**, **RGB** or **RGBA** (to start with): which channels are painted and exported. |
| Measure  | Distance mode: **Along the surface** (to start with) or **Straight line**. |
| Range    | Distance mode: the distance, in the model's units, that is stored as 1; `0` (to start with) is the **farthest** spot's distance. |
| Points   | Distance mode: the point each channel is measured from, at most one per channel; none to start with. |

- Texels are RGBA, 8 bits per channel. An unpainted texel is `0, 0, 0, 0`
  (transparent black in Color mode, every channel 0 in Channels and Weights
  modes).
- Color mode: a texel is an sRGB color with straight (not premultiplied)
  alpha.
- Channels mode: each channel is a value 0–1 (0–255), linear. Channels left
  out by Channels (B and A for RG) are kept in the file but neither painted,
  shown nor exported.
- Weights mode: the same texels as Channels mode, each channel a weight
  0–1. Only the brush and how the model is shown differ, so a paint can go
  back and forth between Channels and Weights freely.
- Distance mode: the same texels once more, each channel a distance 0–1
  (0 at the point, 1 at Range). Switching to Weights and back keeps them,
  so a distance map can be touched up with the weights brush, and one paint
  can hold a distance in R and painted weights in G.
- Changing Mode keeps the texels as they are (the same bytes, read the other
  way). Changing Size resamples the texels to the new size (bilinear, per
  channel).
- The texture maps onto the model through `TEXCOORD_0`, top-left origin (as
  glTF has it); coordinates outside 0–1 repeat. Triangles that share UV
  space (mirrored or stacked islands) share their texels too: painting one
  paints the others.

## Texture paint tab

The Inspector's **Texture paint** tab (`paintbrush`) is shown for a model in
the Input tree. It works like the [Regions tab](regions.md#regions-tab):

- **Add paint** (`plus` and text) adds a paint named `paint`, or
  `paint_<n>`, in Color mode at 1024, and unfolds it. Disabled (hover:
  `The model has no TEXCOORD_0 to paint through`) for a model whose
  primitives have no `TEXCOORD_0`.
- One card per paint, an accordion (the unfolded one is the *chosen*
  paint). Header: the chevron, the name, and, unfolded only, **Delete**
  (`trash-2`), which also deletes its `.apaint` file. Folded, a weak label
  of its mode and size (`Color 1024`, `RGB 512`, `Weights R 512`,
    `Distance R 512`).
- With no paints, a weak `No paints` line.
- Body:
  - **Name** (text; an empty, taken or not allowed name gets the old one
    back). Renaming a paint renames its `.apaint` file.
  - **Mode**: dropdown Color / Channels / Weights / Distance.
  - **Size**: dropdown 256 / 512 / 1024 / 2048 / 4096.
  - Every mode but Color: **Channels**: dropdown R / RG / RGB / RGBA.
  - Distance mode has no brush; it shows the fields [below](#distance-tab)
    in place of the rest of this list, down to Clear.
  - **Paint** (`paintbrush` and text) and **Erase** (`eraser` and text)
    toggle buttons, at most one on, as the regions' are.
  - The **brush** (for the session, shared by all paints):
    - **Size**: slider 2–256 px, the brush's radius on screen; 24 to start
      with.
    - **Strength**: slider 0–1, how far one stroke moves texels toward the
      brush's target (Weights mode: how fast weight
      [builds up](#building-weight-up)); 1 to start with.
    - **Hardness**: slider 0–1, the part of the radius painted at full
      strength before it fades out; 0.5 to start with.
  - Color mode: **Color**, the [color picker](gradients.md#selected-stop)
    (sRGB with alpha); white (`#ffffff`) to start with, for the session.
  - Channels mode:
    - **Channel**: one toggle button per channel Channels allows (`R`, `G`,
      `B`, `A`), exactly one on: the channel painted and shown. R to start
      with; narrowing Channels past the chosen one chooses R.
    - **Value**: slider 0–1, what painting moves the channel toward; 1 to
      start with.
  - Weights mode:
    - **Channel**, as in Channels mode: the weight painted and shown.
    - **Value**: the same slider, here the most painting builds up to (hover:
      `The most painting builds up to`); texels above it are left alone.
    - The **scale**: a bar of the [heatmap](#heatmap)'s colors from 0 (left)
      to 1 (right), 12 px high and as wide as the sliders' rows, with `0`,
      `0.5` and `1` below it in `TEXT_WEAK` and a `TEXT` tick at Value.
  - **Clear** (button, enabled while any texel is painted): every texel
    back to unpainted, and the Points forgotten.
  - A weak hint: `Drag on the model to paint or erase. Shift + drag does
    the other. Right drag orbits.` Weights mode: `Hold or drag on the model
    to build weight up; go slower or over it again for more. Shift + drag
    takes it away. Right drag orbits.`
- Edits of the fields are saved like the locators'. A stroke is saved (the
  `.apaint` file written) when the pointer is released; Clear and a new
  Size save at once.

### Distance tab

In Distance mode the card's body, after Channels, is:

- **Pick point** (`crosshair` and text): a toggle button; while on, the
  point is [picked](#picking-the-point) on the model.
- **Channel**: as in Channels mode: the channel that is measured and shown.
- **Measure**: dropdown Along the surface / Straight line.
- **Range**: a number, 0 or more, in the model's units; `0` reads
  `Farthest`. Hover: `The distance stored as 1; Farthest: the farthest spot
  of the model`.
- The **scale**, as in Weights mode but without a tick, and under it, once
  the chosen channel has been measured, a weak `Farthest spot: <d>` (in the
  model's units, three decimals). It is saved with the point, so it shows
  again when the model is opened later, and the build writes it into the
  mesh as the channel's range while Range is `Farthest`.
- **Recompute** (button, enabled while any channel has a point): measures
  every channel that has a point again, for a model that has changed or is
  posed differently.
- **Clear**, as above.
- A weak hint: `Click or drag on the model to set the point distances are
  measured from. Right drag orbits.`

A new Measure or Range measures every channel that has a point again, at
once.

## 3D view

While the Texture paint tab shows and a paint is chosen, the model is drawn
with that paint instead of its materials, following its pose and animation.
This takes the place of the [regions' colors](regions.md#3d-view) and a
[color set](vertex-colors.md#3d-view) while it lasts:

- **Color** mode: lit, a rough white material whose base color is the
  paint laid over gray (`#808080`) by its alpha.
- **Channels** mode: unlit, the chosen channel alone: **red** for R,
  **green** for G, **blue** for B and **white** for A, as bright as the
  channel's value. Unpainted (0) is **black**.
- **Weights** mode: lit, as Color mode is (so the model's shape still
  reads; the weight is in the hue, not the brightness), the chosen channel
  as a [heatmap](#heatmap).
- **Distance** mode: lit, the chosen channel as a [heatmap](#heatmap) on
  which 0 is blue too (the point itself is at 0), and the channel's point
  marked: a `TEXT` ring of 6 px with a dot in its middle, drawn over the
  model.
- Primitives without `TEXCOORD_0` are drawn gray (Color, Weights, Distance)
  or black (Channels); primitives with morph targets keep their materials.

### Heatmap

A weight is shown as a color, the same in the 3D view, on the tab's scale
and on the brush:

| Weight | Color |
|-------:|-------|
| 0 (no weight at all) | gray, `#808080`, as an unpainted Color paint |
| just above 0 | blue, `#0000ff` |
| 0.25 | cyan, `#00ffff` |
| 0.5 | green, `#00ff00` |
| 0.75 | yellow, `#ffff00` |
| 1 | red, `#ff0000` |

Between two rows the color is blended linearly (per sRGB channel), from blue
at 1/255. Gray is for texels at exactly 0 only, so the faintest weight shows
as blue and the reach of a soft brush can be seen.

### Painting

While the chosen paint's **Paint** or **Erase** is on (only the chosen paint
paints; choosing another paint or leaving the tab turns them off):

- **Left drag** on the model paints. The **brush** is a sphere around the
  point the pointer's ray hits, its radius the brush's Size on screen at that
  point's depth. Between two frames' hits the stroke is filled in with dabs
  every quarter of the radius.
- Every texel of a triangle facing the camera whose spot on the posed model
  lies inside the sphere is painted, with weight `w = Strength × f(d / r)`,
  `d` the spot's distance from the center, `r` the radius and `f` 1 up to
  Hardness, then falling smoothly (smoothstep) to 0 at 1. Texels within one
  texel outside a triangle's UV edges count as its own, so seams do not show
  gaps when sampled bilinearly.
- Within a stroke a texel keeps the largest weight it got, so going over it
  again in the same stroke does not build up: the texel is its value when
  the stroke began moved toward the target by `w`. A new stroke starts from
  there.
- Targets:
  - Color, Paint: the brush color (all four channels).
  - Color, Erase: unpainted, `0, 0, 0, 0`.
  - Channels, Paint: Value in the chosen channel; the other channels are
    left alone.
  - Channels, Erase: 0 in the chosen channel.
- **Shift + left drag** does the other one (erases while painting, paints
  while erasing). Clicking paints a single dab.
- Weights mode paints differently, see [below](#building-weight-up).
- The brush is shown at the pointer while it is over the canvas: a circle of
  its Size, 1 px, in `TEXT` for erasing and the brush color (Color), the
  channel's color (Channels) or Value's [heatmap](#heatmap) color (Weights)
  for painting (following Shift); with a dashed inner circle at Hardness
  when it is below 1.
- **Right drag** orbits the camera; the wheel zooms. Locator markers cannot
  be clicked and the gizmo is not drawn.
- `Escape` (pointer over the canvas) turns Paint and Erase off.

### Building weight up

In Weights mode the brush works like an airbrush: how much weight a texel
gets depends on how long the brush is over it.

- While the left button is held on the model, **Paint** adds to the chosen
  channel of every texel the brush reaches, at `4 × Strength × f(d / r)` per
  second (`f` and the texels reached as above): at Strength 1 the middle of
  a brush held still goes from 0 to 1 in a quarter of a second, at 0.1 in
  two and a half seconds. The other channels are left alone.
- It builds up **within a stroke and across strokes**: holding still,
  dragging slowly or going over a spot again all add more; a quick pass adds
  little.
- Painting stops at **Value**: a texel is never raised above it, and one
  already above it is left as it is.
- **Erase** takes weight away at the same rate, down to 0.
- A frame's weight is shared by the dabs that fill the stroke in since the
  last frame, so the result does not depend on the frame rate or on how the
  stroke is filled in. A frame counts for at most a tenth of a second.
- Clicking adds one frame's worth.
- While Paint or Erase is on and the pointer is over the model, the
  **weight under the pointer** (the chosen channel of the texel the
  pointer's ray hits, `0.00`–`1.00`) is written beside the brush, to its
  lower right, in `TEXT` on a `BG_PANEL` plate; nothing off the model.

## Distance map

A distance map is a channel filled with distances from one **point** on the
model's surface: `value = min(distance / Range, 1)`, so the point is 0
(blue) and everything at Range or beyond is 1 (red). With Range `0` the
farthest spot of the model is 1.

### Measure

- **Along the surface**: the length of the shortest way from the point over
  the mesh, the way an ant would walk it. On a tree this runs down the
  branch the point is on, through the trunk and up the other branches, so
  two twigs that almost touch are still far apart, and a shader gets what
  it needs to move whole branches together.
  - The way goes over the faces: straight across each triangle between
    its corners and a few points along its edges (three per edge on a model
    of up to 20,000 triangles, two up to 60,000, one above). Vertices at
    the same position are one (a UV seam or a hard edge does not stop it),
    in every primitive and mesh of the model alike.
  - Parts that do not touch (leaf cards, a branch stuck into the trunk
    without being joined to it, separate meshes) are **bridged**: each is
    joined to its nearest neighbor by the shortest gap between two of
    their vertices, and the gap's length counts as part of the way. So
    nothing is unreachable, and a leaf continues the branch nearest to it.
  - A texel's distance is the way to the nearest corner or edge point of
    its triangle plus the straight rest to the texel's spot; in the
    triangle holding the point it is straight from the point.
  - Crossing triangles only at those points makes the way a little longer
    than the true shortest one (a few percent at most).
- **Straight line**: the direct distance through space from the point to
  each texel's spot on the model, whatever lies between.

Both are measured on the model **as it is posed** in the 3D view at that
moment, in the model's units (for the rest pose, measure with no animation
playing). Along the surface a pose changes little; in a straight line it
changes everything.

The whole channel is filled: texels no triangle covers become 0, and
texels within one texel outside a triangle's UV edges count as its own, as
for painting. Where triangles share texels the nearest wins.

### Picking the point

While **Pick point** is on (leaving the tab or choosing another paint turns
it off):

- **Left click or drag** on the model puts the chosen channel's point where
  the pointer's ray hits and fills the channel again each frame, so the map
  follows the pointer. Releasing saves the `.apaint` file and the point
  (the triangle hit and the place in it, so it stays on the surface when
  the model is posed).
- The pointer shows a `TEXT` crosshair, and beside it the **value under the
  pointer** (`0.00`–`1.00`) as in [Weights mode](#building-weight-up).
- **Right drag** orbits; `Escape` turns Pick point off.

A point whose triangle the model no longer has is not drawn and is skipped
by Recompute.

## Build

Each paint compiles to a texture `P/<name>.paint.ktx2` (asset path
`P/<name>.paint`, `P` the model's input-relative path without its
extension), built with the model's other textures, with a full mip chain:

| Mode     | Channels | Written as (format `rgba`) | `assetc.usage` | Texels |
|----------|----------|--------------------|----------------|--------|
| Color    |          | `R8G8B8A8_SRGB`    | `color`        | RGBA as painted |
| Channels | R        | `R8_UNORM`         | `data`         | R |
| Channels | RG       | `R8G8_UNORM`       | `data`         | R, G |
| Channels | RGB      | `R8G8B8A8_UNORM`   | `data`         | R, G, B; A is 255 |
| Channels | RGBA     | `R8G8B8A8_UNORM`   | `data`         | R, G, B, A |
| Weights  | any      | as Channels        | `data`         | as Channels |
| Distance | any      | as Channels        | `data`         | as Channels |

Other [texture formats](textures.md#format) encode these: an RGBA texture
as any texture (`BC7`, ASTC, UASTC, or `R8G8B8` for `rgb`), an R texture as
`BC4` (R in R, G, B for ASTC and UASTC), an RG texture as `BC5` (G in alpha
for ASTC and UASTC). `rgb` writes R and RG textures as `rgba` does. (See
[`ktx2.md`](../spec/ktx2.md#formats).)

- Mip levels are computed as for [textures](textures.md) in the format's
  color space (Color: sRGB), from all four channels, before the channels
  are dropped.
- The mesh's geometry is unchanged: a game samples the texture with the
  mesh's `uv` (`TEXCOORD_0`), and finds it by the asset path.
- The mesh lists its paints in a [`PAINTS`](../spec/amesh.bin.md#texture-paints)
  chunk: each paint's name, its texture's asset ID, mode, channels and size,
  and for a distance map its measure and, per channel, the **range**: the
  distance a value of 1 stands for, in the mesh's units, so a shader gets
  the distance back as `value × range`. The range is the paint's Range, or
  with Range `Farthest` the farthest distance the channel was last measured
  with (saved with its point as `farthest`; `0` when it never was). The
  [SDK](sdk.md) reads it as `Mesh::paints`.
- In the Output tree, the compiled mesh's Info tab has a **Paints** row (the
  paints' names, or `none`) and its Chunks tab lists `PAINTS` like any
  chunk: one row per paint with its name, hash, texture, mode, channels,
  size, measure and range. The model's own Info tab (Input tree) has a
  **Paints** row too: how many, or `none`.
- A paint without an `.apaint` file compiles to an unpainted texture of its
  Size.
- The model's build reads the `.apaint` files, so painting rebuilds it.
- A distance map is built from its `.apaint` file like any paint: the
  distances are measured in the editor, not by the build.
- Failures (`<file>.amod: paint "<name>": ...`):
  - a model without `TEXCOORD_0` on any triangle primitive:
    `the model has no TEXCOORD_0`;
  - an `.apaint` file that cannot be read: `<file name>: <problem>`;
  - an `.apaint` image of the wrong size: `<file name> is <w> × <h>, not
    <size> × <size>`.

## Files

A model's `.apaint` files go with it: moving, renaming or deleting the model
in the [file browser](file-operations.md#companions) moves, renames or
deletes them too. They are dimmed in the file browser like sidecars, and are
not sources of their own (the build counts them as skipped).

## Not yet

- Painting through another UV set than `TEXCOORD_0`, and an automatic
  unwrap for models without one (it needs vertices split along seams).
- Undo, layers, a fill tool, brushes from images, and painting in a 2D view
  of the texture.
- Distance maps measured again by the build when the model changes, from
  the rest pose whatever is playing, from several points per channel, or
  stored the other way round (1 at the point).
- A pen's pressure for the Weights brush (the window library does not
  report it), weights kept normalized across channels (summing to 1), and
  smoothing or blurring weights.
- Showing a paint in the compiled mesh preview.
