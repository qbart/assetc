# Paths

A **path** is a curve on a glTF model, through points in model space or on
one of its skeleton's joints: a rope, a rail to grind, a ladder to climb, a
patrol route around a tower, a tail's spline. assetc does not know what a
path means; its **kind** and **properties** tell the game.

Paths are an *enrichment* like [locators](locators.md): kept in the model's
[`.amod`](../spec/amod.md#paths) sidecar, edited in the Inspector's
[Paths tab](#paths-tab) and in the [3D view](#3d-view), and compiled into
its [`.amesh`](../spec/amesh.md#paths) with the curve already measured and
sampled.

## Path

| Field      | Meaning |
|------------|---------|
| Name       | Non-empty, unique among the model's paths. The game finds it by the FNV-1a-64 hash of the name. |
| Kind       | Free text the game groups paths by (`rope`, `rail`, `ladder`, `patrol`, …); empty to start with. |
| Joint      | As a [locator's](locators.md#locator): the first skin's joint, by name; none is model space. |
| Points     | The curve's control points, in order, each a position in the joint's (or model) space. The curve passes through every one. |
| Closed     | The curve goes on from the last point back to the first, as a loop. Off to start with. |
| Tension    | 0 to 1: 0 is a smooth Catmull-Rom curve, 1 straight lines between the points; 0 to start with. |
| Alpha      | 0 to 1, how the curve spaces its knots: 0 uniform, 0.5 centripetal (no cusps or loops at sharp turns; the start), 1 chordal. |
| Properties | Key / value pairs, as the [model's](properties.md#property). |
| Color      | The color it is drawn in, for assetc only (not compiled). |

### Curve

The curve is a **cardinal spline**, computed with the
[`matma`](https://crates.io/crates/matma) crate's curves (`cardinal`,
`cardinal_tangent` and `Spline`'s knots):

- One segment from each point to the next (and, closed, from the last to the
  first). Segment `i` runs from point `i` to point `i + 1`, its shape set by
  the points before and after them, `tension` and `alpha`.
- **Open** paths: past the first and last points the curve uses phantom
  points mirrored through them (`2 p₀ − p₁`, `2 pₙ₋₁ − pₙ₋₂`), so it starts
  and ends exactly on them.
- **Closed** paths: the neighbors wrap around, so the loop is smooth at
  its first point too.
- **Length**: the sum of the straight pieces when each segment is cut into
  32 steps of its parameter.
- A path needs at least 2 points (a closed one, 3) to have a curve.

## Paths tab

The Inspector's **Paths** tab (`spline`) is shown for a model in the Input
tree. It works like the [Shapes tab](shapes.md#shapes-tab):

- **Add path** (`plus` and text) adds a path named `path`, or `path_<n>`, in
  model space with two points, at the origin and a tenth of the model's
  largest extent along X, open, tension 0, alpha 0.5, with no kind,
  properties or color, and unfolds it.
- One card per path, an accordion (one unfolded at a time; the unfolded one
  is the *chosen* path). Header: the chevron, **Visible** (`eye` /
  `eye-off`, for the session), a dot in the path's color (`TEXT_WEAK`
  without one), the name, and, unfolded only, **Delete**.
- Body:
  - **Gizmo** (the locators' group with **Move** only): moves the chosen
    point.
  - **Name** (text; an empty or taken name gets the old one back), **Kind**
    (text), **Color** (the locators' swatches), **Joint** (as the shapes';
    choosing one keeps the points' values, now in the joint's space).
  - **Closed** (checkbox), **Tension** and **Alpha** (sliders 0–1, two
    decimals; Alpha's hover: `0 uniform, 0.5 centripetal, 1 chordal`).
  - A weak line: `<n> segments, length <l>` (three decimals, in the
    parent's units), or `No curve: needs at least 2 points` (3 when
    closed).
  - **Points**: one row per point: its number (from 1; clicking it chooses
    the point, the chosen one in `ACCENT`), X, Y, Z drag fields (three
    decimals) and a remove button (`x`, disabled with two points left).
  - **Add point** (`plus` and text): a point after the chosen one (after the
    last when none is chosen): the middle of the segment to the next point,
    or, after the last point of an open path, the last point moved on by
    the previous segment's offset. The new point is chosen.
  - **Draw** (`pen-line` and text, a toggle filled with `ACCENT` at 25%
    while on): drawing on the model adds points where it hits the surface,
    see [Drawing](#drawing). `Escape` (pointer over the canvas), choosing
    another path or leaving the Paths tab turns it off.
  - **Properties**: the [property editor](properties.md#properties-tab).
- While an animation plays, Gizmo, Joint, the points, Add point and Draw are
  disabled, as for locators.
- Edits are saved like the locators'.

## 3D view

The model's preview draws its visible paths over the model (on top of it,
like locator markers), following their joints:

- The curve as a line through 16 points per segment, 1 px in the path's
  color (`TEXT_WEAK` without one); the chosen path's 2 px and brighter (its
  color mixed halfway to white).
- Each point as a 5 px dot in the line's color; the chosen path's chosen
  point 7 px with a 2 px `ACCENT` ring. Clicking within 8 px of a visible
  path's point chooses that path and point, unfolds its card and shows the
  Paths tab (locator markers win when both are within reach).
- With the **Paths** tab showing, the [gizmo](locators.md#3d-view) in Move
  mode (and `W`) acts on the chosen point of the chosen path, in the path's
  parent axes. `E` does nothing there.
- The toolbar's [Show](models.md#show) **Paths** button hides them all;
  **Add path**, **Add point** and **Draw** turn it back on.

### Drawing

While the chosen path's **Draw** is on:

- **Left press** on the model adds a point where the pointer hits its
  surface, after the chosen point, and chooses it; so clicking adds one
  point.
- **Left drag** keeps adding points along the stroke: one each time the
  pointer has moved 24 px on screen from where the last point was asked
  for, where it hits the model (misses add nothing).
- **Right drag** still orbits the camera; the wheel still zooms.
- Points and locator markers cannot be clicked, and the gizmo is not drawn.

## Build

A model's paths are written to its `.amesh` as the
[`PATHS`, `PATHPTS`, `PATHSMP` and `PATHPRP`](../spec/amesh.md#paths)
chunks (none without paths), their names, kinds and text values in
`STRINGS`:

- The control points as authored, so a game can evaluate the same curve
  itself (`tension`, `alpha` and `closed` are in the record).
- **Samples**: 16 per segment plus one, evenly spaced **by distance** along
  the curve (not by its parameter), each with its position, distance from
  the start, unit tangent and spline parameter, so a game can move along it
  at a constant speed without spline code. A closed path's last sample is
  its first point again, at the full length.
- A path whose joint is not a named joint of the first skin fails the
  build, like a locator's (`<file>.amod: path "<name>": no joint
  "<joint>"`); so does one without a curve (`<file>.amod: path "<name>":
  needs at least 2 points` or `3 points` when closed).

## Inspector Info

- A model in the Input tree: a **Paths** row after Regions: their count, or
  `none`.
- A compiled mesh: a **Paths** row after Regions.
- The [Chunks view](models.md#chunks-view) shows `PATHS`, `PATHPTS`,
  `PATHSMP` and `PATHPRP`.

## Not yet

- Paths from the glTF (curves are not part of glTF; a node naming
  convention could carry them) and from presets.
- Drawing paths in the compiled mesh preview.
- Moving several points at once, or the whole path.
