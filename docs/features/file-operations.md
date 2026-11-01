# File operations

The **Input** tree can change the files it shows: create folders, noise
textures and gradients, rename, and delete. These are real file-system operations on the
project's `input` directory. The **Output** tree is read-only (the build owns
it).

## Context menu

Right-clicking an Input row, or the empty area below the last row (the
`input` directory itself), opens a menu in the [menu style](gui.md#menus):

| Item                 | Target                                    | Shown for          |
|----------------------|-------------------------------------------|--------------------|
| New Folder           | inside the folder, or the file's folder   | always             |
| New Noise ▸ (the `noise/…` generators) | as New Folder, see [`noise.md`](noise.md#creating) | always |
| New Pattern ▸ (the `pattern/…` generators) | as New Folder, see [`noise.md`](noise.md#creating) | always |
| New Gradient         | as New Folder, see [`gradients.md`](gradients.md#creating) | always |
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

F2 (rename), Delete (delete), Enter / Escape while renaming; see
[`KEYS.md`](../../KEYS.md). The keys only act when no text field has focus.
