# Inspector

Shows what a file in the file browser is, and what the build does or did with
it. It fills the right sidebar (see [`gui.md`](gui.md)). The selected file's
image is shown in the [canvas](canvas.md), not in the Inspector.

## Selection

- Clicking a **file** or **folder** row in the Input or Output tree selects
  it. Clicking a folder row also opens / closes it.
- One file is selected at a time, across both tabs: selecting a file in one
  tab replaces a selection in the other. Switching tabs keeps the selection.
- The selected row is filled with `ACCENT` at 25% opacity, with no edge
  marker (hover still uses `BG_HEADER`).
- The selection is kept by path. After a rescan (button or finished build)
  the selected file is inspected again, so the Inspector and canvas show the
  new output of a build. If the file is gone, the Inspector says
  "<name> no longer exists".

## Tabs

Twenty icon tabs, top to bottom: **Info** (`info`), **Parameters**
(`sliders-vertical`) for `.noise`, `.gradient`, `.palette` and model files,
**Locators** (`map-pin`), **Shapes** (`shapes`), **Regions**
(`paint-bucket`), **Paths** (`spline`), **Vertex colors** (`swatch-book`) and
**Properties** (`database`) for model files (see
[`locators.md`](locators.md#locators-tab), [`shapes.md`](shapes.md#shapes-tab),
[`regions.md`](regions.md#regions-tab), [`paths.md`](paths.md#paths-tab),
[`vertex-colors.md`](vertex-colors.md#vertex-colors-tab),
[`properties.md`](properties.md#properties-tab)); Vertex colors is also
shown for compiled meshes with color sets (see
[`vertex-colors.md`](vertex-colors.md#compiled-meshes)),
**Modifiers** (`wrench`) for `.noise` files and images (`.png`, `.jpg`,
`.jpeg`, `.tga`), **Gradients** (`palette`) for `.gradient` files and
**Colors** (`palette`) for `.palette` files, **Types** (`braces`) for
`.aschema` files, **Graph** (`graph-star`) for `.agraph` files,
**Tree** (`list-tree`) for `.atree` files, **Atlas** (`map`) for `*.atlas`
folders,
**Instances** (`package`) for `.adata` files, all nine only
in the Input tree
(see [`noise.md`](noise.md#parameters-tab),
[`gradients.md`](gradients.md#parameters-tab),
[`palettes.md`](palettes.md#colors-tab),
[`schemas.md`](schemas.md#types-tab), [`graphs.md`](graphs.md#graph-tab),
[`trees.md`](trees.md#tree-tab), [`atlases.md`](atlases.md#inspector), [`instances.md`](instances.md#instances-tab)),
**Animations** (`film`), only for models with animations (see
[`models.md`](models.md#animations-tab)), **Mipmaps** (`images`), only for
compiled textures with more than one mip level (see
[Mipmaps tab](#mipmaps-tab)), **Levels of detail** (`bring-to-front`), only
for compiled meshes with reduced levels (see
[`models.md`](models.md#levels-of-detail-tab)), and **Assets** (`files`). Hovering
a tab shows its name.

Only tabs that apply to the selected file are shown, stacked from the top in
the order above with no gaps; the others are hidden (with nothing selected,
only Info). If the active tab does not apply to the newly inspected file, the
Inspector switches to Info (while a new selection is still loading, the
active tab is kept).

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
| Input  | `*.png`, `*.jpg`, `*.jpeg`, `*.tga` (any case) | Source texture |
| Input  | `*.mat.toml`               | Material source    |
| Input  | `*.cube` (any case)        | Color LUT, see [`lut.md`](lut.md#inspector) |
| Input  | `*.exr` (any case)         | HDR environment map, see [`hdr.md`](hdr.md#preview-and-inspector) |
| Input  | `*.noise` (any case)       | Noise, see [`noise.md`](noise.md#inspector-info) |
| Input  | `*.gradient` (any case)    | Gradient, see [`gradients.md`](gradients.md#inspector-info) |
| Input  | `*.palette` (any case)     | Palette, see [`palettes.md`](palettes.md#inspector-info) |
| Input  | `*.ttf`, `*.otf` (any case) | Font, see [`fonts.md`](fonts.md#inspector) |
| Input  | `*.gltf`, `*.glb` (any case) | Model, see [`models.md`](models.md#inspector) |
| Input  | anything else              | Ignored            |
| Output | `*.ktx2` (any case)        | Compiled texture   |
| Output | `*.amat`                   | Compiled material  |
| Output | `*.amesh`                  | Compiled mesh, see [`models.md`](models.md#inspector) |
| Output | `*.aanim`                  | Compiled animation, see [`models.md`](models.md#inspector) |
| Output | `*.afont`                  | Compiled font, see [`fonts.md`](fonts.md#compiled-fonts) |
| Output | `*.atype`, `*.arec`, `*.agph`, `*.atre` | Compiled schema, data, graph or tree, see [Compiled data](#compiled-data) |
| Output | `assets.aman` at the output root | Manifest, see [Manifest view](#manifest-view) |
| Output | anything else              | File               |
| either | folder named `*.env` (any case) | Environment map |
| Input  | folder named `*.atlas` (any case) | Atlas, see [`atlases.md`](atlases.md#inspector) |
| Output | `*.aatlas`                 | Compiled atlas, see [`atlases.md`](atlases.md#inspector) |
| either | any other folder           | Folder             |

Every kind shows **Kind** and **Size** (`B`, `KiB`, `MiB`, `GiB` with one
decimal, e.g. `12.3 KiB`; plain bytes below 1 KiB, e.g. `512 B`).

**Source texture:**

| Row        | Value                                                               |
|------------|---------------------------------------------------------------------|
| Dimensions | `W × H` px                                                          |
| Modifiers  | images only, when the [sidecar](textures.md#modifiers) has any: e.g. `Levels, Blur (off)` |
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
| Dimensions | `W × H` px (level 0), or `W × H × D` px for a 3D texture (shown as its slices side by side) |
| Format     | `R8G8B8A8_SRGB`, `R8G8B8A8_UNORM`, or `vkFormat <n>` for any other  |
| Mip levels | `levelCount`                                                        |
| Asset path | output-relative path without extension                              |
| Asset ID   | as above                                                            |

Material sources, compiled materials, and source textures used by a material
get extra rows, see [`materials.md`](materials.md#inspector).

**Manifest:** Version, Entries (count). The canvas shows its entries, see
[Manifest view](#manifest-view).

## Data tables

Files without an image or 3D preview show their data in the canvas as a
table, like a compiled mesh's [Chunks view](models.md#chunks-view): a header
line (13 px monospace, `TEXT`), then a table with a header row
(`TEXT_WEAK`) and striped 20 px rows in 12 px monospace; long tables scroll
(only visible rows are drawn), columns size to their content and can be
resized. The view bar is empty (no `View` label, no buttons) and the canvas
toolbar is not shown.

### Manifest view

Selecting `assets.aman` (Output tree root) shows its entries:

- Header line: `AMAN v1   <n> entries   <size>`, then a **Filter path…**
  field (240 px): only entries whose asset path or file contains the text
  (ignoring case) are listed; empty lists all. The filter lasts for the
  session.
- Columns: **#** (entry index in the file), **Asset ID** (`0x` + 16 hex
  digits), **Kind** (`Texture`, `Material`, `Mesh`, `Animation`, `Font`),
  **Asset path**, **File** (the entry's path in `output`). Rows are in file
  order (by ID). A file that does not exist in `output` is drawn in
  `TEXT_WEAK` with ` (missing)` appended.
- A manifest that does not decode shows its error as the canvas message.

### Material view

Selecting a compiled material (`*.amat`) shows the record field by field, in
the [spec](../spec/amat.md)'s order:

- Header line: `ACMT v1   <size>`.
- Columns: **Offset** (bytes), **Field** (the spec's name), **Value**:
  floats with 3 decimals; `base_color` and `emissive` start with a 12 × 12 px
  swatch of the color (sRGB-encoded, alpha ignored); `flags` as `0x05` and
  their meaning, e.g. `(mask, double-sided, parallax)`; texture IDs as
  `0x…` followed by the file the manifest maps them to, `(not in manifest)`,
  or `—` for `0`.
- A material that does not decode shows its error as the canvas message.

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

## Compiled data

Compiled schemas, data, graphs and trees (`.atype`, `.arec`, `.agph`,
`.atre` in the Output tree) are read only. Selecting one shows:

- **Info**: Kind (`Compiled schema`, `Compiled data`, `Compiled graph`,
  `Compiled tree`), Size, the counts of its descriptor (Types, Fields and
  Values; Types and Instances and Values; Nodes, Edges and Properties;
  Nodes, Properties and Depth), Asset path and Asset ID.
- The canvas: its [Chunks view](models.md#chunks-view), every chunk as a
  table: one row per record, one column per field of the record's spec
  (hashes in hex, text read from `STRINGS`, colors as `#rrggbbaa`, a
  value's kind by name), and `STRINGS` as offset / text rows.

A compiled animation (`.aanim`) keeps its [player](models.md#compiled-animation-preview)
and also offers **Chunks** in the view bar.

## Mipmaps tab

Shown for a compiled texture (`.ktx2` in the Output tree) in assetc's own
format (see [`ktx2.md`](../spec/ktx2.md)) with more than one mip level. It
switches which level the [canvas](canvas.md) shows.

- One full-width row per level, 28 px tall, inside 8 px margins: `Level <n>`
  (14 px, `TEXT`) and, right-aligned, its size `W × H` (13 px, `TEXT_WEAK`).
  Rows look and react like the [Animations tab](models.md#animations-tab)'s:
  `BG_HEADER` on hover, the chosen one filled with `ACCENT` at 25% opacity.
- Level 0 is chosen to start with. The choice is remembered per file (by
  path) for the session; a rescan keeps it while the level still exists.
- The canvas shows the chosen level as the image, in 2D and 3D, with the
  toolbar (tiling, channels) working as usual. Switching levels keeps the
  image's size on screen (the zoom is scaled by the size ratio, or the view
  stays fitted), so levels can be compared in place; the zoom bar's
  percentage is in the shown level's pixels.
- The Info tab keeps describing the whole file (Dimensions are level 0's).

## Assets tab

Lists the assets a file contains, as recorded in `<output>/assets.aman`
(see [`docs/spec/aman.md`](../spec/aman.md)).

- Enabled only for compiled files (`.ktx2`, `.amat`, `.amesh`, `.aanim` or `.afont` selected in the Output
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

## Loading

Inspecting a file (reading and decoding it, generating a `.noise` image,
loading an environment map's faces, parsing a LUT, rendering a font
specimen) runs on a background
thread, so the UI never waits for it. There is no file watching.

- **When**: the selection changes, after a rescan (button or finished build),
  after a file operation that touches the selection, and after an edit in the
  Parameters / Modifiers tabs is saved.
- **Latest wins**: at most one inspection runs at a time, and at most one
  waits. A new request replaces the waiting one (each inspection rebuilds
  everything, so only the newest matters). When the running one finishes
  while another waits, its result is dropped and the waiting one starts.
- **While loading**: a new selection shows weak "Loading…" in the Inspector
  and the canvas until its result arrives. Re-inspecting the same file keeps
  showing the current result (canvas, Info, tabs) until the new one replaces
  it; edits in progress in the Parameters / Modifiers tabs are kept.
- The status bar shows the [loader](gui.md#layout) while an inspection runs.

## Keybindings

None.
