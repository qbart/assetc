# Palettes

Named colors stored in `.apalette` files (format and image:
[`docs/spec/apalette.md`](../spec/apalette.md)). One file holds a set of
colors, each filling one **cell** of a grid in one image (a palette
texture). Palettes are managed like [gradients](gradients.md): they are
texture sources, the build turns them into `.ktx2`, the canvas previews them
(2D and 3D), and they can fill material slots.

## Creating

In the Input tree's context menu, **New Color ▸ Palette** (after Gradient,
see [`file-operations.md`](file-operations.md)) creates
`palette.apalette` (`palette 2.apalette`, … if taken) with a 128 × 128 image of
16 × 16 cells (8 columns × 8 rows, room for 64 colors) and two colors,
`Black` (`#000000`) and `White` (`#ffffff`). It then selects it and starts
renaming it.

The file tree shows `.apalette` files with the `palette` icon.

## Parameters tab

The Inspector's **Parameters** tab (`sliders-vertical`) edits the image
layout exactly as for gradients (see
[`gradients.md`](gradients.md#parameters-tab)): one **Output** section with
Width, Height, Cell width, Cell height, Layout and the weak line `<n> of <m>
cells used (<columns> × <rows>)`. A size change that would leave too few
cells for the colors is undone. The Modifiers tab does not apply.

## Colors tab

The Inspector's **Colors** tab (`palette`) edits the colors:

- **Swatches**: one 24 × 24 px swatch per color, in palette order, left to
  right with 4 px gaps, wrapping to the tab's width. Each is filled with its
  color (over a checkerboard when it is translucent), 3 px corners, with a
  1 px `TEXT_WEAK` border; the selected one has a 2 px `ACCENT` border.
  Hovering shows the name and color (e.g. `Black #000000`).
  - **Select**: click a swatch. The first color is selected to start with;
    the choice is kept while the file stays selected.
  - **Reorder**: drag a swatch onto another (cursor `grabbing`; the swatch
    under the pointer gets a 2 px `ACCENT` outline) and release: the dragged
    color moves to that place, staying selected.
  - **Add**: a last 24 × 24 px button with the `plus` icon appends
    `Color <n>` with the selected color's value (white with no colors) and
    selects it. It is disabled when every cell is used, with the tooltip "No
    room: enlarge the image or shrink the cells".
- **Selected color**, below the swatches (none with no colors, a weak "No
  colors" line instead):
  - **Name**: text field (the swatch tooltip shows `Color <n>` when empty).
  - The inline **color picker** of the gradient editor (saturation / value
    square, hue and alpha bars, swatch, Hex field, R G B A fields), see
    [`gradients.md`](gradients.md#selected-stop).
  - A row of buttons: **Duplicate** (inserts a copy after it, selected;
    disabled when every cell is used) and **Remove color** (`trash-2`).
- If the file does not parse, both tabs show the error instead.

Edits in both tabs are saved like gradients: written to the file in the
[written form](../spec/apalette.md#written-form) once nothing is being
dragged or typed, then inspected again in the background.

## Inspector Info

Kind `Palette`, the file Size, **Colors** (count), **Cell** (`16 × 16 px`),
**Grid** (`8 × 8`), **Layout** (`rows` or `columns`), then the usual source
texture rows (Dimensions, Asset path, Asset ID, Material, Color, Mip levels,
Output). An invalid file shows an **Error** row and no image.

## Build

- `*.apalette` (any case) files in `input` are texture sources; their asset
  path drops `.apalette`, e.g. `ui/theme.apalette` → `ui/theme.ktx2`.
- Palettes are colors: **sRGB** unless a `[textures] linear` glob matches
  (or a linear material slot uses the file).
- They are **never mipmapped** (1 level): smaller levels would blend
  neighbouring cells. `no_mipmaps` changes nothing.
- The image has an alpha channel only if some color is not opaque.
- An invalid file fails like an undecodable image: the build reports
  `<path>: <error>` and writes no manifest.

## Keybindings

None.
