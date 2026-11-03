# Atlases

An **atlas** glues many images into one texture, with a metadata file that
tells a game where each image (a **frame**) is: sprite sheets, flipbook
animations, icon sets. assetc builds one from a folder.

Binary metadata: [`docs/spec/aatlas.md`](../spec/aatlas.md); settings:
[`docs/spec/atlas.toml.md`](../spec/atlas.toml.md).

## Source

- A folder named `*.atlas` (any case) in the input directory is an atlas,
  for example `fx/explosion.atlas`. Its **asset path** is the folder's
  input-relative path without `.atlas` (`fx/explosion`).
- Its **frames** are its direct children that are texture sources (`png`,
  `jpg`, `jpeg`, `tga`, any case), **sorted by file name** (byte order, so
  `frame_01.png` … `frame_10.png` play in order); a frame's **name** is its
  file name without the extension. Sub-folders and other files are ignored.
- Images inside an atlas folder are not built on their own.
- `atlas.toml` inside the folder holds the atlas' settings (all optional);
  without it the defaults apply.

## Settings

| Setting | Default | Meaning |
|---------|---------|---------|
| Packing | Grid | **Grid**: every frame in a cell of one size, left to right, then top to bottom (a flipbook). **Pack**: frames at their own sizes, packed tightly. |
| Cell width, height | 0 | Grid: the cell size in px; `0` is the largest frame's width (height). A frame larger than its cell is scaled down to fit (aspect kept, Lanczos-3), and centered in it. |
| Columns | 0 | Grid: cells per row; `0` is `ceil(√frames)`. |
| Padding | 2 | Pixels around every frame (between frames and to the atlas' edge, so neighbours never bleed when sampled or mipmapped). |
| Extrude | 0 | Pixels of each frame's edge repeated outward into its padding (at most the padding). |
| Trim | off | Pack: crop each frame to its pixels with alpha above 0 before packing; the crop's offset in the source is kept in the metadata. |
| Power of two | off | Round the atlas' width and height up to powers of two. |
| Max size | 4096 | The largest side allowed; a bigger atlas fails the build. |

**Packing** (Pack): frames are placed largest height first (ties: wider
first, then name), on shelves left to right, each shelf as tall as its first
frame; the atlas width is the smallest power of two, at least the widest
frame plus padding, that keeps the atlas no taller than it is wide (or the
max size). Every frame keeps its own name and order index (its place in the
name-sorted list), wherever it lands.

Color space and mipmaps come from [`[textures]`](../spec/assetc.toml.md#textures),
matched against the atlas' folder path with its extension
(`fx/explosion.atlas`), as for textures.

## Build

| Output | Asset kind | Asset path |
|--------|------------|------------|
| `P.ktx2` | texture | `P` |
| `P.aatlas` | atlas | `P.aatlas` |

An atlas folder without frames, frames that fail to decode, or an atlas
larger than the max size fail the build. The CLI line reads
`<folder> -> P.aatlas (12 frames, 512 × 256 px, grid 4 × 3)` (or `pack`).

## Inspector

- The file tree shows `*.atlas` folders and `.aatlas` files with the `map`
  icon; the frames inside show as usual.
- **Info** for an atlas folder: Kind `Atlas`, **Frames** (count), **Size**
  (`W × H px` of the packed atlas), **Packing** (`grid 4 × 3` or `pack`),
  Asset path, Asset ID, Color, Mip levels and **Output** (as for
  textures); an **Error** row when it cannot be packed.
- **Canvas**: the packed atlas as an image (zoom, pan, channels, as for
  textures), with each frame's rectangle outlined (1 px `ACCENT` at 60%)
  and, on hover, its name and index.
- **Atlas** tab (`map`): the settings above in the
  [property editor](gui.md#property-editor) look (Packing a dropdown, the
  numbers drag fields, the switches checkboxes; Cell width, Cell height and
  Columns only for Grid, Trim only for Pack). Edits are written to
  `atlas.toml` (created on the first edit) once nothing is being dragged or
  typed, and the atlas is packed again.
- A compiled `.aatlas` in the Output tree shows Frames, Size and Packing in
  Info and its [Chunks view](models.md#chunks-view).
