# Instances

A **data file** (`*.adata`) holds **instances** of a
[schema](schemas.md)'s structs: the weapons, enemies and items a game
reads, each a `$type`, an `$id` and the values of its fields. assetc edits
data files with an editor built from the schema, so every value obeys the
schema's rules: types, defaults, limits, optional fields, readonly fields,
conditions, groups and widgets.

The file format is [§4 of the a/1 format](../aschema.md#4-data-file-adata);
what assetc writes is [`docs/spec/adata.md`](../spec/adata.md).

## Creating

In the Input tree's context menu, **New Data ▸ Instance** creates
`data.adata` (`data 2.adata`, … if taken) with no instances, selects it and
starts renaming it, as **New Data ▸ Schema** does (see
[`schemas.md`](schemas.md#creating)). Its `$schema` is the first `.aschema`
(by name) in the same folder, else the first in the input directory (by
input-relative path), else `""`; the [Instances tab](#instances-tab)
changes it.

The file tree shows `.adata` files with the `package` icon.

## Schema

`$schema` is a path relative to the data file's folder, with forward
slashes. The schema is read together with the data file, each time it is
inspected. While it cannot be read (no `$schema`, no such file, or a file
that is not a [readable schema](schemas.md#info-tab)), the data file cannot
be edited: the canvas shows `Schema <path>: <error>` (`CHANNEL_R`) and the
file's text (monospace, read only), and only the Instances tab's Schema
dropdown works.

A file that is not a data file at all (invalid JSON, a missing or unknown
`$dialect`, a newer major version, no `instances` array, an instance that
is not an object) shows its error and text the same way, with nothing to
edit.

## Inspector

### Info tab

For a data file in the Input tree: **Kind** `Data`, **Dialect** `a/1`,
**Schema** (the `$schema` text, `—` when empty, with ` (not found)`
appended when it cannot be read), **Instances** (count) and **Problems**
(`none`, `<n> errors`, `<m> warnings` or `<n> errors, <m> warnings`). An
unreadable file has an **Error** row instead of the last three.

### Instances tab

The Inspector's **Instances** tab (`package`), in the
[property editor](gui.md#property-editor) look:

- **Schema**: a dropdown of every `.aschema` in the input directory, by
  path relative to the data file's folder (`game.aschema`,
  `../defs/game.aschema`); choosing one rewrites `$schema`.
- **Add instance** (`plus` and text, a menu of the schema's structs, in
  the Types tab's order; disabled while the schema cannot be read or has no
  structs): appends an instance of that struct with only `$type` and
  `$id`, chosen. Its id is the struct's name in snake case (`Weapon` →
  `weapon`, `FireBall` → `fire_ball`), with `2`, `3`, … appended while
  another instance of that struct has it.
- One row per instance, in file order, 28 px, as the
  [Types tab](schemas.md#types-tab)'s rows: a **drag handle** that
  reorders the instances, the type (11 px, `TEXT_WEAK`, 80 px wide, cut to
  fit), the id, and right-aligned the [problem icon](#problems) when the
  instance has problems. Hovering fills the row with `BG_HEADER`; the
  chosen row is filled with `ACCENT` at 25% and has **Delete** (`trash-2`)
  on its right. Clicking a row chooses it.
- The first instance is chosen to start with; the choice is kept per file
  for the session.
- With no instances, a weak `No instances`.

## Canvas

Selecting a data file shows the chosen instance's **editor** in the canvas
(no view bar choices), on `BG_CENTER`, scrolling, with 16 px margins; with
no instance chosen, a weak `No instances: add one in the Inspector's
Instances tab`.

**Header** (a row): the type (monospace, `TEXT_WEAK`), **Id** (text,
monospace, 200 px; an id that is not an identifier, or that another
instance of the same type has, reverts when the field loses focus), and
the problem icon of the instance's own problems (its `$type`, its `$id`,
unknown keys).

**Fields**: the struct's fields, sorted by `order` (lowest first; no
`order` is 0; ties keep the schema's order):

- **Groups**: fields without a `group` first, then one section per group,
  in the struct's `groups` order, then groups only fields name, in the
  order fields first use them. A section is a header row (`chevron-down` /
  `chevron-right` and the group's name, 14 px, `TEXT_WEAK`; its `doc` on
  hover) that folds and unfolds its fields; a group with `collapsed` starts
  folded. Folding is kept per file and group for the session.
- **Conditions**: a field whose `show_if` does not hold, or whose `hide_if`
  holds, has no row. Conditions use the
  [resolved](#values) values of its sibling fields, as
  [§2.4](../aschema.md#24-conditions-show_if--hide_if) says.
- **A row** (28 px): the field's **label** (its `label`, else its name; 160
  px less the row's indent, so values line up in one column; cut to fit; `TEXT` when this instance sets the field, `TEXT_WEAK`
  when it inherits it; hovering shows `name: type` and the field's `doc`),
  then, for an optional field, a checkbox (ticked: it has a value;
  unticked: `null`, and no editor), then the value's
  [editor](#editors), then **Reset** (`rotate-ccw`, only when the instance
  sets the field: back to the inherited value), then the problem icon of
  the field's problems.
- **Readonly** fields show their editor disabled.

### Values

A field the instance does not set **inherits** its value: the field's
`default`, else its type's implicit default
([§4.2](../aschema.md#42-sparse-storage)). Inside a struct value, a field
inherits from the enclosing default first: `Monster.weapon`'s default
`{ "damage": 12 }` gives an instance's `weapon.damage` 12, not `Weapon`'s
own `damage` default. The editor always shows the **resolved** value, set
or inherited.

Editing a value sets it in the instance. When the file is saved it is
[normalised](../aschema.md#44-unknown-keys-and-normalisation): a value equal
to the one it would inherit is left out, so setting a field back to its
inherited value is the same as Reset.

### Editors

As the schema's [default editors](schemas.md#defaults), with the schema's
rules:

- `bool`: a checkbox.
- An enum: a dropdown of its values; with `"widget": "toggle"`, a row of
  buttons, one per value, the current one filled `ACCENT` with `BG_BAR`
  text.
- Integers: a drag field of whole numbers, kept within the type's range and
  `min` / `max`, moving by `step` (else 1) per 4 px dragged.
- Floats: a drag field (three decimals), kept within `min` / `max`, moving
  by `step` (else 0.01) per px dragged.
- `string`: text.
- `rgb`, `rgba`: the swatch (double-click: color picker) and hex text.
- `curve`, `gradient`: the preview; double-click opens the
  [curve editor](curves.md#curve-editor) or the gradient editor.
- A struct: `n of m set` (`TEXT_WEAK`) and a fold button
  (`chevron-right` / `chevron-down`, folded to start with); unfolded, its
  fields as rows under the row, indented, with the same rules (sorted by
  `order`, conditions, optional, readonly, Reset), without group headers.
  With `"widget": "inline"` it is always unfolded, without a fold button.
- A list: `n items` and, under the row (indented), one 28 px row per item:
  a **drag handle** (moves the item within the list), the index (`[0]`,
  monospace), the item, and **Remove** (`x`); then **Add item** (`plus`
  and text), appending the item type's implicit default. Editing any item
  sets the whole list.
  - An item of a plain type (number, text, color, enum, …): its editor in
    the row, with the field's rules (range, step, toggle).
  - A **struct** item: a fold button (`chevron-right` / `chevron-down`;
    items start **folded**, the fold state kept per item for the session),
    the struct's name (`TEXT_WEAK`) and, to tell items apart at a glance,
    the item's first two **colors**: 14 px swatches (over a checkerboard) of
    its first two `rgb` / `rgba` fields in field order, resolved; or, with
    no color fields, a 48 × 14 px preview of its first `gradient` field.
    Unfolded, its fields as rows under it, with the same rules as the
    instance's own fields (each showing its resolved value, Reset, no Set
    checkboxes); what an item does not set falls back to its struct's
    defaults.
  - A list item: `n items` and its own items under it.
- `ref<S>`: a dropdown of `—` (`""`) and every instance of `S` in the
  [project](#references): this file's first, in file order, then the other
  files', by file path then file order, each with its file's
  input-relative path right-aligned in `TEXT_WEAK`; a stored id that is
  none of them stays shown. After it, **Go to** (`link-2`, 16 px icon
  button, `TEXT_WEAK`, tooltip `Go to <file>: <id>`; only when the id is
  found) chooses the target: in this file, that instance; in another, it
  selects that file in the Input tree with that instance chosen.
- `asset<K>`: a dropdown of `—` (`""`) and every file of kind `K` in the
  input directory (input-relative paths, sorted), as for schema defaults.

## References

A `ref<S>` field points at an instance instead of holding a copy of its
values: a `Monster`'s `weapon` names the `Weapon` instance `rusty_sword`,
and editing the sword reaches every monster that uses it. The struct is
chosen in the schema with **Type ▸ Reference ▸ Instance**
([type menu](schemas.md#type-menu)); data stores the target's `$id`
([§4.3](../aschema.md#43-refs)).

**The project** of a data file is every `.adata` file in the input
directory whose `$schema` names the same schema file (compared as resolved
paths), found as the file browser lists files (entries starting with `.`
skipped, symlinks not followed). A ref may name an instance of any of
them, and ids are unique per type across all of them
([§4.1](../aschema.md#41-ids-and-the-project)). The project is read from
disk whenever the file is inspected, with the edited file's own instances
taken as edited.

## Problems

Every data check of [§5](../aschema.md#5-validation) is checked whenever
the file or its schema changes, against the [project](#references) for
ids and refs:

| Check | Level |
|-------|-------|
| `$type` not a struct of the schema; `$id` missing or not an identifier | error |
| The same `$type` and `$id` twice in the file | error |
| An instance whose `$type` and `$id` an instance of another file of the project has (`Weapon/sword is also in <file>`) | error |
| A value that does not encode as its field's type (a `null` is fine in an optional field; an `asset<K>` path not of kind `K`) | error |
| A number outside the field's `min` / `max` | warning |
| A ref field without a default left out or `""` (not optional) | warning |
| An `asset<K>` path that does not exist in the input | warning |
| A ref to an id that no instance of that type in the project has (`no Weapon "sword" in the project`) | warning |
| A key that is not a field of its struct (on an instance or a nested struct value); kept | warning |

The problem icon is `bug`, 16 px: `CHANNEL_R` when there is an error,
`ACCENT` for warnings only; hovering lists the messages, warnings prefixed
`warning: `. It marks a field's row, the instance header and the instance's
row in the Instances tab (all of its problems). Problems never block saving.

## Saving

Edits are written once nothing is being dragged or typed, like schemas, in
the [written form](../spec/adata.md#written-form), and the file is
inspected again. Keys that are not fields are kept.

## Build

Each data file is compiled to `<asset path>.adata.bin` (asset kind `data`,
asset path its input-relative path without the extension, plus `.adata.bin`), a
[binary file](../spec/adata.bin.md) of its instances with **every field
resolved**, so a game reads typed values with no defaults to apply.
A file that cannot be read, whose schema cannot be read, or that has
errors, fails the build: `<message>` of the first error, `(and <n> more)`
when there are more; warnings do not. The CLI line reads `<source> ->
<output> (<n> instances)`.

Each ref is compiled with where its target is: the asset ID of the
`.adata.bin` holding it and its index there (the `REFS` chunk), so a game
loads the files a data file depends on and follows a ref without looking
ids up. Since that index changes when another file of the project
changes, an [incremental build](cli.md#assetc-watch) compiles a data file
again whenever any `.adata` file changes.

## Not yet

- Renaming an id everywhere it is referenced; listing what references an
  instance.
- Applying schema refactorings (renamed fields, values, types) to data
  files.
- Undo.
