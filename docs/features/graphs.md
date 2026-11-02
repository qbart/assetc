# Directed graphs

A **directed graph** (`*.agraph`) is nodes placed on a 2D grid, each with
its own data, joined by directed edges that can carry data too: a skill
tree's skills and unlocks, a dialogue's lines and choices, a state machine's
states and transitions. assetc edits them on a grid in the canvas, so a
game gets both the logic (nodes, edges, data) and the layout (positions).

File format: [`docs/spec/agraph.md`](../spec/agraph.md).

## Node and edge

| Node field | Meaning |
|------------|---------|
| Id         | An identifier (`[A-Za-z_][A-Za-z0-9_]*`), unique in the graph; edges refer to it. |
| Label      | Free text shown on the node; the id when empty. |
| X, Y       | Its center, in grid units (one cell is 1 × 1); X grows right, Y down. |
| Data       | Key / value pairs, as the [model's properties](properties.md#property) (Bool, Int, Float, Text). |

| Edge field | Meaning |
|------------|---------|
| From, To   | The nodes it leaves and enters, by id. Any node may connect to any node, itself included, any number of times: every edge is its own, told apart by its place in the file. |
| Label      | Free text, empty to start with. |
| Weight     | A 32-bit signed integer, 1 to start with. |
| Data       | Key / value pairs, as nodes'. |

## Creating

In the Input tree's context menu, **New Data ▸ Directed Graph** creates
`graph.agraph` (`graph 2.agraph`, … if taken) holding one node `start`
(label `Start`) at 0, 0 and no edges, selects it and starts renaming it, as
**New Data ▸ Schema** does (see [`schemas.md`](schemas.md#creating)).

The file tree shows `.agraph` files with the `graph-star` icon.

## Canvas

Selecting a graph in the Input tree shows it in the canvas (no view bar
choices), on `BG_CENTER`:

- **Grid**: a line every 0.25 unit (`STROKE`, only while at least 8 px
  apart), every unit (`TEXT_WEAK` at 30%), and the axes through 0, 0
  (`TEXT_WEAK` at 60%). It fills the canvas.
- **View**: 64 px per unit to start with, the origin at the canvas' center;
  the wheel zooms around the pointer (×1.2 per notch, 8 to 512 px per
  unit), dragging with the right (or middle) button pans,
  double-clicking with the right button fits the graph (its nodes with a
  1-unit margin). The view is kept per file for the session.
- **Nodes**: circles 1 unit across (0.5 unit radius), filled `BG_HEADER`
  with a 1 px `STROKE` border, the label (or id) centered in `TEXT`, 13 px
  at 64 px per unit (scaled, at least 8 px). The chosen node has a 2 px
  `ACCENT` border. Nodes have no ports: the **rim**, the outer 8 px of the
  circle (the outer half while the radius is under 16 px), is where edges
  are dragged from; hovering it draws a 2 px `ACCENT` ring at the border
  and shows the crosshair cursor.
- **Edges**: smooth curves (quadratic Béziers) from their From node's
  border to their To node's border, ending in a 10 px arrowhead along the
  curve; 1.5 px `TEXT_WEAK`, the chosen edge `ACCENT`, 2.5 px. Every edge
  bends to the right of its direction, its middle 0.25 unit off the
  straight line, and each further edge with the same From and To 0.25
  unit more; so edges both ways between two nodes, and repeated edges,
  never overlap. An edge from a node to itself is a loop over the node,
  each further loop of that node larger.
- **Edge labels**: an edge with a label or a weight other than 1 shows
  them at the middle of its curve (`label`, `label · 3`, or `3`), 12 px
  `TEXT_WEAK` (`ACCENT` when chosen) at 64 px per unit (scaled, at least
  8 px, hidden under 6 px), on a `BG_CENTER` backdrop.
- A weak hint in the bottom-left corner: `Double-click to add a node ·
  double-click a node to rename · drag a node's rim onto a node to connect ·
  Shift snaps to 0.25`.

Interactions (left button unless said):

- **Press a node** (inside its rim) chooses it; **dragging** it moves it
  (positions keep 3 decimals). Holding **`Shift`** snaps its center to
  multiples of 0.25.
- **Press a node's rim** and **drag** onto a node: a new edge from the
  first node to that one, chosen. A rubber line follows the pointer from
  the node's border. Released on the node it started from, it adds a loop,
  but only once the pointer has left that node; released elsewhere, nothing
  is added.
- **Click an edge** (within 7 px of its curve) chooses it.
- **Click empty grid** chooses nothing.
- **Double-click empty grid** adds a node `node` (`node_2`, …, label empty)
  there (snapped to 0.25 with `Shift`), chosen.
- **Double-click a node** chooses it and edits its label in place: a text
  field (at least 120 px wide, the node's width when wider) over the node,
  focused, holding the label. **`Enter`** or clicking elsewhere keeps the
  text, **`Escape`** drops it.
- Nodes and edges are removed in the Inspector (no Delete key, which acts
  on the file tree).

## Inspector

### Info tab

For a graph in the Input tree: **Kind** `Graph`, **Dialect**
`agraph/1`, **Nodes**, **Edges** and **Problems** (`none`, or `<n> errors`);
an unreadable file has an **Error** row instead.

### Graph tab

The Inspector's **Graph** tab (`graph-star`) edits what is chosen in the
canvas, in the [property editor](gui.md#property-editor) look:

- Nothing chosen: the node list (one row per node: id and label, weak;
  clicking one chooses it and centers the view on it) and **Add node**
  (`plus` and text, a node at the view's center).
- A node: a header row, **Node** (14 px, `TEXT`) and, at the pane's top
  right, **Delete** (`trash-2` icon button; its edges go too); then **Id**
  (text; an empty, invalid or taken id gets the old one back when it loses
  focus; renaming updates the edges), **Label** (text), **X** and **Y**
  (drag fields, three decimals), **Data** (the property editor), and its
  edges (`→ to` and `← from` rows, with the edge's label when it has one;
  a loop shows `↻`; clicking one chooses it).
- An edge: a header row, **Edge** and **Delete** at the top right as for
  nodes; then **From** and **To** (dropdowns of every node), **Label**
  (text), **Weight** (drag field, whole numbers, −2147483648 to
  2147483647) and **Data**.
- Edits are saved like gradients: once nothing is being dragged or typed,
  then the file is inspected again.

## Problems

Reading checks ids (identifiers, unique) and edges (known nodes). Loops and
repeated edges are allowed. A file with problems is still shown and edited;
its problems are listed under the Info rows' **Problems** count in the
Graph tab (`CHANNEL_R`, 12 px).

## Build

Graphs are not built yet: the build skips them.

## Not yet

- Building graphs into a binary for games, and data typed by a
  [schema](schemas.md).
- Moving several nodes at once, and undo.
