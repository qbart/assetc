# Keys

Keyboard and mouse bindings in the assetc GUI. Keys only act when no text
field has focus.

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

## Canvas (color LUT)

See [`docs/features/lut.md`](docs/features/lut.md#views).

| View    | Input              | Action                              |
|---------|--------------------|-------------------------------------|
| Compare | Left click / drag  | Move the before / after divider     |
| Cube    | as Canvas (3D)     | Orbit, move closer / farther, reset |
| Cube    | Hold `Space`       | Show Before (untransformed cube)    |
