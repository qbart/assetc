# Shapes

A **shape** is a simple volume on a glTF model, on one of its skeleton's
joints or in model space: a hitbox on the head that doubles the damage, a
physics collider, a ragdoll part, a trigger around a door. assetc does not
know what a shape means; its **kind** and **properties** tell the game.

Shapes are an *enrichment* like [locators](locators.md): kept in the model's
[`.amod`](../spec/amod.md#shapes) sidecar, edited in the Inspector's
[Shapes tab](#shapes-tab) and in the [3D view](#3d-view), and compiled into
its [`.amesh`](../spec/amesh.md#shapes).

## Shape

| Field       | Meaning |
|-------------|---------|
| Name        | Non-empty, unique among the model's shapes. The game finds it by the FNV-1a-64 hash of the name. |
| Kind        | Free text the game groups shapes by (`hitbox`, `collider`, `ragdoll`, `trigger`, …); empty to start with. |
| Type        | **Box**, **Sphere**, **Capsule** or **Convex hull**. |
| Joint       | As a [locator's](locators.md#locator): the first skin's joint, by name; none is model space. |
| Position, Rotation | The shape's center and orientation in its parent's space (not for hulls). |
| Size        | Box: **Size** X, Y, Z (full extents along its axes). Sphere: **Radius**. Capsule: **Radius** and **Height**, the distance between the centers of its two half-spheres, along its Y axis. |
| Properties  | Key / value pairs, as the [model's](properties.md#property). |
| Color       | The color it is drawn in, for assetc only (not compiled). |

- **Convex hull**: the smallest convex volume around the model's vertices
  that belong to the shape's joint (the vertices whose largest skin weight is
  that joint's), in the joint's space at the bind pose; in model space,
  around all the model's vertices. It has no position, rotation or size: it
  follows the mesh, and is computed again when the model or the joint
  changes. Fewer than four vertices that are not all in one plane make no
  hull: the build fails for the model (`<file>.amod: shape "<name>": no
  hull`).
- Model space is the [locators'](locators.md#locator).

## Shapes tab

The Inspector's **Shapes** tab (`shapes`) is shown for a model in the Input
tree. It works like the [Locators tab](locators.md#locators-tab):

- **Add shape** (`plus` and text) adds a Box named `shape`, or `shape_<n>`,
  in model space at the origin, unrotated, its size a tenth of the model's
  largest extent on every axis, with no kind, properties or color, and
  unfolds it.
- One card per shape, an accordion like the locators' (one unfolded at a
  time; the unfolded one is the *chosen* shape). Header: the chevron,
  **Visible** (`eye` / `eye-off`, for the session), a dot in the shape's
  color (`TEXT_WEAK` without one), the name, and, unfolded only, **Delete**.
- Body:
  - **Gizmo** (Move / Rotate group, as the locators'; not for hulls).
  - **Name** (text; an empty or taken name gets the old one back), **Kind**
    (text), **Type** (dropdown `Box`, `Sphere`, `Capsule`, `Convex hull`;
    switching keeps the size values that apply and gives the others
    defaults: a tenth of the model's extent).
  - **Color**: the locators' swatches.
  - **Joint**: as the locators' (choosing one puts the shape on it).
  - **Position**, **Rotation** (as the locators'), then by type: **Size**
    X, Y, Z, **Radius**, or **Radius** and **Height** (drag fields, three
    decimals, at least 0.001). A hull shows instead a weak line `<n> points
    around <joint>` (or `around the model`).
  - **Properties**: the [property editor](properties.md#properties-tab).
- While an animation plays, Gizmo, Joint, Position, Rotation and the sizes
  are disabled, as for locators.
- Edits are saved like the locators'.

## 3D view

The model's preview draws its visible shapes as wireframes over the model
(on top of it, like locator markers), following their joints:

- **Box**: its 12 edges. **Sphere**: three circles, across its X, Y and Z
  axes (48 segments). **Capsule**: the circles where its half-spheres meet
  the cylinder (across Y), four lines along Y between them, and two
  half-circles over each end (in the XY and ZY planes). **Hull**: the edges
  of its faces.
- 1 px in the shape's color (`TEXT_WEAK` without one); the chosen shape
  2 px and brighter (its color mixed halfway to white).
- With the **Shapes** tab showing, the [gizmo](locators.md#3d-view) (and
  `W` / `E`) acts on the chosen shape instead of the chosen locator.
- Shapes are not picked by clicking them; choose them in the list.

## Build

A model's shapes are written to its `.amesh` as the
[`SHAPES`, `SHAPEPTS` and `SHAPEPRP`](../spec/amesh.md#shapes) chunks (none
without shapes), their names, kinds and text values in `STRINGS`. A joint is
its index in `SKELETON`; a hull's points are the hull's corners, in the
shape's parent space. A shape whose joint is not a named joint of the first
skin fails the build, like a locator's (`<file>.amod: shape "<name>": no
joint "<joint>"`).

## Inspector Info

- A model in the Input tree: a **Shapes** row after Properties: their
  count, or `none`.
- A compiled mesh: a **Shapes** row after Properties.
- The [Chunks view](models.md#chunks-view) shows `SHAPES`, `SHAPEPTS` and
  `SHAPEPRP`.

## Not yet

- Shapes from the glTF (Unreal's `UCX_` nodes) and from presets.
- Drawing shapes in the compiled mesh preview.
- Size handles in the 3D view.
