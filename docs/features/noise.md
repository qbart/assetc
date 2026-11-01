# Noise textures

Procedural textures stored as parameters in `.noise` files (format and exact
algorithms: [`docs/spec/noise.md`](../spec/noise.md)). They behave like any
other texture source: the build turns them into `.ktx2`, the canvas previews
them, and they can fill material slots (`rock.h.noise` is the height map of
material `rock`).

## Creating

In the Input tree's context menu, **New Noise** opens a submenu of the four
types: **White**, **Value**, **Perlin**, **Worley** (see
[`file-operations.md`](file-operations.md)). Picking one creates
`<type>.noise` (lowercase type name; `<type> 2.noise`, `<type> 3.noise`, …
if taken) with that type and all other parameters at their defaults, selects
it, and starts renaming it.

## Editing

The Inspector's **Parameters** tab (`sliders-vertical` icon) edits a selected
`.noise` file from the Input tree; for any other selection the tab is
disabled. Controls, in a two-column grid like Info (weak labels left):

| Parameter | Control                                             |
|-----------|-----------------------------------------------------|
| Type      | dropdown: White, Value, Perlin, Worley              |
| Size      | dropdown: 64, 128, 256, 512, 1024, 2048, 4096 (a value from the file that is not listed is shown as is and kept until changed) |
| Seed      | integer drag field, 0 … 4294967295                  |
| Frequency | integer slider 1 … 64 (the field accepts up to 256) |
| Octaves   | integer slider 1 … 8                                |
| Gain      | slider 0 … 1, two decimals                          |
| Invert    | checkbox                                            |

- Frequency, Octaves and Gain are disabled (dimmed) for White.
- A change is **written to the file** in the
  [written form](../spec/noise.md#written-form) when it is committed: a
  dropdown or checkbox immediately, a slider or drag field when the pointer is
  released (or Enter is pressed in its text field). The file is then
  inspected again, so the canvas and Info update.
- If the file does not parse, the tab shows the error instead of controls.

## Build

- `*.noise` (any case) files in `input` are texture sources; their asset path
  drops `.noise`, e.g. `textures/clouds.noise` → `textures/clouds` →
  `textures/clouds.ktx2`.
- Generated noise is **linear** (data) unless it fills an sRGB material slot
  (base color, emissive); the `[textures] linear` globs do not apply to it.
  `no_mipmaps` applies as usual.
- An invalid file fails like an undecodable image: the build reports
  `<path>: <error>` and writes no manifest.

## Inspector Info

Kind `Noise`, the file Size, then the parameters as rows (Type, Seed,
Frequency, Octaves, Gain, Invert; the generated size is the Dimensions row),
then the usual source texture rows (Dimensions, Asset path, Asset ID,
Material, Color, Mip levels, Output).
