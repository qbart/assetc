# `.aschema` schemas

Type definitions in the [a/1 property format](../aschema.md), which defines
the file: its keys, type expressions, defaults and validation. This page
only states how assetc writes the files it edits (see
[`docs/features/schemas.md`](../features/schemas.md)).

## Written form

- UTF-8 JSON, 2-space indent, one property or array item per line, `\n`
  line endings and a trailing newline.
- Top level: `$dialect` (`"a/1"`), `types`, then any other keys the file
  had, in their order.
- `types`: one entry per type, in the order of the Inspector's
  [Types tab](../features/schemas.md#types-tab).
- A struct: `kind`, `doc` when not empty, `groups` when it has any (each
  `collapsed` when true, `doc` when not empty, then other keys), `fields`,
  then other keys of the type.
- A field: `name`, `type`, `optional` (only `true`), then when set `default`, `min`, `max`, `step`,
  `doc`, `group`, `order`, `label`, `readonly` (only `true`), `show_if`,
  `hide_if`, `widget`, then other keys of the field, in their order.
- An enum: `kind`, `doc` when not empty, `values`, then other keys.
- Numbers keep their form: an integer stays an integer (`10`, not `10.0`).
- Colours in defaults are written in lower case, `rgba` with 8 digits;
  curve points with all four numbers.

```json
{
  "$dialect": "a/1",
  "types": {
    "Item": {
      "kind": "struct",
      "fields": [
        {
          "name": "name",
          "type": "string"
        }
      ]
    }
  }
}
```
