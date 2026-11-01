# Noise textures

Procedural textures stored as parameters in `.noise` files (format and exact
algorithms: [`docs/spec/noise.md`](../spec/noise.md)): a generator with its
own parameters, followed by a list of modifiers. They behave like any other
texture source: the build turns them into `.ktx2`, the canvas previews them,
and they can fill material slots (`rock.h.noise` is the height map of
material `rock`).

## Generators

| Menu / dropdown name | `type`                |
|----------------------|-----------------------|
| Checkerboard         | `checkerboard`        |
| Uniform              | `noise/uniform`       |
| Value                | `noise/value`         |
| Perlin               | `noise/perlin`        |
| Simplex              | `noise/simplex`       |
| fBm                  | `noise/fbm`           |
| Voronoi              | `noise/voronoi`       |
| Perlin-Worley        | `noise/perlin-worley` |
| Musgrave             | `noise/musgrave`      |
| Ridged               | `noise/ridged`        |
| Curl                 | `noise/curl`          |

Each generator has its own parameters; nothing is shared between types
except the image `size`.

## Creating

In the Input tree's context menu, **New Noise** lists the generators by the
names above (see [`file-operations.md`](file-operations.md)). Picking one
creates `<name>.noise` (the last part of `type`, e.g. `perlin-worley.noise`,
`checkerboard.noise`; `<name> 2.noise`, … if taken) with that generator, its
default parameters and no modifiers, selects it and starts renaming it.

## Parameters tab

The Inspector's **Parameters** tab (`sliders-vertical`) edits the selected
`.noise` file from the Input tree; for anything else it is disabled. It uses
the [property editor](gui.md#property-editor) look, with two sections:

- **Generator**: **Type** (dropdown of the names above) and **Size**
  (dropdown: 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096; a size from
  the file that is not listed is shown as is and kept until changed).
- **<Generator name>** (e.g. "fBm"): one row per parameter of the generator,
  in table order:
  - *int* with a range up to 256 and *float*: slider plus value box;
  - `seed`: a full-width integer field (drag or type);
  - *choice*: dropdown;
  - *gray*: slider plus value box (0–1);
  - *color*: color button.

  A generator without parameters besides `seed` still shows its section.

Changing **Type** switches to that generator's parameters. Parameters are
kept per type for the rest of the session (per file), so switching back
restores what was set; the file only ever stores the current type's.

## Modifiers tab

The **Modifiers** tab (`wrench`) edits the selected `.noise` file's modifiers
(disabled for anything else). Each modifier is a section card, in order:

- Modifier cards do not collapse (no chevron); their parameters are always
  shown.
- Header: a **drag handle** (`grip-horizontal`) in the chevron's place, an
  eye button (`eye` enabled / `eye-off` disabled; a disabled modifier's
  title is drawn in `TEXT_WEAK`), the modifier name (Invert, Levels,
  Brightness / Contrast, Threshold, Posterize, Blur, Normalize, Colorize), and
  on the right a **remove** (`trash-2`) icon button with a tooltip.
- **Reordering**: press the drag handle and move the pointer (cursor `grab`
  on hover, `grabbing` while dragging). The dragged card gets a 1 px `ACCENT`
  outline and a 2 px `ACCENT` line shows where it will go: before the first
  card whose vertical center is below the pointer, or after the last. Releasing
  moves the modifier there; releasing where it already is (directly above or
  below itself) changes nothing.
- Body: the modifier's parameters as rows (as in Parameters), or weak
  "No parameters".
- Below the list: an **Add modifier** button (`plus` icon and text) opening a
  menu of the eight modifiers; the new one is appended, enabled, with default
  parameters.
- With no modifiers, a weak "No modifiers" line precedes the button.

## Saving

Edits are **written to the file** in the
[written form](../spec/noise.md#written-form) when committed: dropdowns,
checkboxes, color changes and the modifier buttons (add, remove, enable)
and drops immediately; sliders and fields when the pointer is released (or
Enter is pressed). The file is then inspected again, so the canvas and Info
update. If the file does not parse, both tabs show the error instead.

## Build

- `*.noise` (any case) files in `input` are texture sources; their asset path
  drops `.noise`, e.g. `textures/clouds.noise` → `textures/clouds.ktx2`.
- Generated noise is **linear** (data) unless it fills an sRGB material slot
  (base color, emissive); the `[textures] linear` globs do not apply to it.
  `no_mipmaps` applies as usual.
- An invalid file fails like an undecodable image: the build reports
  `<path>: <error>` and writes no manifest.

## Inspector Info

Kind `Noise`, the file Size, **Type** (generator name), one row per generator
parameter (labels as in the Parameters tab), **Modifiers** (`none`, or e.g.
`Levels, Blur (off)`), then the usual source texture rows (Dimensions, Asset
path, Asset ID, Material, Color, Mip levels, Output).
