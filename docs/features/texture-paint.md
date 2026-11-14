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
  amount matters, not just where it is.

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
| Mode     | **Color**, **Channels** or **Weights**; Color to start with. |
| Size     | Width and height in texels: 256, 512, 1024, 2048 or 4096; 1024 to start with. |
| Channels | Channels and Weights modes: **R**, **RG**, **RGB** or **RGBA** (to start with): which channels are painted and exported. |

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
  of its mode and size (`Color 1024`, `RGB 512`, `Weights R 512`).
- With no paints, a weak `No paints` line.
- Body:
  - **Name** (text; an empty, taken or not allowed name gets the old one
    back). Renaming a paint renames its `.apaint` file.
  - **Mode**: dropdown Color / Channels / Weights.
  - **Size**: dropdown 256 / 512 / 1024 / 2048 / 4096.
  - Channels and Weights modes: **Channels**: dropdown R / RG / RGB / RGBA.
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
    back to unpainted.
  - A weak hint: `Drag on the model to paint or erase. Shift + drag does
    the other. Right drag orbits.` Weights mode: `Hold or drag on the model
    to build weight up; go slower or over it again for more. Shift + drag
    takes it away. Right drag orbits.`
- Edits of the fields are saved like the locators'. A stroke is saved (the
  `.apaint` file written) when the pointer is released; Clear and a new
  Size save at once.

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
- Primitives without `TEXCOORD_0` are drawn gray (Color, Weights) or black
  (Channels); primitives with morph targets keep their materials.

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

## Build

Each paint compiles to a texture `P/<name>.paint.ktx2` (asset path
`P/<name>.paint`, `P` the model's input-relative path without its
extension), built with the model's other textures, with a full mip chain:

| Mode     | Channels | Format             | `assetc.usage` | Texels |
|----------|----------|--------------------|----------------|--------|
| Color    |          | `R8G8B8A8_SRGB`    | `color`        | RGBA as painted |
| Channels | R        | `R8_UNORM`         | `data`         | R |
| Channels | RG       | `R8G8_UNORM`       | `data`         | R, G |
| Channels | RGB      | `R8G8B8A8_UNORM`   | `data`         | R, G, B; A is 255 |
| Channels | RGBA     | `R8G8B8A8_UNORM`   | `data`         | R, G, B, A |
| Weights  | any      | as Channels        | `data`         | as Channels |

(See [`ktx2.md`](../spec/ktx2.md) for the formats; three-channel formats are
not written, few GPUs sample them.)

- Mip levels are computed as for [textures](textures.md) in the format's
  color space (Color: sRGB), from all four channels, before the channels
  are dropped.
- The mesh is unchanged: a game samples the texture with the mesh's `uv`
  (`TEXCOORD_0`), and finds it by the asset path.
- A paint without an `.apaint` file compiles to an unpainted texture of its
  Size.
- The model's build reads the `.apaint` files, so painting rebuilds it.
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
- A pen's pressure for the Weights brush (the window library does not
  report it), weights kept normalized across channels (summing to 1), and
  smoothing or blurring weights.
- Showing a paint in the compiled mesh preview, and a model Info row for
  paints.
