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
- Bottom: the first problem of the edited file (red) or nothing;
  **Cancel** and **Save** (accent) on the right.

## Settings

Each section starts with a short **intro** (13 px, `TEXT_WEAK`, wrapped)
saying what the section controls and how its values are written (see
[Help texts](#help-texts)).

Each setting is a row:

```
Without mipmaps     [ ui/**                ] [x]              [↺]
no_mipmaps          [+ Add pattern]
                    Textures built with no mipmaps: only the full-size
                    image, no smaller copies. …
```

- Left, 200 px: the setting's **name** in plain words (14 px; `TEXT`, or
  `TEXT_WEAK` when the value is a default), and under it its **key** as
  written in `assetc.toml` (11 px monospace, `TEXT_WEAK`), so the file
  can be matched to the dialog.
- Right: the editor, the reset button, and under them the **help**: what
  the setting does, the values it takes (range and default for numbers)
  and, for lists, what one row holds (12 px, `TEXT_WEAK`, wrapped).
- An empty text or pattern field shows an **example** as hint text
  (`ui/**`).

Editors:

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
  with an `x` button to remove it, then **+ Add pattern** (**+ Add level**
  for `lod_ratios`) adding an empty one.
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
  lists the `[meshes]` settings it does not set yet by name (added with the
  value in effect). **+ Override** adds an entry matching nothing yet.
- **Locator presets** (`[[locators.presets]]`): one card per entry with an
  `x` button: `from`, a dropdown of the input's glTF models (`.gltf`,
  `.glb`), and `match`, a glob list (empty: every model, the key left out).
  **+ Preset** adds one.

## Help texts

Section intros:

| Section | Intro |
|---------|-------|
| General | Where assetc reads your source assets and writes the build. Paths are relative to the folder holding assetc.toml. |
| Textures | Each list holds glob patterns, one per row, matched against a texture's path inside the input folder (ui/button.png). `*` matches within one folder or file name, `**` any number of folders, `?` one character, `{a,b}` either. A texture matching any pattern gets the setting; an empty list matches nothing. |
| Meshes | How glTF models are split into meshlets and simplified into levels of detail. The defaults suit most models; overrides below, or a model's .amod sidecar, change them for some models. |
| Locators | A preset shares the locators of one model (placed in its .amod sidecar) with other models whose skeleton has the same joints, so they are placed once. |
| Pack | assetc pack bundles the build into one file. Patterns here match paths inside the output folder (models/Fox.amesh.bin), with the syntax of the Textures section. |

Settings (name, key, help, example):

| Name | Key | Help | Example |
|------|-----|------|---------|
| Input folder | `input` | Folder with your source assets, relative to assetc.toml's folder. Shared by every target. | `assets` |
| Output folder | `output` | Folder the build writes to, relative to assetc.toml's folder. A named target writes to `<output>-<name>` unless it sets its own. | `build` |
| Linear data | `linear` | Textures holding data rather than color (normal maps, masks) are stored linear instead of sRGB. Textures used by a material take this from their slot. | `**/*_n.png` |
| Without mipmaps | `no_mipmaps` | Textures built with no mipmaps: only the full-size image, no smaller copies. For UI and pixel art drawn at their own size; every other texture gets a full mip chain. | `ui/**` |
| 3D LUTs | `volume_luts` | Color grading LUTs (.cube) built as 3D textures; the others become 2D strips. | `grades/**` |
| Meshlet vertices | `meshlet_max_vertices` | Most vertices in one meshlet, 3–256 (default 64). Match the output limit of your mesh shader. | |
| Meshlet triangles | `meshlet_max_triangles` | Most triangles in one meshlet, 4–512 in steps of 4 (default 124). | |
| Cone weight | `meshlet_cone_weight` | 0–1 (default 0.25). Higher keeps each meshlet's normals close together for better back-face culling; lower makes meshlets more compact. | |
| LOD ratios | `lod_ratios` | One row per reduced level of detail: the share of the full triangle count it keeps (0.5 is half), each smaller than the one before, at most 8. No rows: no levels. | |
| LOD error | `lod_error` | 0–1 (default 0.05). How far a simplified mesh may move from the original, as a share of the mesh's size. | |
| Error step | `lod_error_step` | 0–1 (default 0.05). How much the error limit rises on each retry. | |
| Retries | `lod_attempts` | 0–10 (default 3). Retries with a raised error limit while a level cannot get smaller. | |
| Sloppy fallback | `lod_sloppy_fallback` | After the retries, simplify ignoring the mesh's topology (may open seams). Off: the level reuses the previous one. | |
| UV weight | `lod_uv_weight` | 0–100 (default 1). How much stretching of texture coordinates counts against a simplification. | |
| Normal weight | `lod_normal_weight` | 0–100 (default 0.5). How much change in shading (normals) counts against a simplification. | |
| Cross seams | `lod_permissive` | Allow simplifying across UV and normal seams: fewer triangles, small texture shifts at seams. | |
| Lock borders | `lod_lock_border` | Keep vertices on the mesh's open edges in place, so parts that meet stay joined. | |
| Even triangles | `lod_regularize` | Prefer evenly sized triangles, at some cost to accuracy. | |
| Prune islands | `lod_prune` | Remove small disconnected parts while simplifying. | |
| Overrides | `overrides` | Other values for some models: each override lists patterns of model paths and the settings it changes. When several match, the last one that sets a value wins. | |
| Models | `match` (override) | Patterns of model paths inside the input folder this override applies to. | `characters/**` |
| From model | `from` | The model whose locators are shared. | |
| Applies to | `match` (preset) | Patterns of the models that get them. No rows: every model with the joints they use. | `animals/**` |
| Pack file | `file` | Name of the pack inside the output folder, ending in .pak. | `assetc.pak` |
| Alignment | `alignment` | Every packed file starts at a multiple of this many bytes. Larger values suit memory-mapping and GPU uploads; smaller ones waste less space. | |
| Include | `include` | Output files to pack. The default `**` packs everything. | `**` |
| Exclude | `exclude` | Output files left out, even when included. | `**/*.aanim.bin` |

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
