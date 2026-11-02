# Schemas

A **schema** (`*.aschema`) defines struct and enum types for game data, in
the [a/1 property format](../aschema.md): which fields a `Weapon` has, their
types, defaults, limits and how an editor lays them out. assetc edits schema
files, and data files (`*.adata`) that hold instances of the types (see
[`instances.md`](instances.md)).

The file format is [`docs/aschema.md`](../aschema.md); what assetc writes is
[`docs/spec/aschema.md`](../spec/aschema.md).

## Creating

In the Input tree's context menu, **New Data ▸** (a submenu after New
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
major version, no `types` object, or a type whose `kind`, `fields`,
`values` or `groups` do not have the shape of [§2](../aschema.md#2-schema-file-aschema))
has an **Error** row instead of the last four.

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
  problems has a [problem icon](#problems) before its size. Hovering fills the
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
  monospace), **Doc** (text, the rest of the row), and the
  [problem icon](#problems) when the type itself has problems (its name,
  its enum values).
  - A new name that is not an identifier (`[A-Za-z_][A-Za-z0-9_]*`),
    shadows a primitive or generic, or is another type's name reverts when
    the field loses focus. Renaming rewrites every type expression of the
    file that names the type (`ref<Weapon>`, `list<Weapon>`, …).

### Struct

A struct is edited as **groups holding fields**: groups are defined first,
fields live inside them.

**Sections**, top to bottom:

- **Ungrouped**: the fields without a `group`; a header (`Ungrouped`, 14 px,
  `TEXT_WEAK`) that cannot be moved or removed, its fields and **Add field**.
- One **group card** per group (`BG_PANEL`, 6 px corners, 8 px padding, 8 px
  apart): the groups of the struct's `groups` in order, then names fields
  use without an entry there, in the order fields first use them. Its
  header row (28 px): a **drag handle** (`grip-horizontal`), **Name** (text;
  renaming moves its fields along; an empty or taken name reverts),
  **Collapsed** (checkbox: editors start the group folded), **Doc** (text,
  the rest of the row), the field count (`TEXT_WEAK`), and **Remove**
  (`x`; its fields become ungrouped). Under it, its fields and **Add
  field**. A name with `/` (`Stats/Combat`) is one group, shown with its
  full name.
- **Add group** (`plus` and text): adds an empty group `Group` (`Group2`, …)
  at the end, as an entry of `groups`.

**Arranging**: dragging a group's handle moves the group among the groups
(the Ungrouped section stays first); while a group is dragged every card
shows only its header row. Dragging a field's handle moves the field within
its group; **Group** in the field's details moves it to another group (to
that group's end). Arranging keeps the fields array in the order shown:
ungrouped fields, then each group's fields, group by group; and gives every
shown group an entry in `groups`, in the order shown. Neither changes data
([§6](../aschema.md#6-versioning-and-refactoring)).

**A field** is a row (28 px; hovering fills it `BG_HEADER`), then its
details when unfolded:

| Row part | Editor |
|----------|--------|
| (grip)   | drag handle: moves the field within its group |
| (details) | `settings` (16 px icon button, `ACCENT` while open, else `TEXT_WEAK`): shows or hides the field's details, where its visibility and dependency logic lives |
| Name     | text, monospace, 140 px; an invalid or taken name reverts when it loses focus |
| Type     | text, monospace, 150 px, `CHANNEL_R` while it does not parse or names an unknown type; its `chevron-down` button (24 × 22 px, framed while hovered or open, the whole area clickable) opens the [type menu](#type-menu) |
| Optional | an **Optional** checkbox (off to start with): the field may hold `null` ([optional fields](../aschema.md#optional-fields)) |
| Default  | a **Default** checkbox, then, ticked, the default's value editor (see [Defaults](#defaults)); unticked, nothing |
| (problems) | the [problem icon](#problems), right-aligned, when the field has problems |
| (remove) | `x`: removes the field |

The details (indented under the row, in the [property
editor](gui.md#property-editor) look) show only what the field's type can
use:

- **Label** and **Doc** (text), every field.
- **Min**, **Max**, **Step** (numbers, empty for none): integer and float
  fields only.
- **Widget** (dropdown `—` / `toggle`): enum fields only; (`—` / `inline`):
  struct fields only. A widget the type cannot use is still shown, as a
  problem, until set to `—`.
- **Group** (dropdown: `—` for none, then the struct's groups).
- **Order** (number, empty for none), **Readonly** (checkbox), **Show if**
  and **Hide if** (text, see
  [conditions](../aschema.md#24-conditions-show_if--hide_if)), every field.

**Add field** (`plus` and text, in each section) appends `field` (`field2`,
…) of type `string`, not optional, without a default, to that section.

#### Type menu

The Type's `chevron-down` button opens a menu of submenus; picking an item
sets the field's type (the text field stays editable for anything else):

| Submenu   | Items |
|-----------|-------|
| Value     | `bool`, `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`, `f32`, `f64`, `string` |
| Color     | `rgb`, `rgba`, `gradient` |
| Curve     | `curve` |
| Struct    | the file's structs, in the Types tab's order (disabled when there are none) |
| Enum      | the file's enums, likewise |
| Reference | disabled, empty: picking `ref<…>` types comes later |
| List      | the Value, Color, Curve, Struct and Enum submenus again (and Reference, disabled); an item `T` there sets `list<T>` |

The current type is marked with a `check` in its submenu.

### Enum

**Values**: one row per value, in order: drag handle, the value (text,
monospace; an invalid or taken value reverts) and remove (`x`, disabled for
the last value). **Add value** appends `value` (`value2`, …).

### Defaults

Ticking a field's **Default** sets its `default` to the type's implicit
default (`false`, `0`, `""`, `#000000`, `#000000ff`, a linear curve, black
to white, the enum's first value, `[]`, `null`, `{}` for a struct, `""` for
a reference) and shows its editor; unticking removes `default`. The editors,
in the type's data encoding:

- `bool`: a checkbox.
- An enum: a dropdown of its values.
- Integers and floats: a number field (applied when it loses focus; text
  that is not a number reverts).
- `string`: text.
- `rgb`, `rgba`: a 16 px swatch (the colour over a checkerboard) and the
  hex text (`#c8c8c8`). **Double-clicking the swatch** opens a color picker
  under it (the [gradients' picker](gradients.md#selected-stop), 240 px
  wide; alpha is kept opaque for `rgb`); clicking outside closes it.
- `curve`, `gradient`: a 60 × 18 px preview (with a 1 px `STROKE` frame).
  **Double-clicking it** opens a popover under it, as the colour picker
  does: the [curve editor](curves.md#curve-editor) (320 px wide) or the
  [gradient editor](gradients.md#gradient-editor) (320 px wide); clicking
  outside or `Escape` closes it.
- A struct: `n of m set` (`TEXT_WEAK`) in the row, and under the row
  (indented, before the details) the **struct's fields**, one 28 px row each
  in the struct's field order, columns under the field row's (each nesting
  level indented further): the name (monospace, 140 px), the type
  (monospace, `TEXT_WEAK`, 180 px: the Type field and its button), a
  **Set** checkbox and, ticked, the
  value's editor (any of these, a nested struct's own rows under its row).
  A struct default is partial ([§3](../aschema.md#3-type-expressions)): an
  unticked field is left out and falls back to the struct's own default.
  Ticking one starts from that field's own default, else its type's
  implicit default. A struct already being shown higher up (an embedding
  loop, itself a problem) is edited as JSON text instead.
- A list (`list<T>`): `n items` (`TEXT_WEAK`) in the row, and under the
  row (indented, in the same columns as a struct's fields) one 28 px row per
  item: a drag handle (moves the item within the list), the index (`[0]`,
  monospace, 140 px, under Name), `T` (monospace, `TEXT_WEAK`, 180 px), the
  item's editor (200 px, so the Remove buttons line up; as above: a struct item's fields and a list item's items
  under its row) and **Remove** (`x`). Then **Add item** (`plus` and text),
  appending `T`'s implicit default.
- Refs, and values that do not have their type's JSON shape (a list default
  that is not an array, a struct default that is not an object): JSON text.

A default that does not encode as the type is kept as typed and reported as
a problem.

### Curve editor

See [`curves.md`](curves.md): the curve model, its evaluation (matma's
cubic Hermite), and the popover editor with presets, a square plot, split
tangents and a playhead.

## Problems

Every schema error of [§5](../aschema.md#5-validation) is checked whenever
the file changes: names (identifiers, no `$`, no shadowing of primitives or
generics, unique), enum `values` (non-empty, unique), type expressions (parse, known names, `ref<T>` only of structs, no
`optional<…>`), a non-boolean `optional`, direct embedding recursion, defaults that do not
encode as their type, `min` / `max` / `step` on non-numeric fields,
conditions naming no sibling or ordering against a non-number, and widgets
on types they do not fit. Each problem has a path (`Weapon/damage`) and a
message.

Problems are shown where they are, as the **problem icon**: `bug`, 16 px,
`CHANNEL_R`; hovering it lists the messages (one per line). It marks a
field's row (that field's problems), the type header (problems of the type
itself), and the type's row in the Types tab (all of its problems).
Problems never block saving.

## Saving

Edits are written once nothing is being dragged or typed, like gradients,
in the [written form](../spec/aschema.md#written-form), and the file is
inspected again. Unknown keys of the file, its types, fields and groups are
kept.

## Build

Schemas are not built yet: the build skips them.

## Not yet

- Refactoring data files when fields, values or types are renamed (see
  [`instances.md`](instances.md#not-yet)).
- Migrating files of an older major version.
