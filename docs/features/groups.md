# Making assets from several files

Select several files in the **Input** tree, right-click them, and turn the
group into one thing assetc can build. A dialog shows how assetc reads the
files, with everything filled in from their names. Accepting it moves,
renames and writes files so that the convention picks the group up.

Only **Material** is made this way so far; see [Other groups](#other-groups)
for what could follow.

## Selecting several rows

In the Input tree (see [`gui.md`](gui.md#file-browser)):

| Input | Action |
|-------|--------|
| Click | Select the row, alone |
| `Ctrl` + click (`Cmd` on macOS) | Add the row to the selection, or take it out |
| `Shift` + click | Select every row from the selected row to the clicked one, in the order shown |
| `Escape` | Keep only the selected row (when more than one is selected) |

- The selected row stays the one the [Inspector](inspector.md) shows and
  `Shift` counts from: `Ctrl` + click and `Shift` + click change only which
  other rows are selected with it. With nothing selected, either selects the
  clicked row.
- All selected rows are highlighted alike.
- Selecting a row any other way (click, arrow keys, after an operation)
  selects it alone. Rows that no longer exist after a rescan leave the
  selection.
- The Output tree selects one row at a time.
- Dragging a selected row [moves every selected row](file-operations.md#several-rows)
  into the folder it is dropped on; dragging a row that is not selected moves
  that row alone.
- **Rename**, **Delete**, `F2` and `Delete` act on one row as before: the
  right-clicked row, or the selected row for keys.

## Context menu

Right-clicking a selected row keeps the selection; right-clicking any other
row selects it alone (as before). When every selected row is a texture
source (`png`, `jpg`, `jpeg`, `tga`, `anoise`, `agradient`, `apalette`, see
[`build.md`](build.md#inputs)), the menu starts with:

| Item | Opens |
|------|-------|
| Make Material… | the [Make Material](#make-material) dialog for the selected files |

followed by a separator and the [usual items](file-operations.md#context-menu).
One file is enough: a single base color texture becomes a material too.

## Make Material

A [dialog](gui.md#dialogs), 560 px wide, titled **Make Material**, that
builds a [PBR material](materials.md) from the selected files. The files are
listed in the order shown in the tree.

### Fields

| Field | Filled with |
|-------|-------------|
| **Name** | the [guessed name](#name) |
| one row per file: its name, a slot dropdown, and the name it will get | the [guessed slot](#slots) |
| **Normal map** (shown when a file is in the normal slot) | `OpenGL (+Y up)`, or `DirectX (+Y down)` when the normal file's name says `dx` or `directx` |
| **Put the files in a new folder** | on, unless the files' folder holds nothing but the used files (with their sidecars, `.amipmap`s and mip level files) |
| **Rename files to the naming convention** | on |

The slot dropdown lists the seven slots (`Base color`, `Normal`, …, see
[`materials.md`](materials.md#slots)) and `Not used`. A file not used stays
where it is, with its name.

The last column shows, in `TEXT_WEAK`, where the file goes, relative to the
files' folder (`brick/brick.n.png` when they move into a new folder), or
`stays` when the file neither moves nor is renamed.

### Name

The longest start the selected files' names (without extension) share, cut
at a separator (`_`, `-`, `.`, space) or the end of a name, then without
trailing separators and trailing resolution words (`1k`, `2K`, `4k`, `8k`, …,
`512`, `1024`, `2048`, `4096`, `8192`). `Metal007_2K_Color.jpg`,
`Metal007_2K_NormalGL.jpg` → `Metal007`; `brick.jpg`, `brick.n.jpg` →
`brick`. With nothing shared (or one file without a slot word), the name of
the first file; when that is empty too, `material`.

### Slots

Each file's slot comes from the rest of its name after the material name,
split into words at separators, at lower-to-upper case changes
(`NormalGL` → `normal`, `gl`) and between letters and digits, and read
from the last word back. The first word in this table decides:

| Words (any case) | Slot |
|------------------|------|
| `basecolor`, `albedo`, `diffuse`, `diff`, `color`, `colour`, `col`, `alb` | Base color |
| `normal`, `normals`, `nor`, `nrm`, `norm`, `n` | Normal |
| `roughness`, `rough`, `rgh`, `r` | Roughness |
| `metallic`, `metalness`, `metal`, `met`, `m` | Metallic |
| `ao`, `occlusion`, `occ`, `ambient` | Occlusion |
| `emissive`, `emission`, `emit`, `glow`, `e` | Emissive |
| `height`, `displacement`, `disp`, `bump`, `h` | Height |
| `arm`, `orm`, `rma`, `mra`, `gloss`, `glossiness`, `spec`, `specular`, `opacity`, `alpha`, `mask`, `cavity`, `preview`, `thumb` | Not used (packed or other maps assetc does not take as they are) |

- A file whose name is the material name exactly (`brick.jpg`) is the base
  color, as in the [naming convention](materials.md#finding-materials).
- A file with no word from the table is not used.
- `gl`, `opengl`, `dx`, `directx` and resolution words are skipped; `dx` /
  `directx` on the normal file sets **Normal map** to DirectX.
- Two files in one slot: the first keeps it and the others are not used,
  except that an OpenGL normal map wins over a DirectX one.

### Problems

Shown below the options in `CHANNEL_R` text, one per line; while there is one,
**Make Material** is disabled:

- `Name the material` (empty name); `The name cannot contain / \ or .`
- `<slot> has more than one file`
- `No file is used`
- `<name> already exists in <folder>`: the `.amat`, a renamed file, or a
  file moving into the new folder would replace an existing entry.

### Making it

**Make Material** (the confirming button) does, in this order:

1. With **Put the files in a new folder**, creates `<folder>/<name>`, where
   `<folder>` is the files' folder (the first file's folder when they come
   from several). An existing folder of that name is used as it is.
2. Moves the used files into the target folder (the new folder, or
   `<folder>`) and, with **Rename files to the naming convention**, renames
   them `<name>.<ext>` (base color) and `<name>.<suffix>.<ext>` for the
   other slots (`brick.n.png`), keeping each file's extension. Each file
   takes its [companions](file-operations.md#companions) with it: its
   `.amod` sidecar, its `.amipmap` and its mip level files by name (renamed
   the same way). Paths in the project that point to the moved files are
   [fixed](file-operations.md#paths), as for a move.
3. Writes `<target folder>/<name>.amat` (see
   [`amat.md`](../spec/amat.md#written-by-assetc)): `normal_y = "dx"` for a
   DirectX normal map and, without renaming, a `[textures]` table with each
   used file's path relative to the `.amat`. With renaming and an OpenGL
   normal map the file is empty: the names say it all.

Then the tree is rescanned, the target folder opened, the `.amat` selected
and inspected, and the status bar shows `Made material <name> from N files`
(`1 file`), plus `; fixed paths in K files` when paths changed. A failure
leaves the files as they were (moves are undone, an empty new folder is
removed) and shows `Make Material failed: <reason>`.

Cancel, `Escape` or clicking outside closes the dialog and changes nothing.

## Other groups

Groups that could be made the same way later (not built yet):

| Selection | Becomes |
|-----------|---------|
| images | an [atlas](atlases.md) folder `<name>.atlas` with the images as frames and a default `atlas.toml` |
| numbered images (`run_01.png`, `run_02.png`, …) | an atlas whose frames keep the number order, for flipbooks |
| one large image and smaller copies | a [custom mip chain](mipmaps.md): `<name>.mip<n>.<ext>` level files and an `.amipmap` |
| six images named `px`, `nx`, `py`, … or `right`, `left`, `top`, … | an [environment map](hdr.md) folder |
| an `.ashader` and images | a [custom material](custom-materials.md) with the images assigned to the shader's textures by name |
| a model and images | a material per image set, assigned to the model's primitives in its `.amod` |
| grayscale images | one texture packing them into R, G, B, A (needs a channel-packing source first) |
| several models | one model taking the others' animations (needs animation import from other files first) |
