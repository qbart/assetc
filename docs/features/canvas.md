# Canvas

The center pane previews the selected file (see
[`inspector.md`](inspector.md#selection) for selection), so a source can be
looked at before the build transforms it and compared with what the build
produced.

## View bar

A 36 px bar across the top of the canvas (`BG_PANEL`, 1 px `STROKE` line at
its bottom), always shown:

```
View  [ 2D | 3D ]
```

- `View` in `TEXT_WEAK`, 13 px, 12 px from the left edge, then a segmented
  group of text buttons (13 px text, 10 px horizontal padding, 24 px tall,
  `BG_BAR` fill, 4 px rounded corners).
- The active mode is filled with `ACCENT` with `BG_BAR` text; others use
  `TEXT_WEAK` text, `TEXT` and a `BG_HEADER` fill on hover.
- **2D** is the default. The mode is global (not per image) and lasts for the
  session.
- The bar is always shown, but the mode only applies to **images**; with
  anything else selected, both buttons are disabled (`TEXT_WEAK` at 50%, no
  hover or clicks). An [environment map](#environment-maps) is always shown in
  3D, so its 3D button is drawn active and 2D disabled; for other selections
  neither is active.

The rest of the canvas, below the bar, shows the selection in the chosen
mode. Everything below ("canvas") means that area.

| Mode | Shows |
|------|-------|
| 2D   | the image flat, with zoom, pan and tiling; see [2D](#2d) |
| 3D   | the image as the texture of a slowly rotating shape, with an orbit camera; see [3D](#3d) |

Both modes show the same messages when there is no image to show.

# 2D

## What is shown

| Selection                         | Canvas                                    |
|-----------------------------------|-------------------------------------------|
| nothing                           | "Select a file to preview"                |
| source texture (Input `*.png`, `*.jpg`, `*.jpeg`) | the decoded image |
| compiled texture (Output `*.ktx2`)| level 0, if the file is in assetc's own KTX2 subset (`R8G8B8A8_SRGB` / `R8G8B8A8_UNORM`, no supercompression, see [`ktx2.md`](../spec/ktx2.md)); otherwise "No preview for this format" |
| a file that fails to decode       | "No preview" (the Inspector shows the error) |
| an environment map folder (`*.env`) | the cubemap, in 3D, see [Environment maps](#environment-maps) |
| any other file or folder          | "No preview"                              |
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

In 2D, the view is a **zoom** (screen pixels per image pixel) and a **pan** (the
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

2D controls, only while the pointer is over the canvas; see also
[`KEYS.md`](../../KEYS.md). For 3D see [3D controls](#3d-controls).

| Input                                   | Action                                         |
|-----------------------------------------|------------------------------------------------|
| Scroll wheel (vertical scroll)          | zoom around the pointer: the image point under the pointer stays under it. Each wheel notch zooms by exactly ×1.2; smooth (touchpad) scrolling zooms continuously, ×1.2 per 50 points |
| `Ctrl` + scroll, pinch                  | zoom around the pointer, as above              |
| Drag with the right button              | pan                                            |
| Double-click (left)                     | fit                                            |

Horizontal scrolling does nothing.

## Zoom bar

A small bar in the canvas' bottom-right corner, 8 px from both edges, shown
in 2D while an image is previewed:

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

The first group depends on the mode: **Tile** in 2D, **Shape** in 3D (see
[3D](#3d)). The Channels group is the same in both.

| Group    | Button | Icon                            | Kind   | Action                        |
|----------|--------|---------------------------------|--------|-------------------------------|
| View (2D) | Tile  | `grid-3x3`                      | toggle | [tiling](#tiling) on / off    |
| Shape (3D) | Box  | `box`                           | radio  | render on a cube              |
| Shape (3D) | Sphere | `globe`                       | radio  | render on a UV sphere         |
| Shape (3D) | Cylinder | `cylinder`                  | radio  | render on a capped cylinder   |
| Shape (3D) | Torus | `torus`                        | radio  | render on a torus             |
| Channels | RGB    | three circles, all filled       | radio  | all channels ([channels](#channels)) |
| Channels | R      | three circles, red filled       | radio  | red channel only              |
| Channels | G      | three circles, green filled     | radio  | green channel only            |
| Channels | B      | three circles, blue filled      | radio  | blue channel only             |
| Channels | A      | three circles, none filled      | radio  | alpha channel only            |

Exactly one Channels button is on; clicking it again keeps it on.

The buttons depend on the shown file's channels:

| Source                               | Channels group                    |
|--------------------------------------|-----------------------------------|
| RGBA, and every compiled `.ktx2`     | RGB, R, G, B, A                   |
| RGB (e.g. JPEG, RGB PNG)             | RGB, R, G, B, A (A disabled)      |
| grayscale                            | Gray                              |
| grayscale + alpha                    | Gray, A                           |

**Gray** is a single button for grayscale (single-channel, typically linear
data) images: its icon is the same trefoil with the circles filled black
(top), `TEXT_WEAK` gray (bottom-left) and white (bottom-right). It shows the
image as stored (like RGB) and is always on when there is no other channel
choice. A disabled button is
dimmed: outline in `TEXT_WEAK` at 50% opacity and its fill color at 35%; it
does not react to hover or clicks, and its tooltip adds "(not in this
image)". If an image's remembered channel is not available (the file changed),
RGB (or Gray) is shown.

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

# 3D

The image is used as the base color texture of a lit 3D shape, rendered by
Bevy into the canvas area (below the view bar) behind the egui overlay.

## Scene

- **Shapes** (pick with the Shape group, Box by default; global for the
  session):

  | Shape    | Geometry                                           | UVs                                   |
  |----------|----------------------------------------------------|---------------------------------------|
  | Box      | cube, 1.2 units                                    | the whole image on each face, upright (image top up) on the four sides; on the top face the image top points away from the front (−Z), on the bottom toward it |
  | Sphere   | UV sphere, radius 0.75, 48 × 24 segments, poles on the vertical axis | U around, V from the top pole (image top) to the bottom pole |
  | Cylinder | radius 0.6, height 1.4, 48 segments, capped        | U around the side, V along the height |
  | Torus    | ring radius 0.6, tube radius 0.25                  | U around the ring, V around the tube  |

- The shape **rotates** about the vertical axis at 20°/s, except while it is
  being orbited (dragged). The rotation continues from where it was.
- **Lighting**: one white directional light from the upper left behind the
  camera, plus ambient light, so the shape reads as 3D; base color texture
  only (no normal, ORM or height maps), roughness 0.8, not metallic.
- The texture is sampled with linear filtering and repeat addressing and
  uploaded as sRGB, so the colors match the 2D view.
- **Channels**: the Channels group works as in 2D: RGB shows the image, R / G
  / B / A the chosen channel as opaque grayscale. In RGB, an image with alpha
  is rendered alpha-blended over the background; otherwise the shape is opaque.
- Background: `BG_CENTER`. No checkerboard.
- The zoom bar is not shown; tiling does not apply.

## Camera

An orbit camera looks at the shape's center.

- Starts at yaw 30°, pitch 20° (looking slightly down), distance 3.5 units,
  45° vertical field of view.
- Pitch is limited to −85°…85°, distance to 1.5…10.
- The camera persists across selections and mode switches for the session;
  double-click resets it to the start position.

## 3D controls

While the pointer is over the canvas (see also [`KEYS.md`](../../KEYS.md)):

| Input                         | Action                                                     |
|-------------------------------|------------------------------------------------------------|
| Left or right drag            | orbit: horizontal → yaw, vertical → pitch, 0.3° per point  |
| Scroll wheel, pinch           | dolly: distance ÷ 1.2 per notch toward the shape (× 1.2 away) |
| Double-click (left)           | reset the camera                                           |

## Environment maps

A folder named `*.env` (any case) is a cubemap given as six face images:

| File  | Face | Cube layer |
|-------|------|------------|
| `px.*` | +X  | 0          |
| `nx.*` | −X  | 1          |
| `py.*` | +Y  | 2          |
| `ny.*` | −Y  | 3          |
| `pz.*` | +Z  | 4          |
| `nz.*` | −Z  | 5          |

- Faces are direct children with those names (case-sensitive) and a texture
  source extension (`png`, `jpg`, `jpeg`) in the Input tree, or `.ktx2`
  (assetc's subset, level 0) in the Output tree.
- All six must exist, decode, be square, have the same size, and that size
  must be a power of two; otherwise the canvas shows the problem as its
  message (the Inspector shows it as an Error row).
- Selecting it shows the 3D view with the faces uploaded as an sRGB cubemap
  (stored as given, no flipping):
  - as the **background** (skybox, 1000 cd/m²), and
  - as the only **light** (image-based lighting filtered at runtime,
    intensity 1000): the directional and ambient lights are off.
- The shape is a mirror-like material (white, metallic, roughness 0.2) so the
  lighting and reflections are visible; the Shape group, auto-rotation and
  the orbit camera work as for images. The Channels group is not shown.
- This is a preview only: the build still compiles each face as a plain
  texture.
