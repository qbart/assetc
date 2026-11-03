# Config dialog

**Build → Config…** (`Ctrl+,`, `Cmd+,` on macOS) edits the project's
`assetc.toml` in a dialog instead of by hand: every setting the file can hold
(see [`docs/spec/assetc.toml.md`](../spec/assetc.toml.md)), for the default
target and for each named target. It is disabled without a project.

## Layout

A modal dialog in the look of the [Open source](gui.md#open-source) dialog,
880 × 600 px (smaller when the window is):

```
Config                                      /path/to/assetc.toml
Target [ Default      v ]  [+ Target]  [trash]
+-----------+---------------------------------------------------+
| General   | Input        [ samples              ]             |
| Textures  | Output       [ tmp/build            ]        [↺]  |
| Meshes    |                                                   |
| Locators  |                                                   |
| Pack      |                                                   |
+-----------+---------------------------------------------------+
error line                                     [Cancel] [Save]
```

- Title `Config` (20 px), the file's path on the right in `TEXT_WEAK`.
- **Target** dropdown: `Default` (the top-level settings) and every
  `[targets.<name>]`, in file order. **+ Target** adds one: an inline name
  field (letters, digits, `_`, `-`; not taken), `Enter` adds an empty target
  and selects it, `Escape` cancels. The `trash` button (named targets only)
  removes the selected target with its settings, after a confirmation
  (`Remove target "mobile"?`, Remove / Cancel).
- **Sections** on the left (selectable rows, 140 px): General, Textures,
  Meshes, Locators, Pack. The chosen section's settings fill the right side,
  scrolling when long.
- Bottom: the first problem of the edited file (red, `ERROR`) or nothing;
  **Cancel** and **Save** (accent) on the right.

## Settings

Each setting is a row: its name (180 px, the key as written, e.g.
`no_mipmaps`, with the spec's meaning as tooltip), then its editor:

| Section | Setting | Editor |
|---------|---------|--------|
| General | `input` | text (default target only; for a named target shown disabled, "Shared by every target") |
| General | `output` | text |
| Textures | `linear`, `no_mipmaps`, `volume_luts` | glob list |
| Meshes | `meshlet_max_vertices`, `meshlet_max_triangles`, `lod_attempts` | integer drag field within the spec's range (`meshlet_max_triangles` in steps of 4) |
| Meshes | `meshlet_cone_weight`, `lod_error`, `lod_error_step`, `lod_uv_weight`, `lod_normal_weight` | number drag field within the spec's range |
| Meshes | `lod_sloppy_fallback`, `lod_permissive`, `lod_lock_border`, `lod_regularize`, `lod_prune` | checkbox |
| Meshes | `lod_ratios` | number list (each 0–1) |
| Meshes | `overrides` | list of overrides, see below |
| Locators | `presets` | list of presets, see below |
| Pack | `file` | text |
| Pack | `alignment` | dropdown of powers of two, 16 B to 64 KiB |
| Pack | `include`, `exclude` | glob list |

- A **glob list** / **number list** is a column of single-line fields, each
  with an `x` button to remove it, then a `+` button adding an empty one.
- Every value shown is the one in effect: the file's, or the default when
  the file does not set it (in `TEXT_WEAK` then). Editing a value writes it.
- A row whose key the file sets has a **reset** button (`rotate-ccw`,
  tooltip `Reset`) at its right end, removing the key so the default (for a
  named target: the top-level value) applies again.
- For a **named target**, values it does not set show the top-level value
  (in `TEXT_WEAK`); editing one adds it to `[targets.<name>]` (a table key
  such as `no_mipmaps` to `[targets.<name>.textures]`). Lists are replaced
  whole, as the file's rules say. `output` shows `<output>-<name>` when the
  target does not set it.
- **Mesh overrides** (`[[meshes.overrides]]`): one card per entry, in order,
  with an `x` button: its `match` glob list, then the settings it sets, each
  a row as above with an `x` instead of reset; a **+ Setting** dropdown
  lists the `[meshes]` settings it does not set yet (added with the value in
  effect). **+ Override** adds an entry matching nothing yet.
- **Locator presets** (`[[locators.presets]]`): one card per entry with an
  `x` button: `from`, a dropdown of the input's glTF models (`.gltf`,
  `.glb`), and `match`, a glob list (empty: every model, the key left out).
  **+ Preset** adds one.

## Saving

- The dialog edits a copy of the file; nothing is written until **Save**.
- After every edit the copy is checked as loading the file checks it (types,
  unknown keys, every target resolving), plus what a build checks before it
  starts (`[textures]` and `[pack]` globs, `[meshes]` ranges, `[pack]`
  values). The first problem shows at the bottom and Save is disabled while
  there is one.
- **Save** (or `Ctrl+S`) writes `assetc.toml` and closes the dialog. Only the
  edited keys change: comments, key order, formatting and settings the
  dialog does not show are kept. The project is reloaded and both file trees
  rescanned; the status bar shows `Saved assetc.toml`.
- **Cancel** or `Escape` closes the dialog without writing (a click outside
  does nothing, so a stray click loses no edits).
- If `assetc.toml` cannot be read or parsed when the dialog opens, it shows
  only that error and Close.
