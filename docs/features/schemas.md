# Schemas

A **schema** (`*.aschema`) defines struct and enum types for game data, in
the [a/1 property format](../aschema.md): which fields a `Weapon` has, their
types, defaults, limits and how an editor lays them out. assetc edits schema
files; data files (`*.adata`) that hold instances of the types come later.

The file format is [`docs/aschema.md`](../aschema.md); what assetc writes is
[`docs/spec/aschema.md`](../spec/aschema.md).

## Creating

In the Input tree's context menu, **New data ▸** (a submenu after New
Palette) has **Schema**, which creates `schema.aschema` (`schema 2.aschema`,
… if taken) holding one struct `Item` with one field `name` of type
`string`, selects it and starts renaming it, as **New Gradient** does (see
[`file-operations.md`](file-operations.md)).

The file tree shows `.aschema` files with the `braces` icon.

## Inspector

### Info tab

For a schema in the Input tree: **Kind** `Schema`, **Dialect** `a/1`,
**Types** (`2 structs, 1 enum`; `none`), **Fields** (all structs' fields
together) and **Problems** (`none`, or `<n> errors`). A file that cannot be
read as a schema (invalid JSON, a missing or unknown `$dialect`, a newer
major version, no `types`) has an **Error** row instead of the last four.

### Types tab

