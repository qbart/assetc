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
| From, To   | The nodes it leaves and enters, by id; never the same node, and at most one edge from one node to another (the other direction is a different edge). |
| Data       | Key / value pairs, as nodes'. |

## Creating

In the Input tree's context menu, **New data ▸ Directed Graph** creates
`graph.agraph` (`graph 2.agraph`, … if taken) holding one node `start`
(label `Start`) at 0, 0 and no edges, selects it and starts renaming it, as
**New Data ▸ Schema** does (see [`schemas.md`](schemas.md#creating)).

The file tree shows `.agraph` files with the `workflow` icon.

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
- **Nodes**: 2 × 0.75 unit rounded rectangles (6 px corners at 64 px per
  unit, scaling with the zoom) filled `BG_HEADER` with a 1 px `STROKE`
  border, the label (or id) centered in `TEXT`, 14 px at 64 px per unit
  (scaled, at least 8 px). The chosen node has a 2 px `ACCENT` border. A
  **port**, a 5 px `TEXT_WEAK` circle (`ACCENT` while hovered), sits on
  each node's right edge.
- **Edges**: 1.5 px `TEXT_WEAK` lines from the center of their From node to
  the border of their To node, ending in a 10 px arrowhead; the chosen edge
  in `ACCENT`, 2.5 px.
- A weak hint in the bottom-left corner: `Double-click to add a node · drag
  a port onto a node to connect · Shift snaps to 0.25`.

Interactions (left button unless said):

- **Press a node** chooses it; **dragging** it moves it (positions keep 3
  decimals). Holding **`Shift`** snaps its center to multiples of 0.25.
- **Press a port** and **drag** onto another node: an edge from the port's
  node to that node, chosen. A rubber line follows the pointer; released
  elsewhere, on its own node, or where that edge already exists, nothing is
  added.
- **Click an edge** (within 6 px of its line) chooses it.
- **Click empty grid** chooses nothing.
- **Double-click empty grid** adds a node `node` (`node_2`, …, label empty)
  there (snapped to 0.25 with `Shift`), chosen.
- Nodes and edges are removed in the Inspector (no Delete key, which acts
  on the file tree).

## Inspector

### Info tab

For a graph in the Input tree: **Kind** `Graph`, **Dialect**
`agraph/1`, **Nodes**, **Edges** and **Problems** (`none`, or `<n> errors`);
an unreadable file has an **Error** row instead.

### Graph tab

The Inspector's **Graph** tab (`workflow`) edits what is chosen in the
canvas, in the [property editor](gui.md#property-editor) look:

- Nothing chosen: the node list (one row per node: id and label, weak;
  clicking one chooses it and centers the view on it) and **Add node**
  (`plus` and text, a node at the view's center).
- A node: **Id** (text; an empty, invalid or taken id gets the old one back
  when it loses focus; renaming updates the edges), **Label** (text), **X**
  and **Y** (drag fields, three decimals), **Data** (the property editor),
  its edges (`→ to` and `← from` rows; clicking one chooses it), and
  **Delete node** (`trash-2` and text; its edges go too).
- An edge: **From** and **To** (dropdowns of the nodes; a choice that would
  repeat an edge or loop to itself is not offered), **Data**, and **Delete
  edge**.
- Edits are saved like gradients: once nothing is being dragged or typed,
  then the file is inspected again.

## Problems

Reading checks ids (identifiers, unique), edges (known nodes, not to
themselves, not repeated). A file with problems is still shown and edited;
its problems are listed under the Info rows' **Problems** count in the
Graph tab (`CHANNEL_R`, 12 px).

## Build

Graphs are not built yet: the build skips them.

## Not yet

- Building graphs into a binary for games, and data typed by a
  [schema](schemas.md).
- Moving several nodes at once, undo, and edge labels on the canvas.
