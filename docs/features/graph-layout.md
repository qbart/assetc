# Graph and tree layout

The [directed graph](graphs.md) and [tree](trees.md) canvases can choose
several nodes at once, align and distribute them, lay nodes out
automatically, and undo and redo every change. Everything here works the
same in both editors unless said.

## Selection

Besides the **chosen** node or edge (what the Inspector edits), the canvas
has a **selection**: a set of nodes. The chosen node is always in it; an
edge is never selected.

- **Press a node's body**: chooses it and selects only it, unless it is
  already selected with others; then they all stay selected (so dragging
  moves them all), and releasing it without moving selects only it.
- **`Ctrl` + press a node** (`Cmd` on macOS), body or rim: adds it to the
  selection, or takes it out. Added, it is chosen; taken out, the chosen
  node becomes the first selected node left in file order (none: nothing
  chosen). It neither moves nor connects.
- **Drag on empty grid** (left button): a selection box, 1 px `ACCENT`
  border over `ACCENT` at 10%, from the press to the pointer. On release
  the nodes whose centers are inside are selected (with `Ctrl`: added to the
  selection), and the chosen node is the first of them in file order (or
  stays, when still selected). A click without dragging chooses nothing,
  as before.
- **`Ctrl+A`**: selects every node; the chosen node stays (none: the first
  node).
- **`Escape`**: selects only the chosen node, while no label is edited.
- **Click an edge**, or choose one in the Inspector: the selection is
  emptied.
- Drawing: the chosen node keeps its 2 px `ACCENT` border; the other
  selected nodes have a 2 px border of `ACCENT` at 55%.
- **Moving**: dragging a selected node moves every selected node by the
  same amount. With `Shift` the dragged node's center snaps to 0.25; the
  others keep their offsets from it.
- **`Delete` / `X`** (and Delete in the Inspector) removes every selected
  node with its edges; in a tree, every selected node with everything under
  it, the root excepted.
- In a tree, the choice and selection follow their nodes by id when the
  file is read back (its nodes are stored depth first, so their places
  change).

With **two or more nodes selected** the Inspector's Graph (or Tree) tab
shows, instead of one node's fields: a header **`<n> nodes`** with
**Delete** (`trash-2`) at the top right, **Color** (the swatches; a click
gives every selected node that color, or none), and one row per selected
node (id and label, weak) in file order; clicking a row chooses and
selects only that node.

## Toolbar

