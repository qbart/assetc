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
image center's offset from the canvas center).

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
| Scroll (mouse wheel, two-finger swipe)  | pan (vertical; horizontal with `Shift` or a horizontal swipe) |
| `Ctrl` + scroll, pinch                  | zoom around the pointer: the image point under the pointer stays under it |
| Drag with left or middle button         | pan                                            |
| Double-click                            | fit                                            |

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
