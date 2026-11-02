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
  `ACCENT` border. Nodes have no ports. A node has two areas:
  - the **body**, the inner 60% of the radius: pressing it chooses and
    moves the node;
  - the **rim**, from 60% of the radius to 8 px outside the circle: edges
    are dragged from it. Hovering it shows the band (`ACCENT` at 15%), a
    2 px `ACCENT` ring at the border and the crosshair cursor.
- **Edges**: S-shaped cubic Béziers, as Blender's node links, 1.5 px
  `TEXT_WEAK` (the chosen edge `ACCENT`, 2.5 px), ending in a 10 px
  arrowhead along the curve:
  - An edge runs along the **axis** (horizontal or vertical) on which its
    nodes' centers are farther apart. It leaves its From node's border
    heading toward the To node along that axis and enters the To node's
    border along it too; the two inner control points sit half the ends'
    distance on that axis (at least 0.25 unit) out from the ends.
  - Edges between the same two nodes are **spread across the axis**, 0.2
    unit apart (closer when needed to stay within 0.4 unit of the centers'
    line). When edges run both ways, every edge keeps to the right of its
    direction: the k-th edge (from 0) of a direction is (k + 0.5) × spacing
    off. When they all run one way, they are centered: (k − (n − 1) / 2) ×
    spacing for n edges. So edges both ways, and repeated edges, never
    overlap.
  - An edge from a node to itself is a **loop** over the node, each further
    loop of that node larger.
- **Edge labels**: an edge with a label or a weight other than 1 shows
  them at the middle of its curve (`label`, `label · 3`, or `3`), 12 px
  `TEXT_WEAK` (`ACCENT` when chosen) at 64 px per unit (scaled, at least
  8 px, hidden under 6 px), on a `BG_CENTER` backdrop.
- A weak hint in the bottom-left corner: `Double-click to add a node ·
  double-click a node or edge to rename · drag a node's rim onto a node to connect ·
  Delete or X removes · Shift snaps to 0.25`.

Interactions (left button unless said):

- **Press a node's body** chooses it; **dragging** it moves it
  (positions keep 3 decimals). Holding **`Shift`** snaps its center to
  multiples of 0.25.
- **Press a node's rim** and **drag** onto a node (its body or rim): a new
  edge from the first node to that one, chosen. While dragging:
  - the start node keeps its rim ring;
  - with no node under the pointer, a rubber line (`ACCENT` at 60%, 1.5 px)
    runs from the start node's border to the pointer, with an arrowhead
    there;
  - over a node, that node is the **target**: it is tinted (`ACCENT` at
    15%) with a 2 px `ACCENT` ring, and the rubber edge is drawn as the
    edge to it will be (its curve and arrow, `ACCENT`, 2.5 px).
  Released on the node it started from, it adds a loop, but only once the
  pointer has left that node (before that, the start node is no target);
  released elsewhere, nothing is added.
- **Drag an edge's end** to detach it: each edge has two ends, its
  **start** (where it leaves its From node's border) and its **tip** (its
  arrow's point). Hovering within 8 px of an end draws a 4 px `ACCENT` dot
  there and shows the grab cursor; ends come before rims, the nearest end
  first. Pressing one chooses the edge, and dragging detaches that end:
  the edge is drawn from its other end to the pointer (`ACCENT` at 60%,
  arrow at its tip end), and a node under the pointer is the target, as
  when connecting (tinted, ringed, the edge drawn as it will be). Released
  on a node, that end attaches there (the edge's other fields, and its place
  in the file, are kept; onto its other end's node it becomes a loop);
  released on empty grid, the edge is **removed**.
- **Click an edge** (within 7 px of its curve) chooses it.
- **Click empty grid** chooses nothing.
- **Double-click empty grid** adds a node `node` (`node_2`, …, label empty)
  there (snapped to 0.25 with `Shift`), chosen.
- **Double-click a node** chooses it and edits its label in place: a text
  field (at least 120 px wide, the node's width when wider) over the node,
  focused, holding the label. **`Enter`** or clicking elsewhere keeps the
  text, **`Escape`** drops it.
- **Double-click an edge** (within 7 px of its curve) chooses it and edits
  its label the same way: a 120 px text field centered on the middle of
  its curve.
- **`Delete`** or **`X`** removes the chosen node (with its edges) or edge,
  while the canvas has the keys (the last mouse press landed in it, see
  [file operations](file-operations.md#keybindings)) and no text field is
  focused. **Delete** in the Inspector does the same.

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
