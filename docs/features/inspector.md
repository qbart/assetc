# Inspector

Shows what a file in the file browser is, and what the build does or did with
it. It fills the right sidebar (see [`gui.md`](gui.md)). The selected file's
image is shown in the [canvas](canvas.md), not in the Inspector.

## Selection

- Clicking a **file** or **folder** row in the Input or Output tree selects
  it. Clicking a folder row also opens / closes it.
- One file is selected at a time, across both tabs: selecting a file in one
  tab replaces a selection in the other. Switching tabs keeps the selection.
- The selected row is filled with `ACCENT` at 25% opacity and has a 2 px
  `ACCENT` bar on its left edge (hover still uses `BG_HEADER`).
- The selection is kept by path. After a rescan (button or finished build)
  the selected file is inspected again, so the Inspector and canvas show the
  new output of a build. If the file is gone, the Inspector says
  "<name> no longer exists".

## Tabs

Three icon tabs, top to bottom: **Info** (`info`), **Assets** (`files`),
**Settings** (`settings`). Hovering a tab shows its name.

A tab that does not apply to the selected file is disabled: its icon is drawn
in `TEXT_WEAK` at 50% opacity, it does not react to hover or clicks. If the
active tab becomes disabled by a new selection, the Inspector switches to
Info.

## Info tab

With nothing selected: weak "Nothing selected".

Otherwise, from top to bottom:

1. **File name** (strong) and its path relative to the project root (weak).
2. **Details**: a two-column grid, weak labels on the left, values on the
   right. Values are selectable text, so an asset ID can be copied. Long
   values wrap within the sidebar instead of widening it.

Content scrolls vertically when it does not fit.

### Details by file kind

The kind is decided by tab and file name, the same way the build decides it
(see [`build.md`](build.md#inputs)):

| Tab    | File                       | Kind               |
|--------|----------------------------|--------------------|
| Input  | `*.png`, `*.jpg`, `*.jpeg` (any case) | Source texture |
| Input  | `*.mat.toml`               | Material source    |
| Input  | `*.cube` (any case)        | Color LUT, see [`lut.md`](lut.md#inspector) |
| Input  | anything else              | Ignored            |
| Output | `*.ktx2` (any case)        | Compiled texture   |
| Output | `*.mat`                    | Compiled material  |
| Output | `assets.manifest` at the output root | Manifest |
| Output | anything else              | File               |
| either | folder named `*.env` (any case) | Environment map |
| either | any other folder           | Folder             |

Every kind shows **Kind** and **Size** (`B`, `KiB`, `MiB`, `GiB` with one
decimal, e.g. `12.3 KiB`; plain bytes below 1 KiB, e.g. `512 B`).

**Source texture:**

| Row        | Value                                                               |
|------------|---------------------------------------------------------------------|
| Dimensions | `W × H` px                                                          |
| Asset path | input-relative path without extension, e.g. `textures/Grass`        |
| Asset ID   | `0x` + 16 lowercase hex digits                                      |
| Color      | `sRGB` or `linear`, from [`[textures]`](textures.md#settings)       |
| Mip levels | levels the build will write (`1` if `no_mipmaps` matches)          |
| Output     | output-relative path, e.g. `textures/Grass.ktx2`; `(not built)` appended when that file does not exist |

If `[textures]` has an invalid pattern, Color and Mip levels show the error
instead.

**Compiled texture:**

| Row        | Value                                                               |
|------------|---------------------------------------------------------------------|
| Dimensions | `W × H` px (level 0)                                               |
| Format     | `R8G8B8A8_SRGB`, `R8G8B8A8_UNORM`, or `vkFormat <n>` for any other  |
| Mip levels | `levelCount`                                                        |
| Asset path | output-relative path without extension                              |
| Asset ID   | as above                                                            |

Material sources, compiled materials, and source textures used by a material
get extra rows, see [`materials.md`](materials.md#inspector).

**Manifest:** Version, Entries (count).

**Folder:** Kind and Items (number of direct entries, hidden ones excluded);
no Size row.

**Environment map** (a cubemap given as a folder, see
[`canvas.md`](canvas.md#environment-maps)):

| Row       | Value                                                          |
|-----------|----------------------------------------------------------------|
| Faces     | `6 of 6`, or `4 of 6 (missing nz, pz)`                         |
| Face size | `W × H px` of the faces                                        |

plus an **Error** row when a face fails to decode or the faces are not all
square, the same size, and a power of two.

**Ignored / File:** Kind and Size only.

### Errors

If the file cannot be read or decoded (corrupt PNG, not a KTX2 file, bad
manifest), the details show the rows that are still known (Kind, Size) and an
**Error** row with the message. The canvas shows no image.

## Assets tab

Lists the assets a file contains, as recorded in `<output>/assets.manifest`
(see [`docs/spec/manifest.md`](../spec/manifest.md)).

- Enabled only for compiled files (`.ktx2` or `.mat` selected in the Output
  tree) for now. Each holds exactly one asset; later container formats
  (e.g. meshes with embedded materials) will list several.
- The manifest is read when the app opens and on every rescan (rescan button,
  or after a build finishes). A failed build does not write a manifest, so the
  tab reflects the last successful build.
- One row per manifest entry whose output path is the selected file: `image`
  icon (textures) or `file` icon (materials), the asset path (strong), then
  weak kind (`Texture`, `Material`) and the asset
  ID (`0x` + 16 hex digits, selectable). Rows sorted by asset path.
- Placeholders (weak text) instead of rows:
  - no manifest: "Not built yet"
  - manifest unreadable or invalid: the error message
  - file not in the manifest: "Not in the manifest"

## Settings tab

Not used yet. With a selection it shows weak "No settings for this file";
otherwise "Nothing selected".

## Loading

The selected file is read and decoded on the UI thread when it is selected or
rescanned. There is no file watching.

## Keybindings

None.
