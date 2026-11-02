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
| Preview     | Another glTF model of the Input tree shown at the locator in the [3D view](#attachment-preview), e.g. the sword in the hand; for assetc only, not compiled. |

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
  - **Preview**: a dropdown with `None` and every glTF model in the Input
    tree (input-relative paths, sorted, without the model itself); see
    [Attachment preview](#attachment-preview).
  - **Mirror**: a **Mirror** button (`flip-horizontal-2`) and a dropdown of the
    plane to mirror across: `X` (the default), `Y` or `Z`, in model space
    (the dropdown's choice lasts for the session). See [Mirroring](#mirroring).
- While an [animation plays](#animations), Joint, Position, Rotation, Scale
  and Mirror are disabled, and a weak line under the section says
  `Transforms are locked while an animation plays. Choose Rest pose to edit
  them.`
- A weak hint at the bottom: `Click a locator in the view to choose it. Drag
  its gizmo to move (W) or rotate (E) it, or click the model to place it (S).`
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

### Mirroring

**Mirror** makes the chosen locator's left / right counterpart, or updates
it when it exists:

- **Name**: every left / right token of the name is swapped: `Left` ↔
  `Right`, `left` ↔ `right`, `LEFT` ↔ `RIGHT` anywhere, and `L` ↔ `R`,
  `l` ↔ `r` when they stand alone between separators (`_`, `.`, `-`, space)
  or the name's ends (`hand_l` ↔ `hand_r`, `L.hand` ↔ `R.hand`,
  `LeftHand` ↔ `RightHand`). A name without such a token cannot be mirrored
  (the button is disabled).
- **Joint**: the joint's name with its tokens swapped the same way, when the
  skin has a joint with that name; failing that, the first joint whose name
  equals it once a trailing number (digits, and a `_`, `.` or `-` before
  them) is left out of both, as skeletons often number each side apart
  (`b_LeftForeArm_010` → `b_RightForeArm_07`); otherwise the same joint
  (e.g. the spine).
  Model space stays model space.
- **Transform**: the locator's transform in model space, at the pose shown,
  reflected across the chosen plane through the origin (`S · T · S`, with `S`
  negating that axis, so the result is not mirrored itself), then put in the
  new joint's space. Scale and preview are copied.
- The counterpart is the locator with the mirrored name: it is replaced in
  place, or added after the others when there is none. It becomes the
  chosen locator.
- Mirroring needs the model shown in the 3D view (the button is disabled
  until then).

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

  - Hovering a handle (within 6 px of a line, within 9 px of the center
    dot's center, which wins) draws it 1 px thicker, its color mixed
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
  Rotate, `S` Place; see [`KEYS.md`](../../KEYS.md#canvas-3d).

### Place on surface

The toolbar's **Place** mode (`crosshair`, key `S`) puts a locator on the
model's surface:

- Clicking within 8 px of a marker still chooses it. Clicking the model
  elsewhere places the chosen locator where the pointer's ray first hits a
  triangle, or adds a new locator there (as **Add**) when none is chosen.
  Clicking empty space does nothing; dragging orbits.
- The locator's position is the hit point; its rotation turns its **Y** axis
  along the surface's normal (the triangle's, facing the camera), keeping X
  and Z as the shortest turn from the model's; its scale is kept.
- A locator without a joint on a skinned model is attached to the joint
  that weighs most on the hit triangle (the sum of its three vertices'
  weights), when that joint has a name; a locator with a joint keeps it.
- The ray is cast against the triangles as drawn: skinned meshes in the pose
  shown, through their joints (Bevy's mesh ray cast ignores skinning, so
  assetc casts its own).
- No gizmo is drawn in this mode; the chosen marker shows as usual.

### Animations

While the [Animations tab](models.md#animations-tab) plays an animation,
locators follow their joints, but their transforms cannot be changed: they
are relative to the joint at rest, and the pose shown moves every frame.

- The gizmo is not drawn; Move, Rotate and Place are disabled in the
  toolbar; markers can still be clicked to choose them.
- The Locators tab disables what moves a locator (see
  [Locators tab](#locators-tab)); Add, Delete, Name and Preview still work.
- Choosing **Rest pose** unlocks them.

### Attachment preview

A locator with a **Preview** model shows that model at the locator, so a
socket can be checked with what it will hold:

- The model is loaded like the [preview](models.md#preview)'s (its default
  scene, its own materials), placed by the locator's transform in its parent
  (joint or model) and scaled with the shown model's fit, so it keeps its
  units relative to the model; it follows joints and animations. Its own
  animations do not play.
- It is shown only while the Locators toggle is on and the shown model is
  fitted; a file that does not load shows nothing.
- It is not compiled: the game attaches what it wants.

## Compiled mesh preview

The [compiled mesh preview](models.md#compiled-mesh-preview) draws the
file's locators as markers (dots, axes and names, as above; none chosen, no
gizmo), when its **Locators** toggle (`map-pin`, last in the bounds group,
on to start with, disabled when the file has none) is on:

- A locator on a joint is placed by the joint's bind pose (the inverse of its
  `inverse_bind`) × the locator's transform; one in mesh space by its
  transform. Both are then centered and scaled like the mesh.
- They are drawn for every level of detail.

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

- Importing glTF nodes named `SOCKET_*` and glTF `extras` as locators.
- Presets shared by models with the same skeleton.
- Shapes (hitboxes, colliders), regions, paths and animation events; see the
  other enrichments in [`DEFERRED.md`](../../DEFERRED.md#more-enrichments).
