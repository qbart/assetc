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
| Color       | The color of its marker in the [3D view](#3d-view), as a hint of what it is (e.g. weapons red, effects blue); for assetc only, not compiled. |

- **Model space** is the space of the default scene's root nodes, which is
  the space of a static model's `.amesh` vertices (see
  [Geometry](models.md#geometry)).
- **Joint space**: a locator on a joint follows it: its world transform is
  the joint's world transform × the locator's. Joints are the first skin's
  (see [Skinning](models.md#skinning-and-animations)); a joint is found by the
  first joint of that skin with the given name. Joints without a name cannot
  be used.
- A model's locators come from its file, from [presets](#presets) and from
  its sidecar; see [Inherited locators](#inherited-locators). They are
  listed and compiled in that order.

## Inherited locators

The sidecar holds only what is set for this model; other locators come from
the model's file and from presets shared by models on the same skeleton.

### From the file

Locators placed in a 3D package travel in the glTF itself:

- Every node of the default scene whose name starts with `SOCKET_` becomes a
  locator named after the rest of its name (`SOCKET_hand_r` → `hand_r`;
  `SOCKET_` alone is skipped), as in Unreal Engine.
- A node whose glTF `extras` (Blender's custom properties) have a `locator`
  key becomes one too: `true` names it after the node (skipped when the node
  has no name), a string names it that string. Other values are ignored.
- **Parent**: the node's nearest ancestor that is a named joint of the first
  skin; its transform is the product of the local transforms of the node
  and of its ancestors below that joint. Without such an ancestor it is in
  model space, with its transform from the scene's root.
- In scene order (depth-first, children in file order); a node whose name is
  already taken by an earlier one is skipped.

### Presets

`[[locators.presets]]` in [`assetc.toml`](../spec/assetc.toml.md#locators)
shares one model's locators with others:

```toml
[[locators.presets]]
from = "characters/hero.glb"
match = ["characters/**"]
```

- The preset's locators are those of the `from` model (from its file and its
  sidecar; not its own presets).
- They apply to every other model whose input-relative path matches a
  `match` glob (every model when `match` is left out) **and** whose first
  skin has a named joint for every joint they use, so models on the same
  skeleton share them and others are left alone.
- A `from` that is not a glTF model in the input, a model that does not
  parse, a bad sidecar of it or a bad glob fails every model's build (and
  shows in the Inspector) with `assetc.toml [[locators.presets]] <n>:
  <problem>`.

### Precedence

The file's locators come first, then each preset's in order, then the
sidecar's. A later locator with a name already there replaces the earlier
one in place; new names are added at the end. The result is what the
Inspector lists, the 3D view draws and the build compiles.

### Editing inherited locators

- In the [Locators tab](#locators-tab), an inherited locator's card says
  where it comes from: `From the file` or `From hero.glb` (the preset's
  model file name), and `, changed here` when the sidecar replaces it.
- Its name cannot be edited (the field is disabled).
- Any other change (fields, gizmo, Place, being the target of Mirror) writes
  the whole locator into the sidecar, which then replaces the inherited one.
- **Delete** becomes **Revert** (`refresh-cw`) for a changed inherited
  locator: it drops the sidecar's copy. For an unchanged one it is disabled
  (hover: `From <source>; remove it there`).
- The sidecar keeps only its own locators and copies that differ from what
  they replace: a copy edited back to the inherited values is dropped.

## Locators tab

The Inspector's **Locators** tab (`map-pin`) is shown for a model in the
Input tree (`*.gltf`, `*.glb`). Locators are managed here only; the 3D view
draws them and their gizmo. In the
[property editor](gui.md#property-editor) look, inside 8 px margins, from
the top:

- **Add** (`plus` and text) adds a locator named `locator`, or
  `locator_<n>` (the smallest `n` from 2 that is free), in model space at
  the origin with no rotation and scale 1, after the others, and unfolds it.
- **Place on surface** (`crosshair` and text, key `S`): the next click on
  the model adds a locator there, see [Place on surface](#place-on-surface).
  While armed the button is filled with `ACCENT` at 25%; clicking it again or
  `Escape` disarms it.
- With no locators, nothing else (a weak `No locators` line).
- The **list**: one card per locator, in the model's order, like the
  [gradient cards](gradients.md#gradients-tab). The list is an
  **accordion**: at most one card is unfolded, its locator is the *chosen*
  one (in the tab and in the 3D view); unfolding a card folds the others,
  and folding it chooses none. All are folded to start with.
- **Card header**: the chevron, a 10 px dot in the locator's color (`TEXT`
  without one), the name (a click on it or the chevron folds and unfolds),
  and on the right:
  - **Visible** (a checkbox, on to start with): off hides the locator's
    marker, gizmo and preview in the 3D view, and markers that are hidden
    cannot be clicked. It is not saved: it lasts for the session, per model
    and locator name.
  - **Move** (`move-3d`) and **Rotate** (`rotate-3d`): turn the
    [gizmo](#3d-view) on this locator on in that mode (choosing and
    unfolding it), or off when it already is; the button showing the gizmo's
    mode on the chosen locator is filled with `ACCENT`. Disabled while an
    animation plays or the locator is hidden.
  - Unfolded only: **Delete** (`trash-2`), or **Revert** / disabled Delete for
    an [inherited](#editing-inherited-locators) locator. Deleting chooses
    none.
- **Card body** (unfolded):
  - A weak line for an inherited locator: `From the file` / `From <file>`,
    and `, changed here` when the sidecar replaces it.
  - **Name**: a text field. When it loses focus empty or equal to another
    locator's name, it gets the old name back.
  - **Color**: a row of swatches (18 px squares, 4 px corners): **none**
    (a `TEXT_WEAK` outline with a diagonal line) and eight colors (`#ef4444`,
    `#f97316`, `#eab308`, `#22c55e`, `#06b6d4`, `#3b82f6`, `#a855f7`,
    `#ec4899`); the locator's one has a 2 px `TEXT` border. A color written
    by hand in the sidecar that is not one of them shows as an extra swatch.
  - **Joint**: a dropdown with `None (model space)` and the first skin's named
    joints in skin order. Choosing one puts the locator **on** it: position
    and rotation are reset (0 and none) relative to the new parent, so it
    sits at the joint (or the model's origin); scale is kept.
  - **Position** X, Y, Z (drag fields, 0.001 per point, three decimals),
    **Rotation** X, Y, Z (drag fields, degrees, 0.5 per point, one decimal)
    and **Scale** X, Y, Z (drag fields, 0.01 per point, three decimals, at
    least 0.001). Position and Scale show one decimal less from 10 and two
    less from 100 (at least one), so long values fit the field; the value
    itself keeps its precision.
  - **Preview**: a dropdown with `None` and every glTF model in the Input
    tree (input-relative paths, sorted, without the model itself); see
    [Attachment preview](#attachment-preview).
  - **Mirror**: a **Mirror** button (`flip-horizontal-2`) and a dropdown of the
    plane to mirror across: `X` (the default), `Y` or `Z`, in model space
    (the dropdown's choice lasts for the session). See [Mirroring](#mirroring).
  - While an [animation plays](#animations), Joint, Position, Rotation, Scale
    and Mirror are disabled, and a weak line ends the body:
    `Transforms are locked while an animation plays. Choose Rest pose to
    edit them.`
- A weak hint at the bottom: `Click a locator in the view to choose it.`
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

The model's [preview](models.md#preview) draws its visible locators over
the model, on top of it (they are never hidden by faces, so locators inside
the mesh can be seen and picked), following their joints in the shown pose
and animation. The canvas has no locator buttons: they are in the
[Locators tab](#locators-tab).

- **Markers**: each locator is a dot in its color (`TEXT` without one), 5 px
  across, with three 16 px lines along its X, Y and Z axes (`CHANNEL_R`,
  `CHANNEL_G`, `CHANNEL_B`) and its name to the right of the dot (12 px,
  `TEXT` on a `BG_BAR` 70% plate). The chosen one's dot is 7 px with a 2 px
  `ACCENT` ring.
- **Choosing**: clicking within 8 px of a visible dot chooses that locator
  (the nearest one), unfolds its card and shows the Locators tab; clicking
  elsewhere (without dragging, and not placing) chooses none. Dragging still
  orbits the camera.
- **Gizmo** on the chosen locator, in its own (local) axes, when turned on
  with its card's **Move** or **Rotate** button (or `W` / `E`); off to start
  with, and turned off by choosing another locator through the list's
  chevron, by `Escape`, or by its button again:

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
  - The gizmo's mode lasts for the session; choosing another locator in the
    3D view keeps it on, in that mode, for the new one.
- **Keys** (pointer over the canvas, no text field focused): `W` Move, `E`
  Rotate (on the chosen locator, or off when already in that mode), `S` arms
  Place on surface, `Escape` turns the gizmo off and disarms Place; see
  [`KEYS.md`](../../KEYS.md#canvas-3d).

### Place on surface

**Place on surface** in the tab (or `S`) arms placing; then:

- Clicking within 8 px of a visible marker still chooses it. Clicking the
  model elsewhere **adds a locator** (as **Add**) where the pointer's ray
  first hits a triangle, unfolds it, and disarms. Clicking empty space does
  nothing; dragging orbits.
- The locator's position is the hit point; its rotation turns its **Y** axis
  along the surface's normal (the triangle's, facing the camera), keeping X
  and Z as the shortest turn from the model's.
- On a skinned model it is attached to the joint that weighs most on the hit
  triangle (the sum of its three vertices' weights), when that joint has a
  name; otherwise it is in model space.
- The ray is cast against the triangles as drawn: skinned meshes in the pose
  shown, through their joints (Bevy's mesh ray cast ignores skinning, so
  assetc casts its own).

### Animations

While the [Animations tab](models.md#animations-tab) plays an animation,
locators follow their joints, but their transforms cannot be changed: they
are relative to the joint at rest, and the pose shown moves every frame.

- The gizmo is not drawn; Place on surface and the cards' Move and Rotate
  are disabled (placing is disarmed); markers can still be clicked to choose
  them.
- The cards disable what moves a locator (see
  [Locators tab](#locators-tab)); Add, Delete, Name, Color, Visible and
  Preview still work.
- Choosing **Rest pose** unlocks them.

### Attachment preview

A locator with a **Preview** model shows that model at the locator, so a
socket can be checked with what it will hold:

- The model is loaded like the [preview](models.md#preview)'s (its default
  scene, its own materials), placed by the locator's transform in its parent
  (joint or model) and scaled with the shown model's fit, so it keeps its
  units relative to the model; it follows joints and animations. Its own
  animations do not play.
- It is shown only while the locator is visible and the shown model is
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
  count (all of them, inherited too), `<n> (<m> inherited)` when some are
  inherited, or `none`. A [preset](#presets) error shows in it instead.
- A compiled mesh: a **Locators** row after Skin: their count, or `none`.
- The [Chunks view](models.md#chunks-view) shows `LOCATORS` (Name, Hash,
  Joint, Translation, Rotation, Scale) and `LOCNAMES` (bytes).

## Not yet

- Shapes (hitboxes, colliders), regions, paths and animation events; see the
  other enrichments in [`DEFERRED.md`](../../DEFERRED.md#more-enrichments).
