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
- **Canvas**: one of the [views](#views) below; **Entries** by default: a
  [data table](inspector.md#data-tables) of its entries, header
  `APAK v1   <n> entries   <size>` and a **Filter path…** field: columns
  **#**, **Kind**, **Path**, **Size**, **Offset** (byte offset in the pack),
  **Asset ID** (`0x` + 16 hex digits); rows in contents order (by path). An
  entry whose file no longer matches the loose file in `output` (missing,
  or a different size) is drawn in `TEXT_WEAK` with ` (changed)` or
  ` (missing)` after its path, a sign the pack is older than the build.
- A pack that does not read (bad magic, unknown version, an entry beyond the
  end of the file) shows its error as the canvas message.
- The file tree shows `*.pak` files with the `package` icon.

## Views

The canvas view bar (see [`canvas.md`](canvas.md#view-bar)) offers four views
of a selected pack, like the 2D / 3D / Chunks choice of other files:
**Entries**, **Summary**, **Layout** and **Treemap**. The choice is kept for
the session, for every pack. The idea follows the pack tabs of
[assetc-cpp](https://github.com/qbart/assetc-cpp)'s viewer; the three new
views show how the file is occupied, and by what.

Every view colors bytes by what they are:

| Bytes | Color |
|-------|-------|
| header | `#78787F` |
| contents table | `#5F5F96` |
| alignment padding | `BG_HEADER` (Layout only) |
| texture | `#45BFC7` |
| material | `#EB9645` |
| mesh | `#5996EB` |
| animation | `#59C778` |
| font | `#E078AB` |
| schema | `#E0CC59` |
| data | `#A68CE0` |
| graph | `#8CC7A6` |
| tree | `#C7A68C` |
| atlas | `#5FB3E0` |
| manifest | `#B578DB` |
| other | `#838383` |

In Summary, Layout and Treemap:

- Hovering an entry's block shows a tooltip: its kind (in its color), path,
  size, and in Layout its byte offset (`@4096`).
- Clicking an entry's block selects that file in the Output tree (opening
  the folders above it and scrolling to it), when the loose file still
  exists; otherwise nothing happens.
- The entry selected in the Output tree before is not marked: a pack view
  only shows while the pack itself is selected.
- A pack that does not read shows its error as the canvas message, in every
  view.

### Summary

From the top, 16 px from the canvas edges:

1. A title line: the file name in `TEXT` 14 px, then in `TEXT_WEAK`
   `APAK v1 · <size> · <n> entries · alignment <a>`.
2. The **region bar**, 16 px tall across the width: header, contents table,
   data (the entries' bytes) and padding, each as wide as its share of the
   file, in their colors (data in `#50A05A`).
3. A caption: `header 32 B   contents 1.1 KiB   data 6.1 MiB   padding 2.3 KiB
   (0.1% overhead)`, overhead being header, contents and padding over the
   file size.
4. One row per kind with entries, in the [CLI](#cli) order: the kind's name
   (plural, in its color) in a 110 px column, a bar as long as the kind's share
   of the data (on a `BG_HEADER` track), and `<size> (<count>)` in a 140 px
   column.

### Layout

A memory map of the whole file, offset 0 at the top left, read like text:

- The file is cut into rows of equal bytes, 20 px tall with 4 px between
  them, as many as fit in the canvas below the legend (at least 1, at most
  one row per 64 bytes). Each row spans the canvas width minus 16 px on
  each side; its byte range shows in `TEXT_WEAK` 11 px monospace at its
  left (`0x00000000`), in a 90 px column.
- The header, the contents table and each entry are drawn as a block from
  their first to their last byte, wrapping from row to row; alignment gaps
  stay `BG_HEADER`. A block narrower than 1 px is drawn 1 px wide. Each
  entry starts with a 1 px `BG_CENTER` line, so neighbours of one kind stay
  apart.
- The legend above the rows: a 12 px chip and name for header, contents and
  each kind with entries.

### Treemap

The data only (no header, contents or padding), in the canvas minus 16 px
on each side:

- One column per kind with entries, in the [CLI](#cli) order, as wide as its
  share of the data; the kind's name in `TEXT` at the top left of the column
  when the column is at least 40 px wide.
- In each column, one tile per entry, largest first from the top, as tall as
  its share of the kind's bytes; tiles alternate between the kind's color and
  the same color at 82% brightness, with a 1 px `BG_CENTER` outline. A tile at
  least 46 × 16 px shows the file name (without folders) in `BG_CENTER`,
  clipped to the tile.

