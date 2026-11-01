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
| `*.png`, `*.jpg`, `*.jpeg` (any case) | texture | `<asset path>.ktx2`, see [`ktx2.md`](../spec/ktx2.md) |
| `*.noise` (any case) | texture, generated | `<asset path>.ktx2`, see [`noise.md`](noise.md#build) |
| `*.gradient` (any case) | texture, generated | `<asset path>.ktx2`, see [`gradients.md`](gradients.md#build) |
| `*.mat.toml`, plus suffixed texture groups | material | `<dir>/<name>.mat`, see [`materials.md`](materials.md) |
| anything else      | —          | ignored, counted as skipped                   |

Texture sources that belong to a material are compiled as that material
describes (slot color space, ORM packing), see [`materials.md`](materials.md).

Textures are decoded to 8-bit RGBA (other bit depths and color types are
converted; JPEGs, having no alpha, get alpha 255; baseline and progressive
JPEGs are supported). Color space and mipmaps follow the `[textures]` settings, see
[`textures.md`](textures.md). No compression yet.

## Outputs

```
<output>/
  assets.manifest            see docs/spec/manifest.md
  <asset path>.ktx2          one per texture (incl. <name>.orm.ktx2), same directory structure as input
  <dir>/<name>.mat           one per material
```

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
  | material   | `*.mat`        |

  Anything else in `output` (your own files, `.gitkeep`, notes, …) is never
  touched. `assets.manifest` is rewritten, never removed. Entries starting with `.` and the contents of symlinked directories
  are not looked at, as in the file browser.
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
  `assets.manifest` is **not** written, so a manifest never refers to a broken
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
counts textures and materials: `Built 4 textures, 1 material (3 skipped)`.

Errors are printed as `error: <source>: <message>`; exit code is non-zero if
the build failed.

## GUI

- A **Build** button (`hammer` icon) sits in the left sidebar tab strip, left
  of the rescan button. It is disabled while a build is running or when no
  project is open.
- The build runs on a background thread; the UI stays responsive.
- Status bar shows `Building…` while running, then either
  `Built N textures, K materials (M skipped)` (the materials part only when
  there are materials; singular nouns for 1) (with `, K removed` added inside the
  parentheses when stale outputs were removed) or `Build failed: <first error>` (with the
  error count when more than one).
- When the build finishes, both file trees are rescanned.