The Inspector's **Types** tab (`braces`) lists the selected schema's types,
to choose the one the [canvas](#canvas) edits:

- **Add struct** and **Add enum** (`plus` and text): a struct `Struct`
  (`Struct2`, … if taken) without fields, or an enum `Enum` (`Enum2`, …)
  with one value `value`, added at the end and chosen.
- One row per type, in file order, 28 px: a **drag handle**
  (`grip-horizontal`) that reorders the types (their order in the file),
  the kind (`struct` / `enum`, 11 px, `TEXT_WEAK`, 44 px wide), the name, and
  right-aligned in `TEXT_WEAK` its size (`3 fields`, `1 value`); a type with
  problems has a 6 px dot in `CHANNEL_R` before its size. Hovering fills the
  row with `BG_HEADER`; the chosen row is filled with `ACCENT` at 25%.
  Clicking a row chooses it. The chosen row has **Delete** (`trash-2`) on
  its right; fields of other types that use it then show errors.
- The first type is chosen to start with; the choice is kept per file for
  the session.
- With no types, a weak `No types`.

## Canvas

Selecting a schema in the Input tree shows its **editor** in the canvas
(no view bar choices), on `BG_CENTER`, scrolling, with 16 px margins:

- A file that cannot be read as a schema: its error, then its text
  (monospace, read only). Nothing can be edited until the file is fixed.
- No chosen type: a weak `No types: add one in the Inspector's Types tab`.

The chosen type:

- **Header**: the kind (`struct` / `enum`, `TEXT_WEAK`), **Name** (text,
  monospace) and **Doc** (text, the rest of the row).
  - A new name that is not an identifier (`[A-Za-z_][A-Za-z0-9_]*`),
    shadows a primitive or generic, or is another type's name reverts when
    the field loses focus. Renaming rewrites every type expression of the
    file that names the type (`ref<Weapon>`, `list<Weapon>`, …).

### Struct

**Fields**: a table, one row per field in file order, under a header row
(12 px, `TEXT_WEAK`):

| Column  | Editor |
|---------|--------|
| (grip)  | drag handle: reorders the fields |
| Name    | text, monospace; an invalid or taken name reverts when it loses focus |
| Type    | text, monospace, `CHANNEL_R` while it does not parse or names an unknown type; its `▾` button lists the primitives, the file's types, and `list<…>`, `optional<…>`, `ref<…>` around the current type |
| Default | depends on the type, see [Defaults](#defaults); empty when the field has none, its hint the implicit default |
| Min, Max, Step | numbers; only for integer and float types (disabled otherwise) |
| Group   | text (`Stats/Combat` nests) |
| Label   | text |
| Doc     | text, the rest of the row |
| (more)  | chevron: unfolds the row's layout keys |
| (delete)| `x`: removes the field |

An unfolded row shows, under it: **Order** (number, empty for none),
**Readonly** (checkbox), **Show if** and **Hide if** (text, see
[conditions](../aschema.md#24-conditions-show_if--hide_if)) and **Widget**
(dropdown: none, `toggle` for enum fields, `inline` for struct fields), and
the default's own editor when it has one (curves, gradients).

The field's problems are listed under its row in `CHANNEL_R`, 12 px.

**Add field** (`plus` and text) appends `field` (`field2`, …) of type
`string`.

**Groups**: one row per entry of the struct's `groups`: name (text),
**Collapsed** (checkbox), **Doc** (text) and remove (`x`); **Add group**
adds `Group` (`Group2`, …). Groups that fields name without an entry need
none.

### Enum

**Values**: one row per value, in order: drag handle, the value (text,
monospace; an invalid or taken value reverts) and remove (`x`, disabled for
the last value). **Add value** appends `value` (`value2`, …).

### Defaults

The Default cell edits the field's `default` in its type's data encoding:

- `bool`: dropdown `—` (none), `false`, `true`.
- An enum: dropdown `—` and its values.
- Integers and floats: a number field (empty for none).
- `string`, `rgb`, `rgba`: text, unquoted (`#c8c8c8` for colours, with a
  swatch before the field when it is a colour).
- `curve`: a 60 × 18 px preview of the curve (or `—`) that unfolds the row;
  the unfolded row has **Default** with **Set** (a linear curve) or the
  [curve editor](#curve-editor) and **Clear**.
- `gradient`: a 60 × 18 px preview (or `—`) that unfolds the row; the
  unfolded row has **Set** (black to white) or the
  [gradient editor](gradients.md#gradient-editor) and **Clear**.
- Anything else (structs, lists, optionals, refs): JSON text.

A default that does not encode as the type is kept as typed and reported as
a problem.

### Curve editor

A curve is points `[x, y, in, out]` over `x` 0..1 with slopes as tangents,
evaluated as [matma](https://crates.io/crates/matma)'s cubic **Hermite**
between neighbouring points (each segment's tangents are the slopes times
its width), flat before the first point and after the last:

- An area as wide as the row and 140 px tall, `BG_PANEL`, with a 1 px
  `STROKE` frame and lines at `x` = 0.25, 0.5, 0.75; `y` spans the curve's
  range (at least 0..1) plus 10%.
- The curve as a 2 px `ACCENT` line; each point a 8 px square in `TEXT`, the
  chosen one in `ACCENT` with its tangent handles: 1 px lines to 6 px dots
  40 px away along the in and out slopes.
- **Press a point**: chooses it; **drag** moves it (`x` clamped between its
  neighbours and to 0..1). **Drag a handle**: sets that slope; the other
  follows unless the point's tangents are broken (`Shift` while dragging
  breaks them; a point whose two slopes differ starts broken). Whether
  tangents are joined is not saved.
- **Double-click** the area: adds a point on the curve there; double-click a
  point: removes it (a curve keeps at least one point).
- Under the area: the chosen point's **X**, **Y**, **In**, **Out** (number
  fields, three decimals).

## Problems

Every schema error of [§5](../aschema.md#5-validation) is checked whenever
the file changes: names (identifiers, no `$`, no shadowing of primitives or
generics, unique), kinds, struct `fields`, enum `values` (non-empty,
unique), type expressions (parse, known names, `ref<T>` only of structs, no
`optional<optional<T>>`), direct embedding recursion, defaults that do not
encode as their type, `min` / `max` / `step` on non-numeric fields,
conditions naming no sibling or ordering against a non-number, and widgets
on types they do not fit. Each problem has a path (`Weapon/damage`) and a
message.

Below the editor, **Problems** lists all of the file's problems (path and
message, `CHANNEL_R`); clicking one chooses its type. Problems never block
saving.

## Saving

Edits are written once nothing is being dragged or typed, like gradients,
in the [written form](../spec/aschema.md#written-form), and the file is
inspected again. Unknown keys of the file, its types, fields and groups are
kept.

## Build

Schemas are not built yet: the build skips them.

## Not yet

- `.adata` files: instances, references, the project index, refactoring data
  when fields, values or types are renamed.
- Migrating files of an older major version.
