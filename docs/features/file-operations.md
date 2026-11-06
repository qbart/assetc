# File operations

The **Input** tree can change the files it shows: create folders, noise
textures, gradients, palettes, materials and data files, rename, move by
dragging, copy files in by dropping them from outside, and delete. These are
real file-system operations on the project's `input` directory. The **Output** tree is read-only (the build owns
it).

## Context menu

Right-clicking an Input row, or the empty area below the last row (the
`input` directory itself), opens a menu in the [menu style](gui.md#menus):

| Item                 | Target                                    | Shown for          |
|----------------------|-------------------------------------------|--------------------|
| New Folder           | inside the folder, or the file's folder   | always             |
| New Noise ▸ (the `noise/…` generators) | as New Folder, see [`noise.md`](noise.md#creating) | always |
| New Pattern ▸ (the `pattern/…` generators) | as New Folder, see [`noise.md`](noise.md#creating) | always |
| New Color ▸ Gradient | as New Folder, see [`gradients.md`](gradients.md#creating) | always |
| New Color ▸ Palette  | as New Folder, see [`palettes.md`](palettes.md#creating) | always |
| New Material ▸ PBR Material | as New Folder, see [`materials.md`](materials.md#creating) | always |
| New Material ▸ Custom Material | as New Folder, see [`custom-materials.md`](custom-materials.md#custom-materials-1) | always |
| New Material ▸ Shader | as New Folder, see [`custom-materials.md`](custom-materials.md#shader-files) | always |
| New Data ▸ Schema    | as New Folder, see [`schemas.md`](schemas.md#creating) | always |
| New Data ▸ Directed Graph | as New Folder, see [`graphs.md`](graphs.md#creating) | always |
| New Data ▸ Instance  | as New Folder, see [`instances.md`](instances.md#creating) | always |
| New Data ▸ Tree      | as New Folder, see [`trees.md`](trees.md#creating) | always |
| Rename               | the row                                   | rows               |
| Delete               | the row                                   | rows               |

Right-clicking a row also selects it.

## New Folder

Creates `New Folder` (or `New Folder 2`, `New Folder 3`, …) in the target,
opens the parent folder in the tree, selects the new folder and starts
renaming it.

## Rename

- Starts with **Rename** in the menu or **F2** on the selected Input row.
- The row's name becomes a text field with the whole name selected.
  **Enter** or clicking elsewhere commits, **Escape** cancels.
- The new name must be non-empty, must not be `.` or `..`, must not contain
  `/` or `\`, and must not already exist in the folder; otherwise nothing
  changes and the status bar shows why (see [Errors](#errors)). An unchanged
  name does nothing.
- On success the tree is rescanned and the renamed entry stays selected.
- A file's [`.amod`](../spec/amod.md) sidecar is renamed with it
  (`Fox.glb.amod` → `Hero.glb.amod`).
- The paths that point to or from what was renamed are fixed, as for a
  [move](#paths) (renaming a folder changes the path of everything in it).

## Move

Dragging an Input row onto a folder moves the entry there, with its
[companions](#companions), and fixes the paths in the project that point to
or from what moved (see [Paths](#paths)).

- **Dragging** starts once the pointer, pressed on a row, moves a few pixels;
  a press without moving is still a click. Sidecar rows and atlas settings
  rows drag like any row; a sidecar moves with its file when the file exists.
- While dragging, a label follows the pointer: the row's icon and name, and
  `+ N` in `TEXT_WEAK` when `N` companions go with it, on `BG_PANEL` with a
  1 px `STROKE` border and 4 px corners (as a tooltip).
- **Target**: the row under the pointer. A folder row is that folder; a file
  row (or a sidecar row) is the folder it is in; the empty area below the
  rows is the `input` directory. The target folder's row gets a 1 px `ACCENT`
  outline (the `input` directory: the whole tree view). A target that would
  change nothing or is impossible is not outlined, and dropping on it does
  nothing: the folder the entry is in, the dragged folder itself or a folder
  inside it, anything outside the tree.
- Holding the pointer over a closed folder for 0.6 s opens it. Within 24 px of
  the top or bottom of the tree view, the tree scrolls.
- **Escape** or releasing outside a target cancels.
- On success the tree is rescanned, the target folder is opened, the moved
  entry is selected (and inspected), and the status bar shows `Moved <name>
  to <folder>`, plus ` with N more` for companions and `; fixed paths in K
  files` when paths changed (`<folder>` is `input` for the `input`
  directory).
- An entry with the same name in the target (for the entry or any companion)
  fails the move: `Move failed: <name> already exists in <folder>`. Nothing is
  overwritten; if moving a companion fails, what moved before is moved back.
- Moving changes the [asset path](build.md#asset-ids), so the moved assets'
  IDs (and the IDs other assets store for them) change at the next build.

### Companions

Files that only make sense next to another file go with it, from the same
folder (and below, for a `.gltf`):

| Moved file | Goes with it |
|------------|--------------|
| any file | its [`.amod`](../spec/amod.md) sidecar (`Fox.glb.amod`, `brick.jpg.amod`) |
| `<name>.amat` | the [material's files by name](materials.md): `<name>.<ext>` (base color) and `<name>.<suffix>.<ext>` for the slot suffixes, `<ext>` a texture source (`png`, `jpg`, `jpeg`, `tga`, `anoise`, `agradient`, `apalette`) |
| `<name>.acmat` | the files and atlas folders its shader takes [by name](custom-materials.md): `<name>.<texture>.<ext>` for each texture and input the shader fills, and `<name>.<ext>` for its `bare` texture, each of a kind the texture's usage accepts. None when the shader cannot be read. |
| `*.gltf` | the buffer and image files its `uri`s name (not data URIs) in its folder or below, keeping their place relative to it (`textures/a.png` → `<target>/textures/a.png`), unless another `.gltf` in the input uses them too |

Each companion brings its own sidecar too. A folder moves with everything in
it and has no companions. Dragging a texture of a material moves only that
texture.

### Paths

After a move or rename, every path in the project that pointed at a moved
file or folder points at its new place, and the moved files' own relative
paths still point where they did. Only those path strings change: the rest
of each file is kept byte for byte (comments, order, layout), so hand-written
files stay as they were. Paths relative to a file's folder are rewritten
relative to its folder, `/`-separated; input-relative ones stay
input-relative.

| File | Relative to its folder | Input-relative |
|------|------------------------|----------------|
| `.adata` | `$schema` | |
| every file with [attached structs](structs.md) (`.amat`, `.acmat`, `.amod`, `atlas.toml`, `.agradient`, `.apalette`, `.agraph`, `.atree`) | `schema` of each attachment list (`structs`, `node_structs`, `edge_structs`, `gradient_structs`, `color_structs`) | |
| `.ashader` | `params.schema`, `[stages] fragment`, `vertex` | |
| `.acmat` | `shader`, `[textures]` paths (strings or `path` of a table) | |
| `.amat` | `[textures]` paths | |
| `.gltf` | `buffers[].uri`, `images[].uri` (with `%20` for spaces, as the model reader) | |
| assetc sources above, `.aschema` and `.anoise` | | any other string value that is exactly the input-relative path of a moved file or folder: [`asset<K>`](../aschema.md) values (in instances, struct values, `[params]`, schema defaults) and an `.amod` locator's `preview` |
| `assetc.toml` | | `[[locators.presets]] from`, also under `[targets.<name>]` |

- Globs are not rewritten (`[textures]` patterns, `match` lists in
  `assetc.toml`): a moved file may stop or start matching them.
- A relative path that leaves the `input` directory keeps pointing at the
  same place. Absolute paths, data URIs and empty strings are left alone.
- A file that is not valid TOML or JSON is left alone.
- Editors showing a file reload it after a move, so an open editor does not
  write old paths back.

## Dropping files

Files and folders dropped on the window from outside (a file manager) are
**copied** into a folder of the Input tree, with their
[companions](#companions) from where they come from (a `.gltf`'s files are
copied whether or not other models use them).

- The target folder is the Input row under the pointer, as for a
  [move](#move), when the pointer is over the Input tree; otherwise the
  selected Input folder, or the folder of the selected Input file; otherwise
  the `input` directory. While files hover the window, that folder is
  outlined as a move target is.
- Copies are as they are: their paths are not rewritten.
- A dropped entry already inside `input` is [moved](#move) instead.
- On success the tree is rescanned on the Input tab, the target folder is
  opened, the first copy is selected and the status bar shows `Copied <name>
  to <folder>` (` with N more` for the other copies and companions).
- A name already in the target fails the drop before anything is copied:
  `Copy failed: <name> already exists in <folder>`.

## Delete

- **Delete** in the menu, or the **Delete** key on the selected Input row,
  opens a confirmation dialog: `Delete <name>?`, with `This permanently
  deletes it and everything inside it.` for folders or `This permanently
  deletes the file.` for files, and buttons **Cancel** and **Delete** (the
  confirming button), in the [dialog look](gui.md#dialogs). Escape or
  clicking outside cancels.
- Deleting is permanent (not moved to a trash). A file's
  [`.amod`](../spec/amod.md) sidecar is deleted with it.
- On success the tree is rescanned and the selection is cleared.

## Safety

Every operation is refused unless its path is inside the `input` directory
(compared after resolving symlinks and `..`), and the `input` directory itself
can never be renamed or deleted.

## Errors

A failed operation leaves the files unchanged and shows
`<Operation> failed: <reason>` in the status bar (instead of `Ready` or the
last build result) until the next operation or build.

## Keybindings

F2 (rename), Delete (delete), Enter / Escape while renaming, dragging rows
(move), Escape while dragging (cancel); see
[`KEYS.md`](../../KEYS.md). Moving the selection with the arrow keys is
described in [`gui.md`](gui.md#file-browser). The keys only act when no text field has focus
and the left sidebar has the keys: the last mouse press landed in it (not
in the canvas or the Inspector). So `Delete` after clicking a graph's node
in the canvas removes the node, not the file.
