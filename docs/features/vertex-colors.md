# Vertex colors

A glTF model's vertex colors are compiled into its
[`.amesh`](../spec/amesh.md#color-sets) as **color sets**: named buffers of
one RGBA color per vertex, alongside `VERTICES`. A mesh can have several
sets; a game picks one by the FNV-1a-64 hash of its name.

Sets come from two places:

- **glTF sets**: the model's `COLOR_n` attributes, see [From glTF](#from-gltf).
  Their names can be changed, see [Renaming](#renaming).
- **Authored sets**: made in assetc from the model's shape (a solid color,
  or a gradient laid over the model), see [Authored sets](#authored-sets).

A set can be laid over another one, which then becomes part of it, see
[Blending](#blending). All sets are in one list whose order is the order
they are written in, see [Order](#order).

Both are edited in the Inspector's [Vertex colors tab](#vertex-colors-tab)
and kept in the model's [`.amod`](../spec/amod.md#colors) sidecar.

## From glTF

- Each `COLOR_n` attribute (`n` = 0, 1, …) on any triangle primitive of
  the model becomes the set named `COLOR_n` (or its new name).
- Values are stored as **RGBA, 8 bits per channel, unsigned normalized,
  linear** (as glTF defines them; not sRGB-encoded): floats are clamped to
  0–1 and rounded to the nearest of 255 steps, 16-bit values rounded to 8
  bits, and RGB colors (`VEC3`) get alpha 255.
- A primitive without a set that another primitive has gets white
  (`255, 255, 255, 255`) for its vertices in that set, which leaves its
  material unchanged.
- Vertices are written in the same order as `VERTICES`, so vertex `v` of
  every set belongs to vertex `v` of the mesh. A primitive whose normals
  are generated flat (no `NORMAL`) has its vertices split per triangle;
  each copy keeps its source vertex's colors.
- Models without `COLOR_n` and without authored sets get no color set
  chunks.

## Renaming

A glTF set's name is `COLOR_n` until it is given another one. The sidecar
then lists the set as a `[[colors]]` entry with `gltf = "COLOR_n"` and its
name. Only the name changes: the colors stay as the glTF has them.

A glTF set the sidecar lists but the model no longer has (a new export
without it) fails the build until it is deleted from the list.

## Order

Sets are written in the order of the list:

- glTF sets the sidecar does not list come first, in order of `n`;
- then every `[[colors]]` entry, glTF or authored, in sidecar order.

So a model without a sidecar keeps its glTF order, and an authored set added
to a model whose new export brings `COLOR_0` moves to second place (a game
finds sets by name, so only code relying on positions notices). Dragging
cards in the tab sets any order; assetc then writes glTF entries as needed
to keep it, leaving out a leading run of unrenamed glTF sets in order of `n`,
which the first rule already places.

## Authored sets

An authored set gives every vertex a color computed from its position: in
model space, as the vertex is compiled (a skinned mesh's vertices at the bind
pose).

| Field  | Meaning |
|--------|---------|
| Name   | Non-empty, unique among all of the model's sets (glTF ones by their names). |
| Fill   | **Solid**, **Linear** or **Radial**; Linear to start with. |
| Color  | Solid: the one color, sRGB with alpha. |
| From   | Linear and Radial: a point in model space. Linear: where the gradient starts; Radial: its center. |
| To     | Linear and Radial: a point in model space. Linear: where the gradient ends; Radial: a point on its edge, so the radius is the distance from From. |
| Stops  | Linear and Radial: the gradient, as the [gradient editor](gradients.md#gradient-editor) edits it: at least two stops, each a position 0–1 and an sRGB color with alpha. |

Each vertex at `p` gets position `t` along the gradient:

- **Linear**: `t = (p − From) · (To − From) / |To − From|²`, the vertex
  projected onto the line from From to To.
- **Radial**: `t = |p − From| / |To − From|`.
- `t` is clamped to 0–1 (the first stop's color before it, the last's past
  it); From and To at the same point give `t = 0`.

Its color is the gradient's at `t` as a `.gradient` gradient without
dithering computes it ([blending](../spec/gradient.md#image): sRGB values,
premultiplied alpha), then stored like a glTF set: R, G and B converted from
sRGB to linear and rounded to 8 bits, alpha as it is. A Solid set's color is
stored the same way.

## Blending

An authored set can **blend with** another set of the model (glTF or
authored), its *base*: the set's colors are laid over the base's, and the
result is the set's colors. The base is then part of it: it is **not written
on its own**, so two sets become one. Blends chain (a base can blend with a
third set); a set cannot blend with itself or, through a chain, with a set
blending with it.

| Field  | Meaning |
|--------|---------|
| With   | The base, by name. |
| Mode   | **Mix**, **Multiply**, **Screen**, **Overlay** or **Add**; Mix to start with. |
| Amount | 0 to 1, how much of the set is laid over; 1 to start with. |

Per vertex, with both colors as sRGB values 0–1 (a glTF set's linear values
converted to sRGB, an authored set's as computed above before storing), `b`
the base and `t` the set:

- `f` per channel of R, G, B: Mix `t`; Multiply `b × t`; Screen
  `1 − (1 − b)(1 − t)`; Overlay `2bt` when `b < 0.5`, else
  `1 − 2(1 − b)(1 − t)`; Add `min(b + t, 1)`.
- `k = t.alpha × Amount`.
- RGB `b + (f − b) × k`, alpha `b.alpha + (1 − b.alpha) × k`.
- The result is stored like an authored set's color: RGB from sRGB to
  linear, rounded to 8 bits.

A set that is not blended is stored exactly as before: a glTF set's values
unchanged.



## Vertex colors tab

The Inspector's **Vertex colors** tab (`swatch-book`, the `.gradient` file
icon) is shown for a model in the Input tree:

- **Add color set** (`plus` and text) adds an authored set at the end of
  the list, named `colors` (or `colors_<n>`), Linear from the bottom to the
  top of the model (From and To at the middle of the model's bounding box in
  X and Z, at its lowest and highest Y), black (`#000000`) at 0 to white
  (`#ffffff`) at 1, and unfolds it.
- One card per set, in [order](#order); an accordion (one unfolded at a
  time; the unfolded one is the *chosen* set). Header:
  - the chevron and a **drag handle** (`grip-horizontal`): dragging it moves
    the card, as gradients are reordered (see
    [`noise.md`](noise.md#modifiers-tab)), which sets the order sets are
    written in;
  - a `link-2` icon when the set blends with another (`TEXT`, hover:
    `Blends with <base>`) or another blends with it (`TEXT_WEAK`, hover:
    `Blended into <set>: not written on its own`);
  - the name;
  - on the right: unfolded, **Delete** (`trash-2`) for authored sets and for
    glTF entries the model does not have; folded, a 48 × 12 px preview of an
    authored set (3 px corners, over a checkerboard: its gradient, first stop
    on the left, or its Solid color), or a weak `glTF` label for a glTF set
    (shown unfolded too).
- With no sets at all, a weak `No color sets` line.
- A **glTF set**'s body:
  - **Name** (text): an empty name gives the set `COLOR_n` back; a name
    another set has gets the old one back.
  - A weak line: `From the glTF's COLOR_n`, or `COLOR_n is not in the model`.
- An **authored set**'s body:
  - **Name** (text; an empty or taken name gets the old one back). Renaming
    a set renames it in the blends that use it.
  - **Fill**: dropdown Solid / Linear / Radial.
  - **Blend with**: dropdown `None` and the sets it may blend with (not
    itself, nor sets that blend with it, directly or through others).
    While set: **Mode** (dropdown) and **Amount** (slider 0–1, two
    decimals).
  - Solid: **Color**, the [color picker](gradients.md#selected-stop).
  - Linear and Radial:
    - **Gizmo** (the locators' group with **Move** only): moves the chosen
      end, From or To.
    - **From** and **To** (Radial: **Center** and **Edge**): X, Y, Z drag
      fields (three decimals); clicking a row's label chooses that end (the
      chosen one's label in `ACCENT`). From is chosen to start with.
    - **Fit**: buttons **X**, **Y** and **Z** lay the gradient along that
      axis of the model's bounding box: Linear from its lowest to its
      highest side through the box's middle; Radial centered on the box,
      reaching half its size along the axis.
    - **Swap** (button): From and To trade places.
    - The [gradient editor](gradients.md#gradient-editor) (bar, handles and
      the selected stop's color picker).
- Deleting a set clears the blends that used it.
- Edits are saved like the locators'.

## 3D view

While the Vertex colors tab shows and a set is chosen, the model's preview
draws the model in that set's colors (blended with its base, when it has
one) instead of its materials (lit, a rough
white material times the colors; alpha is not shown), following its
animation. Primitives a glTF set misses are white. This takes the place of
the [regions' colors](regions.md#3d-view) while it lasts.

For a chosen Linear or Radial set, From and To are drawn over the model (on
top of it, like path points) in model space:

- A 1 px `TEXT` line from From to To; for Radial, also a circle of the
  radius around From, facing the camera.
- Each end as a 5 px dot in `TEXT`, the chosen one 7 px with a 2 px `ACCENT`
  ring. Clicking within 8 px of an end chooses it.
- The [gizmo](locators.md#3d-view) in Move mode (and `W`) acts on the chosen
  end, in model axes. `E` does nothing there.

The 3D view shows the first instance's colors where one glTF mesh is drawn
by several nodes (the build computes each instance's own).

## Build

- A glTF entry for an attribute the model does not have fails the build:
  `<file>.amod: color set "<name>": no COLOR_<n> in the model`.
- Two sets with the same name (glTF sets the sidecar does not list are
  named `COLOR_n`) fail it: `<file>.amod: color set "<name>": name used
  twice`.
- A blend with a set the model does not have, or a loop of blends, fails it:
  `<file>.amod: color set "<name>": blends with no set "<base>"`,
  `<file>.amod: color set "<name>": blends in a loop`.
- Bases are not written; every other set is, in order.
- Each set's `COLORSET` record says where it came from in `source`: `0`
  glTF (unblended), `1` assetc (authored, or blended).

## Inspector Info

- A model in the Input tree: a **Color sets** row after Textures: the
  names of the `COLOR_n` attributes its primitives have (`COLOR_0, COLOR_1`),
  or `none`.
- A compiled mesh: a **Color sets** row after Skin: the set names, or
  `none`.
- The [Chunks view](models.md#chunks-view) shows `COLORSET` (with
  `source` as `glTF` or `assetc`) and `COLORS`.

## Previews

- The model's [preview](models.md#preview) shows `COLOR_0` as Bevy's glTF
  loader does: multiplied with the base color.

### Compiled meshes

A compiled mesh with color sets (in the Output tree) has a **Vertex colors**
tab (`swatch-book`) to pick the set its
[preview](models.md#compiled-mesh-preview) shows:

- A list of rows like the [Animations tab](models.md#animations-tab)'s:
  first **None**, then the mesh's sets in file order, each with its name
  and, right-aligned and weak, its source (`glTF` or `assetc`).
- **None** (the default) shows the mesh without vertex colors.
- A set: in **Material** mode it multiplies the base color, as a glTF
  renderer does with `COLOR_0`; in **Shaded** mode the mesh is drawn in the
  set's colors instead of light gray. Wireframe and the ID modes are
  unchanged. Alpha is not shown.
- The choice is remembered per mesh (by path) for the session, and applies
  to every level of detail.

## Not yet

- More fills: baked ambient occlusion, curvature, noise, region colors
  (these need vertices split along region edges).
- Leaving a glTF set out of the build without blending it.
- A compiled mesh preview mode showing any set on its own, and wider
  formats (16-bit or float channels; the record's `format` leaves room for
  them).
