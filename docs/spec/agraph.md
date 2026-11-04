# `.agraph` directed graphs

Nodes with positions and data, joined by directed edges with data; see
[`docs/features/graphs.md`](../features/graphs.md). UTF-8 JSON in the style
of the [a/1 property format](../aschema.md): every file is valid JSON,
`$`-keys are reserved, and readers keep keys they do not know.

## Top level

| Key        | Required | Value |
|------------|----------|-------|
| `$dialect` | yes      | `"agraph/1"`; a missing or other dialect, or a newer major version, makes the file unreadable |
| `structs`  | no       | Array of [attachments](#structs): the graph's own structs; written only when not empty |
| `node_structs` | no   | Array of attachments every node has; written only when not empty |
| `edge_structs` | no   | Array of attachments every edge has; written only when not empty |
| `values`   | no       | The graph's [struct values](#structs); written only when not empty |
| `nodes`    | yes      | Array of [nodes](#node), in the order they were added |
| `edges`    | yes      | Array of [edges](#edge), in the order they were added |

Other top-level keys are kept, after these.

## Node

| Key     | Required | Value |
|---------|----------|-------|
| `id`    | yes      | Identifier (`[A-Za-z_][A-Za-z0-9_]*`), unique among the nodes |
| `label` | no       | String shown on the node; written only when not empty |
| `color` | no       | `"#rrggbb"`, the node's color in editors; written only when set |
| `x`     | yes      | Number: the center's X, in grid units (right is positive) |
| `y`     | yes      | Number: the center's Y, in grid units (down is positive) |
| `data`  | no       | Object of [values](#data); written only when not empty |
| `values` | no      | The node's [struct values](#structs); written only when not empty |

## Edge

| Key      | Required | Value |
|----------|----------|-------|
| `from`   | yes      | The id of the node it leaves |
| `to`     | yes      | The id of the node it enters; may be `from` (a loop) |
| `label`  | no       | String; `""` when missing; always written |
| `weight` | no       | Integer from −2147483648 to 2147483647 (`i32`); `1` when missing; always written |
| `data`   | no       | Object of [values](#data); written only when not empty |
| `values` | no       | The edge's [struct values](#structs); written only when not empty |

Edges are told apart by their place in `edges`: any number of edges may
have the same `from` and `to`.

## Data

Each key is a non-empty string; each value is `true` / `false` (Bool), a
JSON integer (Int, `i64`), a JSON number with a fraction or exponent (Float,
`f64`, written with a decimal point, e.g. `1.0`), or a string (Text).
Keys are written sorted.

## Structs

[Attached structs](../features/structs.md) and their values:

- An **attachment** is an object `{ "schema": <path>, "type": <name> }`:
  the schema's path relative to this file's folder (forward slashes) and
  the struct's name, both strings. Other keys are kept.
- **`values`** is an object with a key per attached struct's name, each an
  object of the struct's field values, as an
  [instance](adata.md#written-form) writes them (sparse, normalised; a
  struct with nothing set left out). `values` on the top level are the
  graph's (`structs`), on a node the node's (`node_structs`), on an edge
  the edge's (`edge_structs`).

```json
  "node_structs": [
    { "schema": "game.aschema", "type": "QuestStep" }
  ],
```

## Written form

UTF-8, 2-space indent, one property or array item per line, `\n` line
endings and a trailing newline; keys in the order of the tables above, then
any other keys the object had; an attachment's keys are `schema`, `type`,
then any others. Positions are written as numbers, integers
without a decimal point (`2`, `1.25`).

```json
{
  "$dialect": "agraph/1",
  "nodes": [
    {
      "id": "start",
      "label": "Start",
      "x": 0,
      "y": 0
    },
    {
      "id": "dash",
      "x": 2.5,
      "y": 0,
      "data": {
        "cost": 2,
        "name": "Dash"
      }
    }
  ],
  "edges": [
    {
      "from": "start",
      "to": "dash",
      "label": "unlock",
      "weight": 1
    }
  ]
}
```
