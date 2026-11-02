# Locators

A **locator** is a named point with an orientation (and scale) on a glTF
model, optionally attached to one of its skeleton's joints: where a sword
sits in a hand, a muzzle, a hat, a seat, a camera anchor, an effect or sound
emitter. assetc does not know what a locator means; the game looks it up by
name and uses its transform.

Locators are the first kind of *enrichment*: data authored in assetc on top
of a model, kept in the model's [`.amod`](../spec/amod.md#locators) sidecar
and compiled into its [`.amesh`](../spec/amesh.md#locators). They are edited
in the Inspector's [Locators tab](#locators-tab) and directly in the
[3D view](#3d-view).

## Locator

| Field       | Meaning                                                                 |
|-------------|-------------------------------------------------------------------------|
| Name        | Identifies the locator; non-empty and unique within the model. The game finds it by the FNV-1a-64 hash of the name (see [`amesh.md`](../spec/amesh.md#locators)). Case is kept. |
| Joint       | The skin joint it is attached to, by the glTF node's **name**; none puts it in model space. Names survive re-exporting the model, indices do not. |
| Translation | Offset in the parent's space (the joint's, or the model's), in model units. |
| Rotation    | Unit quaternion `x y z w` relative to the parent. The Inspector edits it as Euler angles in degrees (X, then Y, then Z). |
| Scale       | Per axis, relative to the parent.                                        |

- **Model space** is the space of the default scene's root nodes, which is
  the space of a static model's `.amesh` vertices (see
  [Geometry](models.md#geometry)).
- **Joint space**: a locator on a joint follows it: its world transform is
  the joint's world transform × the locator's. Joints are the first skin's
  (see [Skinning](models.md#skinning-and-animations)); a joint is found by the
  first joint of that skin with the given name. Joints without a name cannot
  be used.
- Locators are stored in the sidecar's order and compiled in that order.

## Locators tab

The Inspector's **Locators** tab (`map-pin`) is shown for a model in the
Input tree (`*.gltf`, `*.glb`). It lists and edits the model's locators, in
the [property editor](gui.md#property-editor) look, inside 8 px margins.

- A list of the locators, rows like the
  [Animations tab](models.md#animations-tab)'s (28 px, hover `BG_HEADER`,
  chosen `ACCENT` at 25%): the name (14 px, `TEXT`) and, right-aligned, its
  joint's name or `model` (13 px, `TEXT_WEAK`). Clicking a row chooses the
  locator, in the tab and in the 3D view. With no locators: weak
  `No locators`.
- Below the list, a row of buttons: **Add** (`plus`) adds a locator named
  `locator`, or `locator_<n>` (the smallest `n` from 2 that is free), in model
  space at the origin with no rotation and scale 1, after the others, and
  chooses it; **Delete** (`trash-2`, enabled with a locator chosen) removes the
  chosen one and chooses the one after it (or before, when it was last).
- With a locator chosen, a **Locator** section (not folding) with:
  - **Name**: a text field. When it loses focus empty or equal to another
    locator's name, it gets the old name back.
  - **Joint**: a dropdown with `None (model space)` and the first skin's named
    joints in skin order. Choosing another keeps the locator where it is in
    the 3D view (its transform is converted to the new parent's space, at the
    pose shown), when the view has the model; otherwise the values are kept as
    they are.
  - **Position** X, Y, Z (drag fields, 0.001 per point, three decimals),
    **Rotation** X, Y, Z (drag fields, degrees, 0.5 per point, one decimal)
    and **Scale** X, Y, Z (drag fields, 0.01 per point, three decimals, at
    least 0.001).
- A weak hint at the bottom: `Click a locator in the view to choose it. Drag
  its gizmo to move (W) or rotate (E) it.`
- The chosen locator (by index) is remembered while the file is the
  inspected one; it is cleared when another file is inspected.

### Saving

Edits are written to the sidecar as the
[Parameters tab](models.md#parameters-tab) writes it: buttons and the joint
dropdown at once, fields and gizmo drags when the pointer is released (or the
field loses focus), creating the sidecar or deleting it when nothing is left
in it. Saving a sidecar does **not** reload the model in the 3D view (only
the file's own changes, a rescan or selecting it again do), so the view, the
camera and a playing animation stay as they are.

If the sidecar does not parse, the tab shows the error instead.

## 3D view

The model's [preview](models.md#preview) draws its locators over the model,
on top of it (they are never hidden by faces, so locators inside the mesh can
be seen and picked), following their joints in the shown pose and animation.

- **Markers**: each locator is a 5 px dot with three 16 px lines along its
  X, Y and Z axes (`CHANNEL_R`, `CHANNEL_G`, `CHANNEL_B`) and its name to the
  right of the dot (12 px, `TEXT` on a `BG_BAR` 70% plate). The chosen one's
  dot is `ACCENT` and 7 px; the others' are `TEXT`.
- **Choosing**: clicking within 8 px of a dot chooses that locator (the
  nearest one) and shows the Locators tab; clicking elsewhere (without
  dragging) clears the choice. Dragging still orbits the camera.
- **Gizmo** on the chosen locator, in its own (local) axes, in one of two
  modes:

  | Mode   | Draws | Dragging |
  |--------|-------|----------|
  | Move   | an 80 px arrow per axis (2 px line, a 6 px triangle head) in the axis color, and a 7 px `ACCENT` center dot | an arrow: moves the locator along that axis, following the pointer along the arrow on screen; the center dot: moves it in the plane facing the camera |
  | Rotate | a 70 px circle per axis, in the plane across that axis (48 segments, 2 px, the axis color) | a circle: turns the locator around that axis by the angle the pointer turns around the gizmo's center |

  - Hovering a handle (within 6 px) draws it 1 px thicker, its color mixed
    halfway to white; a drag that starts on a handle moves or turns the
    locator instead of orbiting the camera.
  - An arrow whose axis points nearly at the camera (its screen length under
    15% of 80 px) is not drawn and cannot be dragged.
  - Sizes are on screen: the gizmo stays the same size when the camera moves
    closer or farther.
- **Toolbar**: a group above the Color vision group:

  | Button     | Icon        | Action |
  |------------|-------------|--------|
  | Move       | `move-3d`   | gizmo in Move mode (the default) |
  | Rotate     | `rotate-3d` | gizmo in Rotate mode |
  | Locators   | `map-pin`   | toggle: show markers and the gizmo (on to start with); off hides them and ignores clicks on them |
  | Add locator | `plus`     | adds a locator, as the tab's Add button, and shows the Locators tab |

  The mode and the toggle last for the session, for every model.
- **Keys** (pointer over the canvas, no text field focused): `W` Move, `E`
  Rotate; see [`KEYS.md`](../../KEYS.md#canvas-3d).
- The view computes positions from the pose being shown; editing while an
  animation plays edits the locator relative to its joint, so it keeps
  following it.

## Build

A model's locators are written to its `.amesh` as the
[`LOCATORS` and `LOCNAMES`](../spec/amesh.md#locators) chunks (none when it
has no locators). A joint is stored as its index in `SKELETON`, `−1` for
model space; the rotation is normalized.

The model fails to build (`<file>.amod: locator "<name>": <problem>`) when a
locator's joint is not a named joint of the first skin (`no joint "<joint>"`,
or `no skeleton for joint "<joint>"` when the model has no skin), or its
rotation is zero.

## Inspector Info

- A model in the Input tree: a **Locators** row after Levels of detail: their
  count, or `none`.
- A compiled mesh: a **Locators** row after Skin: their count, or `none`.
- The [Chunks view](models.md#chunks-view) shows `LOCATORS` (Name, Hash,
  Joint, Translation, Rotation, Scale) and `LOCNAMES` (bytes).

## Not yet

- Showing locators in the [compiled mesh preview](models.md#compiled-mesh-preview).
- Placing a locator by clicking the model's surface, mirroring left / right
  by name, and previewing another model attached to a locator.
- Shapes (hitboxes, colliders), regions, paths and animation events; see the
  other enrichments in [`DEFERRED.md`](../../DEFERRED.md#more-enrichments).