The graph and tree canvases get the [canvas toolbar](canvas.md#toolbar) in
the top-right corner, its groups top to bottom:

| Group | Button | Icon | Enabled | Action |
|-------|--------|------|---------|--------|
| History | Undo | `undo-2` | a step to undo | [Undo](#undo) |
| History | Redo | `redo-2` | a step to redo | [Redo](#undo) |
| Align | Align left | `align-start-vertical` | 2+ selected | every center's x to the smallest |
| Align | Align horizontal centers | `align-center-vertical` | 2+ selected | every center's x to the middle of the smallest and largest |
| Align | Align right | `align-end-vertical` | 2+ selected | every center's x to the largest |
| Align | Align top | `align-start-horizontal` | 2+ selected | every center's y to the smallest |
| Align | Align vertical centers | `align-center-horizontal` | 2+ selected | every center's y to the middle of the smallest and largest |
| Align | Align bottom | `align-end-horizontal` | 2+ selected | every center's y to the largest |
| Arrange | Distribute horizontally | `align-horizontal-distribute-center` | 3+ selected | the leftmost and rightmost centers stay; the others, in x order (ties in file order), are spread evenly between them |
| Arrange | Distribute vertically | `align-vertical-distribute-center` | 3+ selected | the same along y |
| Arrange | Snap to grid | `magnet` | 1+ nodes | every center to the nearest multiple of 0.25: the selected nodes with 2+ selected, else all |
| Layout | Layout | `network` | 1+ nodes | opens or closes the [Layout panel](#layout); on while open |

Each action is one [undo](#undo) step. Positions are written with 3
decimals, as when moving a node. A disabled button keeps the pointer from
the canvas, as the toolbar does everywhere.

## Layout

The **Layout panel** lays nodes out automatically. It opens to the left of
the toolbar, its top level with the Layout button, 260 px wide, in the
[property editor](gui.md#property-editor) look on `BG_PANEL` with a 1 px
`STROKE` border and 6 px rounded corners. It takes the pointer, and closes
with the Layout button or `Escape`.

| Row | Shown for | Editor | Default |
|-----|-----------|--------|---------|
| **Layout** | all | dropdown: graphs **Layered**, **Circle**, **Force**, **Grid**; trees **Tidy tree**, **Radial**, **Force**, **Grid** | Layered / Tidy tree |
| **Direction** | Layered, Tidy tree | dropdown: **Down**, **Right**, **Up**, **Left** (where the layers go) | Down |
| **Node spacing** | all | drag field 0.5–10, steps of 0.25 | 1.5 |
| **Layer spacing** | Layered, Tidy tree, Radial | drag field 0.5–10, steps of 0.25 | 2 |
| **Columns** | Grid | drag field 0–100; `0` is `ceil(√n)` | 0 |

Under the rows, a weak line says what Apply moves: `The 5 selected nodes`
with 2+ selected, else `All 12 nodes`; then **Apply** (accent). Apply lays
the nodes out as one [undo](#undo) step; the panel stays open, so options
can be tried one after another. The options are kept per file for the
session, not in the file.

### What is laid out

- With 2+ nodes selected, those nodes; else every node. Nodes not laid out
  do not move.
- Only edges between laid-out nodes count. Loops are ignored, and repeated
  edges between two nodes count once.
- A tree's laid-out nodes form a forest: its **roots** are the laid-out
  nodes whose parent is not laid out (the root itself when all are), in
  depth-first order; a node's **children** are its laid-out children, in
  order.
- The result is moved so the middle of its centers' bounds lands on the
  middle of the old bounds, rounded to 0.25. Positions keep 3 decimals.
- The same nodes, edges, positions and options always give the same
  layout.

### Layered (graphs)

Nodes in layers along the direction, edges running from one layer to a
later one wherever the graph allows:

1. **Cycles** are broken: a depth-first walk from each unvisited node in
   file order (successors in edge order) reverses, for layering only, every
   edge that leads back to a node still on the walk.
2. **Layers**: a node without predecessors is on layer 0; any other node is
   one layer past its lowest predecessor (the longest path).
3. **Order** in each layer: first by a breadth-first walk from layer 0 in
   file order; then 4 sweeps down and up the layers, each node ordered by
   the mean place of its neighbours in the layers before it (down) or after
   it (up), ties and nodes without such neighbours keeping their order.
4. **Positions**: layer `k` lies `k × layer spacing` along the direction.
   Across it, the nodes of a layer keep their order at least node spacing
   apart, each as near as it can get (least squares) to the mean of its
   neighbours in earlier layers; then to its neighbours in later layers;
   then to earlier ones again.

**Down** puts layer 0 at the top and orders across left to right;
**Right** puts it on the left and orders top to bottom; **Up** and
**Left** mirror them.

### Tidy tree (trees)

Each node above its children, subtrees never overlapping:

- Leaves take consecutive places node spacing apart, in depth-first order;
  each parent sits midway between its first and last child; depth `d` lies
  `d × layer spacing` along the direction.
- Several roots (a selection) stand side by side in depth-first order, as
  if they were the children of one node.
- Directions as for Layered.

### Circle (graphs)

Nodes evenly on a circle, clockwise from the top, in the order of a
depth-first walk that follows edges both ways (starting from the first node
in file order, then from the next node not yet reached), so connected
nodes sit together. Its radius puts neighbours node spacing apart:
`spacing / (2 sin(π / n))` for `n` nodes (two nodes: across a circle of
that size).

### Radial (trees)

The root at the center, each depth on a ring around it:

- Each leaf gets an equal share of the turn, in depth-first order,
  clockwise from the top; a parent sits at the middle angle of its first
  and last leaf.
- Ring `d` has radius `d × step`, where the step is the layer spacing, or
  more when the outermost ring would put leaves closer than node spacing.
- Several roots (a selection) sit on the first ring, as the children of an
  empty center.

### Force (graphs and trees)

Connected nodes pulled together, all nodes pushed apart
(Fruchterman–Reingold):

- Starts from the nodes' current positions; nodes on the same spot are
  first spread on a small circle, in file order.
- 300 steps. Every two nodes repel with `k² / d` and every edge (a tree's
  links) attracts with `d² / k`, `k` being the node spacing; a weak pull
  toward the middle keeps separate parts together. Each step moves a node
  at most a limit that falls from `2 k` to 0 over the steps.

### Grid (graphs and trees)

Rows of `columns` nodes, left to right and top to bottom, node spacing
apart: in file order (graphs) or depth-first order (trees).

## Undo

Every change the editor writes is a **step**: a whole drag, a typed id or
label, an edge connected, an alignment, a layout, a node added or
removed, a field in the Graph or Tree tab. A step is recorded when the
change is written to the file, once nothing is being dragged or typed.

- **Undo** (`Ctrl+Z`, `Cmd+Z` on macOS, or the toolbar) writes the file as
  it was before the last step; **Redo** (`Ctrl+Shift+Z` or `Ctrl+Y`,
  `Cmd+Shift+Z` on macOS, or the toolbar) writes it again. The keys act
  while the canvas has the keys and no text field is focused.
- Up to 100 steps per file, kept for the session: switching files and back
  keeps them. A new step clears what could be redone.
- Changes made to the file outside the editor are not steps; undo writes
  the editor's earlier versions over them.
- The choice and selection are kept for nodes that still exist (by id).
