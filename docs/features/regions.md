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
| Submeshes | glTF primitives whose triangles are all in the region, each by its glTF mesh and primitive index (a [submesh](models.md#geometry) of the compiled mesh). |
| Triangles | Triangles painted into the region, each by its glTF mesh, primitive and index in the primitive. |
| Color     | The color it is shown in, for assetc only (not compiled). |

- A triangle is in the region when its primitive's material is one of the
  region's materials, **or** its primitive is one of the region's
  submeshes, **or** it was painted into it.
- A primitive of a mesh used by several nodes is several submeshes in the
  compiled mesh; choosing it puts all of them in the region.
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
  - **Submeshes**: one checkbox per triangle primitive of the model, in
    file order: `<mesh> / <primitive>`, with `<mesh>` the glTF mesh's name
    (`mesh_<index>` when it has none) and `<primitive>` the primitive's
    index in it; hover shows its material (`no material`) and triangle
    count.
  - **Painted**: `<n> triangles`, then two toggle buttons, at most one
    on (filled with `ACCENT` at 25% while on; clicking the one that is on
    turns it off, the other turns it on and this one off): **Paint**
    (`paintbrush` and text) and **Erase** (`eraser` and text, the rubber);
    then a **Brush** slider (2–64 px, 12 to start with, for the session) and
    a **Clear** button (enabled with painted triangles) that removes them
    all.
  - A weak hint: `Drag on the model to paint or erase. Shift + drag does
    the other. Right drag orbits.`
- Edits are saved like the locators'; a stroke is saved when the pointer is
  released.

## 3D view

While the toolbar's [Show](models.md#show) **Regions** is on (and the
model has regions), the model is drawn in its regions' colors instead of its
materials, following the pose and animation (except while a
[vertex color set](vertex-colors.md#3d-view) is shown):

- Each triangle is flat shaded (its own normal, lit like the model) in the
  color of the last visible region in the list that covers it, or gray
  (`#808080`) when no visible region does. Hidden regions (eye off) leave
  their triangles to the others.
- Turning **Regions** off shows the model's materials again, so the two
  can be compared.
- The chosen region's triangles are also outlined (1 px, its color, on top
  of the model, without depth; triangles facing away from the camera are
  left out).
- A primitive with morph targets keeps its materials.
- Turning Show **Regions** off also turns Paint and Erase off; either
  turns it back on.

### Painting

While the chosen region's **Paint** or **Erase** is on (only one region
paints at a time; choosing another region, or leaving the Regions tab,
turns them off):

- **Left drag** on the model paints: every frame, the triangle under the
  pointer and the triangles whose center lies within the brush's radius of
  the point hit (the brush's size on screen, at the model's center), and
  that face the camera, are added to the region (Paint) or removed from it
  (Erase). **Shift + left drag** does the other one. Clicking paints once.
- The brush is shown at the pointer while it is over the canvas: a circle
  of its radius, 1 px, in the region's color for painting and `TEXT` for
  erasing (following Shift).
- **Right drag** still orbits the camera; the wheel still zooms.
- Locator markers cannot be clicked, and the gizmo is not drawn.
- `Escape` (pointer over the canvas) turns Paint and Erase off.

## Build

A model with regions gets [`REGIONS`, `REGTAGS` and `TRIREGN`](../spec/amesh.md#regions)
in its `.amesh` (none without regions): one record per region, and one
32-bit mask per triangle of the full mesh (level of detail 0), bit `i` set
when the triangle is in region `i` (by material, submesh or painting). Reduced levels of detail carry no
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
