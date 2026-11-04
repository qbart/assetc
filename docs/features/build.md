# Build

Compiles the project's `input` directory into its `output` directory (both
from [`assetc.toml`](../spec/assetc.toml.md)): textures and
[materials](materials.md), with asset IDs, output layout, and the manifest
shared by every asset type.

## Asset IDs

Every compiled asset has a stable 64-bit ID:

```
id = FNV-1a-64(utf8(asset path))
```

- **Asset path** = the source path relative to `input`, `/`-separated, with
  the final extension removed. Case is preserved.
  `assets/textures/Grass.png` → `textures/Grass`.
- FNV-1a 64: offset basis `0xcbf29ce484222325`, prime `0x100000001b3`, applied
  byte by byte.
- The ID does not depend on the source format, so replacing `grass.png` with a
  different image format later keeps the same ID.
- Two sources with the same asset path are a build error, e.g. `a/b.png` and
  `a/b.PNG`, or `a/b.png` and `a/b.jpg`.
- ID `0` is reserved (it means "no texture" in materials); an asset path that
  hashes to it is a build error.

## Inputs

The input directory is walked recursively with the same rules as the file
browser: entries starting with `.` are skipped, symlinks are not followed.

| Source             | Asset kind | Output                                        |
|--------------------|------------|-----------------------------------------------|
| `*.png`, `*.jpg`, `*.jpeg`, `*.tga` (any case) | texture | `<asset path>.ktx2`, see [`ktx2.md`](../spec/ktx2.md) |
| `*.anoise` (any case) | texture, generated | `<asset path>.ktx2`, see [`noise.md`](noise.md#build) |
| `*.agradient` (any case) | texture, generated | `<asset path>.ktx2`, see [`gradients.md`](gradients.md#build) |
| `*.apalette` (any case) | texture, generated | `<asset path>.ktx2`, see [`palettes.md`](palettes.md#build) |
| folder `*.atlas` (any case) | texture and atlas | `<asset path>.ktx2` and `<asset path>.aatlas.bin`, see [`atlases.md`](atlases.md#build) |
| `*.exr` (any case) | texture, cubemap | `<asset path>.ktx2`, see [`hdr.md`](hdr.md#build) |
| `*.cube` (any case) | texture, LUT strip | `<asset path>.ktx2`, see [`lut.md`](lut.md#build) |
| `*.amat`, plus suffixed texture groups | material | `<dir>/<name>.amat.bin`, see [`materials.md`](materials.md) |
| `*.gltf`, `*.glb` (any case) | mesh, plus its materials, textures and animations | `<asset path>.amesh.bin` and more, see [`models.md`](models.md#build) |
| `*.ttf`, `*.otf` (any case) | font, plus its SDF atlas texture | `<asset path>.afont.bin` and `<asset path>.ktx2`, see [`fonts.md`](fonts.md#build) |
| `*.aschema` (any case) | schema | `<asset path>.aschema.bin`, see [`schemas.md`](schemas.md#build) |
| `*.adata` (any case) | data | `<asset path>.adata.bin`, resolved, see [`instances.md`](instances.md#build) |
| `*.agraph` (any case) | graph | `<asset path>.agraph.bin`, see [`graphs.md`](graphs.md#build) |
| `*.atree` (any case) | tree | `<asset path>.atree.bin`, see [`trees.md`](trees.md#build) |
| anything else      | —          | ignored, counted as skipped                   |

Texture sources that belong to a material are compiled as that material
describes (slot color space, ORM packing), see [`materials.md`](materials.md).

Textures are decoded to 8-bit RGBA (other bit depths and color types are
converted; JPEGs, having no alpha, get alpha 255; baseline and progressive
JPEGs are supported; TGAs may be uncompressed or RLE-compressed, true-color,
grayscale or color-mapped, with either origin). Color space and mipmaps follow the `[textures]` settings, see
[`textures.md`](textures.md). No compression yet.

## Outputs

```
<output>/
  assetc.aman.bin            see docs/spec/aman.bin.md
  <asset path>.ktx2          one per texture (incl. <name>.orm.ktx2), same directory structure as input
  <dir>/<name>.amat.bin      one per material
  <model>.amesh.bin          one per glTF model, with <model>/… for its materials and textures
  <model>.aanim.bin          one per skinned, animated glTF model
  <font>.afont.bin           one per font, beside its atlas <font>.ktx2
  <path>.aschema.bin         one per schema, data file, graph, tree and atlas
  (.adata.bin, .agraph.bin, .atree.bin, .aatlas.bin)
```

- **Naming**: every file in assetc's own binary formats ends in `.bin`, after
  the extension of the format: `<name>.<format>.bin`. Where a source and its
  output share a format name, the output is the source's name plus `.bin`
  (`forest.atree` → `forest.atree.bin`). A bare `.a<name>` file is always a
  source, a `.bin` one always compiled. Textures stay `.ktx2`.
- Output directories are created as needed; existing files are overwritten.
- `output` must not be the input directory or contain it (e.g. `output = "."`
  with `input = "assets"`); the build refuses to start, since cleaning could
  otherwise delete files outside assetc's own output.

### Removing stale outputs

After a **successful** build (manifest written), outputs that no source
produces any more, e.g. after a source was renamed or deleted, are removed:

- Only files matching an **output pattern** are candidates. Each asset kind
  declares the extension of the files it writes, and the patterns are exactly
  those extensions, matched case-insensitively:

  | Asset kind | Output pattern |
  |------------|----------------|
  | texture    | `*.ktx2`       |
  | material   | `*.amat.bin`   |
  | mesh       | `*.amesh.bin`  |
  | animation  | `*.aanim.bin`  |
  | font       | `*.afont.bin`  |
  | schema     | `*.aschema.bin` |
  | data       | `*.adata.bin`  |
  | graph      | `*.agraph.bin` |
  | tree       | `*.atree.bin`  |
  | atlas      | `*.aatlas.bin` |

  Anything else in `output` (your own files, `.gitkeep`, notes, …) is never
  touched. `assetc.aman.bin` is rewritten, never removed; a leftover
  `assets.manifest`, `assets.aman` or `assets.aman.bin` (its names before) at the output root is
  removed, as are files with the extensions outputs had before `.bin`
  (`*.amesh`, `*.amat`, `*.aanim`, `*.afont`, `*.atype`, `*.arec`, `*.agph`,
  `*.atre`, `*.aatlas`). Entries
  starting with `.` and the contents of symlinked directories are not looked
  at, as in the file browser.
- A candidate is kept if its `output`-relative path matches an output of this
  build, compared **case-insensitively**, so that on case-insensitive file
  systems an output whose case changed is not deleted right after being
  written.
- Every other candidate is deleted. Directories left empty by a deletion are
  removed too, up to (not including) `output`; directories that were already
  empty, or still contain anything, stay.
- A failed build removes nothing, so the previous manifest keeps matching the
  files on disk.
- If a deletion fails, the build reports it as an error (the new manifest has
  already been written and is valid).

## Failure

- Every source is attempted; failures are collected and reported together.
- If any source fails, or an ID collision is found, the build fails and
  `assetc.aman.bin` is **not** written, so a manifest never refers to a broken
  set of outputs. Individual `.ktx2` files that did succeed are still written.
- Building without a project (`assetc.toml` not found) is an error.
- An invalid glob pattern in `[textures]` is an error; nothing is compiled.
- `output` equal to or containing `input` is an error; nothing is compiled.

## CLI

`assetc build` runs the build for the discovered project:

```
$ assetc build
textures/Grass.png -> textures/Grass.ktx2 (srgb, 10 levels)
textures/dirt.png -> textures/dirt.ktx2 (srgb, 9 levels)
removed textures/old.ktx2
Built 2 textures (1 skipped, 1 removed) into /path/to/project/build
```

Each built asset prints `<source> -> <output> (<details>)`; see
[`materials.md`](materials.md#cli) for material and ORM lines. The summary
counts what was built by kind, see [Summary](#summary).

Errors are printed as `error: <source>: <message>`; exit code is non-zero if
the build failed.

## Summary

After a successful build the CLI's last line and the GUI's status bar read
`Built <counts> (<M> skipped)`, with `, <U> up to date` inside the
parentheses when an [incremental build](cli.md#assetc-watch) left assets up
to date, then `, <K> removed` when stale outputs were removed:

- `<counts>`: each kind of asset built at least once, in this order, as
  `<n> <noun>` joined by `, `: textures (`.ktx2`), materials (`.amat.bin`),
  meshes (`.amesh.bin`), animations (`.aanim.bin`), fonts (`.afont.bin`), schemas
  (`.aschema.bin`), data files (`.adata.bin`), graphs (`.agraph.bin`), trees (`.atree.bin`),
  atlases (`.aatlas.bin`); singular nouns for 1 (`1 mesh`, `2 meshes`, `1 data file`).
  `nothing` when nothing was built.
- Outputs written along with a model or font count too (a model's `.aanim.bin`,
  `.amat.bin`s and textures, a font's atlas), though the CLI prints one line for
  the model or font only.
- Example: `Built 12 textures, 1 material, 1 mesh, 1 animation, 1 font (1 skipped)`.

## GUI

- A **Build** button (`hammer` icon) sits in the left sidebar tab strip, left
  of the rescan button. It is disabled while a build is running or when no
  project is open. Its tooltip shows its shortcut: `Build (Ctrl+B)`.
- `Ctrl+B` (`Cmd+B` on macOS) builds, like the button, from anywhere (even
  while a text field has focus); it does nothing
  when the button would be disabled.
- The build runs on a background thread; the UI stays responsive.
- Status bar shows `Building…` while running, then either the
  [summary](#summary) or `Build failed: <first error>` (with the error count
  when more than one).
- When the build finishes, both file trees are rescanned.
