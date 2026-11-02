# `.adata` data files

Instances of a schema's structs in the [a/1 property format](../aschema.md),
which defines the file (§4): its keys, sparse storage, refs and validation.
This page only states how assetc writes the files it edits (see
[`docs/features/instances.md`](../features/instances.md)).

## Written form

- UTF-8 JSON, 2-space indent, one property or array item per line, `\n`
  line endings and a trailing newline.
- Top level: `$dialect` (`"a/1"`), `$schema` (the schema's path relative to
  this file's folder, forward slashes), `instances`, then any other keys
  the file had, in their order.
- `instances`: one object per instance, in the order of the Inspector's
  [Instances tab](../features/instances.md#instances-tab).
- An instance, normalised as [§4.4](../aschema.md#44-unknown-keys-and-normalisation)
  says:
  1. `$type`, then `$id`;
  2. any other `$`-keys, in their order;
  3. the fields it sets, in the struct's field order, each only when its
     value differs from the value it would inherit (numbers compare by
     value: `10` equals `10.0`);
  4. keys that are not fields, in their order.
- A struct value (a field of struct type, at any depth) is written the same
  way as an instance's fields (3 and 4); one left with no keys is left out.
- A list is written whole when it differs from what it inherits; its items
  of a struct type are written as struct values are, against their
  struct's own defaults, an item left with no keys as `{}`.
- Numbers keep their form: an integer stays an integer (`10`, not `10.0`);
  values of integer fields are written as integers.
- Colours are written in lower case, `rgba` with 8 digits; curve points
  with all four numbers.
- `null` is written for an optional field set to no value when it would
  otherwise inherit a value.

```json
{
  "$dialect": "a/1",
  "$schema": "game.aschema",
  "instances": [
    {
      "$type": "Weapon",
      "$id": "rusty_sword",
      "name": "Rusty Sword",
      "damage": 12
    },
    {
      "$type": "Enemy",
      "$id": "goblin",
      "stats": {
        "hp": 40
      },
      "weapon": "rusty_sword"
    }
  ]
}
```

## Built form

What the [build](../features/instances.md#build) writes: the written form
with every value **resolved**:

- An instance has `$type`, `$id`, then **every field** of its struct in
  field order, each its set value or the value it inherits
  ([§4.2](../aschema.md#42-sparse-storage)); `null` for an optional field
  with no value. Other `$`-keys and keys that are not fields are left out.
- A struct value (at any depth, list items too) has every field of its
  struct the same way, inheriting from the enclosing defaults.
- `$schema` is unchanged: the schema is built to the same relative place.
- Values are in their canonical form (lower-case colors, `rgba` with 8
  digits, curve points with all four numbers, integers as integers).
