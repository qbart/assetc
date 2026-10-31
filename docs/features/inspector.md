# Inspector

Shows what a file in the file browser is, and what the build does or did with
it. Lives in the bottom half of the right sidebar (see [`gui.md`](gui.md)).

## Selection

- Clicking a **file** row in the Input or Output tree selects it. Clicking a
  folder row still only opens/closes it. Clicking an asset in the Assets panel
  selects its compiled file (see [`assets.md`](assets.md#selection)).
- One file is selected at a time, across both tabs: selecting a file in one
  tab replaces a selection in the other. Switching tabs keeps the selection.
- The selected row is filled with `ACCENT` at 25% opacity and has a 2 px
  `ACCENT` bar on its left edge (hover still uses `BG_HEADER`).
- The selection is kept by path. After a rescan (button or finished build)
  the selected file is inspected again, so the Inspector shows the new
  output of a build. If the file is gone, the Inspector says
  "<name> no longer exists".

## Info tab

With nothing selected: weak "Nothing selected".

Otherwise, from top to bottom:

1. **Preview** (textures only), see [Preview](#preview).
2. **File name** (strong) and its path relative to the project root (weak).
3. **Details**: a two-column grid, weak labels on the left, values on the
   right. Values are selectable text, so an asset ID can be copied.

Content scrolls vertically when it does not fit.

### Details by file kind

The kind is decided by tab and file name, the same way the build decides it
(see [`build.md`](build.md#inputs)):

| Tab    | File                       | Kind               |
|--------|----------------------------|--------------------|
| Input  | `*.png` (any case)         | Source texture     |
| Input  | anything else              | Ignored            |
| Output | `*.ktx2` (any case)        | Compiled texture   |
| Output | `assets.manifest` at the output root | Manifest |
| Output | anything else              | File               |

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

**Manifest:** Version, Entries (count).

**Ignored / File:** Kind and Size only.

### Errors

If the file cannot be read or decoded (corrupt PNG, not a KTX2 file, bad
manifest), the details show the rows that are still known (Kind, Size) and an
**Error** row with the message. There is no preview.

## Preview

- Source texture: the decoded PNG. Compiled texture: level 0. Only `.ktx2`
  files in assetc's own subset (see [`ktx2.md`](../spec/ktx2.md):
  `R8G8B8A8_SRGB` / `R8G8B8A8_UNORM`, no supercompression) are previewed;
  others show the weak text "No preview for this format".
- Pixels are shown as stored, for both color spaces (linear data textures look
  as they would in an image viewer).
- Fitted to the Inspector width minus 8 px margins and at most 240 px tall,
  keeping aspect ratio, centered. Never scaled up beyond 8× its pixel size.
- Images larger than 512 px on either side are downscaled to fit 512 × 512
  (aspect kept) before upload; the Dimensions row still shows the real size.
- Magnified pixels stay sharp (nearest filtering); minified ones are smoothed.
- Drawn over an 8 px checkerboard (`CHECKER_LIGHT` / `CHECKER_DARK`) so
  transparency is visible.

## Settings tab

Not used yet. With a selection it shows weak "No settings for this file";
otherwise "Nothing selected".

## Loading

The selected file is read and decoded on the UI thread when it is selected or
rescanned. There is no file watching.

## Keybindings

None.
