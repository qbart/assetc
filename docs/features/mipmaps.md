# Custom mipmaps

A texture's mip chain is [generated](textures.md#mipmaps) unless the texture
has an **`.amipmap`** file next to it. That file says how many levels the
texture gets and lets any of them come from an image prepared by hand
instead of being generated: levels that fade to a flat color, keep alpha
coverage, are sharpened or redrawn for small sizes, or are tinted to debug
which level a GPU samples.

```
textures/
  rock.png            level 0, the texture itself
  rock.png.amipmap    the texture has a custom mip chain
  rock.mip1.png       level 1, by name
  rock.mip2.png       level 2, by name
                      levels 3 and up: generated from level 2
```

[`samples/mipmaps/`](../../samples/mipmaps) is this in small: `tinted.png`
with six levels, each a different color with its number on it (1 and 2 by
name, 3 the chosen `tints/blue.png`, 4 and 5 generated from 3).

## The `.amipmap` file

`<texture file name>.amipmap` (the full file name plus `.amipmap`, as an
[`.amod`](../spec/amod.md) sidecar is named), in the texture's folder; see
[`amipmap.md`](../spec/amipmap.md) for the format.

```toml
levels = 6

[files]
1 = "rock_half.png"
3 = "../shared/gray32.png"
```

- **`levels`**: how many levels the texture is written with, level 0
  included: `1` up to the [full chain](textures.md#mipmaps)'s count. Left
  out: the full chain, down to 1×1. `levels = 1` writes the base level only.
- **`[files]`**: the image of a level, by its number (1 and up), as a path
  relative to the texture's folder.
- An empty file is a full chain whose levels are [by name](#levels) or
  generated.

It applies to textures that are built on their own from an image (`.png`,
`.jpg`, `.jpeg`, `.tga`) or an `.anoise` file, whether a
[material](materials.md) uses them or not.

## Levels

Level 0 is always the texture. Every other level `n` is, in this order:

1. **A chosen file**: `[files] n`.
2. **A file by name**: `<name>.mip<n>.<ext>` in the texture's folder, `<name>`
   the texture's file name without its extension, `<n>` the level in decimal
   and `<ext>` any texture source extension (`rock.mip1.png`,
   `rock.mip2.tga` for `rock.png`; `brick.normal.mip1.png` for
   `brick.normal.png`).
3. **Generated** from the level above it, as every level is without an
   `.amipmap` (see [`textures.md`](textures.md#mipmaps)). The level above may
   itself come from a file, so a chain can be hand-made down to some level
   and generated from there.

A level file:

- must be exactly the level's size, `max(1, floor(width / 2^n))` ×
  `max(1, floor(height / 2^n))` of the texture; it is not resized;
- is loaded as a texture source is (any image format or generated source,
  with the [modifiers](textures.md#modifiers) of its own `.amod` sidecar)
  and stored as it is, in the texture's color space (for a sRGB texture its
  pixels are sRGB);
- has its green channel inverted when the texture's is (a
  [DirectX normal map](materials.md));
- is part of the texture, not a texture of its own: it gets no `.ktx2`, no
  asset ID, and cannot fill a material's slot. A file may be a level of
  several textures. Files by name belong to the texture whether or not
  `levels` reaches them.

Without an `.amipmap`, a file named `rock.mip1.png` is an ordinary texture.

## Build

- The texture is written with the levels above; its line shows how many came
  from files:

  ```
  textures/rock.png -> textures/rock.ktx2 (srgb, 9 levels, 2 custom)
  ```

- The `.amipmap` is the texture's own setting and wins over
  [`no_mipmaps`](textures.md#settings) and a
  [custom material](custom-materials.md)'s `mipmaps = false`.
- The texture is rebuilt when its `.amipmap`, a level file or a level file's
  `.amod` changes.
- An `.amipmap` file is not a source of its own (the build counts it as
  skipped).
- Failures, reported for `<texture file name>.amipmap`:

  | Problem | Message |
  |---------|---------|
  | the file does not parse | the TOML or [format](../spec/amipmap.md) problem |
  | no texture next to it | `no texture <texture file name> next to it` |
  | the texture is a gradient or palette | `gradients and palettes have no mipmaps` |
  | the texture is only packed into another one (an ORM or a custom material's pack) | `<texture> is only packed into another texture, which generates its mipmaps` |
  | the texture is itself a level of another | `<texture> is a mip level of <other texture>` |
  | a chosen file is missing or not a texture source | `level <n> = "<path>" is not a texture source in input` |
  | two files by name for one level | `level <n> has two files by name: <a> and <b>` |

  and, for the texture:

  | Problem | Message |
  |---------|---------|
  | more levels than the full chain | `levels = <n>, but a <w> × <h> texture has at most <max>` |
  | a chosen file for a level the chain does not have | `level <n> (<file>): a <w> × <h> texture has only <max> levels` |
  | a level file cannot be read | `level <n> (<file>): <problem>` |
  | a level file of the wrong size | `level <n> (<file>) is <w> × <h>, not <W> × <H>` |

## Mipmaps tab

The Inspector's [Mipmaps tab](inspector.md#mipmaps-tab) is also shown for an
image or `.anoise` file in the Input tree. It shows the chain the build
writes and edits the `.amipmap`:

- **Custom mip chain** (checkbox): on creates an empty
  `<file>.amipmap`, off deletes it (its settings are lost; level files are
  kept).
- With it on:
  - **Levels**: a number from 1 to the full chain's count, `ACCENT` with a
    Reset button while the file sets `levels` (as the keys of a
    [material](materials.md#material-tab)); Reset is the full chain.
  - A hint: `Levels come from the files chosen below, else from files named
    <name>.mip<n>.<ext> next to the texture, else they are generated from
    the level above.`
- **Levels**: one row per level the build writes, as for a compiled texture:
  `Level <n>`, where it comes from, and its size. The source is the
  texture's file name for level 0, `Generated`, or the name of the file by
  name, in `TEXT_WEAK`; a chosen file's path is in `ACCENT` (as what a file
  sets is). A source too long for the row loses its start to `…`; hovering
  the row says it in full (`rock.mip1.png, by its name`, `rock_half.png,
  chosen`, `Generated from level 2`, `The texture itself`). A level with a
  problem shows the problem under its row in `CHANNEL_R` (and is previewed
  generated).
  Clicking a row shows that level in the canvas, as for a compiled texture;
  level 0 to start with.
- With Custom mip chain on and a level other than 0 chosen, **Level `<n>`
  file**: a dropdown of `By name: <file>` (or `By name: none`), which removes
  the level's `[files]` entry, and every texture source of the input except
  the texture itself, as paths relative to its folder (the ones in that
  folder first, then the rest, each by name); choosing one sets
  `[files] n`. `ACCENT` with Reset while set.
- With Custom mip chain on, **Fill in missing levels** (button, under the
  level's file): writes an image for every level of the chain that has no
  file yet, to start drawing from. Each is the level as the build generates
  it (the level above, [resized](textures.md#mipmaps)), saved as
  `<name>.mip<n>.png` next to the texture, so it is that level's file
  [by name](#levels) from then on.
  - Levels that already have a file, chosen or by name, are left alone, as
    is any file already at that path: nothing is overwritten. A level whose
    file has a problem (the wrong size, unreadable) counts as having one.
  - Only the levels the chain has (`levels`) are written, never level 0.
  - The build's output does not change until a written file is edited.
  - Disabled when every level has a file (hover: `Every level has a file`)
    and while the `.amipmap` is not saved yet. The status bar says how many
    were written (`Wrote 3 level files`).
- When the `.amipmap` does not apply or does not parse, its
  [failure](#build) is shown in `CHANNEL_R` at the top, and the chain is shown
  as the build would write it without the file.
- Without a project input the tab is not shown for sources.
- Edits are saved at once (when nothing is being dragged or typed into);
  the texture is inspected again and the tree rescanned.

The Info tab's **Mip levels** row counts the levels the build writes, and
with an `.amipmap` a **Custom mipmaps** row says how many come from files
(`2 of 9 levels from files`).

## Files

- The `.amipmap` and the level files by name go with their texture when it
  is [moved](file-operations.md#companions) or copied in, and are renamed
  with it (`rock.png` → `stone.png` renames `rock.png.amipmap` →
  `stone.png.amipmap` and `rock.mip1.png` → `stone.mip1.png`). Chosen files
  stay where they are; their paths in `[files]` are
  [fixed](file-operations.md#paths) when either end moves.
- Deleting the texture deletes its `.amipmap`. Level files are kept (they
  are then ordinary textures).
- `.amipmap` rows are dimmed in the file browser like sidecars, with the
  `images` icon.

## Not yet

- Custom levels for packed textures (ORM, a custom material's packs),
  atlases, cubemaps and a model's embedded textures.
- Resizing a level file that is not the level's size, and other filters
  (Kaiser, alpha coverage) for generated levels.
- Listing an `.amipmap` nested under its texture in the file browser.
