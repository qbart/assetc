# Keys

Keyboard and mouse bindings in the assetc GUI. Keys only act when no text
field has focus, except the global ones.

## Global

See [`docs/features/gui.md`](docs/features/gui.md#menus).

| Input                         | Action |
|-------------------------------|--------|
| `Ctrl+Q` (`Cmd+Q` on macOS)   | Quit   |
| `Ctrl+B` (`Cmd+B` on macOS)   | Build (see [`docs/features/build.md`](docs/features/build.md#gui)) |
| `Ctrl+R` (`Cmd+R` on macOS)   | Rescan the Input and Output trees |

## File browser (Input tree)

See [`docs/features/file-operations.md`](docs/features/file-operations.md).

| Input           | Action                                     |
|-----------------|--------------------------------------------|
| Right click     | File operations menu                       |
| `F2`            | Rename the selected entry                  |
| `Delete`        | Delete the selected entry (asks first)     |
| `Enter`         | While renaming: commit                     |
| `Escape`        | While renaming: cancel                     |

## Canvas (2D)

Active while the pointer is over the canvas (see
[`docs/features/canvas.md`](docs/features/canvas.md#controls)).

| Input                                  | Action                           |
|----------------------------------------|----------------------------------|
| Scroll wheel                           | Zoom around the pointer          |
| `Ctrl` + scroll, pinch                 | Zoom around the pointer          |
| Right drag                             | Pan                              |
| Double-click                           | Fit image to the canvas          |

## Canvas (3D)

Active while the pointer is over the canvas (see
[`docs/features/canvas.md`](docs/features/canvas.md#3d-controls)).

| Input                                  | Action                           |
|----------------------------------------|----------------------------------|
| Left or right drag                     | Orbit the camera                 |
| Scroll wheel, pinch                    | Move the camera closer / farther |
| Double-click                           | Reset the camera                 |

On a model (see [`docs/features/locators.md`](docs/features/locators.md#3d-view)):

| Input                                  | Action                                       |
|----------------------------------------|----------------------------------------------|
| Left click on a locator                | Choose it (unfolds its card)                 |
| Left click elsewhere (no drag)         | Choose none; while placing, add a locator where the model is hit |
| Left drag on a gizmo handle            | Move / rotate the chosen locator (not while an animation plays) |
| `W`                                    | Gizmo: Move (again: off); with the Paths tab, on the chosen path point; with the Vertex colors tab, on the chosen gradient end |
| `E`                                    | Gizmo: Rotate (again: off); nothing with the Paths or Vertex colors tab |
| `S`                                    | Arm Place on surface                         |
| `Escape`                               | Gizmo off, disarm Place on surface, stop painting or erasing, stop drawing path points |
| Left click on a path point             | Choose that path and point (unfolds its card) |
| Left click on a gradient end           | Choose that end (From or To) of the chosen color set |

While a region paints (see [`docs/features/regions.md`](docs/features/regions.md#painting)):

| Input                                  | Action                                       |
|----------------------------------------|----------------------------------------------|
| Left drag / click on the model         | Paint (Paint on) or erase (Erase on) triangles |
| `Shift` + left drag / click            | The other one: erase while painting, paint while erasing |
| Right drag                             | Orbit the camera                             |

On the animation timeline (see [`docs/features/animation-events.md`](docs/features/animation-events.md#timeline)):

| Input                                  | Action                                       |
|----------------------------------------|----------------------------------------------|
| `Space` (pointer over the timeline)    | Play / pause                                 |
| Left press / drag on the ruler         | Move the playhead there (pauses)             |
| Left press on an event marker          | Choose that event                            |
| Left drag on an event marker           | Move the event in time                       |
| Double-click on the track              | Add an event there                           |
| Double-click on an event marker        | Delete the event                             |

While a path draws (see [`docs/features/paths.md`](docs/features/paths.md#drawing)):

| Input                                  | Action                                       |
|----------------------------------------|----------------------------------------------|
| Left click on the model                | Add a point there, after the chosen one      |
| Left drag on the model                 | Add a point every 24 px along the stroke     |
| Right drag                             | Orbit the camera                             |

## Canvas (color LUT)

See [`docs/features/lut.md`](docs/features/lut.md#views).

| View    | Input              | Action                              |
|---------|--------------------|-------------------------------------|
| Compare | Left click / drag  | Move the before / after divider     |
| Cube    | as Canvas (3D)     | Orbit, move closer / farther, reset |
| Cube    | Hold `Space`       | Show Before (untransformed cube)    |
