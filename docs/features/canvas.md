# Canvas

The center pane previews the selected file (see
[`inspector.md`](inspector.md#selection) for selection), so a source can be
looked at before the build transforms it and compared with what the build
produced.

## What is shown

| Selection                         | Canvas                                    |
|-----------------------------------|-------------------------------------------|
| nothing                           | "Select a file to preview"                |
| source texture (Input `*.png`, `*.jpg`, `*.jpeg`) | the decoded image |
| compiled texture (Output `*.ktx2`)| level 0, if the file is in assetc's own KTX2 subset (`R8G8B8A8_SRGB` / `R8G8B8A8_UNORM`, no supercompression, see [`ktx2.md`](../spec/ktx2.md)); otherwise "No preview for this format" |
| a file that fails to decode       | "No preview" (the Inspector shows the error) |
| any other file                    | "No preview"                              |
| a selected file that disappeared  | "<name> no longer exists"                 |

Messages are weak text centered in the canvas.

- Pixels are shown as stored, for both color spaces (linear data textures look
  as they would in an image viewer).
- The image is uploaded at full resolution. Only images larger than the GPU's
  maximum texture size are downscaled to fit it (the Inspector still shows the
  real dimensions).
- Magnified pixels stay sharp (nearest filtering); minified ones are smoothed
  (linear filtering).
- The canvas background is `BG_CENTER`. The image area is drawn over an 8 px
  (screen pixels) checkerboard of `CHECKER_LIGHT` / `CHECKER_DARK`, so
  transparency is visible.
- The file is loaded with the Inspector: on selection and on rescan, on the
  UI thread.

## View

The view is a **zoom** (screen pixels per image pixel) and a **pan** (the
image center's offset from the canvas center). With [tiling](#tiling) on,
"the image" below means the whole 3 × 3 block.

- Zoom range: 1/32 (3.125%) to 64 (6400%).
- **Fit**: the largest zoom at which the whole image fits the canvas with a
  24 px margin on every side (within the zoom range), centered.
- Selecting a different file resets the view to fit. A rescan of the same file
  keeps the view.
- While the view is still the automatic fit (no zoom or pan since), resizing
  the canvas (window, sidebars) refits.
- Panning is clamped so that, on each axis, at least 32 px of the image (or
  all of it, if it is smaller) stays inside the canvas. A zoomed-in image can
  be panned to any of its edges, but never lost off-screen.

## Controls

Only while the pointer is over the canvas; see also [`KEYS.md`](../../KEYS.md).

| Input                                   | Action                                         |
|-----------------------------------------|------------------------------------------------|
| Scroll wheel (vertical scroll)          | zoom around the pointer: the image point under the pointer stays under it. Each wheel notch zooms by exactly ×1.2; smooth (touchpad) scrolling zooms continuously, ×1.2 per 50 points |
| `Ctrl` + scroll, pinch                  | zoom around the pointer, as above              |
| Drag with the right button              | pan                                            |
| Double-click (left)                     | fit                                            |

Horizontal scrolling does nothing.

## Zoom bar

A small bar in the canvas' bottom-right corner, 8 px from both edges, shown
while an image is previewed:

```
[ − ][ 100% ][ + ][ Fit ]
```

- `BG_BAR` fill, 1 px `STROKE` border, square corners, 28 px tall, 13 px text,
  buttons edge to edge with 8 px horizontal padding; hover fills a button with
  `BG_HEADER`.
- `−` / `+`: divide / multiply the zoom by √2, around the canvas center.
- The percentage is the current zoom, rounded to a whole percent; clicking it
  sets 100% (1 image pixel = 1 screen pixel, at the window's scale factor),
  around the canvas center.
- `Fit`: fit.
- The bar takes the pointer: scrolling or dragging on it does not pan.

## Toolbar

A vertical column of button groups in the canvas' top-right corner, 8 px
from the top and right edges, groups stacked top to bottom with an 8 px gap,
shown only while an image is previewed:

```
[ ▦ ]
     8 px
[RGB]
[ R ]
[ G ]
[ B ]
[ A ]
```

- Each group: `BG_BAR` fill, 6 px rounded corners, buttons 36 × 32 px stacked
  edge to edge with an 18 px icon, separated by 1 px horizontal `STROKE`
  lines.
- A button that is on is filled with `ACCENT` (the group's top corners
  rounded on its first button, bottom corners on its last) and its icon
  outline is drawn in `BG_BAR`. A button that is off
  draws its outline in `TEXT_WEAK`; hover fills it with `BG_HEADER` and draws
  the outline in `TEXT`.
- Each button shows its name as a tooltip.
- The toolbar takes the pointer like the zoom bar.

| Group    | Button | Icon                            | Kind   | Action                        |
|----------|--------|---------------------------------|--------|-------------------------------|
| View     | Tile   | `grid-3x3`                      | toggle | [tiling](#tiling) on / off    |
| Channels | RGB    | three circles, all filled       | radio  | all channels ([channels](#channels)) |
| Channels | R      | three circles, red filled       | radio  | red channel only              |
| Channels | G      | three circles, green filled     | radio  | green channel only            |
| Channels | B      | three circles, blue filled      | radio  | blue channel only             |
| Channels | A      | three circles, none filled      | radio  | alpha channel only            |

Exactly one Channels button is on; clicking it again keeps it on.

### Channel icons

Three Lucide `circle-small` icons, each 16 px, arranged as a trefoil inside
the 18 px icon area: red at the top, green bottom-left, blue bottom-right,
slightly overlapping (centers 3.2 px above, and 3.7 px left / right and 2.8 px
below, the icon center). A circle that is "filled" is painted with its color
(`CHANNEL_R`, `CHANNEL_G`, `CHANNEL_B`) inside its outline; the others show
only the outline.

## Channels

- **RGB** (default): the image as stored, including alpha over the
  checkerboard.
- **R**, **G**, **B**, **A**: that channel alone, shown as **grayscale** (the
  value in all three color channels) and **opaque**, so data maps such as the
  packed ORM channels, or an alpha mask, can be read directly. Images without
  alpha (e.g. JPEGs) show white for A.
- Like tiling, the choice is remembered per image for the session and
  starts at RGB; it combines with tiling.
- The single-channel image is built on the CPU from the loaded pixels when
  the channel is chosen, and kept until the selection or channel changes.

## Tiling

Shows the image 3 × 3 times, edge to edge, to check that it tiles without
visible seams.

- The tiles touch exactly, with no gap or overlap, and are drawn identically
  (the center tile is the image itself).
- The checkerboard covers the whole 3 × 3 block.
- Fit, zoom and the pan clamp use the 3 × 3 block. Toggling tiling while the
  view is still the automatic fit refits; otherwise the zoom stays and the
  pan is clamped again.
- Tiling is remembered **per image** (by its path) for the rest of the
  session: switching to another image shows that image's own setting, and
  coming back restores it. It starts off for every image.
