# `.atree` trees

One root node with nested children, each with a position and data; see
[`docs/features/trees.md`](../features/trees.md). UTF-8 JSON in the style
of the [a/1 property format](../aschema.md): every file is valid JSON,
`$`-keys are reserved, and readers keep keys they do not know.

## Top level

| Key        | Required | Value |
|------------|----------|-------|
| `$dialect` | yes      | `"atree/1"`; a missing or other dialect, or a newer major version, makes the file unreadable |
| `root`     | yes      | The root [node](#node) |

Other top-level keys are kept, after these.

## Node

| Key        | Required | Value |
|------------|----------|-------|
| `id`       | yes      | Identifier (`[A-Za-z_][A-Za-z0-9_]*`), unique in the whole tree |
| `label`    | no       | String shown on the node; written only when not empty |
| `color`    | no       | `"#rrggbb"`, the node's color in editors; written only when set |
| `x`        | yes      | Number: the center's X, in grid units (right is positive) |
| `y`        | yes      | Number: the center's Y, in grid units (down is positive) |
| `data`     | no       | Object of values, as in [`.agraph`](agraph.md#data); written only when not empty |
| `children` | no       | Array of nodes under it, in order; written only when not empty |

## Written form

UTF-8, 2-space indent, one property or array item per line, `\n` line
endings and a trailing newline; keys in the order of the tables above, then
any other keys the object had. Positions are written as numbers, integers
without a decimal point (`2`, `1.25`).

```json
{
  "$dialect": "atree/1",
  "root": {
    "id": "root",
    "label": "Root",
    "x": 0,
    "y": 0,
    "children": [
      {
        "id": "dash",
        "color": "#3b82f6",
        "x": -1,
        "y": 1.5,
        "data": {
          "cost": 2
        }
      }
    ]
  }
}
```
