# Keys

Keyboard and mouse bindings in the assetc GUI. Keys only act when no text
field has focus, except the global ones. The file browser's keys act only
after the last mouse press landed in the left sidebar; a graph's keys only
after it landed in the canvas.

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
| `F2`            | Rename the selected entry (after a press in the sidebar) |
| `Delete`        | Delete the selected entry, asks first (after a press in the sidebar) |
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
| Left click on an orientation gizmo dot | Look along that axis (`+X`, `−X`, …) |

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
| `,` / `.`                              | Pause and step back / forward to the previous / next 0.01 s |
| `Shift` + `,` / `.`                    | The same by 0.001 s                          |
| `E`                                    | Toggle an event at the playhead (delete the one there, or add one) |
| Left press / drag on the ruler         | Move the playhead there (pauses)             |
| Left press on an event marker          | Choose that event                            |
| Left drag on an event marker           | Move the event in time                       |
| Double-click on the track              | Add an event there                           |
| Double-click on an event marker        | Delete the event                             |

In a schema's editor (see [`docs/features/schemas.md`](docs/features/schemas.md#defaults)):

| Input                                  | Action                                       |
|----------------------------------------|----------------------------------------------|
| Double-click on a colour default's swatch | Open the color picker                     |
| Double-click on a curve default's preview | Open the curve editor                     |
| Double-click on a gradient default's preview | Open the gradient editor               |
| Left drag on a group's handle          | Move the group (fields fold meanwhile)       |
| Left drag on a field's handle          | Move the field within its group              |
| Left drag on a list item's handle      | Move the item within a list default          |

In a curve editor (see [`docs/features/curves.md`](docs/features/curves.md#editing)); keys
act while the pointer is over the editor:

| Input                                  | Action                                       |
|----------------------------------------|----------------------------------------------|
| Left press / drag on a point           | Choose it / move it                          |
| Left press on empty plot               | Choose none                                  |
| Left drag on a tangent handle          | Rotate that slope (joined: both)             |
| `Shift` + drag on a tangent handle     | Split the slopes, then rotate this one       |
| Double-click on empty plot             | Add a point on the curve there               |
| Double-click on a point                | Delete it (not the first, last, or one of two) |
| `[` / `]`                              | Previous / next point (wrapping)             |
| `Home` / `End`                         | First / last point                           |
| `S`                                    | Split / join the chosen point's slopes       |
| `A`                                    | Add a point after the chosen one             |
| `Delete` / `X`                         | Delete the chosen point                      |
| `Space`                                | Pause / resume the playhead                  |
| `Escape`, click outside                | Close the editor                             |

On a directed graph (see [`docs/features/graphs.md`](docs/features/graphs.md#canvas)):

| Input                                  | Action                                       |
|----------------------------------------|----------------------------------------------|
| Scroll wheel                           | Zoom around the pointer                      |
| Right or middle drag                   | Pan                                          |
| Right double-click                     | Fit the graph                                |
| Left press / drag on a node's body     | Choose it / move it                          |
| `Shift` while moving or adding a node  | Snap its center to 0.25                      |
| Left drag from a node's rim onto a node | Add an edge (onto the same node: a loop)    |
| `Delete` / `X`                         | Remove the chosen node (with its edges) or edge |
| Double-click on a node                 | Edit its label in place                      |
| Double-click on an edge                | Edit its label in place                      |
| `Enter` / `Escape` while editing a label | Keep / drop the new label                  |
| Left click on an edge                  | Choose it                                    |
| Left drag on an edge's start or tip    | Detach that end: drop on a node to attach it there, on empty grid to remove the edge |
| Left click on empty grid               | Choose nothing                               |
| Double-click on empty grid             | Add a node there                             |

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
