# Regions

A **region** is a named set of a glTF model's triangles with tags the game
reads: the surface type of an area for footsteps and impacts (`metal`,
`wood`), a part that can break off (`breakable`), where a tint applies
(`tint`). A triangle can be in several regions at once (the visor is `glass`
and `breakable`).

Regions are an *enrichment* like [locators](locators.md): kept in the
model's [`.amod`](../spec/amod.md#regions) sidecar, edited in the
Inspector's [Regions tab](#regions-tab) and painted in the
[3D view](#painting), and compiled into its
[`.amesh`](../spec/amesh.md#regions) as a mask per triangle.

## Region

| Field     | Meaning |
|-----------|---------|
| Name      | Non-empty, unique among the model's regions. The game finds it by the FNV-1a-64 hash of the name. |
| Tags      | As a [locator's](locators.md#locator): names the game reads (`metal`, `breakable`), none to start with. |
| Materials | glTF materials whose triangles are all in the region, by name ([as compiled](models.md#names): `material_<index>` for an unnamed one). |
| Triangles | Triangles painted into the region, each by its glTF mesh, primitive and index in the primitive. |
| Color     | The color it is shown in, for assetc only (not compiled). |

- A triangle is in the region when its primitive's material is one of the
  region's materials **or** it was painted into it.
- A model has at most 32 regions.
- **Triangle index**: the triangle's place in its primitive's triangle list
  as assetc reads it (strips and fans converted to lists). A triangle of a
  mesh used by several nodes is in the region on each of them. Painted
  triangles survive edits of the sidecar and rebuilding, but not a
  re-export of the model that reorders its triangles; material regions do.

## Regions tab

The Inspector's **Regions** tab (`paint-bucket`) is shown for a model in the
Input tree. It works like the [Locators tab](locators.md#locators-tab):

- **Add region** (`plus` and text) adds a region named `region`, or
  `region_<n>`, without tags, materials or triangles, in the first color of
  the [locator swatches](locators.md#locators-tab) not used by another
  region (or the first), and unfolds it. Disabled with 32 regions (hover:
  `A model has at most 32 regions`).
- One card per region, an accordion like the locators' (the unfolded one is
  the *chosen* region). Header: the chevron, **Visible** (`eye` / `eye-off`,
  for the session), a dot in the region's color, the name, and, unfolded
  only, **Delete**.
- Body:
  - **Name** (text; an empty or taken name gets the old one back) and
    **Tags** (as a locator's).
  - **Color**: the locator swatches, without "none" (a region always has a
    color).
  - **Materials**: one checkbox per material the model's triangles use, by
    name, in file order.
  - **Painted**: `<n> triangles`, then a **Paint** toggle button
    (`paintbrush` and text, filled with `ACCENT` at 25% while on), a
    **Brush** slider (2–64 px, 12 to start with, for the session) and a
    **Clear** button (enabled with painted triangles) that removes them all.
  - A weak hint: `Paint: drag on the model. Shift + drag erases. Right drag
    orbits.`
- Edits are saved like the locators'; a stroke is saved when the pointer is
  released.

## 3D view

The model's preview shows its visible regions on the triangles they cover,
over the model (drawn on top, without depth; triangles facing away from the
camera are left out), following the pose and animation:

- Each covered triangle is filled with its region's color at 30% opacity; a
  triangle in several visible regions is filled by each, in list order.
- The chosen region's triangles are also outlined (1 px, its color).
- The toolbar's [Show](models.md#show) **Regions** button hides them all
  (and turns Paint off); Paint turns it back on.

### Painting

While the chosen region's **Paint** is on (only one region paints at a
time; choosing another turns it off):

- **Left drag** on the model adds triangles to the region: every frame, the
  triangle under the pointer and the triangles whose center lies within the
  brush's radius of the point hit (the brush's size on screen, at the
  model's center), and that face the camera. **Shift + left drag** removes
  them instead. Clicking paints once.
- **Right drag** still orbits the camera; the wheel still zooms.
- Locator markers cannot be clicked, and the gizmo is not drawn.
- `Escape` (pointer over the canvas) turns Paint off.

## Build

A model with regions gets [`REGIONS`, `REGTAGS` and `TRIREGN`](../spec/amesh.md#regions)
in its `.amesh` (none without regions): one record per region, and one
32-bit mask per triangle of the full mesh (level of detail 0), bit `i` set
when the triangle is in region `i`. Reduced levels of detail carry no
regions. Painted triangles that do not exist in the model (after a
re-export) are left out without error.

## Inspector Info

- A model in the Input tree: a **Regions** row after Shapes: their count, or
  `none`.
- A compiled mesh: a **Regions** row after Shapes.
- The [Chunks view](models.md#chunks-view) shows `REGIONS`, `REGTAGS` and
  `TRIREGN`.

## Not yet

- Regions for reduced levels of detail, regions from the glTF and presets,
  and showing regions in the compiled mesh preview.
