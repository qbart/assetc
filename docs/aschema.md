# a property format (a/1)

A JSON dialect for defining struct-like types (`.aschema`) and storing
typed instances of them (`.adata`). It is a straight port of bi's `bi/1`
format (`.bischema` / `.bidata`) with the names changed; every rule below
matches bi's implementation, so files convert by renaming the extension
and swapping the `$dialect` / `$schema` values.

Every file is valid JSON, so `jq`, `git diff` and any JSON loader still
work. The rules below only give the keys their meaning.

| Kind   | Holds                                          | Extension |
|--------|------------------------------------------------|-----------|
| Schema | Type definitions (structs, enums)              | `.aschema` |
| Data   | Instances of schema types with concrete values | `.adata`   |

- A data file points at exactly one schema file.
- A schema file is self-contained: a/1 has no imports.
- Instances are **sparse**: a field that is omitted takes its default from the schema.
- Encoding: UTF-8 with `\n` line endings. Writers use a 2-space indent and one property per line. Readers accept any valid JSON layout.

---

## 1. Header and reserved keys

The first key of every file is `"$dialect": "a/1"`. `a` names the format
and `1` is the major version.

How a reader handles a file:

1. **Pick the kind from the extension.** `.aschema` is a schema, `.adata` is data.
2. **Parse the JSON.** If `$dialect` is missing or unknown, refuse the file. Never guess what it is.
3. **Compare the major version.**
   - Older: offer a migration.
   - Newer: refuse the whole file. Never read part of it.

Any key that starts with `$`, at the top level or on any object, is
reserved for format metadata. Type names, field names and instance keys
must not start with `$`.

Optional `.gitattributes` entry so hosting tools highlight the files as JSON:

```
*.aschema linguist-language=JSON
*.adata   linguist-language=JSON
```

---

## 2. Schema file (`.aschema`)

| Top-level key | Required | Value                               |
|---------------|----------|-------------------------------------|
| `$dialect`    | yes      | `"a/1"`                             |
| `types`       | yes      | Object: type name → type definition |

**Identifiers.** Every name is an identifier matching `[A-Za-z_][A-Za-z0-9_]*`.
This covers type names, field names and `$id` values.

**Type names:**
- Unique within the file.
- May not shadow a primitive (`bool`, `i32`, `string`, `rgb`, …) or a generic (`list`, `ref`).
- PascalCase by convention.
- May be referenced in any order.

