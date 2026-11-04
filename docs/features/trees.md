# Trees

A **tree** (`*.atree`) is nodes placed on a 2D grid, each with its own
data, with **one root** and any number of children under each node, any
depth: a skill tree, a dialogue's branches, a menu, an outline. There are
no edges: a node's place in the tree is its parent. assetc edits trees
exactly as it edits [directed graphs](graphs.md), with the tree's rules.

File format: [`docs/spec/atree.md`](../spec/atree.md).

## Node

| Field  | Meaning |
|--------|---------|
| Id     | An identifier (`[A-Za-z_][A-Za-z0-9_]*`), unique in the tree. |
| Label  | Free text shown on the node; the id when empty. |
| Color  | None, or one of the [preset colors](graphs.md#node-colors), for telling nodes apart at a glance. |
| X, Y   | Its center, in grid units (one cell is 1 × 1); X grows right, Y down. |
| Data   | Key / value pairs, as the [model's properties](properties.md#property) (Bool, Int, Float, Text). |
| Parent | The node it is under; the root has none. |
| Children | The nodes under it, in order. |

## Creating

In the Input tree's context menu, **New Data ▸ Tree** creates `tree.atree`
(`tree 2.atree`, … if taken) holding one root `root` (label `Root`) at 0, 0,
selects it and starts renaming it, as **New Data ▸ Schema** does (see
[`schemas.md`](schemas.md#creating)).

The file tree shows `.atree` files with the `list-tree` icon.

## Canvas

As the [graph canvas](graphs.md#canvas): the grid, the view (zoom, pan,
fit), nodes as circles with a body and a rim, colors, hovering, moving
(`Shift` snaps), in-place label editing, `Delete` / `X`. The differences:

- **Links** instead of edges: one from each node to each of its children,
  the graph edges' S-shaped curve (spread the same way when there are
  several), 1.5 px `TEXT_WEAK`, **without an arrowhead**, and no label.
  Links are not chosen, detached or labelled.
- The **root** has a 2 px `TEXT_WEAK` border (`ACCENT` when chosen).
- **Drag a node's rim**:
  - onto **empty grid**: adds a child of that node there (`node`,
    `node_2`, …, label empty), chosen; while dragging, a rubber link (`ACCENT`
    at 60%, 1.5 px) runs from the node's border to the pointer;
  - onto **another node**: moves that node, with everything under it, to
    be the last child of the first one. The root, the dragged-from node
    and its own ancestors are no targets (that would make a loop): they are
    not tinted and dropping on them does nothing. A target is shown as in
    graphs (tinted, ringed, the link drawn as it will be).
- **Double-click empty grid**: adds a child of the chosen node (of the root
  when no node is chosen) there, chosen.
- **`Delete` / `X`**, and Delete in the Inspector, remove the chosen node
  with everything under it; the root cannot be removed.
- The hint reads `Double-click to add a child · double-click a node to
  rename · drag a node's rim to add a child or onto a node to move it under
  · Delete or X removes · Shift snaps to 0.25`.

## Inspector

### Info tab

For a tree in the Input tree: **Kind** `Tree`, **Dialect** `atree/1`,
**Nodes**, **Depth** (levels, the root alone is 1) and **Problems**
(`none`, or `<n> errors`); an unreadable file has an **Error** row
instead.

### Tree tab

The Inspector's **Tree** tab (`list-tree`), in the
[property editor](gui.md#property-editor) look, as the
[Graph tab](graphs.md#graph-tab):

- Nothing chosen: the outline, one row per node in depth-first order,
  indented 16 px per level (id and label, weak; clicking one chooses it and
  centers the view on it), and **Add child** (`plus` and text, a child of
  the root at the view's center).
- A node: the header row (**Node**, and **Delete** at the top right,
  disabled for the root); **Id**, **Label**, **Color** (swatches: none and
  the presets), **X**, **Y**, **Parent** (a dropdown of every node it can
  move under: not itself nor any node under it; `—` and disabled for the
  root), **Data**, its **Children** (one row each, clicking one chooses
  it) and **Add child** (a child 1 unit below it).

### Structs tab

The [Structs tab](structs.md#structs-tab) (`boxes`) attaches
[structs](structs.md) to the tree and to every node (Tree and Node
structs), and edits the values of what is chosen: the tree's with nothing
chosen, else the chosen node's.

## Problems

Reading checks ids (identifiers, unique). A file with problems is still
shown and edited; its problems are listed in the Tree tab (`CHANNEL_R`, 12
px).

## Build

Each tree is compiled to `<asset path>.atree.bin` (asset kind `tree`, asset
path its input-relative path without the extension, plus `.atree.bin`), a
[binary file](../spec/atree.bin.md) of its nodes, breadth first, with its
[attached structs](structs.md#build). A file that cannot be read,
or has [problems](#problems) or struct errors, fails the build with the first one (`(and <n>
more)`). The CLI line reads `<source> -> <output> (<n> nodes, depth <d>)`.
