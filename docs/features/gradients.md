# Gradients

Color gradients stored as stops in `.gradient` files (format and exact
rendering: [`docs/spec/gradient.md`](../spec/gradient.md)). One file holds
several gradients, each drawn as a vertical gradient (first stop at the
bottom) in one **cell** of a grid in one image (a gradient lookup atlas). Like `.noise` files they are texture sources: the
build turns them into `.ktx2`, the canvas previews them (2D and 3D), and they
can fill material slots.

## Creating

In the Input tree's context menu, **New Gradient** (a plain item after New
Pattern, see [`file-operations.md`](file-operations.md)) creates
`gradient.gradient` (`gradient 2.gradient`, … if taken) with a 256 × 256
image of 16 × 64 cells (16 columns × 4 rows, room for 64 gradients) and one
gradient named `Gradient 1`, black (`#000000`) at
0 to white (`#ffffff`) at 1, no dithering. It then selects it and starts
renaming it.

The file tree shows `.gradient` files with the `swatch-book` icon.

## Parameters tab

The Inspector's **Parameters** tab (`sliders-vertical`) edits the selected
`.gradient` file's image layout (the Modifiers tab does not apply). It uses
the [property editor](gui.md#property-editor) look, with one **Output**
section (collapsible):

- **Width**, **Height** (integer fields, 1–4096), **Cell width** (1–Width)
  and **Cell height** (1–Height): the image and the size of each gradient's
  cell, drag or type.
- **Layout**: dropdown "Rows (left to right, then down)" / "Columns (top to
  bottom, then right)": the order the gradients fill the cells in.
- A weak line `<n> of <m> cells used (<columns> × <rows>)`.

A size change that would leave too few cells for the gradients is undone, so
the editor never writes an invalid file.

## Gradients tab

The Inspector's **Gradients** tab (`palette`) lists the selected
`.gradient` file's gradients, one card each, in order:

- Cards **fold** like property sections: a chevron (or a click on the title)
  folds and unfolds the card, open by default, remembered for the session.
- Header: the chevron, a **drag handle** (`grip-horizontal`) to reorder
  gradients exactly as modifiers are reordered (see
  [`noise.md`](noise.md#modifiers-tab)), and the title (the name, or
  `Gradient <n>` when it is empty). On the right:
  - unfolded: a **remove** (`trash-2`) icon button;
  - folded: a small preview of the gradient instead (48 × 12 px, 3 px
    corners, over a checkerboard, first stop on the left as in the bar), and
    no remove button.
- Body (unfolded):
  - **Name**: text field.
  - **Dither**: dropdown None / Crosshatch / Pixel.
  - The [gradient editor](#gradient-editor).
- Below the cards: an **Add gradient** button (`plus` icon and text) that
  appends `Gradient <n>` (black to white, no dithering). It is disabled when
  every cell is used, with the tooltip "No room: enlarge the image or shrink
  the cells".
- With no gradients, a weak "No gradients" line precedes the button.
- If the file does not parse, both tabs show the error instead.

Edits in both tabs are saved like noise parameters (see [`noise.md`](noise.md#saving)):
written to the file in the [written form](../spec/gradient.md#written-form)
once nothing is being dragged or typed, then inspected again in the
background.

## Gradient editor

A gradient's stops are edited with a bar, a row of stop handles under it and
the selected stop's fields. Each gradient card remembers its selected stop
(the first one to start with).

### Bar and handles

- **Bar**: the card body's full width, 24 px tall, 4 px corners, drawn over
  an 8 px `CHECKER_LIGHT` / `CHECKER_DARK` checkerboard. It shows the
  gradient laid on its side, first stop on the left (as positions run in the
  editor), with the image's blending and dithering, at the bar's size in
  screen pixels.
- **Handles**: under the bar, one per stop, centered on its position: a
  12 × 12 px square of the stop's color (over a checkerboard when it is
  translucent) with a 1 px `TEXT_WEAK` border and a small triangle pointing
  up at the bar. The selected stop's border is 2 px `ACCENT`. Hovering shows
  the stop's color and position (e.g. `#ff6b35 @ 50.0%`).
- **Select**: press a handle.
- **Move**: drag a handle sideways (clamped to 0–100%). Stops reorder when
  passing each other; the dragged one stays held.
- **Add**: press the bar, or the handle row where there is no handle: a stop
  is added there in the color the gradient already has at that point,
  selected, and keeps following the pointer until release.
- **Remove**: double-click a handle, drag it more than 36 px below the
  handle row and release (it is drawn at 40% opacity while it would be
  removed), or use the Remove button. A gradient keeps at least two stops:
  removing is ignored then.

### Selected stop

Below the handles, for the selected stop:

- **Color picker**, inline, as wide as the card body:
  - **Saturation / value square**, 120 px tall: saturation left to right,
    value top to bottom, for the current hue. A ring (white with a dark
    outline) marks the color. Press or drag to pick.
  - **Hue bar**, 14 px tall: the hue circle left to right. A vertical marker
    shows the hue. Press or drag to pick.
  - **Alpha bar**, 14 px tall: the color from transparent to opaque over a
    checkerboard. A vertical marker shows the alpha. Press or drag to pick.
  - Bars and square have 4 px corners. The hue is kept while saturation or
    value is 0 (where it cannot be read back from the color), so dragging
    through black or gray does not lose it.
  - A row with a 24 × 24 px **swatch** of the color (over a checkerboard),
    then a **Hex** field: `#rrggbb` (or `#rrggbbaa` when not opaque), applied
    on Enter or when the field loses focus if it is a valid color (with or
    without `#`, 6 or 8 digits); otherwise it reverts.
  - A row of **R**, **G**, **B**, **A** integer fields, 0–255 (drag or type),
    each with its letter as a weak label.
- **Position**: slider plus value box, 0–100 (%), one decimal.
- A row of buttons: **Reverse** (mirror the stops: `at → 1 − at`),
  **Distribute** (space the stops evenly, keeping their order) and
  **Remove stop** (`trash-2`, disabled with two stops).

## Inspector Info

Kind `Gradient`, the file Size, **Gradients** (count), **Cell**
(`16 × 64 px`), **Grid** (`16 × 4`), **Layout** (`rows` or `columns`),
then the usual source texture rows (Dimensions, Asset path, Asset ID,
Material, Color, Mip levels, Output). An invalid file shows an **Error** row
and no image.

## Build

- `*.gradient` (any case) files in `input` are texture sources; their asset
  path drops `.gradient`, e.g. `luts/sky.gradient` → `luts/sky.ktx2`.
- Gradients are colors: **sRGB** unless a `[textures] linear` glob matches
  (or a linear material slot uses the file).
- They are **never mipmapped** (1 level): smaller levels would blend
  neighbouring cells. `no_mipmaps` changes nothing.
- The image has an alpha channel only if some stop is not opaque.
- An invalid file fails like an undecodable image: the build reports
  `<path>: <error>` and writes no manifest.