**Recursion:**
- Allowed through `ref<T>` or `list<T>`.
- Direct embedding is an error: a struct that contains itself, directly or through a chain of other structs, as plain struct-typed fields.
- `list<S>`, `ref<S>` and [optional fields](#optional-fields) all break the chain, so `{ "name": "next", "type": "Node", "optional": true }` inside `Node` is fine.

### 2.1 `struct`

| Key      | Required | Value                                        |
|----------|----------|----------------------------------------------|
| `kind`   | yes      | `"struct"`                                   |
| `fields` | yes      | Array of field objects, in display order     |
| `doc`    | no       | One-line description                         |
| `groups` | no       | Object: group name → `{ "collapsed": bool, "doc": string }` |

`fields` is an array, not an object, because order matters for display.

Field object:

| Key        | Required | Value |
|------------|----------|-------|
| `name`     | yes | Identifier, unique within the struct, no `$`. It is the key used in data files. |
| `type`     | yes | Type expression (§3) |
| `optional` | no  | `true`: the field may hold `null`, no value (see [optional fields](#optional-fields)); `false` when missing |
| `default`  | no  | Value in the type's data encoding. Replaces the implicit default. |
| `min`, `max`, `step` | no | Numbers. Only allowed on integer and float fields. |
| `doc`      | no  | One-line description |
| `group`    | no  | Section name; `/` nests sections (`"Stats/Combat"`) — layout |
| `order`    | no  | Number; fields sort by it, lowest first; unset = 0; array order breaks ties — layout |
| `label`    | no  | Display text instead of the name — layout |
| `readonly` | no  | `true`: shown but not editable — layout |
| `show_if`  | no  | Condition (§2.4); the row exists only while it holds — layout |
| `hide_if`  | no  | Condition; the row is hidden while it holds — layout |
| `widget`   | no  | `"toggle"` (enum fields only: all values shown, current one marked) or `"inline"` (struct fields only: always expanded) — layout |

**Layout keys.** These are `group`, `order`, `label`, `readonly`,
`show_if`, `hide_if`, `widget` and `groups`. They change how an editor
draws a field and never what the data means. A loader that is not an
editor can ignore them, but it must still keep them when it rewrites a
schema.

**Field identity.** A field is identified only by its `name`. Data files
store values by name, and nothing else points at a field. That means:

- **Renaming a field** is a refactoring: rewrite the key in every data file (§6).
- **Reordering the array** changes display order and nothing else.

### 2.2 `enum`

| Key      | Required | Value                                    |
|----------|----------|------------------------------------------|
| `kind`   | yes      | `"enum"`                                 |
| `values` | yes      | Non-empty array of unique strings        |
| `doc`    | no       | One-line description                     |

Enum values are stored by name, never by index. Reordering `values` does
not change data. Renaming a value is a refactoring.

### 2.3 Minimal schema

```json
{
  "$dialect": "a/1",
  "types": {
    "Rarity": { "kind": "enum", "values": ["common", "rare", "epic"] },
    "Weapon": {
      "kind": "struct",
      "fields": [
        { "name": "name",   "type": "string" },
        { "name": "damage", "type": "i32", "default": 10, "min": 0, "max": 999 },
        { "name": "rarity", "type": "Rarity", "default": "common" }
      ]
    }
  }
}
```

### 2.4 Conditions (`show_if` / `hide_if`)

A condition tests a **sibling** field, meaning another field of the same
struct:

| Form            | Meaning                         |
|-----------------|---------------------------------|
| `f`             | `f == true`                     |
| `!f`            | `f == false`                    |
| `f == v`, `f != v` | equality (JSON value equality) |
| `f < n`, `f <= n`, `f > n`, `f >= n` | numeric comparison; `n` must be a number |

**Parsing rules:**
- Trim whitespace first.
- Try the operators in this order: `==`, `!=`, `<=`, `>=`, `<`, `>`. The first one found splits the text.
- The left side must be an identifier.
- The right side becomes:
  - `true` or `false` → a boolean;
  - anything that parses as a finite number → a number;
  - anything else → a string, with surrounding quotes removed if present.

**Evaluation** uses the sibling's *resolved* value, so a default counts. The
condition is false in two cases: the sibling is missing, or a numeric
comparison has a non-number on either side.

**Schema errors:**
- a condition that names no sibling;
- `<`, `<=`, `>`, `>=` with a non-numeric right side.

---

## 3. Type expressions

A field's `type` is a string with no whitespace inside it:

```
type      := primitive | name | generic
generic   := ( "list" | "ref" ) "<" type ">"
name      := identifier                 ; a key of `types`
primitive := "bool"
           | "i8" | "i16" | "i32" | "i64"
           | "u8" | "u16" | "u32" | "u64"
           | "f32" | "f64"
           | "string" | "rgb" | "rgba" | "curve" | "gradient"
```

Parse each type string once, when the schema loads, into a tree. Check
every leaf name against the primitives and the keys of `types`. An unknown
name is a schema error.

| Expression | Meaning | Data encoding | Implicit default |
|---|---|---|---|
| `bool` | boolean | `true` / `false` | `false` |
| `i8` `i16` `i32` `i64` | signed integer | JSON integer (no fraction, no exponent: `12`, not `12.0`) inside the width's range | `0` |
| `u8` `u16` `u32` `u64` | unsigned integer | same, `>= 0` | `0` |
| `f32` `f64` | float | any JSON number | `0` |
| `string` | UTF-8 text | JSON string | `""` |
| `rgb` | colour, 8 bits/channel | `"#rrggbb"` | `"#000000"` |
| `rgba` | colour with alpha | `"#rrggbbaa"` | `"#000000ff"` |
| `curve` | tuning curve over 0..1 | array of points `[x, y, in, out]` | `[[0,0,1,1],[1,1,1,1]]` (linear) |
| `gradient` | colour stops over 0..1 | array of stops `[t, "#rrggbbaa"]` | `[[0,"#000000ff"],[1,"#ffffffff"]]` |
| `E` (enum) | one of `E.values` | string equal to a value | first value |
| `S` (struct) | embedded value owned by the parent | object of `S`'s fields (sparse) | object of `S`'s defaults |
| `list<T>` | ordered sequence | array of `T` encodings | `[]` |
| `ref<S>` | reference to an instance of struct `S` | string holding the target's `$id` | none: **required** |

**Nesting rules:**
- `ref<T>` requires `T` to be a struct. A primitive, enum or list is an error.
- Anything else composes: `list<ref<Weapon>>`, `list<list<f32>>`, `list<Vec2>`.

### Optional fields

Whether a value may be missing is a property of the **field**, not of its
type: `"optional": true` next to `"type"`. There is no `optional<T>` type,
so every type expression maps onto plain JSON values that any language's
loader can read, without a language's option type.

- An optional field holds `null` or its type's encoding.
- Its implicit default is `null`; a `default` replaces it as for any field.
- `optional` on a field of any type is allowed, refs included.

**Embed or ref?**
- **Embed** (`S`) small, value-like data that each parent owns, such as `Vec2` or a stats block.
- **Ref** (`ref<S>`) things with identity that are shared, so one edit reaches every user.

**Required refs.** A `ref` field without a `default` is required. If an
instance leaves it out, report a warning. An empty string `""` means "not
set yet" and is also a warning. Make the field optional (`"optional":
true`) when a missing ref is legitimate.

**Struct defaults are partial.** A `default` on a struct-typed field may
leave keys out; those keys fall back to the struct's own field defaults,
recursively. The same holds for a struct value stored in data: missing keys
resolve to defaults.

### 3.1 Colours

- `#` followed by 6 (`rgb`) or 8 (`rgba`) hex digits.
- Readers accept upper case; writers emit lower case.
- An `rgba` written with 6 digits reads as opaque (`ff` alpha) and is written back with 8.

### 3.2 Curves

A curve is an array of points. Each point is `[x, y, in_tangent, out_tangent]`,
with tangents given as slopes (Unity's keyframe).

- A point may be shortened to `[x, y]`; missing tangents are `0`.
- A curve has at least two points.
- `x` must be in `0..1`.
- Points must be sorted by `x`; two points may share an `x` (a step).
- Breaking any of these rules is an error.
- Writers always emit all four numbers.
- Whether the two tangents are "joined" is editor state and is never stored.

### 3.3 Gradients

A gradient is an array of stops. Each stop is `[t, "#rrggbb" | "#rrggbbaa"]`;
6 digits read as opaque.

- `t` must be in `0..1`.
- Stops must be sorted by `t`.
- There must be at least 2 stops.
- Breaking any rule is an error.

Evaluation: linear interpolation in sRGB between neighbouring stops; flat
(the end stop's colour) outside the first and last stop.

---

## 4. Data file (`.adata`)

| Top-level key | Required | Value |
|---------------|----------|-------|
| `$dialect`    | yes | `"a/1"` |
| `$schema`     | yes | Path to the `.aschema`, relative to this file's directory, with forward slashes |
| `instances`   | yes | Array of instance objects, in display order |

Instance object:

| Key            | Required | Value |
|----------------|----------|-------|
| `$type`        | yes | Name of a **struct** in the schema (enums cannot be instantiated) |
| `$id`          | yes | Identifier, unique **per type** across all data files that share the schema |
| `<field name>` | no  | The field's value in its data encoding |

### 4.1 Ids and the project

- **Ids are scoped per type.** A `Weapon` and an `Enemy` may both be called `boss`, because every `ref<S>` already names the type to look in.
- **The project** is every `.adata` file whose `$schema` resolves to the same schema file. bi finds them by walking the project root and skipping git-ignored and hidden paths.
- **The index.** Keep a project-wide map `($type, $id) → file`. It powers ref validation, completion, jump-to-target, reverse lookup and id renames.

### 4.2 Sparse storage

- A missing field takes its default: the explicit `default` if there is one, otherwise the implicit one.
- Storing a value equal to its default is legal. Writers should remove it (normalise, §4.4) so diffs show only real overrides.
- This is why changing a default in the schema changes every instance that did not override it.

### 4.3 Refs

- A ref is stored as the target's `$id` string. A `list<ref<S>>` is an array of id strings.
- Cycles are legal (A → B → A), because files store only ids.
- **Two-pass load:**
  1. Register every instance under `($type, $id)`, with defaults applied.
  2. Walk the ref fields and replace each id string with a handle or pointer.

### 4.4 Unknown keys and normalisation

**Unknown keys.** An unknown key on an instance, or on a nested struct
object, is a name that is not a field of its struct. It is usually left
over from a deleted or renamed field. Writers **keep** it and report a
warning. Data is never dropped silently.

**Canonical write order of an instance** (what bi's `:w` produces):

1. `$type`, then `$id`.
2. Any other `$`-keys, in their existing order.
3. Fields in schema array order, but only the ones whose resolved value differs from the field default. Nested struct values are made sparse the same way, and colours are lower-cased and expanded to their canonical length.
4. Unknown keys, in their existing order.

### 4.5 Example

```json
{
  "$dialect": "a/1",
  "$schema": "game.aschema",
  "instances": [
    { "$type": "Weapon", "$id": "rusty_sword", "name": "Rusty Sword", "damage": 12 },
    { "$type": "Weapon", "$id": "dagger", "name": "Dagger", "damage": 7, "rarity": "rare" }
  ]
}
```

`rusty_sword` leaves out `rarity`, so it resolves to `"common"`.

---

## 5. Validation

**Errors** make the file unusable as structured data. An editor falls back
to raw text, and a loader fails. **Warnings** are reported but never block
loading or saving.

| Check | Where | Level |
|---|---|---|
| Invalid JSON; missing/unknown `$dialect`; newer major version | any | error |
| Missing `types` / `instances` / `$schema`; `$schema` file not found | file | error |
| Type name not an identifier, shadows a primitive or generic, or duplicated | schema | error |
| Unknown `kind`; struct without `fields`; enum with empty or duplicate `values` | schema | error |
| Field name not an identifier, duplicated, or `$`-prefixed | schema | error |
| Type expression unparsable or names an unknown type | schema | error |
| `ref<T>` with non-struct `T`; direct embed recursion; `optional` not a boolean | schema | error |
| `default` not encodable as the field type; `min`/`max`/`step` on a non-numeric field | schema | error |
| `show_if`/`hide_if` naming no sibling, or ordering op with non-number; `widget` on a type it doesn't fit; unknown `widget` value | schema | error |
| `$type` not a struct of the schema; `$id` missing or not an identifier | data | error |
| Duplicate `($type, $id)` across the project | data | error |
| Value not encodable as its type (string in `i32`, `300` in `u8`, `12.5` in `i32`, unknown enum value, `"red"` in `rgb`, curve with < 2 points or a point out of `0..1` or unsorted, gradient with < 2 stops) | data | error |
| Number outside `min`/`max` | data | warning |
| Ref to an id that does not exist for that type (dangling) | data | warning |
| Required ref absent or `""` | data | warning |
| Unknown key on an instance or nested struct | data | warning (kept) |
| Deleting an instance that others reference | editor | confirm |

If the schema fails to load, every data file that uses it fails too, and
the schema error is reported once.

---

## 6. Versioning and refactoring

**Versioning.** Only incompatible changes bump the major version (`a/2`).
Adding an optional key (a new field attribute, a new `doc`) or a new
primitive does not. A reader of a/1 **must ignore** unknown non-`$` keys
on type and field definitions **and keep them** when it writes the schema
back.

**Refactorings.** A schema change that affects data must be applied to
every `.adata` file of the schema in the same step:

| Change | Effect on data |
|---|---|
| Rename a field | Rename the key in every instance, including nested struct values and list items of that struct type |
| Rename an enum value | Rewrite the value wherever it is stored |
| Rename a type | Rewrite `$type` of its instances |
| Rename an `$id` | Rewrite the `$id` and every ref to it (scalar and list) |
| Change a field's type | Keep values that still encode; the rest become errors to fix |
| Change a default | No data change; instances without an override pick up the new value |
| Delete a field | Keys stay as unknown-key warnings until pruned |
| Reorder fields | No data change |

**Hand edits.** A rename done by hand, outside a tool, looks like a deleted
field plus a new one. The old values surface as unknown-key warnings, and a
"remap old → new" operation fixes them.

**No field numbers.** Fields have no numeric ids in a/1, because data is
addressed by name everywhere. If a binary export addressed by field number
is ever needed, a/2 introduces ids, and the migration assigns them from
array order.

---

## 7. Full example

`game.aschema`:

```json
{
  "$dialect": "a/1",
  "types": {
    "Rarity": {
      "kind": "enum",
      "values": ["common", "rare", "epic", "legendary"],
      "doc": "Drop tier, drives colour and loot tables"
    },
    "Element": { "kind": "enum", "values": ["none", "fire", "ice", "lightning"] },
    "Vec2": {
      "kind": "struct",
      "fields": [
        { "name": "x", "type": "f32", "default": 0 },
        { "name": "y", "type": "f32", "default": 0 }
      ]
    },
    "Stats": {
      "kind": "struct",
      "doc": "The numbers every living thing has",
      "fields": [
        { "name": "hp",    "type": "i32", "default": 100, "min": 1, "max": 9999 },
        { "name": "armor", "type": "u8",  "default": 0,   "min": 0, "max": 100 },
        { "name": "speed", "type": "f32", "default": 1.0, "min": 0, "max": 10, "step": 0.1 }
      ]
    },
    "Weapon": {
      "kind": "struct",
      "doc": "A thing to hit with",
      "groups": { "Advanced": { "collapsed": true, "doc": "Rarely tuned by hand" } },
      "fields": [
        { "name": "name", "type": "string", "doc": "Shown in the inventory" },
        { "name": "damage", "type": "i32", "default": 10, "min": 0, "max": 999, "group": "Combat" },
        { "name": "element", "type": "Element", "default": "none", "group": "Combat", "widget": "toggle" },
        { "name": "crit_chance", "type": "f32", "default": 0.05, "min": 0, "max": 1, "step": 0.01, "group": "Combat", "label": "Crit chance" },
        { "name": "crit_multiplier", "type": "f32", "default": 2, "min": 1, "max": 5, "step": 0.1, "group": "Combat", "show_if": "crit_chance > 0" },
        { "name": "rarity", "type": "Rarity", "default": "common" },
        { "name": "two_handed", "type": "bool", "default": false },
        { "name": "tags", "type": "list<string>" },
        { "name": "tint", "type": "rgb", "default": "#c8c8c8" },
        { "name": "glow", "type": "rgba", "group": "Advanced" },
        { "name": "falloff", "type": "curve", "group": "Combat" },
        { "name": "trail", "type": "gradient", "group": "Advanced" },
        { "name": "offset", "type": "Vec2", "default": { "x": 0.5 }, "group": "Advanced", "widget": "inline" },
        { "name": "id_hash", "type": "u32", "group": "Advanced", "readonly": true },
        { "name": "notes", "type": "string", "optional": true, "group": "Advanced" }
      ]
    },
    "Enemy": {
      "kind": "struct",
      "fields": [
        { "name": "name", "type": "string", "order": -1 },
        { "name": "stats", "type": "Stats", "widget": "inline" },
        { "name": "weapon", "type": "ref<Weapon>" },
        { "name": "drops", "type": "list<ref<Weapon>>" },
        { "name": "spawn", "type": "Vec2" },
        { "name": "leader", "type": "ref<Enemy>", "optional": true },
        { "name": "boss", "type": "bool", "default": false, "group": "Boss" },
        { "name": "boss_phases", "type": "u8", "default": 1, "min": 1, "max": 3, "group": "Boss", "show_if": "boss" },
        { "name": "boss_music", "type": "string", "optional": true, "group": "Boss", "hide_if": "!boss" }
      ]
    }
  }
}
```

`level1.adata`:

```json
{
  "$dialect": "a/1",
  "$schema": "game.aschema",
  "instances": [
    {
      "$type": "Weapon",
      "$id": "rusty_sword",
      "name": "Rusty Sword",
      "damage": 12,
      "tags": ["melee", "starter"],
      "id_hash": 3141
    },
    {
      "$type": "Weapon",
      "$id": "frost_hammer",
      "name": "Frost Hammer",
      "damage": 40,
      "element": "ice",
      "crit_chance": 0,
      "rarity": "epic",
      "two_handed": true,
      "tint": "#7fd4ff",
      "glow": "#7fd4ff80",
      "falloff": [[0, 0.2, 0, 0], [0.6, 1, 0, 0], [1, 0.4, -1.5, -1.5]],
      "trail": [[0, "#7fd4ffff"], [0.7, "#7fd4ff80"], [1, "#7fd4ff00"]],
      "id_hash": 1618,
      "notes": "Slows on hit"
    },
    {
      "$type": "Enemy",
      "$id": "goblin",
      "name": "Goblin",
      "stats": { "hp": 40, "speed": 1.4 },
      "weapon": "rusty_sword",
      "drops": ["rusty_sword", "frost_hammer"],
      "spawn": { "x": 12, "y": 3 },
      "leader": "goblin_chief"
    },
    {
      "$type": "Enemy",
      "$id": "goblin_chief",
      "name": "Goblin Chief",
      "stats": { "hp": 120, "armor": 10 },
      "weapon": "frost_hammer",
      "drops": ["frost_hammer"],
      "spawn": { "x": 20, "y": 3 },
      "boss": true,
      "boss_phases": 2,
      "boss_music": "chief_theme.ogg"
    }
  ]
}
```

**How the values resolve:**

- **`rusty_sword`**
  - `offset` is `{x: 0.5, y: 0}`: `x` comes from the field default and `y` from `Vec2`'s default.
  - `rarity` is `"common"` (default).
  - `crit_chance` is `0.05`, so `crit_multiplier` is visible.
- **`frost_hammer`**
  - `crit_chance` is `0`, so `crit_multiplier` is hidden.
- **`goblin`**
  - `stats.armor` is `0` (from `Stats`).
  - Its `leader` ref resolves on the second load pass.
- **`goblin_chief`**
  - No `leader` key, so it is `null`.
  - `boss` is true, so `boss_phases` and `boss_music` are visible.

---

## 8. Implementation checklist

1. **JSON parsing.** Preserve object key order so round-trips keep the layout.
2. **Schema loader.**
   - Check the header.
   - Build the type table.
   - Parse type expressions into trees.
   - Check names, nesting rules and embed recursion. To check recursion, do a depth-first walk over plain struct-typed fields only; `list`, `ref` and optional fields break the chain.
   - Check defaults, numeric attributes, conditions and widgets.
3. **`resolve(type, value?)`.** Returns the fully defaulted value. Structs merge recursively; a missing value gives the field default, then the implicit default.
4. **`check(type, value)`.** Produces the errors and warnings from §5, with a path for each, such as `goblin/stats/hp` or `drops/[1]`.
5. **Data loader.**
   - Check the header.
   - Resolve `$schema` relative to the file.
   - Check instances.
   - Register `($type, $id)` in the project index.
   - Resolve refs in a second pass.
6. **Writer.**
   - Normalise (§4.4).
   - Keep unknown keys and unknown schema attributes.
   - Lower-case colours.
   - Write curve points in full.
   - Use a 2-space indent and a trailing newline.
7. **Refactorings (§6).** Apply them to every data file of the schema in one step.

