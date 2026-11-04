# `.apalette` file format

An `.apalette` file describes one image holding a set of named colors, one per
**cell** of a grid, each cell filled with its color. It is a texture source:
the build compiles the image to KTX2 (see
[`docs/features/palettes.md`](../features/palettes.md#build)). The file is
TOML, UTF-8.

## Written form

```toml
width = 128
height = 128
cell_width = 16
cell_height = 16
layout = "rows"

[[colors]]
name = "Black"
color = "#000000"

[[colors]]
name = "Glass"
color = "#9fd8ff80"
```

assetc writes files exactly in this form: `width`, `height`, `cell_width`,
`cell_height`, `layout`, then the [structs](#structs) when there are any,
then one `[[colors]]` table per color, each preceded by a blank line, with
`name` and `color`, then its `[colors.values.<Type>]` tables.

## Top level

| Key           | Type    | Range             | Default | Meaning                           |
|---------------|---------|-------------------|---------|-----------------------------------|
| `width`       | integer | 1–4096            | 128     | image width, px                   |
| `height`      | integer | 1–4096            | 128     | image height, px                  |
| `cell_width`  | integer | 1–`width`         | 16      | width of each color's cell, px    |
| `cell_height` | integer | 1–`height`        | 16      | height of each color's cell, px   |
| `layout`      | string  | `rows`, `columns` | `rows`  | the order cells are filled in     |
| `colors`      | array of tables | —         | empty   | the colors, in cell order         |

The grid has `columns = floor(width / cell_width)` and
`rows = floor(height / cell_height)`; `colors.len()` must not exceed
`columns × rows` (`<n> colors need <n> cells but the grid has <m>`). Any
other key is an error (`unknown key <key>`), but for the
[structs](#structs) keys `structs`, `color_structs` and `values`.

## Color

| Key     | Type   | Default | Meaning                                                  |
|---------|--------|---------|----------------------------------------------------------|
| `name`  | string | `""`    | label shown in the editor; not used by the image         |
| `color` | string | —       | `#rrggbb` or `#rrggbbaa`, hex digits in any case, straight (not premultiplied) alpha; `ff` when left out |
| `values` | table | none    | this color's [struct values](#structs) |

Errors (counting colors from 1): `color <n>: missing color`,
`color <n>: invalid color "<text>" (use #rrggbb or #rrggbbaa)`,
`color <n>: unknown key <key>`.

## Image

The image is `width × height` RGBA8 (straight alpha), starting fully
transparent (`0 0 0 0`). Color `i` (from 0) fills its whole cell, whose
top-left pixel is `(column × cell_width, row × cell_height)`, placed exactly
like a [gradient's cell](agradient.md#image):

| `layout`  | Order                              | Cell of color `i`                                 |
|-----------|------------------------------------|---------------------------------------------------|
| `rows`    | left to right, then top to bottom  | column `i mod columns`, row `floor(i / columns)`  |
| `columns` | top to bottom, then left to right  | row `i mod rows`, column `floor(i / rows)`        |

Unused cells, and any columns or rows the grid does not cover, stay
transparent. A cell 1 × 1 px wide gives a plain lookup strip: e.g.
`width = 16`, `height = 1`, `cell_width = 1`, `cell_height = 1` holds 16
colors, texel `i` being color `i`.

## Structs

[Attached structs](../features/structs.md) of the file and of each color:

- **`[[structs]]`**: one table per attachment, in order: `schema` (the
  schema's path relative to this file's folder, forward slashes) and `type`
  (the struct's name), both strings; any other key is an error.
- **`[values.<Type>]`**: the file's values of the attached struct `<Type>`,
  one key per field it sets, sparse and normalised as an
  [instance's](adata.md#written-form), each value in
  [TOML form](../features/structs.md#values-in-toml) (struct values and
  lists inline, `null` as `{ "$null" = true }`). A struct with nothing set
  has no table.
- **`[[color_structs]]`**: attachments every color has, as `[[structs]]`;
  each color's values are its **`[colors.values.<Type>]`** tables, written
  after its `color`.

assetc writes them after the top-level keys, before the first `[[colors]]`: the `[[structs]]` tables, the `[[color_structs]]` tables, then the `[values.<Type>]` tables in
attachment order (struct values inline), each after a blank line:

```toml
[[structs]]
schema = "game.aschema"
type = "Pickup"

[values.Pickup]
mass = 2.5
tags = ["loot", "rare"]
stats = { hp = 40, armor = 2 }
```
