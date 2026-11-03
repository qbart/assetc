# Pack

A **pack** bundles the whole build into one file, so a game opens a single
file instead of thousands: a table of contents, then every output file's bytes
(format: [`docs/spec/pak.md`](../spec/pak.md)). The idea follows `assetc
--pack` of [assetc-cpp](https://github.com/qbart/assetc-cpp); its options come
from the [`[pack]`](../spec/assetc.toml.md#pack) table of `assetc.toml`.

## What is packed

Packing always builds first (as [`assetc build`](build.md) does); a failed
build packs nothing and leaves an existing pack as it was. Then it lists the
output directory as the build does (entries starting with `.` skipped,
symlinks not followed) and takes every file that

- matches one of `include` (default `["**"]`: everything),
- matches none of `exclude` (default `[]`),
- is not a pack itself (`*.pak`, any case, anywhere in `output`).

Patterns are globs with the syntax of [`[textures]`](textures.md#settings),
matched against the path relative to `output` (`models/Fox.amesh.bin`).
The manifest (`assetc.aman.bin`) is packed like any other file, so the game
can resolve asset IDs from the pack alone.

Each entry gets:

- **Path**: relative to `output`, `/`-separated.
- **Kind**: from the file's name: the [manifest kind](../spec/aman.bin.md)
  of its extension (texture `.ktx2`, material `.amat.bin`, mesh, animation,
  font, schema, data, graph, tree, atlas), **manifest** for
  `assetc.aman.bin`, **other** for anything else.
- **Asset ID**: the manifest's ID for the file when the manifest lists it
  (a texture's ID hashes its asset path without `.ktx2`), otherwise the
  [FNV-1a hash](build.md#asset-ids) of its path. A game can look an asset up
  by ID without reading the manifest.

## Output

The pack is written to `<output>/<file>` (`file` default `assetc.pak`), next
to the files it holds, so it shows in the Output tree. It is written to a
temporary file beside it first and renamed, so a reader never sees half a
pack. Packing the same files with the same options gives byte-identical
packs.

A pack is not a build output: [stale output removal](build.md#removing-stale-outputs)
never deletes it (a pack left behind after `file` changed stays until you
delete it).

Targets pack their own output with their own `[pack]` settings
(`[targets.<name>.pack]`, merged key by key).

## CLI

```
$ assetc pack
… (the build's lines, as assetc build prints them)
Built 13 textures, 2 materials, 1 mesh, 1 animation, 1 font (0 skipped) into /p/build
Packed 19 files (6.2 MiB) into /p/build/assetc.pak
  layout     32 B header + 1.1 KiB contents + 6.2 MiB data (412 B alignment)
  textures      13   5.9 MiB
  materials      2   544 B
  meshes         1   236.2 KiB
  animations     1   53.0 KiB
  fonts          1   9.8 KiB
  manifest       1   336 B
```

- `assetc pack [--target <name>]`: build, then pack; the target as in
  [`assetc build`](cli.md#assetc-build).
- After the build's output, prints `Packed <n> files (<size>) into <path>`,
  then the layout (header, contents table, data, bytes of alignment padding)
  and one line per kind with entries: name, count, bytes, in the order
  textures, materials, meshes, animations, fonts, schemas, data files,
  graphs, trees, atlases, manifest, other.
- A failed build exits non-zero without packing, as `assetc build` does; a
  failed pack (an invalid `[pack]` setting, a file that cannot be read or
  written) prints `error: <message>` and exits non-zero.

## GUI

- **Build → Pack** (`Ctrl+Shift+B`, `Cmd+Shift+B` on macOS) builds and
  packs on a background thread, like [Build](build.md#gui); disabled while a
  build or pack runs or without a project.
- The status bar shows `Building…`, then `Packing…`, then
  `Packed <n> files (<size>) into <file>`, or the failure (`Build failed:
  …` / `Pack failed: …`). Both file trees are rescanned when it finishes.

## Inspector

Selecting a `*.pak` file in the Output tree:

- **Info** rows: Kind `Pack`, Size, **Format** (`APAK v1`), **Entries**,
  **Alignment** (`16 B`), **Layout** (`32 B header + 1.1 KiB contents +
  6.2 MiB data`), **Padding** (alignment bytes), then one row per kind
  with entries, as `<count> · <size>` (`Textures  13 · 5.9 MiB`).
- **Canvas**: a [data table](inspector.md#data-tables) of its entries, header
  `APAK v1   <n> entries   <size>` and a **Filter path…** field: columns
  **#**, **Kind**, **Path**, **Size**, **Offset** (byte offset in the pack),
  **Asset ID** (`0x` + 16 hex digits); rows in contents order (by path). An
  entry whose file no longer matches the loose file in `output` (missing,
  or a different size) is drawn in `TEXT_WEAK` with ` (changed)` or
  ` (missing)` after its path, a sign the pack is older than the build.
- A pack that does not read (bad magic, unknown version, an entry beyond the
  end of the file) shows its error as the canvas message.
- The file tree shows `*.pak` files with the `package` icon.
